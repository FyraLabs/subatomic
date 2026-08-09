#![warn(rust_2018_idioms)]

use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod cli;
mod createrepo;

fn main() -> color_eyre::Result<()> {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();
    color_eyre::install().expect("cannot install color_eyre");

    tracing::debug!(ver = env!("CARGO_PKG_VERSION"), "きりたん");
    let cli = cli::Cli::parse();
    createrepo::run(cli)
}
