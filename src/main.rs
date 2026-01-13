use polynomial::data_engine::DataEngine;
use std::sync::Arc;
mod common;
use common::Result;
#[tokio::main]
async fn main() -> Result<()> {
    // console_subscriber::init();
    let data_engine = Arc::new(DataEngine::new());
    data_engine.start();
    tokio::signal::ctrl_c().await.expect("fail to listen to event");
    Ok(())
}
