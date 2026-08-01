use crate::cli::{CreaterepoArgs, CreaterepoMode};
use color_eyre::Result;
use color_eyre::eyre::{ContextCompat, bail};
use libsubatomic::pkg::Package;
use libsubatomic::repodata::{RepoCache, repomd::DataType};
use std::collections::HashSet;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use tracing::{debug, error, info};

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

    let rpms_to_process = match &args.mode {
        CreaterepoMode::Auto { .. } => {
            super let paths: Vec<PathBuf> = (jwalk::WalkDir::new(&args.input).into_iter())
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpm")))
                .collect();
            &paths
        }
        CreaterepoMode::Manual { add, .. } => add,
    };

    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &args.output)?;
    cache.zstd_level = args.zstd_level;

    let mut parsed = 0;
    let mut skipped = 0;
    let mut cached = 0;
    let mut expected_keys = HashSet::new();
    let mut new = Vec::new();

    for (i, path) in rpms_to_process.iter().enumerate() {
        let key = path.file_name().context("missing filename")?.as_bytes();
        expected_keys.insert(key);

        let should_skip = if let CreaterepoMode::Auto { incremental } = &args.mode {
            *incremental && cache.has(key)?
        } else {
            // In manual mode we always process the list
            false
        };

        if should_skip {
            cached += 1;
            continue;
        }

        info!(progress = format!("[{}/{}]", i + 1, rpms_to_process.len()), path = %path.display(), "parsing");
        match Package::open(path) {
            Ok((pkg, mut rpmreader)) => {
                let appstream_frag = if args.appstream {
                    Package::appstream_frag(&mut rpmreader)?
                } else {
                    Vec::new()
                };
                new.push((pkg, path.file_name().context("missing filename")?, appstream_frag));
                parsed += 1;
            }
            Err(e) => {
                error!(path = %path.display(), error = %e, "skipping");
                skipped += 1;
            }
        }
    }
    cache.insert_pkgs(new)?;

    info!(parsed, skipped, cached, "processing complete");

    if let CreaterepoMode::Manual { remove, .. } = &args.mode
        && !remove.is_empty()
    {
        let to_remove: Vec<&[u8]> = remove.iter().map(String::as_bytes).collect();
        let not_found = cache.delete_pkgs(&to_remove)?;
        if !not_found.is_empty() {
            error!(not_found = not_found.len(), "some packages not found in cache");
        }
    }

    if let CreaterepoMode::Manual { comps: Some(comps_path), .. } = &args.mode {
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

    if let CreaterepoMode::Auto { incremental: true } = &args.mode {
        let removed = cache.prune(&expected_keys)?;
        if removed > 0 {
            info!(removed, "pruned stale packages");
        }
    }

    if args.compact {
        drop(cache.compact()?);
    }

    info!(dir = %args.output.display(), "repodata written");
    Ok(())
}
