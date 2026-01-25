// src/lib.rs
//! Polyfill-rs: High-performance Rust client for Polymarket
//!
//! # Features
//!
//! - **High-performance order book management** with optimized data structures
//! - **Real-time market data streaming** with WebSocket support
//! - **Trade execution simulation** with slippage protection
//! - **Detailed error handling** with specific error types
//! - **Rate limiting and retry logic** for robust API interactions
//! - **Ethereum integration** with EIP-712 signing support
//! - **Benchmarking tools** for performance analysis
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use polyfill_rs::{ClobClient, OrderArgs, Side};
//! use rust_decimal::Decimal;
//! use std::str::FromStr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create client (compatible with polymarket-rs-client)
//!     let mut client = ClobClient::with_l1_headers(
//!         "https://clob.polymarket.com",
//!         "your_private_key",
//!         137,
//!     );
//!
//!     // Get API credentials
//!     let api_creds = client.create_or_derive_api_key(None).await.unwrap();
//!     client.set_api_creds(api_creds);
//!
//!     // Create and post order
//!     let order_args = OrderArgs::new(
//!         "token_id",
//!         Decimal::from_str("0.75").unwrap(),
//!         Decimal::from_str("100.0").unwrap(),
//!         Side::BUY,
//!     );
//!
//!     let result = client.create_and_post_order(&order_args).await.unwrap();
//!     println!("Order posted: {:?}", result);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Advanced Usage
//!
//! ```rust,no_run
//! use polyfill_rs::{ClobClient, OrderBookImpl};
//! use rust_decimal::Decimal;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a basic client
//!     let client = ClobClient::new("https://clob.polymarket.com");
//!
//!     // Get market data
//!     let markets = client.get_sampling_markets(None).await.unwrap();
//!     println!("Found {} markets", markets.data.len());
//!
//!     // Create an order book for high-performance operations
//!     let mut book = OrderBookImpl::new("token_id".to_string(), 100); // 100 levels depth
//!     println!("Order book created for token: {}", book.token_id);
//!
//!     Ok(())
//! }
//! ```

use tracing::info;

// Global constants
pub const DEFAULT_CHAIN_ID: u64 = 137; // Polygon
pub const DEFAULT_BASE_URL: &str = "https://clob.polymarket.com";
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const DEFAULT_RATE_LIMIT_RPS: u32 = 100;

// Initialize logging
pub fn init() {
    tracing_subscriber::fmt::init();
    info!("Polyfill-rs initialized");
}

// Re-export main types
pub use crate::types::{
    ApiCredentials,
    // Additional compatibility types
    ApiKeysResponse,
    AssetType,
    Balance,
    BalanceAllowance,
    BalanceAllowanceParams,
    BatchMidpointRequest,
    BatchMidpointResponse,
    BatchPriceRequest,
    BatchPriceResponse,
    BookParams,
    ClientConfig,
    ClientResult,
    FillEvent,
    Market,
    MarketSnapshot,
    MarketsResponse,
    MidpointResponse,
    NegRiskResponse,
    NotificationParams,
    OpenOrder,
    OpenOrderParams,
    Order,
    OrderBook,
    OrderBookSummary,
    OrderDelta,
    OrderRequest,
    OrderStatus,
    OrderSummary,
    OrderType,
    PriceResponse,
    Rewards,
    Side,
    SimplifiedMarket,
    SimplifiedMarketsResponse,
    SpreadResponse,
    StreamMessage,
    TickSizeResponse,
    Token,
    TokenPrice,
    TradeParams,
    WssAuth,
    WssChannelType,
    WssSubscription,
};

pub use crate::book::{OrderBook as OrderBookImpl, OrderBookManager};
pub use crate::client::OrderArgs;
pub use crate::client::{ClobClient, PolyfillClient};
pub use crate::decode::Decoder;
pub use crate::errors::{PolyfillError, Result};
pub use crate::fill::{FillEngine, FillResult};
pub use crate::state::GlobalState;
pub use crate::stream::{MarketStream, StreamManager, WebSocketStream};
pub use crate::utils::{crypto, math, rate_limit, retry, time, url};

pub mod auth;
pub mod book;
pub mod bot_error;
pub mod buffer_pool;
pub mod client;
pub mod common;
pub mod config;
pub mod connection_manager;
pub mod data_client;
pub mod data_engine;
pub mod decode;
pub mod dns_cache;
pub mod errors;
pub mod execute_egine;
pub mod fill;
pub mod http_config;
pub mod orders;
pub mod rand;
pub mod state;
pub mod stream;
pub mod types;
pub mod utils;

pub mod prelude {
    pub use crate::book::{BookAnalytics, OrderBook, OrderBookManager};
    pub use crate::common::{
        CRYPTO_PATTERNS, EVENT_URL, MARKET_URL, Market, Token, WEBSOCKET_MARKET_URL,
    };
    pub use crate::stream::{MockStream, WebSocketStream};
    pub use crate::types::{StreamMessage, WssAuth, WssChannelType};
}
