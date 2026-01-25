use polynomial::data_engine::DataEngine;
use polynomial::errors::Result;
use polynomial::execute_egine;
use polynomial::{ClobClient, config};
use polynomial::{GlobalState, execute_egine::ExecuteEgine};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let global_state = Arc::new(GlobalState::new());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let data_engine = Arc::new(DataEngine::new(global_state.clone(), tx).await);
    let config = config::load_config();
    // let execute_egine = ExecuteEgine::new();
    data_engine.run();

    tokio::signal::ctrl_c()
        .await
        .expect("fail to listen to event");
    Ok(())
}
