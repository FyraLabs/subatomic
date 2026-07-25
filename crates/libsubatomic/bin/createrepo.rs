//! CLI tool for generating YUM/DNF repodata from a directory of RPM packages.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::Parser;

use libsubatomic::pkg::Package;
use libsubatomic::repodata::RepoCache;

#[derive(Parser)]
#[command(name = "createrepo_rs")]
#[command(about = "Generate YUM/DNF repodata from a directory of RPM packages")]
struct Args {
    /// Directory containing RPM files
    #[arg(short, long)]
    input: PathBuf,

    /// Directory to write repodata XML files into
    #[arg(short, long)]
    output: PathBuf,

    /// LMDB cache directory for intermediate XML fragments
    #[arg(short, long, default_value = ".subatomic-cache")]
    cache: PathBuf,

    /// Repository name (used as the LMDB database name)
    #[arg(long, default_value = "repo")]
    repo_name: String,

    /// Zstd compression level (0–22, default 0)
    #[arg(long, default_value_t = 0)]
    zstd_level: i32,

    /// Re-use cached XML fragments from previous runs;
    /// only parse and insert packages not already in the cache.
    #[arg(long)]
    incremental: bool,
}

fn parse_package(path: &Path) -> Option<Package> {
    match Package::try_from(path) {
        Ok(pkg) => Some(pkg),
        Err(e) => {
            eprintln!("warning: skipping {}: {e}", path.display());
            None
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if !args.input.is_dir() {
        return Err(format!("input is not a directory: {}", args.input.display()).into());
    }

    std::fs::create_dir_all(&args.output)?;

    // Full rebuilds start with a fresh cache so stale entries don't persist
    if !args.incremental && args.cache.exists() {
        std::fs::remove_dir_all(&args.cache)?;
    }
    std::fs::create_dir_all(&args.cache)?;

    let mut rpm_paths: Vec<PathBuf> = std::fs::read_dir(&args.input)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .map(|ext| ext.eq_ignore_ascii_case("rpm"))
                .unwrap_or(false)
        })
        .collect();
    rpm_paths.sort();
    let total = rpm_paths.len();
    if total == 0 {
        eprintln!("warning: no .rpm files found in {}", args.input.display());
        return Ok(());
    }
    eprintln!("found {total} rpm files");

    let mut cache = RepoCache::new(&args.repo_name, &args.cache)?;
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
            eprintln!("cached, skipping {name}");
            cached += 1;
            continue;
        }

        eprint!("\rparsing [{:>w$}/{total}] {name}", i + 1, w = total.to_string().len());
        if let Some(pkg) = parse_package(path) {
            cache.insert(&key, &pkg, path)?;
            parsed += 1;
            if i + 1 < total {
                eprint!("\r{name}\x1B[K\n");
            }
        } else {
            skipped += 1;
            if i + 1 < total {
                eprint!("\r{name}\x1B[K\n");
            }
        }
    }
    eprintln!();

    if skipped > 0 {
        eprintln!("warning: {skipped} packages could not be parsed");
    }
    if parsed > 0 {
        eprintln!("inserted {parsed} packages into cache");
    }
    if cached > 0 {
        eprintln!("skipped {cached} cached packages");
    }
    if cache.len() == 0 {
        eprintln!("warning: cache is empty; nothing to write");
        return Ok(());
    }

    eprintln!("writing repodata ...");
    let temp_dir = tempfile::tempdir_in(&args.output)?;
    cache.write_all(&args.output, temp_dir.path())?;

    // Prune stale entries on incremental runs so the cache doesn't grow forever
    if args.incremental {
        let expected_refs: HashSet<&str> = expected_keys.iter().map(String::as_str).collect();
        let removed = cache.prune(&expected_refs)?;
        if removed > 0 {
            eprintln!("pruned {removed} stale cached packages");
        }
    }

    println!("repodata written to {}", args.output.display());
    Ok(())
}
