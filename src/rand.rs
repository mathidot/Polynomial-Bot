use crate::types::{BookMessage, OrderSummary, PriceChange, PriceChangeMessage};
use chrono::Utc;
use rand::Rng;
use rand::distr::{Alphabetic, Distribution, SampleString, StandardUniform, Uniform};
use rand::prelude::*;
use rand_pcg::Pcg64;
use rand_seeder::{Seeder, SipHasher};
use rust_decimal::Decimal;

pub fn generate_random_hex<R: Rng + ?Sized>(rng: &mut R, len: usize) -> String {
    const CHARSET: &[u8] = b"0123456789abcdef";
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

impl Distribution<OrderSummary> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> OrderSummary {
        let price = Decimal::new(rng.random_range(1..100), 2);
        let size = Decimal::new(rng.random_range(10..50000), 1);
        OrderSummary { price, size }
    }
}

impl Distribution<BookMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BookMessage {
        let bids_count = rng.random_range(1..=5);
        let asks_count = rng.random_range(1..=5);
        BookMessage {
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            hash: format!("0x{}", generate_random_hex(rng, 64)),
            timestamp: Utc::now().timestamp_millis() as u64,
            bids: (1..=bids_count)
                .map(|_| rng.random::<OrderSummary>())
                .collect(),
            asks: (1..=asks_count)
                .map(|_| rng.random::<OrderSummary>())
                .collect(),
        }
    }
}

impl Distribution<PriceChange> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> PriceChange {
        let side = if rng.random_bool(0.5) { "BUY" } else { "SELL" }.to_string();
        let price = Decimal::new(rng.random_range(1..100), 2);
        let best_bid = Decimal::new(rng.random_range(1..100), 2);
        let best_ask = Decimal::new(rng.random_range(1..100), 2);
        let size = Decimal::new(rng.random_range(1..1000), 0);
        PriceChange {
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            price,
            size,
            side,
            hash: format!("0x{}", generate_random_hex(rng, 64)),
            best_bid,
            best_ask,
        }
    }
}

impl Distribution<PriceChangeMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> PriceChangeMessage {
        let changes_count = rng.random_range(1..=5);
        PriceChangeMessage {
            market: format!("0x{}", generate_random_hex(rng, 40)),
            price_changes: (1..=changes_count)
                .map(|_| rng.random::<PriceChange>())
                .collect(),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_book_message() {
        let msg: BookMessage = rand::rng().random();
        println!("{:?}", msg);
    }
}
