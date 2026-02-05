use crate::Side;
use crate::types::{
    BestBidAskMessage, BookMessage, EventMessage, LastTradePriceMessage, MarketOrder,
    MarketResolvedMessage, NewMarketMessage, OrderMessage, OrderSummary, PriceChange,
    PriceChangeMessage, StreamMessage, TickSizeChangeMessage, TradeMessage,
};
use chrono::Utc;
use rand::Rng;
use rand::distr::{Distribution, StandardUniform};
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
        let side = if rng.random_bool(0.5) {
            Side::BUY
        } else {
            Side::SELL
        };
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

impl Distribution<TickSizeChangeMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> TickSizeChangeMessage {
        TickSizeChangeMessage {
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            old_tick_size: Decimal::new(rng.random_range(1..1000), 3),
            new_tick_size: Decimal::new(rng.random_range(1..1000), 3),
            side: if rng.random_bool(0.5) {
                Side::BUY
            } else {
                Side::SELL
            },
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Distribution<LastTradePriceMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> LastTradePriceMessage {
        LastTradePriceMessage {
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            fee_rate_bps: Decimal::new(0, 0),
            price: Decimal::new(rng.random_range(1..100), 2),
            side: if rng.random_bool(0.5) {
                Side::BUY
            } else {
                Side::SELL
            },
            size: Decimal::new(rng.random_range(1..1000), 0),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Distribution<BestBidAskMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BestBidAskMessage {
        BestBidAskMessage {
            market: format!("0x{}", generate_random_hex(rng, 40)),
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            best_bid: Decimal::new(rng.random_range(0..100), 2),
            best_ask: Decimal::new(rng.random_range(0..100), 2),
            spread: Decimal::new(rng.random_range(0..10), 2),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Distribution<EventMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> EventMessage {
        EventMessage {
            id: format!("0x{}", generate_random_hex(rng, 16)),
            ticker: generate_random_hex(rng, 16),
            slug: generate_random_hex(rng, 16),
            title: generate_random_hex(rng, 16),
            description: generate_random_hex(rng, 16),
        }
    }
}

impl Distribution<NewMarketMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> NewMarketMessage {
        NewMarketMessage {
            id: format!("0x{}", generate_random_hex(rng, 16)),
            question: generate_random_hex(rng, 16),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            slug: generate_random_hex(rng, 16),
            description: generate_random_hex(rng, 16),
            asset_ids: (0..2).map(|_| generate_random_hex(rng, 16)).collect(),
            outcomes: (0..2)
                .map(|_| if rng.random_bool(0.5) { "Yes" } else { "No" }.to_string())
                .collect(),
            event_message: rng.random(),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Distribution<MarketResolvedMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> MarketResolvedMessage {
        let asset_ids: Vec<String> = (0..2)
            .map(|_| format!("0x{}", generate_random_hex(rng, 16)))
            .collect();
        let outcomes: Vec<String> = vec!["Yes".to_string(), "No".to_string()];
        let winner_index = if rng.random_bool(0.5) { 0 } else { 1 };

        MarketResolvedMessage {
            id: format!("0x{}", generate_random_hex(rng, 16)),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            question: "Will Bitcoin hit $100k by the end of 2025?".to_string(),
            slug: "btc-100k-2025".to_string(),
            description: "Settlement based on Coinbase price feed.".to_string(),
            asset_ids: asset_ids.clone(),
            outcomes: outcomes.clone(),
            winning_asset_id: asset_ids[winner_index].clone(),
            winning_outcome: outcomes[winner_index].clone(),
            event_message: rng.random::<EventMessage>(),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Distribution<MarketOrder> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> MarketOrder {
        MarketOrder {
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            order_id: format!("0x{}", generate_random_hex(rng, 16)),
            owner: format!("0x{}", generate_random_hex(rng, 40)),
            outcome: if rng.random_bool(0.5) { "Yes" } else { "No" }.to_string(),
            matched_amount: Decimal::new(rng.random_range(1..500), 1),
            price: Decimal::new(rng.random_range(1..100), 2),
        }
    }
}

impl Distribution<TradeMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> TradeMessage {
        let now = Utc::now().timestamp_millis() as u64;

        TradeMessage {
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            id: format!("0x{}", generate_random_hex(rng, 16)),
            taker_order_id: format!("0x{}", generate_random_hex(rng, 16)),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            owner: format!("0x{}", generate_random_hex(rng, 40)),
            trade_owner: format!("0x{}", generate_random_hex(rng, 40)),
            last_update: now,
            matchtime: now,
            timestamp: now,
            side: if rng.random_bool(0.5) {
                Side::BUY
            } else {
                Side::SELL
            },
            status: "FILLED".to_string(),
            outcome: if rng.random_bool(0.5) { "Yes" } else { "No" }.to_string(),
            trade_type: "TRADE".to_string(),
            price: Decimal::new(rng.random_range(100..10000), 2),
            size: Decimal::new(rng.random_range(1..1000), 1),
            maker_orders: (0..rng.random_range(1..=3))
                .map(|_| rng.random::<MarketOrder>())
                .collect(),
        }
    }
}

impl Distribution<OrderMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> OrderMessage {
        let original_size_raw = rng.random_range(100..1000);
        let matched_size_raw = rng.random_range(0..=original_size_raw);

        OrderMessage {
            id: format!("0x{}", generate_random_hex(rng, 16)),
            asset_id: format!("0x{}", generate_random_hex(rng, 16)),
            market: format!("0x{}", generate_random_hex(rng, 40)),
            owner: format!("0x{}", generate_random_hex(rng, 40)),
            order_owner: format!("0x{}", generate_random_hex(rng, 40)),
            associate_trades: (0..rng.random_range(0..=3))
                .map(|_| format!("0x{}", generate_random_hex(rng, 16)))
                .collect(),
            original_size: Decimal::new(original_size_raw, 1),
            size_matched: Decimal::new(matched_size_raw, 1),
            price: Decimal::new(rng.random_range(1..100), 2),
            side: if rng.random_bool(0.5) {
                Side::BUY
            } else {
                Side::SELL
            },
            outcome: if rng.random_bool(0.5) { "Yes" } else { "No" }.to_string(),
            order_type: "PLACEMENT".to_string(),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl Distribution<StreamMessage> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> StreamMessage {
        match rng.random_range(0..10) {
            0 => StreamMessage::Book(rng.random()),
            1 => StreamMessage::PriceChange(rng.random()),
            2 => StreamMessage::TickSizeChange(rng.random()),
            3 => StreamMessage::LastTradePrice(rng.random()),
            4 => StreamMessage::BestBidAsk(rng.random()),
            5 => StreamMessage::NewMarket(rng.random()),
            6 => StreamMessage::MarketResolved(rng.random()),
            7 => StreamMessage::Trade(rng.random()),
            8 => StreamMessage::Order(rng.random()),
            _ => StreamMessage::Heartbeat(Utc::now()),
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

    #[test]
    fn test_random_message() {
        for _ in 1..100 {
            let msg: StreamMessage = rand::rng().random();
            println!("{:?}", msg);
        }
    }
}
