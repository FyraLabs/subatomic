#![warn(rust_2018_idioms)]
#![feature(try_blocks_heterogeneous)]

pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod repohdl;

use std::sync::Arc;

use crate::config::Config;
use crate::db::create_pool;
use axum::Router;
use axum::extract::{FromRef, State};
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
        Arc::clone(&input.config)
    }
}
impl FromRef<AppState> for Arc<sqlx::Pool<sqlx::Postgres>> {
    fn from_ref(input: &AppState) -> Self {
        Arc::clone(&input.pool)
    }
}
impl FromRef<AppState> for Arc<crate::repohdl::Locker> {
    fn from_ref(input: &AppState) -> Self {
        Arc::clone(&input.locker)
    }
}

pub type DbState = State<Arc<sqlx::Pool<sqlx::Postgres>>>;
pub type CfgState = State<Arc<Config>>;
pub type LockerState = State<Arc<repohdl::Locker>>;

#[tokio::main]
async fn main() {
    Registry::default().with(EnvFilter::from_default_env()).with(tracing_logfmt::layer()).init();

    let config = Config::from_env().expect("cannot obtain config from env");
    let config = Arc::new(config);

    let pool = create_pool(&config.database_url).await.expect("cannot create pool");
    let pool = Arc::new(pool);

    let locker = Arc::new(repohdl::Locker::new(Arc::clone(&pool), Arc::clone(&config)));

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    let app = Router::new()
        .route("/v1/repos", get(api::repos::list_repos))
        .route("/v1/repos", post(api::repos::create_repo))
        .route("/v1/repos/:name", put(api::repos::upload_pkgs))
        .route("/v1/repos/:name", delete(api::repos::delete_repo))
        .route("/v1/repos/:name/comps", put(api::repos::push_comps))
        .route("/v1/repos/:name/comps", delete(api::repos::del_comps))
        .route("/v1/repos/:name/key", get(api::repos::get_key))
        .route("/v1/repos/:name/key", put(api::repos::set_key))
        .route("/v1/repos/:name/key", delete(api::repos::del_key))
        // .route("/v1/repos/:name/resign", post(api::repos::resign))
        .route("/v1/repos/:name/rpms", get(api::repos::list_rpms))
        .route("/v1/repos/:name/rpms", post(api::repos::del_rpms))
        .route("/v1/repos/:name/refresh", post(api::repos::refresh_repo))
        .route_layer(axum::middleware::from_fn_with_state(config.clone(), auth::jwt_auth))
        .with_state(AppState { config: Arc::clone(&config), pool, locker });

    let addr = format!("{}:{}", config.server_host, config.server_port);
    tracing::info!(addr, "Starting server");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
