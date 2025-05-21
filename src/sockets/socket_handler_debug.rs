use crate::models::AppState;
use crate::models::ClientMsg;
use crate::pty::PtyManager;
use axum::{
    extract::{
        Extension,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};

use bytes::Bytes;
use std::sync::Arc;
use tokio::select;
use tokio::time::{self, Duration};

pub async fn ws_handler_debug(ws: WebSocketUpgrade, Extension(state): Extension<Arc<AppState>>) -> impl IntoResponse {
    let pty_dbg = Arc::clone(&state.pty_dbg);
    ws.on_upgrade(move |socket| debug_session(socket, pty_dbg))
}

async fn debug_session(mut socket: WebSocket, pty: Arc<PtyManager>) {
    let (mut rx, history) = pty.subscribe().await;
    let _ = socket.send(Message::Binary(Bytes::from(history))).await;

    let mut buf = Vec::new();
    let mut tick = time::interval(Duration::from_millis(20));

    loop {
        select! {
            Ok(bytes) = rx.recv() => buf.extend_from_slice(&bytes),

            _ = tick.tick() => {
                if !buf.is_empty() {
                    let _ = socket
                        .send(Message::Binary(Bytes::from(std::mem::take(&mut buf))))
                        .await;
                }
            }

            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(txt))) => {
                    if let Ok(cmd) = serde_json::from_str::<ClientMsg>(&txt) {
                        apply_cmd(cmd, &pty).await;
                    }
                }
                Some(Ok(Message::Binary(bin))) => {
                    if let Ok(cmd) = serde_json::from_slice::<ClientMsg>(&bin) {
                        apply_cmd(cmd, &pty).await;
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

async fn apply_cmd(cmd: ClientMsg, pty: &PtyManager) {
    match cmd {
        ClientMsg::Data { value } => {
            let _ = pty.write(value.as_bytes()).await;
        }
        ClientMsg::Resize { value } => {
            let _ = pty.resize(value.rows, value.cols).await;
        }
        ClientMsg::Heartbeat => {}
    }
}
