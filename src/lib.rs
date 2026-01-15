// src/lib.rs
pub mod book;
pub mod bot_error;
pub mod common;
pub mod context;
pub mod data_engine;
pub mod errors;
pub mod stream;
pub mod types;
pub mod utils;

pub mod prelude {
    pub use crate::book::{BookAnalytics, OrderBook, OrderBookManager};
    pub use crate::common::{
        CRYPTO_PATTERNS, EVENT_URL, MARKET_URL, Market, Result, Token, WEBSOCKET_MARKET_URL,
    };
    pub use crate::context::BotContext;
    pub use crate::stream::{MockStream, WebSocketStream};
    pub use crate::types::{StreamMessage, WssAuth, WssChannelType};
}
