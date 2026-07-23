#![warn(rust_2018_idioms)]

use axum::Router;
use axum::routing::get;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry};

#[tokio::main]
async fn main() {
    Registry::default().with(EnvFilter::from_default_env()).with(tracing_logfmt::layer()).init();
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
