use dotenvy::dotenv;
use polynomial::DEFAULT_CHAIN_ID;
use polynomial::data_engine::DataEngine;
use polynomial::errors::Result;
use polynomial::{ClobClient, FillEngine, config};
use polynomial::{GlobalState, execute_egine::ExecuteEgine};
use rust_decimal_macros::dec;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let logging_config = config::load_logging_config();

    let file_appender =
        tracing_appender::rolling::daily(&logging_config.log_dir, &logging_config.log_file);

    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::new(&logging_config.level))
        .with(fmt::layer().with_thread_ids(true).with_ansi(true))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(non_blocking),
        )
        .init();

    tracing::info!("🚀 Starting Polynomial Trading System...");

    // load config
    let private_key = env::var("PK").expect("PRIVATE_KEY must be set in .env file");
    let strategy_config = config::load_strategy_config();

    let global_state = Arc::new(GlobalState::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let data_engine = Arc::new(DataEngine::new(global_state.clone(), tx).await);
    let client = ClobClient::with_l1_headers(
        "https://clob.polymarket.com",
        &private_key,
        DEFAULT_CHAIN_ID,
    );
    let fill_engine = FillEngine::new(dec!(10), dec!(0.5), 0);
    let mut execute_egine =
        ExecuteEgine::new(client, rx, strategy_config, fill_engine, global_state);
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
