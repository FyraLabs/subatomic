//! CLI tool for generating YUM/DNF repodata from a directory of RPM packages.
//! reference implementation of libsubatomic's [`RepoCache`] and [`Package`] types,
//! implemented as a clone of `createrepo`

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::Parser;
use tracing::{debug, info, instrument, trace, warn};

use itertools::Itertools;
use libsubatomic::pkg::Package;
use libsubatomic::repodata::RepoCache;

#[derive(Parser)]
#[command(name = "createrepo_rs")]
#[command(about = "Generate YUM/DNF repodata from a directory of RPM packages")]
struct Args {
    #[arg(short, long)]
    input: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(short, long, default_value = ".subatomic-cache")]
    cache: PathBuf,
    #[arg(long, default_value = "repo")]
    repo_name: String,
    #[arg(long, default_value_t = 0)]
    zstd_level: i32,
    #[arg(long)]
    incremental: bool,
}

#[instrument(skip_all)]
fn parse_package(path: &Path) -> Option<Package> {
    match Package::open(path) {
        Ok((pkg, _)) => {
            trace!(path = %path.display(), "parsed package");
            Some(pkg)
        }
        Err(e) => {
            warn!(path = %path.display(), error = %e, "skipping package");
            None
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    if !args.input.is_dir() {
        return Err(format!("input is not a directory: {}", args.input.display()).into());
    }

    std::fs::create_dir_all(&args.output)?;

    if !args.incremental && args.cache.exists() {
        debug!(cache = %args.cache.display(), "removing stale cache");
        std::fs::remove_dir_all(&args.cache)?;
    }
    std::fs::create_dir_all(&args.cache)?;

    let mut rpm_paths: Vec<PathBuf> = std::fs::read_dir(&args.input)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|ext| ext.eq_ignore_ascii_case("rpm")).unwrap_or(false))
        .collect();
    rpm_paths.sort();
    let total = rpm_paths.len();
    if total == 0 {
        warn!(dir = %args.input.display(), "no .rpm files found");
        return Ok(());
    }
    info!(count = total, "found rpm files");

    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &args.output)?;
    cache.zstd_level = args.zstd_level;

    let mut parsed = 0usize;
    let mut skipped = 0usize;
    let mut cached = 0usize;
    let mut expected_keys: HashSet<String> = HashSet::with_capacity(total);

    for (i, path) in rpm_paths.iter().enumerate() {
        let name = path.display().to_string();
        let key = match path.canonicalize() {
            Ok(p) => p.to_string_lossy().into_owned(),
            Err(_) => name.clone(),
        };
        expected_keys.insert(key.clone());

        if args.incremental && cache.has(&key) {
            debug!(path = %name, "cached, skipping");
            cached += 1;
            continue;
        }

        info!(progress = format!("[{}/{}]", i + 1, total), path = %name, "parsing");
        if let Some(pkg) = parse_package(path) {
            cache.insert(&key, &pkg, path.as_os_str())?;
            parsed += 1;
        } else {
            skipped += 1;
        }
    }

    if skipped > 0 {
        warn!(count = skipped, "packages could not be parsed");
    }
    if parsed > 0 {
        info!(count = parsed, "inserted packages into cache");
    }
    if cached > 0 {
        info!(count = cached, "skipped cached packages");
    }
    if cache.len() == 0 {
        warn!("cache is empty; nothing to write");
        return Ok(());
    }

    info!("writing repodata");
    cache.write_all(&args.output)?;

    if args.incremental {
        let expected_refs: HashSet<&str> = expected_keys.iter().map(String::as_str).collect();
        let removed = cache.prune(&expected_refs)?;
        if removed > 0 {
            info!(count = removed, "pruned stale cached packages");
        }
    }

    info!(dir = %args.output.display(), "repodata written");
    Ok(())
}
