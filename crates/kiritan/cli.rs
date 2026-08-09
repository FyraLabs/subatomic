use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "kiritan")]
/// Alternative to `createrepo_c`.
pub struct Cli {
    /// Path to directory for a list of rpms, which will be searched recursively.
    #[arg(default_value = ".")]
    pub input: PathBuf,
    /// Path to the `repodata/` directory, where xml metadata will be written to.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Path to cache directory, initialized automatically if it doesn't exist yet.
    #[arg(short, long, default_value = ".subatomic-cache")]
    pub cache: PathBuf,
    /// Preferred identifier of the repository.
    #[arg(long, default_value = "repo")]
    pub repo_name: String,
    /// Compression level of `zstd`. A level of 0 fallbacks to the default value of the internal
    /// rust zstd library (which should be 3).
    #[arg(long, default_value_t = 0)]
    pub zstd_level: i32,
    /// Number of workers for `zstdmt` (multithreading). This speeds up the final repo metadata xml
    /// generation at the cost of a sharp memory peak. The default value is the number of logical
    /// cores. Setting this to 1 separates zstd and hashing into 2 different threads, while setting
    /// this to 0 causes both to happen on the same thread. Setting this to a value larger than 0
    /// may cause the peak memory usage to skyrocket by more than 10 times, so use this with caution.
    #[arg(long, default_value_t = 0)]
    pub zstd_multi: i64,
    /// Whether appstream support should be enabled.
    #[arg(long)]
    pub appstream: bool,
    /// Whether the cache should be compacted after the run, otherwise outdated records will not be
    /// removed. You should enable this to prevent the cache from growing indefinitely.
    #[arg(long, default_value_t = true)]
    pub compact: bool,
    #[command(subcommand)]
    pub mode: CreaterepoMode,
}

impl Cli {
    pub fn output(&self) -> &Path {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        self.output.as_deref().unwrap_or_else(|| DIR.get_or_init(|| self.input.join("repodata")))
    }
}

#[derive(Subcommand)]
pub enum CreaterepoMode {
    /// In auto mode, satm scans for changes in the target directory automatically.
    Auto {
        /// Disable caching. When the cache is enabled, kiritan looks up package filenames.
        #[arg(long)]
        no_cache: bool,
    },
    /// In manual mode, satm modifies the cache and repodata according to user input.
    Manual {
        /// Added / updated packages.
        #[arg(long)]
        add: Vec<PathBuf>,
        /// Removed packages.
        #[arg(long)]
        remove: Vec<String>,
        /// Path to comps.xml if added/updated.
        #[arg(long)]
        comps: Option<PathBuf>,
    },
}
