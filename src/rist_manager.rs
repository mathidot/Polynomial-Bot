use crate::GlobalState;
use crate::Result;
use anyhow::Ok;
use rust_decimal::Decimal;
use std::sync::Arc;

pub struct RistManager {
    pub amount: Decimal,
    global_state: Arc<GlobalState>,
}

impl RistManager {
    pub fn new(amount: Decimal, global_state: Arc<GlobalState>) -> Self {
        Self {
            amount,
            global_state,
        }
    }

    pub fn get_amount(&self, token_id: &str) -> Result<Decimal> {
        let book = self.global_state.get_book(token_id)?;
        let bids = book.bids_fast(None);
        bids.iter().rev()
    }
}
