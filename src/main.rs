#![warn(rust_2018_idioms)]
#![feature(try_blocks_heterogeneous)]

pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod repohdl;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::Config;
use crate::db::create_pool;
use axum::Router;
use axum::extract::{DefaultBodyLimit, FromRef, State};
use axum::routing::{delete, get, post, put};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    pool: Arc<sqlx::Pool<sqlx::Postgres>>,
    locker: Arc<crate::repohdl::Locker>,
}
impl FromRef<AppState> for Arc<Config> {
    fn from_ref(input: &AppState) -> Self {
        Self::clone(&input.config)
    }
}
impl FromRef<AppState> for Arc<sqlx::Pool<sqlx::Postgres>> {
    fn from_ref(input: &AppState) -> Self {
        Self::clone(&input.pool)
    }
}
impl FromRef<AppState> for Arc<crate::repohdl::Locker> {
    fn from_ref(input: &AppState) -> Self {
        Self::clone(&input.locker)
    }
}

pub type DbState = State<Arc<sqlx::Pool<sqlx::Postgres>>>;
pub type CfgState = State<Arc<Config>>;
pub type LockerState = State<Arc<repohdl::Locker>>;

fn main() {
    Registry::default().with(EnvFilter::from_default_env()).with(tracing_logfmt::layer()).init();

    tracing::info!("starting subatomic");
    let term = register_termsigs().expect("cannot register termsigs");
    mainloop(&term);
}

fn mainloop(term: &Arc<AtomicBool>) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("cannot build tokio");
    let handle = runtime.spawn(inner_main());

    loop {
        if handle.is_finished() {
            tracing::info!("main finished");
            _ = runtime.block_on(handle).inspect_err(|e| tracing::error!(?e, "cannot join main"));
            return;
        }
        if term.load(Ordering::Relaxed) {
            handle.abort();
            tracing::warn!("Exit due to termination signal");
            _ = runtime.block_on(handle).inspect_err(|e| tracing::error!(?e, "cannot join main"));
            return;
        }
    }
}

async fn inner_main() {
    let config = Config::from_env().expect("cannot obtain config from env");
    let config = Arc::new(config);

    let pool = create_pool(&config.database_url).await.expect("cannot create pool");
    let pool = Arc::new(pool);

    let locker = Arc::new(repohdl::Locker::new(Arc::clone(&pool), Arc::clone(&config)));

    let app = Router::new()
        .route("/v1/repos", get(api::repos::list_repos))
        .route("/v1/repos/{name}", put(api::repos::create_repo))
        .route("/v1/repos/{name}", delete(api::repos::delete_repo))
        .route("/v1/repos/{name}", post(api::repos::upload_pkgs))
        .route("/v1/repos/{name}/comps", put(api::repos::push_comps))
        .route("/v1/repos/{name}/comps", delete(api::repos::del_comps))
        .route("/v1/repos/{name}/key", get(api::repos::get_key))
        .route("/v1/repos/{name}/key", put(api::repos::set_key))
        .route("/v1/repos/{name}/key", delete(api::repos::del_key))
        .route("/v1/repos/{name}/rpms", get(api::repos::list_rpms))
        .route("/v1/repos/{name}/rpms", post(api::repos::del_rpms))
        .route("/v1/repos/{name}/refresh", post(api::repos::refresh_repo))
        .route("/v1/repos/{name}/rebuild", post(api::repos::rebuild_repo))
        .route("/v1/keys", post(api::keys::create_key))
        .route("/v1/keys", get(api::keys::list_keys))
        .route("/v1/keys/{id}", get(api::keys::get_key))
        .route("/v1/keys/{id}", delete(api::keys::del_key))
        .route_layer(axum::middleware::from_fn_with_state(Arc::clone(&config), auth::jwt_auth))
        .with_state(AppState { config: Arc::clone(&config), pool, locker })
        .layer(DefaultBodyLimit::max(config.body_limit));

    let addr = format!("{}:{}", config.server_host, config.server_port);
    tracing::info!(addr, "starting server");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Register termination signals to watch
///
/// We need to tidy up before exiting. Watching these signals can give us a chance to actually
/// clean things up.
///
/// # Errors
/// Return errors when your computer becomes a potato.
pub fn register_termsigs() -> std::io::Result<std::sync::Arc<std::sync::atomic::AtomicBool>> {
    let term = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for sig in signal_hook::consts::TERM_SIGNALS {
        signal_hook::flag::register_conditional_shutdown(*sig, 1, std::sync::Arc::clone(&term))?;
        signal_hook::flag::register(*sig, std::sync::Arc::clone(&term))?;
    }
    Ok(term)
}
