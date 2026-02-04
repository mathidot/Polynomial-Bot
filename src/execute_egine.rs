use crate::OrderArgs;
use crate::client::ClobClient;
use crate::config::StrategyConfig;
use crate::fill::FillStatus;
use crate::types::{MarketOrderRequest, TokenInfo};
use crate::{FillEngine, GlobalState, OrderRequest, OrderType, Side};
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

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
            let strategy_cfg = &self.config.tail_eater;
            let token_id = &token_info.token_id;
            let signal_price = token_info.price;

            if signal_price < strategy_cfg.buy_threshold {
                continue;
            }

            if signal_price > strategy_cfg.buy_upper {
                continue;
            }

            info!(
                "Signal triggered for {}: price {} >= threshold {}",
                token_id, signal_price, strategy_cfg.buy_threshold
            );

            let Ok(book) = self.global_state.get_book(token_id) else {
                warn!("Order book not found for token: {}", token_id);
                continue;
            };

            let order_request = MarketOrderRequest {
                token_id: token_id.clone(),
                side: Side::BUY,
                amount: strategy_cfg.trade_unit,
                slippage_tolerance: Some(strategy_cfg.max_slippage),
                client_id: Some(format!(
                    "te_{}_{}",
                    token_id,
                    chrono::Utc::now().timestamp_millis()
                )),
            };

            let simulation = match self.fill_egine.execute_market_order(&order_request, &book) {
                Ok(res) => res,
                Err(e) => {
                    error!("Simulation execution error: {:?}", e);
                    continue;
                }
            };

            match simulation.status {
                FillStatus::Filled | FillStatus::Partial => {
                    let avg_price = simulation.average_price;
                    let actual_slippage = if avg_price > signal_price {
                        (avg_price - signal_price) / signal_price
                    } else {
                        dec!(0)
                    };

                    if actual_slippage > strategy_cfg.max_slippage {
                        warn!(
                            "Simulation rejected: slippage too high ({} > {})",
                            actual_slippage, strategy_cfg.max_slippage
                        );
                        continue;
                    }

                    info!(
                        "Simulation passed: avg_price={}, size={}, status={:?}",
                        avg_price, simulation.total_size, simulation.status
                    );

                    let args = OrderArgs {
                        token_id: token_id.clone(),
                        price: signal_price,
                        size: strategy_cfg.trade_unit,
                        side: Side::BUY,
                    };

                    match self.client.create_and_post_order(&args).await {
                        Ok(order_id) => info!("Successfully posted order. ID: {:?}", order_id),
                        Err(e) => error!("API Error posting order: {:?}", e),
                    }
                }
                _ => {
                    info!(
                        "Simulation did not result in a fill: {:?}",
                        simulation.status
                    );
                }
            }
        }
    }

    pub async fn run(&mut self) {
        info!("TailEater ExecuteEngine started");
        self.on_tick().await;
    }
}
