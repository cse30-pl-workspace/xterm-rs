pub mod common;
pub use common::{AppError, AppState, ClientMsg, scrollback_lines};

pub mod logger;
pub use logger::Logger;
