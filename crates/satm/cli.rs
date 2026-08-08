use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "satm")]
/// Tool for interactive with subatomic servers and an alternative to `createrepo_c`.
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Generate repodata locally (like createrepo)
    Createrepo(CreaterepoArgs),

    /// Interact with the repos endpoint on subatomic server
    Repo(RepoArgs),

    /// Interact with the keys endpoint on subatomic server
    Key(KeyArgs),
}

#[derive(Parser)]
pub struct CreaterepoArgs {
    /// Path to directory for a list of rpms, which will be searched recursively.
    #[arg(short, long)]
    pub input: PathBuf,
    /// Path to the `repodata/` directory, where xml metadata will be written to.
    #[arg(short, long)]
    pub output: PathBuf,
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

#[derive(Subcommand)]
pub enum CreaterepoMode {
    /// In auto mode, satm scans for changes in the target directory automatically.
    Auto {
        /// Lookup the file name in the cache.
        #[arg(long)]
        incremental: bool,
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

#[derive(Parser)]
pub struct RepoArgs {
    /// Base URL of the subatomic API server
    #[arg(long, env = "SUBATOMIC_API_URL")]
    pub url: Option<String>,

    /// JWT token for authentication (also via `SUBATOMIC_API_TOKEN` env)
    #[arg(long, env = "SUBATOMIC_API_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    #[command(subcommand)]
    pub subcmd: RepoSubcommand,
}

#[derive(Subcommand)]
pub enum RepoSubcommand {
    List,
    Create {
        name: String,
    },
    Upload {
        name: String,
        paths: Vec<PathBuf>,
    },
    Delete {
        name: String,
    },
    Comps {
        #[command(subcommand)]
        action: CompsAction,
    },
    RepoKey {
        #[command(subcommand)]
        action: RepoKeyAction,
    },
    Pkg {
        #[command(subcommand)]
        action: PkgAction,
    },
    Refresh {
        name: String,
    },
}

#[derive(Subcommand)]
pub enum CompsAction {
    Upload { name: String, file: PathBuf },
    Delete { name: String },
}

#[derive(Subcommand)]
pub enum RepoKeyAction {
    Get { name: String },
    Set { name: String, id: i32 },
    Delete { name: String },
}

#[derive(Subcommand)]
pub enum PkgAction {
    List { name: String },
    Delete { name: String, rpms: Vec<String> },
}

#[derive(Parser)]
pub struct KeyArgs {
    /// Base URL of the subatomic API server
    #[arg(long, env = "SUBATOMIC_API_URL")]
    pub url: Option<String>,

    /// JWT token for authentication (also via `SUBATOMIC_API_TOKEN` env)
    #[arg(long, env = "SUBATOMIC_API_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    #[command(subcommand)]
    pub subcmd: KeySubcommand,
}

#[derive(Subcommand)]
pub enum KeySubcommand {
    List,
    Get {
        id: i32,
    },
    /// Create a new signing key.
    Create {
        /// Key name.
        name: String,
        /// User ID of the key, probably in the format `Repository Name <mail@example.com>`.
        userid: String,
    },
    Delete {
        id: i32,
    },
}
