use crate::book::OrderBook;
use crate::common::{
    CRYPTO_PATTERNS, MarketResponse, TokenResponse, TokenType, WEBSOCKET_MARKET_URL,
};
use crate::errors::{PolyfillError, Result, StreamErrorKind};
use crate::state::GlobalState;
use crate::types::{
    BookMessage, OrderDelta, PriceChangeMessage, StreamMessage, TokenInfo, WssAuth, WssChannelType,
    WssSubscription,
};
use crate::{BookSnapshot, ClobClient, DEFAULT_BASE_URL, MarketStream, TokenApi};
use chrono::Utc;
use chrono::{DateTime, Datelike};
use dashmap::{DashMap, DashSet};
use futures::stream::{SplitSink, SplitStream};
use futures::{Sink, SinkExt, Stream, StreamExt, future};
use rust_decimal_macros::dec;
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinSet;
use tracing::info;

fn generate_crypto_slugs() -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    let base_now = chrono::Local::now();

    for i in 0..3 {
        let future_date = base_now + chrono::Duration::days(i);
        let f_month = future_date.format("%B").to_string().to_lowercase();
        let f_day = future_date.day();
        let f_suffix = format!("-on-{}-{}", f_month, f_day);
        for p in CRYPTO_PATTERNS {
            slugs.push(format!("{}{}", p, f_suffix));
        }
    }
    return slugs;
}

pub fn parse_market(market: MarketResponse) -> Vec<TokenResponse> {
    market
        .tokens
        .iter()
        .enumerate()
        .zip(market.outcomes.iter())
        .map(|((i, id), outcome)| TokenResponse {
            token_id: id.clone(),
            outcome: outcome.clone(),
            winner: { if i == 0 { true } else { false } },
            is_valid: true,
            token_type: TokenType::default(),
        })
        .collect()
}

pub struct SubscribedChannel {
    pub chan_type: WssChannelType,
    pub chan_stream: Box<dyn MarketStream>,
}

pub struct StreamProvider {
    writer: Mutex<Pin<Box<dyn Sink<Value, Error = PolyfillError> + Send>>>,
    reader: Mutex<Pin<Box<dyn Stream<Item = Result<StreamMessage>> + Send>>>,
}

impl StreamProvider {
    pub async fn new<S>(mut s: S) -> Self
    where
        S: MarketStream,
    {
        match s.connect().await {
            Ok(_) => info!("stream is connecting"),
            Err(e) => {
                tracing::error!("stream fail to connect: {}", e);
            }
        }

        let (w, r) = s.split();
        let writer: Pin<Box<dyn Sink<Value, Error = PolyfillError> + Send>> = Box::pin(w);
        let reader: Pin<Box<dyn Stream<Item = Result<StreamMessage>> + Send>> = Box::pin(r);

        Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        }
    }

    pub async fn send(&self, msg: Value) -> Result<()> {
        self.writer
            .lock()
            .await
            .as_mut()
            .send(msg)
            .await
            .map_err(|e| PolyfillError::Stream {
                message: format!("fail to send msg: {}", e),
                kind: StreamErrorKind::SubscriptionFailed,
            })
    }

    pub async fn next(&self) -> Option<StreamMessage> {
        match self.reader.lock().await.as_mut().next().await {
            Some(Ok(msg)) => Some(msg),
            _ => None,
        }
    }
}

pub struct DataEngine {
    client: Arc<ClobClient>,
    subscribe_tokens: DashSet<String>,
    token_tx: Arc<Mutex<mpsc::UnboundedSender<TokenResponse>>>,
    token_rx: Arc<Mutex<mpsc::UnboundedReceiver<TokenResponse>>>,
    subscribe_provider: DashMap<WssChannelType, Arc<StreamProvider>>,
    global_state: Arc<GlobalState>,
    token_info_tx: mpsc::UnboundedSender<TokenInfo>,
}

impl DataEngine {
    pub async fn new(
        state: Arc<GlobalState>,
        token_info_tx: mpsc::UnboundedSender<TokenInfo>,
        channels: Vec<SubscribedChannel>,
    ) -> Self {
        let (token_tx, token_rx) = mpsc::unbounded_channel();
        let subscribe_provider: DashMap<WssChannelType, Arc<StreamProvider>> = DashMap::new();
        for chan in channels {
            let provider = StreamProvider::new(chan.chan_stream).await;
            subscribe_provider.insert(chan.chan_type, Arc::new(provider));
        }

        Self {
            client: Arc::new(ClobClient::new_internet(DEFAULT_BASE_URL)),
            subscribe_tokens: DashSet::new(),
            token_tx: Arc::new(Mutex::new(token_tx)),
            token_rx: Arc::new(Mutex::new(token_rx)),
            subscribe_provider,
            global_state: state,
            token_info_tx: token_info_tx,
        }
    }

    async fn stream_crypto_tokens(&self) {
        let internal = std::time::Duration::from_secs(10 * 60);
        loop {
            let market_ids: Vec<String>;
            match self.get_crypto_markets_id().await {
                Ok(ids) => {
                    market_ids = ids;
                }
                Err(e) => {
                    tracing::error!("fail to get crypto ids: {}", e);
                    tokio::time::sleep(internal).await;
                    continue;
                }
            }

            let mut set = JoinSet::new();
            for id in market_ids {
                let client = self.client.clone();
                set.spawn(async move { client.get_market_by_id(&id).await });
            }
            while let Some(res) = set.join_next().await {
                match res {
                    Ok(Ok(market)) => {
                        let tokens = parse_market(market);
                        for mut token in tokens {
                            token.token_type = TokenType::CRYPTO;
                            match self.token_tx.lock().await.send(token) {
                                Ok(_) => (),
                                Err(e) => tracing::error!("transfer token meets err: {}", e),
                            }
                        }
                    }
                    Ok(Err(e)) => tracing::error!("api fail to request: {:?}", e),
                    Err(e) => tracing::error!("task crash down: {:?}", e),
                }
            }
            tokio::time::sleep(internal).await;
        }
    }

    async fn receive_crypto_tokens(self: Arc<Self>) {
        loop {
            if let Some(token) = self.token_rx.lock().await.recv().await {
                if !self.subscribe_tokens.contains(&token.token_id) {
                    let engine = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = engine.subscribe_token(token).await {
                            tracing::error!("fail to subscribe: {}", e);
                        }
                    });
                }
            }
        }
    }

    async fn subscribe_token(&self, token: TokenResponse) -> Result<()> {
        let chan_type = match token.token_type {
            TokenType::CRYPTO => WssChannelType::Crypto,
            TokenType::SPORTS => WssChannelType::Sports,
        };

        let target_provider = self
            .subscribe_provider
            .get(&chan_type)
            .map(|r| r.value().clone())
            .ok_or_else(|| {
                PolyfillError::stream(
                    "cannot find target stream",
                    StreamErrorKind::SubscriptionFailed,
                )
            })?;

        let token_subscription = WssSubscription {
            channel_type: "market".to_string(),
            asset_ids: vec![token.token_id.to_string()],
            operation: Some("subscribe".to_string()),
            auth: None,
        };
        target_provider
            .send(serde_json::to_value(token_subscription)?)
            .await?;
        Ok(())
    }

    fn on_book(&self, book: BookMessage) -> Result<()> {
        info!("Handle BookMessage: {:?}", book.clone());
        let token_id = book.asset_id.clone();
        let mut order_book = OrderBook::new(token_id.clone(), 100);
        order_book.set_tick_size(dec!(0.001))?;
        let book_snapshot = BookSnapshot {
            asset_id: book.asset_id,
            timestamp: book.timestamp,
            asks: book.asks,
            bids: book.bids,
        };

        order_book
            .apply_book_snapshot(book_snapshot)
            .inspect_err(|e| tracing::error!("apply_book_snapshot failed: {}", e))?;

        self.global_state
            .insert_order_book(order_book)
            .map_err(|_| PolyfillError::internal_simple("fail to insert order_book"))?;

        match self.global_state.get_price(&token_id) {
            Ok(Some(price)) => {
                let token_info = TokenInfo {
                    token_id: token_id,
                    price: price,
                };

                info!("send token_info to execute_engine");

                match self.token_info_tx.send(token_info) {
                    Ok(()) => tracing::info!("translate token_info"),
                    Err(_) => tracing::error!("err while translate token_info"),
                };
            }
            Ok(None) => {
                tracing::error!("get_none_price");
            }
            Err(e) => {
                tracing::error!("get price error: {}", e);
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn on_price_change(&self, price_changes: PriceChangeMessage) -> Result<()> {
        Ok(())
    }

    async fn handle_message(&self) -> Result<()> {
        let provider = match self.subscribe_provider.get_mut(&WssChannelType::Crypto) {
            Some(provider) => provider.clone(),
            None => {
                return Err(PolyfillError::Stream {
                    message: "cannot find target stream".to_string(),
                    kind: StreamErrorKind::SubscriptionFailed,
                });
            }
        };
        while let Some(message) = provider.next().await {
            match message.clone() {
                StreamMessage::Book(book) => {
                    if let Err(_) = self.on_book(book) {
                        tracing::error!("fail to handle book message");
                        continue;
                    }
                }
                StreamMessage::PriceChange(price_change) => {
                    // println!("receive price change: {:?}", price_change);
                }
                StreamMessage::TickSizeChange(tick_size_change) => {
                    // println!("receive tick size change: {:?}", tick_size_change);
                }
                StreamMessage::LastTradePrice(last_trade_price) => {
                    // println!("receive last trade price: {:?}", last_trade_price);
                }
                StreamMessage::BestBidAsk(best_bid_ask) => {
                    // println!("receive best bid ask: {:?}", best_bid_ask);
                }
                StreamMessage::NewMarket(new_market) => {
                    // println!("receive new market: {:?}", new_market);
                }
                StreamMessage::MarketResolved(market_resolved) => {
                    // println!("receive market resolved: {:?}", market_resolved);
                }
                StreamMessage::Trade(trade) => {
                    // println!("receive trade: {:?}", trade);
                }
                StreamMessage::Order(order) => {
                    // println!("receive order: {:?}", order);
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn run(self: Arc<Self>) {
        let engine1 = self.clone();
        tokio::spawn(async move {
            engine1.receive_crypto_tokens().await;
        });

        let engine2 = self.clone();
        tokio::spawn(async move {
            engine2.stream_crypto_tokens().await;
        });

        let engine3 = self.clone();
        tokio::spawn(async move {
            let _ = engine3.handle_message().await;
        });
    }

    async fn get_crypto_markets_id(&self) -> Result<Vec<String>> {
        let slugs = generate_crypto_slugs();
        return self.get_crypto_markets_by_slugs(slugs).await;
    }

    async fn get_crypto_markets_by_slugs(&self, slugs: Vec<String>) -> Result<Vec<String>> {
        let futures = slugs
            .iter()
            .map(|slug| self.get_market_id_by_slug(slug.clone()));
        let results = future::join_all(futures).await;
        let market_ids: Vec<String> = results
            .into_iter()
            .flat_map(|res| res.ok())
            .flatten()
            .collect();
        Ok(market_ids)
    }

    async fn get_market_id_by_slug(&self, event_slug: String) -> Result<Vec<String>> {
        self.client.get_market_id_by_slug(event_slug).await
    }

    #[allow(dead_code)]
    async fn get_specified_tag_ids(
        &mut self,
        filtered_list: Option<HashSet<String>>,
    ) -> Result<Vec<String>> {
        let res = self.client.get_specified_tag_ids(filtered_list).await;
        return res;
    }

    pub async fn get_live_sports_tokens(&mut self) -> Result<Vec<String>> {
        let params = HashMap::from([
            ("closed".to_string(), "false".to_string()),
            ("active".to_string(), "true".to_string()),
        ]);

        let val = self.client.get_events_by_params(params).await?;
        let events: Vec<String> =
            serde_json::from_value(val).map_err(|_| PolyfillError::Parse {
                message: "fail to parse events params".to_string(),
                source: None,
            })?;
        println!("{:?}", events);
        Ok(events)
    }
}
