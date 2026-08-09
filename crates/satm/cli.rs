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
    /// Interact with the repos endpoint on subatomic server
    Repo(RepoArgs),

    /// Interact with the keys endpoint on subatomic server
    Key(KeyArgs),
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
