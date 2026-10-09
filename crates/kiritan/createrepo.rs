use crate::cli::{Cli, CreaterepoMode};
use color_eyre::Result;
use color_eyre::eyre::bail;
use jwalk::rayon::iter::{ParallelBridge, ParallelIterator};
use kuchiyose::comp::CompConfig;
use kuchiyose::ftmm::Ftmm;
use kuchiyose::store::StoreBackend;
use libsubatomic::metan_prelude::*;
use libsubatomic::repo::FragRequest;
use libsubatomic::repo::hierarchy::Hierarchize;
use libsubatomic::{Cache, CacheConfig};
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use tracing::{debug, error, info};

pub fn run(args: Cli) -> Result<()> {
    if !args.input.is_dir() {
        bail!("input is not a directory: {}", args.input.display());
    }

    std::fs::create_dir_all(args.output())?;

    let hier = libsubatomic::repo::hierarchy::Satm0FlatHierarchy {
        base: args.output().display().to_string().into(),
    };

    let metans: Metans = {
        let mut v: Metans = vec![
            Arc::new(PrimaryMetan::default()) as Arc<dyn Metan>,
            Arc::new(FilelistsMetan::default()),
            Arc::new(OtherMetan::default()),
        ];
        if args.appstream {
            let mut metan = AppstreamMetan::default();
            metan.repo = args.repo_name.clone().into();
            v.push(Arc::new(metan) as Arc<dyn Metan>);
        }
        v
    };

    let comp_cfg = CompConfig::Zstd(kuchiyose::comp::zstd::Cfg {
        level: args.zstd_level,
        multi: args.zstd_multi.try_into().unwrap_or_else(|_| num_cpus::get() as u32),
    });

    let cfg = CacheConfig {
        repo: args.repo_name.clone().into(),
        cache_dir: args.cache.clone(),
        hier: hier.clone(),
        store: Arc::new(StoreBackend::Local),
        lmdb_map_size: libsubatomic::cache::DEFAULT_MAP_SIZE,
        ftmm: Ftmm::Sha256,
        ..
    };

    if let CreaterepoMode::Auto { no_cache: true } = args.mode
        && args.cache.exists()
    {
        debug!("removing stale cache");
        std::fs::remove_dir_all(&args.cache)?;
    }
    std::fs::create_dir_all(&args.cache)?;

    let cache = Cache::new(cfg.clone(), metans)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

    let (add, remove, comps) = match args.mode {
        CreaterepoMode::Auto { .. } => {
            let cache = Arc::new(cache);
            process_rpms_auto(&args, cache, &comp_cfg)?;
            return Ok(());
        }
        CreaterepoMode::Manual { ref add, ref remove, ref comps } => (add, remove, comps),
        CreaterepoMode::Md { delete, key, file } => {
            let repo = libsubatomic::Repo { tempdir: None, cache, sig: None, comp_cfg, .. };
            if delete {
                rt.block_on(repo.del_custom(&key))?;
                return Ok(());
            }
            let f = file.expect("filename should be provided unless with --delete");
            let content = rt.block_on(tokio::fs::File::open(&f))?;
            let Some(filename) = f.file_name().expect("bad filename").to_str() else {
                bail!("invalid utf-8: {}", f.display())
            };
            rt.block_on(repo.write_custom(&key, &filename, content))?;
            return Ok(());
        }
    };

    let cache = Arc::new(cache);
    let cache2 = Arc::clone(&cache);

    let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
    let joinhdl = std::thread::spawn(move || {
        cache2.update_frags(&rx).inspect_err(|e| tracing::error!(?e, "update_frags failed"))
    });

    let len = add.len();
    add.into_iter().enumerate().par_bridge().for_each(|(i, p)| {
        info!(progress = format!("[{}/{len}]", i + 1), path = %p.display(), "queued");
        let Ok(computed) = cache
            .compute(p, libsubatomic::cache::ComputeInput::default())
            .inspect_err(|err| tracing::error!(?err, "cannot compute frag"))
        else {
            return;
        };
        tx.send((p.into(), FragRequest::Put(computed))).expect("can't send");
    });
    drop(tx);
    joinhdl.join().expect("can't join")?;

    if !remove.is_empty() {
        let to_remove: Vec<&[u8]> = remove.iter().map(String::as_bytes).collect();
        for not_found in cache.delete_pkgs(&to_remove)? {
            error!(
                not_found = %std::ffi::OsStr::from_bytes(not_found).display(),
                "some packages not found in cache"
            );
        }
    }

    let cache = Arc::into_inner(cache).expect("cache arc should be single");

    if let Some(comps_path) = comps {
        let fd = rt.block_on(tokio::fs::File::open(comps_path))?;
        let repo = libsubatomic::Repo { cache, comp_cfg: comp_cfg.clone(), .. };
        rt.block_on(repo.write_custom("group", "comps.xml", fd))?;
    } else {
        info!("writing repodata");
        rt.block_on(cache.write_all(&comp_cfg))?;
        if args.compact {
            cache.compact_close()?;
        }
    }

    info!(dir = %args.output().display(), "repodata written");
    Ok(())
}

fn process_rpms_auto(
    args: &Cli,
    cache: Arc<Cache<libsubatomic::repo::hierarchy::Satm0FlatHierarchy>>,
    comp_cfg: &CompConfig,
) -> Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
    let cache2 = Arc::clone(&cache);
    let joinhdl = std::thread::spawn(move || {
        cache2.update_frags(&rx).inspect_err(|e| tracing::error!(?e, "update_frags failed"))
    });

    jwalk::WalkDir::new(&args.input).into_iter().par_bridge().try_for_each_init(
        || cache.env.read_txn().expect("cannot create rtxn"),
        |txn, fd| -> Result<()> {
            let p = fd?.path();
            if !p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpm")) {
                return Ok(());
            }

            let Some(filename) = p.file_name() else {
                return Ok(());
            };
            if cache.cfg.hier.locate_relative(filename).is_none() {
                return Ok(());
            };
            let Some(link) = cache.cfg.hier.locate_relative(filename) else {
                return Ok(());
            };
            let mut input = libsubatomic::cache::ComputeInput::default();
            input.link = Some(link);

            let req = if cache.epo.get(txn, filename.as_bytes())?.is_some() {
                FragRequest::Cached
            } else {
                let Ok(computed) = cache
                    .compute(&p, input)
                    .inspect_err(|err| tracing::error!(?err, "cannot compute frag"))
                else {
                    return Ok(());
                };
                FragRequest::Put(computed)
            };
            tx.send((p, req)).expect("can't send");
            Ok(())
        },
    )?;

    drop(tx);
    debug!("joining");
    let (n_new, n_cached) = joinhdl.join().expect("cannot join")?;
    info!(?n_new, ?n_cached, "all rpms processed");

    info!("writing repodata");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(cache.write_all(&comp_cfg))?;

    if args.compact {
        Arc::into_inner(cache).expect("cache arc should be single").compact_close()?;
    }
    Ok(())
}
