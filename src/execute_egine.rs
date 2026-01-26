use crate::client::ClobClient;
use crate::config::{self, StrategyConfig, TailEaterStrategy};
use crate::fill::{FillStats, FillStatus};
use crate::types::TokenInfo;
use crate::{FillEngine, GlobalState, OrderRequest, state};
use crate::{OrderArgs, Result};
use futures::channel::mpsc::UnboundedReceiver;
use rust_decimal_macros::dec;
use rustls::client;
use std::sync::Arc;
use tokio::sync::mpsc;
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

    pub async fn tick(&mut self) {
        while let Some(token_info) = self.token_rx.recv().await {
            println!("exec engine recv token info: {:?}", token_info);
            let token_id = token_info.token_id;
            let current_price = token_info.price;
            if current_price >= self.config.tail_eater.buy_threshold {
                if let Ok(book) = self.global_state.get_book(&token_id) {
                    let order_request = OrderRequest {
                        token_id: token_id.clone(),
                        side: crate::Side::BUY,
                        price: token_info.price,
                        size: dec!(0),
                        order_type: crate::OrderType::GTC,
                        expiration: None,
                        client_id: Some("tail_eater_limit_order".to_string()),
                    };

                    if let Ok(limit_result) =
                        self.fill_egine.execute_limit_order(&order_request, &book)
                    {
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
            }
        }
    }

    pub async fn run(&mut self) {
        self.tick().await;
    }
}
