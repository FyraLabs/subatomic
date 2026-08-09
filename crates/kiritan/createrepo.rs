#![allow(clippy::cast_possible_truncation)]
use crate::cli::{Cli, CreaterepoMode};
use color_eyre::Result;
use color_eyre::eyre::bail;
use jwalk::rayon::iter::{ParallelBridge, ParallelIterator};
use libsubatomic::pkg::Package;
use libsubatomic::repodata::{RepoCache, repomd::DataType};
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use tracing::{debug, error, info, trace};

pub fn run(args: Cli) -> Result<()> {
    if !args.input.is_dir() {
        bail!("input is not a directory: {}", args.input.display());
    }

    std::fs::create_dir_all(args.output())?;

    if let CreaterepoMode::Auto { no_cache: true } = args.mode
        && args.cache.exists()
    {
        debug!("removing stale cache");
        std::fs::remove_dir_all(&args.cache)?;
    }
    std::fs::create_dir_all(&args.cache)?;

    let (add, remove, comps) = match args.mode {
        CreaterepoMode::Auto { no_cache } => {
            process_rpms_auto(&args, !no_cache)?;
            return Ok(());
        }
        CreaterepoMode::Manual { add, remove, comps } => (add, remove, comps),
    };
    let output = args.output.unwrap_or_else(|| args.input.join("repodata"));

    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &output)?;
    cache.zstd_level = args.zstd_level;
    cache.zstd_multi = args.zstd_multi.try_into().unwrap_or_else(|_| num_cpus::get() as u32);
    let cache = Arc::new(cache);
    let cache2 = Arc::clone(&cache);

    let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
    let joinhdl = std::thread::spawn(move || {
        cache2.update_frags(&rx).inspect_err(|e| tracing::error!(?e, "update_frags failed"))
    });

    let len = add.len();

    add.into_iter().enumerate().par_bridge().try_for_each(|(i, path)| {
        info!(progress = format!("[{}/{}]", i + 1, len), path = %path.display(), "parsing");
        match Package::open(&path) {
            Ok((pkg, mut rpmreader)) => {
                trace!(filename = %path.display(), "process");
                let mut frag = libsubatomic::repodata::FragEph::new(&pkg, path.as_os_str());
                if args.appstream {
                    frag.app = libsubatomic::repodata::Frag(Some(Package::appstream_frag(
                        &mut rpmreader,
                    )?));
                }
                trace!(filename = %path.display(), "sending");
                tx.send((path, Some(frag))).expect("tx should be open");
            }
            Err(e) => error!(path = %path.display(), error = %e, "skipping"),
        }
        libsubatomic::err::Res::Ok(())
    })?;
    drop(tx);
    joinhdl.join().expect("can't join")?;

    if !remove.is_empty() {
        let to_remove: Vec<&[u8]> = remove.iter().map(String::as_bytes).collect();
        for not_found in cache.delete_pkgs(&to_remove)? {
            error!(not_found = %std::ffi::OsStr::from_bytes(not_found).display(), "some packages not found in cache");
        }
    }

    let mut cache = Arc::into_inner(cache).expect("cache arc should be single");

    if let Some(comps_path) = comps {
        let comps_bytes = std::fs::read(comps_path)?;
        let repo = libsubatomic::Repo { cache, sig: None, use_appstream: args.appstream };
        // TODO: どっちに附則するかチグハグだね
        // maybe add this fn to repocache too?
        repo.add_comps(&comps_bytes)?;
        cache = repo.cache;
    }

    let mut datatypes = vec![DataType::Primary, DataType::Filelists, DataType::Other];
    if args.appstream {
        datatypes.push(DataType::Appstream);
    }

    info!("writing repodata");
    let _repomd = cache.write_all(&datatypes)?;

    if args.compact {
        cache.compact_close()?;
    }

    info!(dir = %output.display(), "repodata written");
    Ok(())
}

fn process_rpms_auto(args: &Cli, incremental: bool) -> Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
    let mut cache = RepoCache::new(&args.repo_name, &args.cache, args.output())?;
    cache.zstd_level = args.zstd_level;
    cache.zstd_multi = args.zstd_multi.try_into().unwrap_or_else(|_| num_cpus::get() as u32);
    let cache = Arc::new(cache);
    let cache2 = Arc::clone(&cache);

    let joinhdl = std::thread::spawn(move || {
        cache2.update_frags(&rx).inspect_err(|e| tracing::error!(?e, "update_frags failed"))
    });
    jwalk::WalkDir::new(&args.input).into_iter().par_bridge().try_for_each_init(
        || {
            tracing::debug!("creating rtxn");
            cache.env.read_txn().expect("cannot create rtxn")
        },
        |txn, fd| {
            let p = fd?.path();
            if !p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpm")) {
                return Ok::<_, color_eyre::Report>(());
            }
            let filename = p.file_name().expect("no filename");
            if incremental && cache.db_epo.get(txn, filename.as_bytes())?.is_some() {
                tx.send((p, None))?;
                return Ok(());
            }
            debug!(filename = %filename.display(), "parsing");
            match Package::open(&p) {
                Ok((pkg, mut rpmreader)) => {
                    trace!(filename = %filename.display(), "process");
                    let mut frag = libsubatomic::repodata::FragEph::new(&pkg, p.as_os_str());
                    if args.appstream {
                        frag.app = libsubatomic::repodata::Frag(Some(Package::appstream_frag(
                            &mut rpmreader,
                        )?));
                    }
                    trace!(filename = %filename.display(), "sending");
                    tx.send((p, Some(frag)))?;
                }
                Err(e) => error!(path = %p.display(), error = %e, "skipping"),
            }
            Ok(())
        },
    )?;
    drop(tx); // close
    debug!("joining");
    let (n_new, n_cached) = joinhdl.join().expect("cannot join")?;
    info!(?n_new, ?n_cached, "all rpms processed");

    let mut datatypes = vec![DataType::Primary, DataType::Filelists, DataType::Other];
    if args.appstream {
        datatypes.push(DataType::Appstream);
    }

    info!("writing repodata");
    let _repomd = cache.write_all(&datatypes)?;

    if args.compact {
        Arc::into_inner(cache).expect("cache arc should be single").compact_close()?;
    }
    Ok(())
}
