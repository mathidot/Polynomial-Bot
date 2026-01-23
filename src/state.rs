use crate::common::Result;
use crate::types::Side;
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct PriceInfo {
    best_ask_price: AtomicU64,
    best_bid_price: AtomicU64,
}
pub struct State {
    tokens: DashMap<String, PriceInfo>,
}

impl State {
    pub fn new() -> Self {
        Self {
            tokens: DashMap::new(),
        }
    }

    pub fn insert(&self, token_id: String, info: PriceInfo) {
        self.tokens.insert(token_id, info);
    }

    pub fn update_ask_price(&self, token_id: String, best_ask_price: Decimal) {
        if let Some(info) = self.tokens.get_mut(&token_id) {
            let price_u64 = best_ask_price.mantissa() as u64;
            info.best_ask_price.store(price_u64, Ordering::Relaxed);
        }
    }

    pub fn update_bid_price(&self, token_id: String, best_bid_price: Decimal) {
        if let Some(info) = self.tokens.get_mut(&token_id) {
            let price_u64 = best_bid_price.mantissa() as u64;
            info.best_bid_price.store(price_u64, Ordering::Relaxed);
        }
    }
}
