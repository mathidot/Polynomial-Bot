use criterion::{black_box, criterion_group, criterion_main, Criterion};
use polyfill_rs::book::OrderBook;
use polyfill_rs::types::{OrderDelta, Side};
use polyfill_rs::OrderBookManager;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::str::FromStr;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn benchmark_apply_delta(c: &mut Criterion) {
    c.bench_function("apply_delta_insert", |b| {
        let mut book = OrderBook::new("bench_token".to_string(), 100);
        let delta = OrderDelta {
            token_id: "bench_token".to_string(),
            timestamp: chrono::Utc::now(),
            side: Side::BUY,
            price: dec!(0.5),
            size: dec!(100),
            sequence: 1,
        };
        
        b.iter(|| {
            book.apply_delta(black_box(delta.clone())).unwrap();
        })
    });
}

fn benchmark_market_impact(c: &mut Criterion) {
    let mut book = OrderBook::new("bench_token".to_string(), 100);
    // Fill book with 50 levels
    for i in 0..50 {
        let price = Decimal::from_str(&format!("0.{}", 50 + i)).unwrap();
        let delta = OrderDelta {
            token_id: "bench_token".to_string(),
            timestamp: chrono::Utc::now(),
            side: Side::SELL,
            price,
            size: dec!(100),
            sequence: i as u64,
        };
        book.apply_delta(delta).unwrap();
    }

    c.bench_function("calculate_market_impact_50_levels", |b| {
        b.iter(|| {
            book.calculate_market_impact(black_box(Side::BUY), black_box(dec!(1000)))
        })
    });
}

fn benchmark_order_book_manager(c: &mut Criterion) {
    let manager = Arc::new(OrderBookManager::new(100));
    let rt = Runtime::new().unwrap();
    
    c.bench_function("manager_concurrent_updates", |b| {
        b.to_async(&rt).iter(|| async {
            let delta = OrderDelta {
                token_id: "bench_token".to_string(),
                timestamp: chrono::Utc::now(),
                side: Side::BUY,
                price: dec!(0.5),
                size: dec!(100),
                sequence: 1,
            };
            manager.get_or_create_book("bench_token").unwrap();
            manager.apply_delta(black_box(delta)).unwrap();
        })
    });
}

criterion_group!(benches, benchmark_apply_delta, benchmark_market_impact, benchmark_order_book_manager);
criterion_main!(benches);
