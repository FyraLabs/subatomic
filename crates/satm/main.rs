#![warn(rust_2018_idioms)]

use clap::Parser;
use color_eyre::eyre::bail;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod api_client;
mod cli;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    _ = dotenvy::dotenv();

    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();
    color_eyre::install().expect("cannot install color_eyre");

    tracing::debug!(ver = env!("CARGO_PKG_VERSION"), "satm");
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Repo(cmd) => {
            let Some((url, token)) = (cmd.url.or_else(|| std::env::var("SUBATOMIC_API_URL").ok()))
                .zip(cmd.token.or_else(|| std::env::var("SUBATOMIC_API_TOKEN").ok()))
            else {
                bail!("Supply SUBATOMIC_API_URL and one of SUBATOMIC_API_TOKEN or --token");
            };

            let client = api_client::ApiClient::new(&url, token);
            handle_api_repos(cmd.subcmd, client).await?;
            Ok(())
        }
        cli::Command::Key(cmd) => {
            let Some((url, token)) = (cmd.url.or_else(|| std::env::var("SUBATOMIC_API_URL").ok()))
                .zip(cmd.token.or_else(|| std::env::var("SUBATOMIC_API_TOKEN").ok()))
            else {
                bail!("Supply SUBATOMIC_API_URL and one of SUBATOMIC_API_TOKEN or --token");
            };

            let client = api_client::ApiClient::new(&url, token);
            handle_api_keys(cmd.subcmd, client).await?;
            Ok(())
        }
    }
}

async fn handle_api_repos(
    subcmd: cli::RepoSubcommand,
    client: api_client::ApiClient,
) -> color_eyre::Result<()> {
    match subcmd {
        cli::RepoSubcommand::List => {
            let repos = client.list_repos().await?;
            println!("{}", serde_json::to_string_pretty(&repos)?);
        }
        cli::RepoSubcommand::Create { name } => {
            let repo = client.create_repo(&name).await?;
            println!("{}", serde_json::to_string_pretty(&repo)?);
        }
        cli::RepoSubcommand::Upload { name, paths } => {
            client.upload_pkgs(&name, &paths).await?;
        }
        cli::RepoSubcommand::Delete { name } => client.delete_repo(&name).await?,
        cli::RepoSubcommand::Comps { action } => match action {
            cli::CompsAction::Upload { name, file } => client.upload_comps(&name, &file).await?,
            cli::CompsAction::Delete { name } => client.delete_comps(&name).await?,
        },
        cli::RepoSubcommand::RepoKey { action } => match action {
            cli::RepoKeyAction::Get { name } => {
                let public_key = client.get_repo_key(&name).await?;
                println!("{public_key}");
            }
            cli::RepoKeyAction::Set { name, id } => client.set_repo_key(&name, id).await?,
            cli::RepoKeyAction::Delete { name } => client.del_repo_key(&name).await?,
        },
        cli::RepoSubcommand::Pkg { action } => match action {
            cli::PkgAction::List { name } => {
                for rpm in client.list_rpms(&name).await? {
                    println!("{rpm}");
                }
            }
            cli::PkgAction::Delete { name, rpms } => {
                let not_found = client.delete_rpms(&name, &rpms).await?;
                for f in not_found {
                    tracing::error!("not found: {f}");
                }
            }
        },
        cli::RepoSubcommand::Refresh { name } => client.refresh_repo(&name).await?,
    }
    Ok(())
}

async fn handle_api_keys(
    subcmd: cli::KeySubcommand,
    client: api_client::ApiClient,
) -> color_eyre::Result<()> {
    match subcmd {
        cli::KeySubcommand::List => {
            let keys = client.list_keys().await?;
            println!("{}", serde_json::to_string_pretty(&keys)?);
        }
        cli::KeySubcommand::Get { id } => {
            let public_key = client.get_key(id).await?;
            println!("{public_key}");
        }
        cli::KeySubcommand::Create { name, userid } => {
            let key = client.create_key(&name, &userid).await?;
            println!("{}\n{}", key.id, key.public_armor);
        }
        cli::KeySubcommand::Delete { id } => client.del_key(id).await?,
    }
    Ok(())
}
