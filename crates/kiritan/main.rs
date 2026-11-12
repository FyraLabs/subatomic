#![warn(rust_2018_idioms)]
#![feature(default_field_values)]
#![feature(iterator_try_collect)]

use clap::Parser;
use tracing_subscriber::EnvFilter;

mod cli;
mod createrepo;

fn main() -> color_eyre::Result<()> {
    color_eyre::install().expect("cannot install color_eyre");
    let cli = cli::Cli::parse();
    let filter = EnvFilter::builder()
        .with_default_directive(cli.verbose.tracing_level_filter().into())
        .from_env_lossy();
    tracing_subscriber::FmtSubscriber::builder().with_env_filter(filter).init();

    tracing::debug!(ver = env!("CARGO_PKG_VERSION"), "きりたん");
    createrepo::run(cli)
}
