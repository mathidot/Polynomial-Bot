use crate::OrderBookManager;
use crate::book::OrderBook;
use crate::{PolyfillError, Result};
use dashmap::DashMap;
use rust_decimal::Decimal;
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
}
