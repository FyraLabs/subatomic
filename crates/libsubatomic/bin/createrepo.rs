//! CLI tool for generating YUM/DNF repodata from a directory of RPM packages.

use std::path::PathBuf;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if !args.input.is_dir() {
        return Err(format!("input is not a directory: {}", args.input.display()).into());
    }

    std::fs::create_dir_all(&args.output)?;
    std::fs::create_dir_all(&args.cache)?;

    let mut cache = RepoCache::new(&args.repo_name, &args.cache, &args.output)?;
    cache.zstd_level = args.zstd_level;

    let rpm_paths: Vec<PathBuf> = std::fs::read_dir(&args.input)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|ext| ext.eq_ignore_ascii_case("rpm")).unwrap_or(false))
        .collect();

    if rpm_paths.is_empty() {
        eprintln!("warning: no .rpm files found in {}", args.input.display());
    }

    let packages: Vec<(Package, PathBuf)> = rpm_paths
        .into_iter()
        .map(|path| {
            let (pkg, _) =
                Package::open(path.as_path()).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((pkg, path))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    let refs = packages.iter().map(|(pkg, path)| (pkg, path.as_os_str())).collect_vec();

    cache.insert_pkgs(refs)?;

    cache.write_all(&args.output)?;

    println!("repodata written to {}", args.output.display());
    Ok(())
}
