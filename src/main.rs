use dotenvy::dotenv;
use polynomial::data_engine::DataEngine;
use polynomial::errors::Result;
use polynomial::{ClobClient, FillEngine, config};
use polynomial::{DEFAULT_CHAIN_ID, execute_egine};
use polynomial::{GlobalState, execute_egine::ExecuteEgine};
use rust_decimal_macros::dec;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let private_key = env::var("PK").expect("PRIVATE_KEY must be set in .env file");
    let global_state = Arc::new(GlobalState::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let data_engine = Arc::new(DataEngine::new(global_state.clone(), tx).await);
    let config = config::load_config();
    let client = ClobClient::with_l1_headers(
        "https://clob.polymarket.com",
        &private_key,
        DEFAULT_CHAIN_ID,
    );
    let fill_engine = FillEngine::new(dec!(10), dec!(0.5), 0);
    let mut execute_egine = ExecuteEgine::new(client, rx, config, fill_engine, global_state);
    let data_engine_handle = tokio::spawn(async move {
        data_engine.run();
    });

    let execute_engine_handle = tokio::spawn(async move {
        execute_egine.run().await;
    });

    let _ = tokio::try_join!(data_engine_handle, execute_engine_handle);

    tokio::signal::ctrl_c()
        .await
        .expect("fail to listen to event");
    Ok(())
}
