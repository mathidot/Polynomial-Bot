use crate::Result;
use crate::client::ClobClient;
use crate::config::{self, Config, Strategy, TailEaterStrategy};
use crate::types::TokenInfo;
use crate::{FillEngine, GlobalState, OrderRequest};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::mpsc;
pub struct ExecuteEgine {
    notify_channel: mpsc::Receiver<TokenInfo>,
    config: Strategy,
    client: Arc<ClobClient>,
    fill_egine: FillEngine,
    global_state: Arc<GlobalState>,
}

impl ExecuteEgine {
    pub async fn tick(&mut self) {
        while let Some(token_info) = self.notify_channel.recv().await {
            let current_price = token_info.price.to_f64().unwrap_or(0.0);
            if current_price >= self.config.tail_eater.buy_threshold {
                if let Ok(book) = self.global_state.get_book(&token_info.token_id) {
                    let order_request = OrderRequest {
                        token_id: token_info.token_id,
                        side: token_info.side,
                        price: token_info.price,
                        size: dec!(0),
                        order_type: crate::OrderType::GTC,
                        expiration: None,
                        client_id: Some("tail_eater_limit_order".to_string()),
                    };

                    if let Ok(limit_result) =
                        self.fill_egine.execute_limit_order(&order_request, &book)
                    {
                        println!("Limit order execution result:");
                        println!("  Status: {:?}", limit_result.status);
                        println!("  Total size: {}", limit_result.total_size);
                        println!("  Average price: {}", limit_result.average_price);
                    }
                }
            }
        }
    }
}
