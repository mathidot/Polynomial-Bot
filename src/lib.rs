// src/lib.rs
pub mod common;
pub mod context;
pub mod fetcher;
pub mod bot_error;

pub mod prelude {
    pub use crate::common::EVENT_URL;
    pub use crate::common::MARKET_URL;
    pub use crate::common::Result;
    pub use crate::context::BotContext;
}
