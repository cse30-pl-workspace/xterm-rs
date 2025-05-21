// run  := cargo run -- --resource /home/jyh/project/xterm-rs/static
// dir  := .
// kid  :=

use std::sync::Arc;

use anyhow::Context;
use axum::{Extension, Router, routing::get};
use tower_http::services::ServeDir;

mod index;
mod models;
mod sockets;
mod pty;

use index::index;

use pty::PtyManager;
use models::{AppState, Logger};
use sockets::{ws_handler, ws_handler_debug};

use clap::{Parser, ValueHint};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "/bin/bash")]
    command: String,
    #[arg(long, default_value = "24")]
    rows: u16,
    #[arg(long, default_value = "80")]
    cols: u16,
    #[arg(long, value_hint=ValueHint::DirPath)]
    resource: Option<std::path::PathBuf>,
    #[arg(short, long, default_value = "8080")]
    port: usize,
    #[arg(long, default_value = "1000")]
    scrollback: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let pty = Arc::new(PtyManager::new(args.rows, args.cols).await?);
    let pty_dbg = Arc::new(PtyManager::new(args.rows, args.cols).await?);

    let static_path = args
        .resource
        .unwrap_or(std::env::current_exe()?.parent().context("path error")?.join("static"));

    let state = Arc::new(AppState {
        start: std::time::Instant::now(),
        pty: Arc::clone(&pty),
        pty_dbg: Arc::clone(&pty_dbg),
        logger: Logger::new("test.cast", "heartbeat.log"),
        size: Arc::new(tokio::sync::RwLock::new((args.rows, args.cols))),
        scrollback: args.scrollback,
    });

    let app = Router::new()
        .nest_service("/static", ServeDir::new(static_path))
        .route("/ws", get(ws_handler))
        .route("/", get(index))
        .route("/debug", get(index))
        .route("/debug/ws", get(ws_handler_debug))
        .layer(Extension(state));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;

    axum::serve(listener, app).await.context("server error")
}
