use crate::models::AppState;
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

use crate::models::{ClientMsg, scrollback_lines};

pub async fn ws_handler(ws: WebSocketUpgrade, Extension(state): Extension<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client_session(socket, state))
}

async fn flush(buf: &mut Vec<u8>, state: &AppState, socket: &mut WebSocket) -> anyhow::Result<()> {
    if buf.is_empty() {
        return Ok(());
    }

    let data = std::mem::take(buf);

    let (rows, cols) = *state.size.read().await;
    let start = scrollback_lines(&data, rows, cols, state.scrollback);
    let data = &data[start..].to_vec();

    state.logger.output(state.start.elapsed().as_secs_f32(), data.clone());

    socket.send(Message::Binary(Bytes::copy_from_slice(data))).await?;
    Ok(())
}

async fn client_session(mut socket: WebSocket, state: Arc<AppState>) {
    let (mut rx, history) = state.pty.subscribe().await;
    if socket.send(Message::Binary(Bytes::from(history))).await.is_err() {
        return;
    }

    let mut buf = Vec::new();
    let mut interval = time::interval(Duration::from_millis(10));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    loop {
        select! {
            Ok(bytes) = rx.recv() => {
                buf.extend_from_slice(&bytes);
            }

            _ = interval.tick() => {
                if !buf.is_empty() {
                    flush(&mut buf, &state, &mut socket).await.ok();
                }
            }


            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(txt))) => {
                        if let Ok(cmd) = serde_json::from_str::<ClientMsg>(&txt) {
                            if handle(cmd, &state, &mut socket).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(bin))) => {
                        if let Ok(cmd) = serde_json::from_slice::<ClientMsg>(&bin) {
                            if handle(cmd, &state, &mut socket).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

async fn handle(msg: ClientMsg, state: &AppState, sock: &mut WebSocket) -> anyhow::Result<()> {
    match msg {
        ClientMsg::Data { value } => {
            state
                .logger
                .input(state.start.elapsed().as_secs_f32(), value.as_bytes().to_vec());
            state.pty.write(value.as_bytes()).await?;
        }
        ClientMsg::Resize { value } => {
            state
                .logger
                .resize(state.start.elapsed().as_secs_f32(), value.rows, value.cols);
            state.pty.resize(value.rows, value.cols).await?;
            let mut sz = state.size.write().await;
            *sz = (value.rows, value.cols);
        }
        ClientMsg::Heartbeat => {
            state.logger.heartbeat();
            sock.send(Message::Text(r#"{"event":"heartbeat-pong"}"#.into())).await?;
        }
    }
    Ok(())
}
