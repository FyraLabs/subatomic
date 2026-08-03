use crate::cli::{CreaterepoArgs, CreaterepoMode};
use color_eyre::Result;
use color_eyre::eyre::{ContextCompat, bail};
use jwalk::rayon::iter::{ParallelBridge, ParallelIterator};
use libsubatomic::pkg::Package;
use libsubatomic::repodata::RepoCacheFragment;
use libsubatomic::repodata::{RepoCache, repomd::DataType};
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use tracing::{debug, error, info, trace};

pub fn run(args: CreaterepoArgs) -> Result<()> {
    if !args.input.is_dir() {
        bail!("input is not a directory: {}", args.input.display());
    }

    std::fs::create_dir_all(&args.output)?;

    if let CreaterepoMode::Auto { incremental: false } = args.mode
        && args.cache.exists()
    {
        debug!("removing stale cache");
        std::fs::remove_dir_all(&args.cache)?;
    }
    std::fs::create_dir_all(&args.cache)?;

    let (add, remove, comps) = match args.mode {
        CreaterepoMode::Auto { incremental } => {
            process_rpms_auto(&args, incremental)?;
            return Ok(());
        }
        CreaterepoMode::Manual { add, remove, comps } => (add, remove, comps),
    };

    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &args.output)?;
    cache.zstd_level = args.zstd_level;

    let results = add.iter().enumerate().par_bridge().map(|(i, path)| {
        info!(progress = format!("[{}/{}]", i + 1, add.len()), path = %path.display(), "parsing");
        match Package::open(path) {
            Ok((pkg, mut rpmreader)) => {
                let appstream_frag = if args.appstream {
                    Package::appstream_frag(&mut rpmreader)?
                } else {
                    Vec::new()
                };
                Ok((0, Some((pkg, path.file_name().context("missing filename")?, appstream_frag)), 0))
            }
            Err(e) => {
                error!(path = %path.display(), error = %e, "skipping");
                Ok((0, None, 1))
            }
        }
    }).collect::<Result<Vec<_>, color_eyre::Report>>()?;

    let mut skipped = 0;
    let mut cached = 0;
    let mut new = Vec::with_capacity(results.len());

    for (c, p, s) in results {
        cached += c;
        new.extend(p);
        skipped += s;
    }
    let parsed = new.len();
    cache.insert_pkgs(new)?;

    info!(parsed, skipped, cached, "processing complete");

    if !remove.is_empty() {
        let to_remove: Vec<&[u8]> = remove.iter().map(String::as_bytes).collect();
        let not_found = cache.delete_pkgs(&to_remove)?;
        if !not_found.is_empty() {
            error!(not_found = not_found.len(), "some packages not found in cache");
        }
    }

    if let Some(comps_path) = comps {
        let comps_bytes = std::fs::read(comps_path)?;
        let repo =
            libsubatomic::Repo { cache: cache.clone(), sig: None, use_appstream: args.appstream };
        // TODO: どっちに附則するかチグハグだね
        // maybe add this fn to repocache too?
        repo.add_comps(&comps_bytes)?;
    }

    let mut datatypes = vec![DataType::Primary, DataType::Filelists, DataType::Other];
    if args.appstream {
        datatypes.push(DataType::Appstream);
    }

    info!("writing repodata");
    let _repomd = cache.write_all(&datatypes)?;

    if args.compact {
        drop(cache.compact()?);
    }

    info!(dir = %args.output.display(), "repodata written");
    Ok(())
}

fn process_rpms_auto(args: &CreaterepoArgs, incremental: bool) -> Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &args.output)?;
    cache.zstd_level = args.zstd_level;
    let cache = Arc::new(cache);
    let cache2 = Arc::clone(&cache);

    let mut txn = cache.env.write_txn()?;
    let db: libsubatomic::repodata::FragsDb =
        cache.env.create_database(&mut txn, Some(&cache.repo))?;
    txn.commit()?;
    let db = Arc::new(db);
    let db2 = Arc::clone(&db);
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("time underflow")
        .as_micros();
    let joinhdl = std::thread::spawn(move || {
        cache2
            .update_frags(db2, rx, epoch)
            .inspect_err(|e| tracing::error!(?e, "update_frags failed"))
    });
    jwalk::WalkDir::new(&args.input).into_iter().par_bridge().try_for_each(|fd| {
        let p = fd?.path();
        if !p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpm")) {
            return Ok::<_, color_eyre::Report>(());
        }
        let filename = p.file_name().expect("no filename");
        if incremental {
            let txn = cache.env.read_txn()?;
            if let Some(mut frag) = db.get(&txn, filename.as_bytes())? {
                txn.commit()?;
                frag.epoch = epoch;
                tx.send((filename.as_bytes().to_owned(), frag, false))?;
                return Ok(());
            }
            txn.commit()?;
        }
        debug!(filename = %filename.display(), "parsing");
        match Package::open(&p) {
            Ok((pkg, mut rpmreader)) => {
                trace!(filename = %filename.display(), "process");
                let appstream_frag = if args.appstream {
                    Package::appstream_frag(&mut rpmreader)?
                } else {
                    Vec::new()
                };
                let mut frag = RepoCacheFragment::new(&pkg, p.as_os_str(), appstream_frag);
                frag.epoch = epoch;
                trace!(filename = %filename.display(), "sending");
                tx.send((filename.as_bytes().to_owned(), frag, true))?;
            }
            Err(e) => error!(path = %p.display(), error = %e, "skipping"),
        }
        Ok(())
    })?;
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
