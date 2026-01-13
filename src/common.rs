use rust_decimal::Decimal;
use serde::{ Deserialize, Serialize };
use serde_with::{ json::JsonString, serde_as };

pub const EVENT_URL: &str = "https://gamma-api.polymarket.com/events";
pub const SLUG_URL: &str = "https://gamma-api.polymarket.com/events/slug";
pub const MARKET_URL: &str = "https://gamma-api.polymarket.com/markets";
pub const SPORT_URL: &str = "https://gamma-api.polymarket.com/sports";
pub const WEBSOCKET_MARKET_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

pub static CRYPTO_PATTERNS: &[&str] = &[
    "bitcoin-above",
    "ethereum-above",
    "bitcoin-up-or-down",
    "xrp-up-or-down",
    "ethereum-price",
    "xrp-price",
    "xrp-above",
    "solana-above",
    "ethereum-up-or-down",
];

pub type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketType {
    POLYMARKET,
    KALSHI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TokenType {
    #[default]
    CRYPTO,
    SPORTS,
}

#[derive(Debug, Clone, Hash)]
pub enum Outcome {
    YES,
    NO,
}
// Market information
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    pub question: String,
    pub condition_id: String,
    #[serde(rename = "clobTokenIds")]
    #[serde_as(as = "JsonString")]
    pub tokens: Vec<String>,
    pub slug: String,
    pub liquidity: Decimal,
    pub start_date: String,
    #[serde(default)]
    pub description: String,
    #[serde_as(as = "JsonString")]
    pub outcomes: Vec<String>,
    #[serde_as(as = "JsonString")]
    pub outcome_prices: Vec<Decimal>,
    pub volume: Decimal,
    pub active: bool,
    pub closed: bool,
    #[serde(default)]
    pub spread: Decimal,
    #[serde(default)]
    pub last_trade_price: Decimal,
    #[serde(default)]
    pub best_bid: Decimal,
    pub best_ask: Decimal,
    #[serde(default)]
    #[serde(rename = "questionID")]
    pub question_id: String,
    #[serde(default)]
    pub new: bool,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub enable_order_book: bool,
    #[serde(default)]
    pub order_price_min_tick_size: Decimal,
    #[serde(default)]
    pub order_min_size: Decimal,
    #[serde(default)]
    pub volume_num: Decimal,
    #[serde(default)]
    pub liquidity_num: Decimal,
    #[serde(default)]
    pub end_date_iso: String,
    #[serde(default)]
    pub start_date_iso: String,
    #[serde(default)]
    pub neg_risk: bool,
}

impl Default for Token {
    fn default() -> Self {
        Self {
            token_type: TokenType::CRYPTO,
            token_id: String::new(),
            outcome: String::new(),
            winner: false,
            is_valid: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Token {
    pub token_type: TokenType,
    pub token_id: String,
    pub outcome: String,
    #[serde(default)]
    pub winner: bool,
    pub is_valid: bool,
}
