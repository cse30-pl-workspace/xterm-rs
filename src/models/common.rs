use crate::models::Logger;
use crate::pty::PtyManager;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use memchr::memrchr;
use serde::Deserialize;
use std::{sync::Arc, time::Instant};
use tokio::sync::RwLock;
use unicode_width::UnicodeWidthChar;

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum ClientMsg {
    Data { value: String },
    Resize { value: Size },
    Heartbeat,
}

#[derive(Deserialize, Debug)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

pub struct AppState {
    pub start: Instant,
    pub pty: Arc<PtyManager>,
    pub pty_dbg: Arc<PtyManager>,
    pub logger: Arc<Logger>,
    pub size: Arc<RwLock<(u16, u16)>>,
    pub scrollback: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("bad request: {0}")]
    BadRequest(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
        }
    }
}

pub fn scrollback_lines(buf: &[u8], rows: u16, cols: u16, scrollback: u32) -> usize {
    let max_lines = rows as u32 + scrollback + 50; // rows + scrollback + 50?

    let mut line_count = 0;
    let mut col = 0;
    let mut i = buf.len();

    while i > 0 && line_count < max_lines {
        i -= 1;
        let b = buf[i];

        match b {
            b'\n' => {
                line_count += 1;
                col = 0;
            }
            0x20..=0x7e => {
                col += 1;
                if col == cols {
                    line_count += 1;
                    col = 0;
                }
            }
            0x80..=0xff => {
                let (ch, size) = {
                    let mut start = i;
                    while start > 0 && (buf[start] & 0b1100_0000) == 0b1000_0000 {
                        start -= 1;
                    }
                    let s = std::str::from_utf8(&buf[start..=i]).unwrap_or(" ");
                    (s.chars().next().unwrap(), i - start + 1)
                };
                col += UnicodeWidthChar::width(ch).unwrap_or(1) as u16;
                if col >= cols {
                    line_count += 1;
                    col = if col == cols { 0 } else { col - cols };
                }
                i -= size - 1;
            }
            0x1b => {
                if let Some(lbrk) = memrchr(b'm', &buf[..=i]) {
                    i = lbrk.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
    i
}
