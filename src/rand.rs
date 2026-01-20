use crate::types::{BookMessage, OrderSummary};
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
            market: format!("0x{}", generate_random_hex(rng, 40)), // 模拟以太坊地址长度
            hash: format!("0x{}", generate_random_hex(rng, 64)),   // 模拟交易哈希长度
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_book_message() {
        let msg: BookMessage = rand::rng().random();
        println!("{:?}", msg);
    }
}
