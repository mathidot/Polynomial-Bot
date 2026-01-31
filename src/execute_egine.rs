use crate::OrderArgs;
use crate::client::ClobClient;
use crate::config::StrategyConfig;
use crate::fill::FillStatus;
use crate::types::TokenInfo;
use crate::{FillEngine, GlobalState, OrderRequest};
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;
pub struct ExecuteEgine {
    client: ClobClient,
    token_rx: mpsc::UnboundedReceiver<TokenInfo>,
    config: StrategyConfig,
    fill_egine: FillEngine,
    global_state: Arc<GlobalState>,
}

impl ExecuteEgine {
    pub fn new(
        client: ClobClient,
        rx: mpsc::UnboundedReceiver<TokenInfo>,
        cfg: StrategyConfig,
        fill: FillEngine,
        state: Arc<GlobalState>,
    ) -> Self {
        Self {
            client,
            token_rx: rx,
            config: cfg,
            fill_egine: fill,
            global_state: state,
        }
    }

    async fn on_tick(&mut self) {
        while let Some(token_info) = self.token_rx.recv().await {
            info!("exec engine recv token info: {:?}", token_info);
            let token_id = token_info.token_id;
            let current_price = token_info.price;

            if current_price < self.config.tail_eater.buy_threshold {
                continue;
            }

            let Ok(book) = self.global_state.get_book(&token_id) else {
                continue;
            };

            let order_request = OrderRequest {
                token_id: token_id.clone(),
                side: crate::Side::BUY,
                price: token_info.price,
                size: dec!(0),
                order_type: crate::OrderType::GTC,
                expiration: None,
                client_id: Some("tail_eater_limit_order".to_string()),
            };

            let Ok(limit_result) = self.fill_egine.execute_limit_order(&order_request, &book)
            else {
                continue;
            };

            match limit_result.status {
                FillStatus::Filled => {
                    let args = OrderArgs {
                        token_id: token_id,
                        price: token_info.price,
                        size: dec!(0),
                        side: crate::Side::BUY,
                    };
                    match self.client.create_and_post_order(&args).await {
                        Ok(_) => (),
                        Err(_) => (),
                    }
                }
                _ => (),
            }
        }
    }

    pub async fn run(&mut self) {
        self.on_tick().await;
    }
}
