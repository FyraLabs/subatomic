//! CLI tool for generating YUM/DNF repodata from a directory of RPM packages.

use std::path::{Path, PathBuf};

use clap::Parser;

use itertools::Itertools;
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
}

fn parse_package(path: &Path) -> Option<(Package, PathBuf)> {
    match Package::try_from(path) {
        Ok(pkg) => Some((pkg, path.into())),
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
    std::fs::create_dir_all(&args.cache)?;

    let mut rpm_paths: Vec<PathBuf> = std::fs::read_dir(&args.input)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|ext| ext.eq_ignore_ascii_case("rpm")).unwrap_or(false))
        .collect();
    rpm_paths.sort();
    let total = rpm_paths.len();
    if total == 0 {
        eprintln!("warning: no .rpm files found in {}", args.input.display());
        return Ok(());
    }
    eprintln!("found {total} rpm files");

    let mut packages = Vec::with_capacity(total);
    for (i, path) in rpm_paths.iter().enumerate() {
        let name = path.display().to_string();
        eprint!("\rparsing [{:>w$}/{total}] {name}", i + 1, w = total.to_string().len());
        if let Some(pair) = parse_package(path) {
            packages.push(pair);
        }
        if i + 1 < total {
            // overwrite the counter with just the path, then move to next line
            eprint!("\r{name}\x1B[K\n");
        }
    }
    eprintln!();
    if packages.len() != total {
        eprintln!("warning: {}/{total} packages could not be parsed", total - packages.len());
    }
    if packages.is_empty() {
        eprintln!("warning: no packages could be parsed; repodata will be empty");
    }

    eprintln!("generating xml fragments ...");
    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &args.output)?;
    cache.zstd_level = args.zstd_level;
    let refs: Vec<(&Package, &Path)> =
        packages.iter().map(|(pkg, path)| (pkg, path.as_path())).collect();
    cache.insert_pkgs(refs)?;

    eprintln!("writing repodata ...");
    let temp_dir = tempfile::tempdir()?;
    cache.write_all(&args.output)?;

    println!("repodata written to {}", args.output.display());
    Ok(())
}
