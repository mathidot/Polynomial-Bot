use dotenvy::dotenv;
use polynomial::SubscribedChannel;
use polynomial::WebSocketStream;
use polynomial::WssChannelType;
use polynomial::config::EngineMode;
use polynomial::errors::Result;
use polynomial::stream::MockStream;
use polynomial::{ClobClient, DataEngine, FillEngine, config};
use polynomial::{
    DEFAULT_BASE_URL, DEFAULT_CHAIN_ID, DEFAULT_WEBSOCKET_MARKET_URL, DEFAULT_WEBSOCKET_USER_URL,
    GlobalState, execute_egine::ExecuteEgine,
};
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

    let engine_config = config::load_engine_config();
    let global_state = Arc::new(GlobalState::new());
    let (tx, rx) = mpsc::unbounded_channel();
    let mut subscribed_channels = Vec::new();

    match engine_config.engine_mode {
        EngineMode::MOCK => {
            subscribed_channels.push(SubscribedChannel {
                chan_type: WssChannelType::Crypto,
                chan_stream: Box::new(MockStream::new(10000)),
            });

            subscribed_channels.push(SubscribedChannel {
                chan_type: WssChannelType::Sports,
                chan_stream: Box::new(MockStream::new(10000)),
            });

            subscribed_channels.push(SubscribedChannel {
                chan_type: WssChannelType::User,
                chan_stream: Box::new(MockStream::new(10000)),
            });
        }
        EngineMode::REAL => {
            subscribed_channels.push(SubscribedChannel {
                chan_type: WssChannelType::Crypto,
                chan_stream: Box::new(WebSocketStream::new(DEFAULT_WEBSOCKET_MARKET_URL)),
            });

            subscribed_channels.push(SubscribedChannel {
                chan_type: WssChannelType::Sports,
                chan_stream: Box::new(WebSocketStream::new(DEFAULT_WEBSOCKET_MARKET_URL)),
            });

            subscribed_channels.push(SubscribedChannel {
                chan_type: WssChannelType::User,
                chan_stream: Box::new(WebSocketStream::new(DEFAULT_WEBSOCKET_USER_URL)),
            });
        }
    }

    let data_engine =
        Arc::new(DataEngine::new(global_state.clone(), tx, subscribed_channels).await);
    let data_engine_handle = tokio::spawn(async move {
        data_engine.run();
    });

    // load config
    let private_key = env::var("PK").expect("PRIVATE_KEY must be set in .env file");

    let strategy_config = config::load_strategy_config();
    let client = ClobClient::with_l1_headers(DEFAULT_BASE_URL, &private_key, DEFAULT_CHAIN_ID);
    let fill_engine = FillEngine::new(dec!(10), dec!(0.5), 0);
    let mut execute_egine =
        ExecuteEgine::new(client, rx, strategy_config, fill_engine, global_state);

    let execute_engine_handle = tokio::spawn(async move {
        execute_egine.run().await;
    });

    let _ = tokio::try_join!(data_engine_handle, execute_engine_handle);

    tokio::signal::ctrl_c()
        .await
        .expect("fail to listen to event");
    Ok(())
}
