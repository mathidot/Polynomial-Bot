use crate::book::OrderBookManager;
use crate::common::{
    CRYPTO_PATTERNS,
    EVENT_URL,
    MARKET_URL,
    Market,
    Result,
    SLUG_URL,
    SPORT_URL,
    Token,
    TokenType,
    WEBSOCKET_MARKET_URL,
};
use crate::stream::{ MockStream, WebSocketStream };
use crate::types::{ StreamMessage, WssAuth, WssChannelType, WssSubscription };
use anyhow::anyhow;
use chrono::Datelike;
use chrono::Utc;
use dashmap::{ DashMap, DashSet };
use futures::stream::{ SplitSink, SplitStream };
use futures::{ Sink, SinkExt, Stream, StreamExt, future };
use polyfill_rs::{ ClobClient, PolyfillError, crypto };
use serde_json::{ Value, json };
use std::collections::HashMap;
use std::result::Result::Ok;
use std::{ collections::HashSet, sync::Arc };
use tokio::sync::{ Mutex, mpsc };
use tokio::task::JoinSet;

trait TokenApi {
    async fn get_events_by_params(&self, params: HashMap<String, String>) -> Result<Value>;
    async fn get_specified_tag_ids(
        &self,
        filtered_list: Option<HashSet<String>>
    ) -> Result<Vec<String>>;

    async fn get_market_id_by_slug(&self, event_slug: String) -> Result<Vec<String>>;

    async fn get_market_by_id(&self, condition_id: &str) -> Result<Market>;
}

impl TokenApi for ClobClient {
    async fn get_events_by_params(&self, params: HashMap<String, String>) -> Result<Value> {
        let response = self.http_client
            .get(EVENT_URL)
            .json(&params)
            .send().await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?;

        let ret = response.json::<Value>().await.map_err(|e| anyhow::anyhow!("{}", e));
        ret
    }

    async fn get_specified_tag_ids(
        &self,
        filtered_list: Option<HashSet<String>>
    ) -> Result<Vec<String>> {
        let mut tags_set: HashSet<String> = HashSet::new();
        let mut ret: Vec<String> = Vec::new();

        let filtered_list_ref = filtered_list.as_ref();

        let sports_json: Value = self.http_client
            .get(SPORT_URL)
            .send().await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?
            .json().await?;

        sports_json
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                entry
                    .get("sport")
                    .and_then(|v| v.as_str())
                    .map_or(false, |s| { filtered_list_ref.map_or(true, |set| set.contains(s)) })
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
        let resp_json: Value = self.http_client
            .get(slug_url)
            .send().await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?
            .json().await?;

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
        let response = self.http_client
            .get(format!("{}/{}", MARKET_URL, condition_id))
            .send().await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?;

        response
            .json::<Market>().await
            .map_err(|e| {
                anyhow::anyhow!(
                    PolyfillError::parse(format!("Failed to parse response: {}", e), None)
                )
            })
    }
}

pub struct DataEngine {
    client: Arc<ClobClient>,
    subscribe_tokens: DashSet<String>,
    subscribe_tx: Arc<Mutex<mpsc::UnboundedSender<Token>>>,
    subscribe_rx: Arc<Mutex<mpsc::UnboundedReceiver<Token>>>,
    subscribe_write_stream: DashMap<WssChannelType, Arc<Mutex<SplitSink<WebSocketStream, Value>>>>,
    subscribe_read_stream: DashMap<WssChannelType, Arc<Mutex<SplitStream<WebSocketStream>>>>,
    // mock_subscribe_write_stream: DashMap<WssChannelType, Arc<Mutex<SplitSink<MockStream, Value>>>>,
    // mock_subscribe_read_stream: DashMap<WssChannelType, Arc<Mutex<SplitStream<MockStream>>>>,
    book_manager: OrderBookManager,
}

impl DataEngine {
    pub async fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // let auth = WssAuth {
        //     address: "your_eth_address".to_string(),
        //     signature: "your_signature".to_string(),
        //     timestamp: chrono::Utc::now().timestamp() as u64,
        //     nonce: "random_nonce".to_string(),
        // };

        let channels = [WssChannelType::Crypto, WssChannelType::Sports, WssChannelType::User];
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

        Self {
            client: Arc::new(ClobClient::new_internet("https://clob.polymarket.com")),
            subscribe_tokens: DashSet::new(),
            subscribe_rx: Arc::new(Mutex::new(rx)),
            subscribe_tx: Arc::new(Mutex::new(tx)),
            subscribe_write_stream,
            subscribe_read_stream,
            book_manager: OrderBookManager::new(100),
        }
    }

    fn parse_market(market: Market) -> Vec<Token> {
        market.tokens
            .iter()
            .enumerate()
            .zip(market.outcomes.iter())
            .map(|((i, id), outcome)| Token {
                token_id: id.clone(),
                outcome: outcome.clone(),
                winner: {
                    if i == 0 { true } else { false }
                },
                is_valid: true,
                token_type: TokenType::default(),
            })
            .collect()
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
                        let tokens = DataEngine::parse_market(market);
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
                    println!("recv token: {}", token.token_id);

                    let engine = Arc::clone(&self);

                    // tokio::spawn(async move {
                    //     let auth = WssAuth {
                    //         address: "your_eth_address".to_string(),
                    //         signature: "your_signature".to_string(),
                    //         timestamp: chrono::Utc::now().timestamp() as u64,
                    //         nonce: "random_nonce".to_string(),
                    //     };
                    //     let mut stream = WebSocketStream::new(WEBSOCKET_MARKET_URL);
                    //     stream = stream.with_auth(auth.clone());
                    //     if let Ok(_) = stream
                    //         .subscribe_market_channel(vec![token.token_id.clone()])
                    //         .await
                    //     {
                    //         println!("subscribe token successfully");
                    //     } else {
                    //         println!("fail to subscribe token")
                    //     }
                    //     while let Some(message) = stream.next().await {
                    //         dbg!(message);
                    //     }
                    // });

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
        let token_id = token.token_id.clone();

        if let Ok(_) = self.book_manager.get_book(&token.token_id) {
            return Ok(());
        }

        let chan_type = match token.token_type {
            TokenType::CRYPTO => WssChannelType::Crypto,
            TokenType::SPORTS => WssChannelType::Sports,
        };

        let target_stream = self.subscribe_write_stream.get(&chan_type).map(|r| r.value().clone());

        if let Some(stream_mutex) = target_stream {
            {
                let mut stream = stream_mutex.lock().await;
                let token_subscription = WssSubscription {
                    channel_type: "market".to_string(),
                    asset_ids: vec![token.token_id.to_string()],
                    operation: Some("subscribe".to_string()),
                    auth: None,
                };
                stream.send(serde_json::to_value(token_subscription)?).await?;
            }
            let book = self.book_manager.get_or_create_book(&token_id);
            Ok(())
        } else {
            Err(anyhow!("No stream found for {:?}", chan_type).into())
        }
    }

    async fn handle_crypto_message(&self) -> Result<()> {
        if let Some(crypto_stream) = self.subscribe_read_stream.get_mut(&WssChannelType::Crypto) {
            let mut lock = crypto_stream.lock().await;
            while let Some(message) = lock.next().await {
                dbg!(message.clone());
                match message? {
                    StreamMessage::MarketBookUpdate { data } => {
                        self.book_manager.apply_delta(data)?;
                    }
                    StreamMessage::MarketTrade { data } => {
                        println!("Trade: {} tokens at ${}", data.size, data.price);
                    }
                    StreamMessage::Heartbeat { .. } => {
                        // Connection is alive
                    }
                    _ => {}
                }
            }
            return Ok(());
        } else {
            return Err(anyhow!("No stream found for crypto stream").into());
        }
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
            let ret = engine3.handle_crypto_message().await;
        });
    }

    async fn get_crypto_markets_id(&self) -> Result<Vec<String>> {
        let slugs = DataEngine::generate_crypto_slugs();
        return self.get_crypto_markets_by_slugs(slugs).await;
    }

    async fn get_crypto_markets_by_slugs(&self, slugs: Vec<String>) -> Result<Vec<String>> {
        let futures = slugs.iter().map(|slug| self.get_market_id_by_slug(slug.clone()));
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

    async fn get_specified_tag_ids(
        &mut self,
        filtered_list: Option<HashSet<String>>
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
        let events: Vec<String> = serde_json
            ::from_value(val)
            .map_err(|e| anyhow::anyhow!("here should be string of vec [{}]", e))?;
        println!("{:?}", events);
        Ok(events)
    }
}
