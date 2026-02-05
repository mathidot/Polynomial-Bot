use crate::book::OrderBook;
use crate::types::PriceChange;
use crate::{BookMessage, BookSnapshot, BookWithSequence, OrderBookManager, OrderDelta};
use crate::{PolyfillError, Result};
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

pub struct PriceInfo {
    best_ask_price: AtomicU64,
    best_bid_price: AtomicU64,
}
pub struct GlobalState {
    tokens: DashMap<String, PriceInfo>,
    book_manager: Arc<RwLock<OrderBookManager>>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            tokens: DashMap::new(),
            book_manager: Arc::new(RwLock::new(OrderBookManager::new(100))),
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
        let mut books = self.book_manager.write().map_err(|_| {
            PolyfillError::internal_simple("fail to accquire BookManager write lock")
        })?;
        books.insert(book)?;
        Ok(())
    }

    pub fn has_order_book(&self, token_id: &str) -> Result<bool> {
        let books = self
            .book_manager
            .read()
            .map_err(|_| PolyfillError::internal_simple("fail to acquire BookManager read lock"))?;
        books.is_exist(token_id)
    }

    pub fn get_book(&self, token_id: &str) -> Result<crate::book::OrderBook> {
        let books = self
            .book_manager
            .read()
            .map_err(|_| PolyfillError::internal_simple("fail to acquire BookManager read lock"))?;
        books.get_book(&token_id)
    }

    pub fn get_price(&self, token_id: &str) -> Result<Option<Decimal>> {
        let books = self
            .book_manager
            .read()
            .map_err(|_| PolyfillError::internal_simple("fail to acquire BookManager read lock"))?;
        let book = books.get_book(token_id)?;
        Ok(book.mid_price())
    }

    pub fn update_order_book(&mut self, book: BookWithSequence) -> Result<()> {
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

    pub fn apply_delta(&mut self, delta: OrderDelta) -> Result<()> {
        let books = self.book_manager.write().map_err(|_| {
            PolyfillError::internal_simple("fail to accquire BookManager write lock")
        })?;
        books.apply_delta(delta)?;
        Ok(())
    }
}
