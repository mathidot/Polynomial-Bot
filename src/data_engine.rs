use crate::book::{OrderBook, OrderBookManager};
use crate::common::{
    CRYPTO_PATTERNS, EVENT_URL, MARKET_URL, Market, Result, SLUG_URL, SPORT_URL, Token, TokenType,
    WEBSOCKET_MARKET_URL,
};
use crate::stream::{MockStream, WebSocketStream};
use crate::types::{
    BookMessage, OrderDelta, StreamMessage, WssAuth, WssChannelType, WssSubscription,
};
use anyhow::anyhow;
use chrono::Utc;
use chrono::{DateTime, Datelike};
use dashmap::{DashMap, DashSet};
use futures::stream::{SplitSink, SplitStream};
use futures::{Sink, SinkExt, Stream, StreamExt, future};
use polyfill_rs::{ClobClient, PolyfillError, book, crypto};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinSet;

trait TokenApi {
    async fn get_events_by_params(&self, params: HashMap<String, String>) -> Result<Value>;
    async fn get_specified_tag_ids(
        &self,
        filtered_list: Option<HashSet<String>>,
    ) -> Result<Vec<String>>;

    async fn get_market_id_by_slug(&self, event_slug: String) -> Result<Vec<String>>;

    async fn get_market_by_id(&self, condition_id: &str) -> Result<Market>;
}

impl TokenApi for ClobClient {
    async fn get_events_by_params(&self, params: HashMap<String, String>) -> Result<Value> {
        let response = self
            .http_client
            .get(EVENT_URL)
            .json(&params)
            .send()
            .await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?;

        let ret = response
            .json::<Value>()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e));
        ret
    }

    async fn get_specified_tag_ids(
        &self,
        filtered_list: Option<HashSet<String>>,
    ) -> Result<Vec<String>> {
        let mut tags_set: HashSet<String> = HashSet::new();
        let mut ret: Vec<String> = Vec::new();

        let filtered_list_ref = filtered_list.as_ref();

        let sports_json: Value = self
            .http_client
            .get(SPORT_URL)
            .send()
            .await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?
            .json()
            .await?;

        sports_json
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                entry
                    .get("sport")
                    .and_then(|v| v.as_str())
                    .map_or(false, |s| {
                        filtered_list_ref.map_or(true, |set| set.contains(s))
                    })
            })
            .filter_map(|entry| entry.get("tags")?.as_str())
            .flat_map(|s| s.split(','))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .for_each(|s| {
                let tag = s.to_string();
                if tags_set.insert(tag.clone()) {
                    ret.push(tag);
                }
            });

        Ok(ret)
    }

    async fn get_market_id_by_slug(&self, event_slug: String) -> Result<Vec<String>> {
        let slug_url = format!("{}/{}", SLUG_URL, event_slug);
        let resp_json: Value = self
            .http_client
            .get(slug_url)
            .send()
            .await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?
            .json()
            .await?;

        let markets = resp_json
            .as_object()
            .ok_or_else(|| anyhow!("expect markets data but got nothing"))?;
        let market_ids: Vec<String> = markets
            .get("markets")
            .and_then(|m: &Value| m.as_array())
            .into_iter()
            .flatten()
            .filter_map(|m: &Value| {
                m.get("id")
                    .and_then(|id| id.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .collect();
        Ok(market_ids)
    }

    async fn get_market_by_id(&self, condition_id: &str) -> Result<Market> {
        let response = self
            .http_client
            .get(format!("{}/{}", MARKET_URL, condition_id))
            .send()
            .await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?;

        response.json::<Market>().await.map_err(|e| {
            anyhow::anyhow!(PolyfillError::parse(
                format!("Failed to parse response: {}", e),
                None
            ))
        })
    }
}

fn parse_market(market: Market) -> Vec<Token> {
    market
        .tokens
        .iter()
        .enumerate()
        .zip(market.outcomes.iter())
        .map(|((i, id), outcome)| Token {
            token_id: id.clone(),
            outcome: outcome.clone(),
            winner: { if i == 0 { true } else { false } },
            is_valid: true,
            token_type: TokenType::default(),
        })
        .collect()
}

type MessageHandler =
    Box<dyn Fn(&mut OrderBookManager, StreamMessage) -> Result<()> + Send + Sync + 'static>;
pub struct DataEngine {
    client: Arc<ClobClient>,
    subscribe_tokens: DashSet<String>,
    subscribe_tx: Arc<Mutex<mpsc::UnboundedSender<Token>>>,
    subscribe_rx: Arc<Mutex<mpsc::UnboundedReceiver<Token>>>,
    subscribe_write_stream: DashMap<WssChannelType, Arc<Mutex<SplitSink<WebSocketStream, Value>>>>,
    subscribe_read_stream: DashMap<WssChannelType, Arc<Mutex<SplitStream<WebSocketStream>>>>,
    // mock_subscribe_write_stream: DashMap<WssChannelType, Arc<Mutex<SplitSink<MockStream, Value>>>>,
    // mock_subscribe_read_stream: DashMap<WssChannelType, Arc<Mutex<SplitStream<MockStream>>>>,
    book_manager: Arc<std::sync::Mutex<OrderBookManager>>,
    message_handles: HashMap<std::mem::Discriminant<StreamMessage>, MessageHandler>,
}

impl DataEngine {
    pub async fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let channels = [
            WssChannelType::Crypto,
            WssChannelType::Sports,
            WssChannelType::User,
        ];
        let subscribe_write_stream = DashMap::new();
        let subscribe_read_stream = DashMap::new();

        for chan in channels {
            let stream = WebSocketStream::new(WEBSOCKET_MARKET_URL);
            // stream = stream.with_auth(auth.clone());
            println!("initialize");
            match stream.init_and_split().await {
                Ok((writer, reader)) => {
                    subscribe_write_stream.insert(chan, Arc::new(Mutex::new(writer)));
                    subscribe_read_stream.insert(chan, Arc::new(Mutex::new(reader)));
                }
                Err(e) => {
                    eprintln!("Failed to connect to channel {:?}: {}", chan, e);
                }
            }
        }

        let mut message_handles = HashMap::new();

        Self {
            client: Arc::new(ClobClient::new_internet("https://clob.polymarket.com")),
            subscribe_tokens: DashSet::new(),
            subscribe_rx: Arc::new(Mutex::new(rx)),
            subscribe_tx: Arc::new(Mutex::new(tx)),
            subscribe_write_stream,
            subscribe_read_stream,
            book_manager: Arc::new(std::sync::Mutex::new(OrderBookManager::new(100))),
            message_handles: message_handles,
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
                    eprintln!("fail to get crypto ids: {}", e);
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
                            match self.subscribe_tx.lock().await.send(token) {
                                Ok(_) => (),
                                Err(e) => eprint!("transfer token meets err: {}", e),
                            }
                        }
                    }
                    Ok(Err(e)) => eprintln!("api fail to request: {:?}", e),
                    Err(e) => eprintln!("task crash down: {:?}", e),
                }
            }
            tokio::time::sleep(internal).await;
        }
    }

    async fn receive_crypto_tokens(self: Arc<Self>) {
        loop {
            if let Some(token) = self.subscribe_rx.lock().await.recv().await {
                if !self.subscribe_tokens.contains(&token.token_id) {
                    let engine = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = engine.subscribe_token(token).await {
                            eprintln!("fail to subscribe: {}", e);
                        }
                    });
                }
            }
        }
    }

    async fn subscribe_token(&self, token: Token) -> Result<()> {
        let book_manager = self.book_manager.lock().map_err(|e| PolyfillError::)

        if let Ok(_) = self.book_manager.lock().map_err(|e| PolyfillError::P)
        get_book(&token.token_id) {
            return Ok(());
        }

        let chan_type = match token.token_type {
            TokenType::CRYPTO => WssChannelType::Crypto,
            TokenType::SPORTS => WssChannelType::Sports,
        };

        let target_stream = self
            .subscribe_write_stream
            .get(&chan_type)
            .map(|r| r.value().clone());

        if let Some(stream_mutex) = target_stream {
            {
                let mut stream = stream_mutex.lock().await;
                let token_subscription = WssSubscription {
                    channel_type: "market".to_string(),
                    asset_ids: vec![token.token_id.to_string()],
                    operation: Some("subscribe".to_string()),
                    auth: None,
                };
                stream
                    .send(serde_json::to_value(token_subscription)?)
                    .await?;
            }
            Ok(())
        } else {
            Err(anyhow!("No stream found for {:?}", chan_type).into())
        }
    }

    fn handle_book_message(&self, book: BookMessage) -> Result<()> {
        if self.book_manager.lock().is_exist(&book.asset_id)? {
            return Ok(());
        }
        let token_id = book.asset_id.clone();
        let mut order_book = OrderBook::new(token_id.clone(), 100);
        for bid in book.bids {
            let delta = OrderDelta {
                token_id: token_id.clone(),
                timestamp: DateTime::from_timestamp_millis(book.timestamp as i64)
                    .unwrap_or(Utc::now()),
                side: crate::types::Side::BUY,
                price: bid.price,
                size: bid.size,
                sequence: 0,
            };
            order_book.apply_delta(delta)?;
        }
        for ask in book.asks {
            let delta = OrderDelta {
                token_id: token_id.clone(),
                timestamp: DateTime::from_timestamp_millis(book.timestamp as i64)
                    .unwrap_or(Utc::now()),
                side: crate::types::Side::SELL,
                price: ask.price,
                size: ask.size,
                sequence: 0,
            };
            order_book.apply_delta(delta)?;
        }
        self.book_manager.insert(order_book)?;
        Ok(())
    }

    async fn handle_message(&self) -> Result<()> {
        let crypto_stream = match self.subscribe_read_stream.get_mut(&WssChannelType::Crypto) {
            Some(stream) => stream.clone(),
            None => return Err(anyhow!("No stream found for crypto stream").into()),
        };
        let mut lock = crypto_stream.lock().await;
        while let Some(Ok(message)) = lock.next().await {
            match message.clone() {
                StreamMessage::Book(book) => {
                    self.handle_book_message(book);
                }
                StreamMessage::PriceChange(price_change) => {
                    println!("receive price change: {:?}", price_change);
                }
                StreamMessage::TickSizeChange(tick_size_change) => {
                    println!("receive tick size change: {:?}", tick_size_change);
                }
                StreamMessage::LastTradePrice(last_trade_price) => {
                    println!("receive last trade price: {:?}", last_trade_price);
                }
                StreamMessage::BestBidAsk(best_bid_ask) => {
                    println!("receive best bid ask: {:?}", best_bid_ask);
                }
                StreamMessage::NewMarket(new_market) => {
                    println!("receive new market: {:?}", new_market);
                }
                StreamMessage::MarketResolved(market_resolved) => {
                    println!("receive market resolved: {:?}", market_resolved);
                }
                StreamMessage::Trade(trade) => {
                    println!("receive trade: {:?}", trade);
                }
                StreamMessage::Order(order) => {
                    println!("receive order: {:?}", order);
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn start(self: Arc<Self>) {
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
            let ret = engine3.handle_message().await;
        });
    }

    async fn get_crypto_markets_id(&self) -> Result<Vec<String>> {
        let slugs = DataEngine::generate_crypto_slugs();
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
        let events: Vec<String> = serde_json::from_value(val)
            .map_err(|e| anyhow::anyhow!("here should be string of vec [{}]", e))?;
        println!("{:?}", events);
        Ok(events)
    }
}
