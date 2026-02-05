use criterion::{criterion_group, criterion_main, Criterion, BatchSize};
use polynomial::{
    GlobalState,
    execute_egine::ExecuteEgine,
    FillEngine, ClobClient,
    config::{StrategyConfig, TailEaterStrategy},
    types::{TokenInfo, BookWithSequence, OrderSummary},
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

fn setup_execution_path() -> (String, Arc<Mutex<GlobalState>>, ExecuteEgine) {
    let state = Arc::new(Mutex::new(GlobalState::new()));
    let (_tx, rx) = mpsc::unbounded_channel();
    
    let config = StrategyConfig {
        tail_eater: TailEaterStrategy {
            buy_threshold: dec!(0.5),
            max_slippage: dec!(0.05),
            trade_unit: dec!(10),
            buy_upper: dec!(0.8),
        }
    };

    let fill_engine = FillEngine::new(dec!(10), dec!(0.5), 0);
    // Use a dummy URL, we won't hit network
    let client = ClobClient::new_internet("https://api.polymarket.com");
    
    let execute_engine = ExecuteEgine::new(
        client,
        rx,
        config,
        fill_engine,
        state.clone()
    );
    
    ("token_123".to_string(), state, execute_engine)
}

fn prepare_book(token_id: &str) -> BookWithSequence {
    // Create a realistic book with depth
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    
    // Asks from 0.55 to 0.65
    for i in 0..20 {
        asks.push(OrderSummary {
            price: dec!(0.55) + Decimal::from(i) * dec!(0.005),
            size: dec!(100),
        });
    }
    
    // Bids from 0.50 to 0.40
    for i in 0..20 {
        bids.push(OrderSummary {
            price: dec!(0.50) - Decimal::from(i) * dec!(0.005),
            size: dec!(100),
        });
    }

    BookWithSequence {
        token_id: token_id.to_string(),
        bids,
        asks,
        sequence: 100,
        timestamp: 1234567890,
    }
}

fn bench_full_path(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("full_execution_path", |b| {
        b.to_async(&rt).iter_batched(
            || {
                let (token_id, state, execute_engine) = setup_execution_path();
                let book = prepare_book(&token_id);
                let token_info = TokenInfo {
                    token_id: token_id.clone(),
                    price: dec!(0.55), // Matches lowest ask, triggers buy if threshold <= 0.55
                };
                (state, execute_engine, book, token_info)
            },
            |(state, mut execute_engine, book, token_info)| async move {
                // 1. Update State (Data Engine logic)
                {
                    let lock = state.lock().unwrap();
                    lock.update_order_book(book).unwrap();
                }
                
                // 2. Decision Logic (Execute Engine logic)
                // This simulates the "brain" of the bot: deciding to trade based on new state
                let _ = execute_engine.try_generate_order(&token_info);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_full_path);
criterion_main!(benches);
