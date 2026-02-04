use crate::GlobalState;
use crate::Result;
use rust_decimal::Decimal;
use std::sync::Arc;

pub struct RistManager {
    global_state: Arc<GlobalState>,
    trade_unit: Decimal,
}

impl RistManager {
    pub fn new(trade_unit: Decimal, global_state: Arc<GlobalState>) -> Self {
        Self {
            global_state,
            trade_unit,
        }
    }

    pub fn get_amount(&self, token_id: &str) -> Result<Decimal> {
        let book = self.global_state.get_book(token_id)?;
        let bids = book.bids_fast(None);
        Ok(self.trade_unit)
    }
}
