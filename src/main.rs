use polynomial::data_engine::DataEngine;
use polynomial::errors::Result;
use std::sync::Arc;
#[tokio::main]
async fn main() -> Result<()> {
    // console_subscriber::init();
    // let data_engine = Arc::new(DataEngine::new().await);
    // data_engine.start();
    // tokio::signal::ctrl_c()
    //     .await
    //     .expect("fail to listen to event");
    Ok(())
}
