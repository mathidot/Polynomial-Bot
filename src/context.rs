use std::sync::Arc;
use polyfill_rs::{ ClobClient };
use dashmap::DashMap;
use crate::common::MarketType;

#[derive(Clone)]
pub struct BotContext {
    pub inner: Arc<BotContextInnner>,
}

impl BotContext {
    pub fn new() -> Self {
        Self { inner: Arc::new(BotContextInnner::new()) }
    }
}

pub struct BotContextInnner {
    pub client: ClobClient,
}

impl BotContextInnner {
    fn new() -> Self {
        Self {
            client: ClobClient::new("https://clob.polymarket.com"),
        }
    }
}
