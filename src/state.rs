use crate::book::OrderBook;
use crate::{BookSnapshot, BookWithSequence, OrderBookManager, OrderDelta};
use crate::{PolyfillClient, PolyfillError, Result};
use alloy_sol_types::abi::token;
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashSet;
use std::sync::RwLock;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub struct PriceInfo {
    best_ask_price: AtomicU64,
    best_bid_price: AtomicU64,
}
pub struct GlobalState {
    tokens: DashMap<String, PriceInfo>,
    book_manager: Arc<OrderBookManager>,
    hold_tokens: Arc<RwLock<HashSet<String>>>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            tokens: DashMap::new(),
            book_manager: Arc::new(OrderBookManager::new(100)),
            hold_tokens: Arc::new(RwLock::new(HashSet::new())),
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

    pub fn insert_order_book(&self, book: OrderBook) -> Result<()> {
        self.book_manager.insert(book)?;
        Ok(())
    }

    pub fn has_order_book(&self, token_id: &str) -> Result<bool> {
        self.book_manager.is_exist(token_id)
    }

    pub fn get_book(&self, token_id: &str) -> Result<crate::book::OrderBook> {
        self.book_manager.get_book(token_id)
    }

    pub fn get_price(&self, token_id: &str) -> Result<Option<Decimal>> {
        let book = self.book_manager.get_book(token_id)?;
        Ok(book.mid_price())
    }

    pub fn update_order_book(&self, book: BookWithSequence) -> Result<()> {
        let token_id = book.token_id.clone();
        let mut order_book = OrderBook::new(token_id.clone(), 100);
        order_book.set_tick_size(dec!(0.001))?;
        let book_snapshot = BookSnapshot {
            asset_id: book.token_id,
            timestamp: book.timestamp,
            asks: book.asks,
            bids: book.bids,
            sequence: book.sequence,
        };

        order_book
            .apply_book_snapshot(book_snapshot)
            .inspect_err(|e| tracing::error!("apply_book_snapshot failed: {}", e))?;

        self.insert_order_book(order_book)
    }

    pub fn apply_delta(&self, delta: OrderDelta) -> Result<()> {
        self.book_manager.apply_delta(delta)?;
        Ok(())
    }

    pub fn is_hold(&self, token_id: &str) -> Result<bool> {
        let hold_tokens = self
            .hold_tokens
            .try_read()
            .map_err(|_| PolyfillError::Internal {
                message: "fail to unlock Rwlock".to_string(),
                source: None,
            })?;

        Ok(hold_tokens.contains(token_id))
    }

    pub fn hold(&self, token_id: &str) -> Result<()> {
        let mut hold_tokens =
            self.hold_tokens
                .try_write()
                .map_err(|_| PolyfillError::Internal {
                    message: "fail to unlock Rwlock".to_string(),
                    source: None,
                })?;

        hold_tokens.insert(token_id.to_string());
        Ok(())
    }
}
