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
};
use chrono::Datelike;
use polyfill_rs::{ ClobClient };
use tokio::task::JoinSet;
use std::{ collections::HashSet, sync::Arc };
use std::collections::HashMap;
use serde_json::Value;
use polyfill_rs::PolyfillError;
use std::result::Result::Ok;
use anyhow::{ anyhow };
use futures::future;
use tokio::sync::{ mpsc, Mutex };

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
                    .map_or(false, |s| filtered_list_ref.map_or(true, |set| set.contains(s)))
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
            .filter_map(|m: &Value|
                m
                    .get("id")
                    .and_then(|id| id.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            )
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
    subscribe_tokens: Arc<Mutex<HashSet<String>>>,
    subscribe_tx: mpsc::UnboundedSender<Token>,
    subscribe_rx: mpsc::UnboundedReceiver<Token>,
}

impl DataEngine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            client: Arc::new(ClobClient::new_internet("https://clob.polymarket.com")),
            subscribe_tokens: Arc::new(Mutex::new(HashSet::new())),
            subscribe_rx: rx,
            subscribe_tx: tx,
        }
    }

    fn parse_market(market: Market) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(2);
        let mut yes_token = Token::default();
        let mut no_token = Token::default();
        if let Some(yes_id) = market.tokens.get(0) {
            yes_token.token_id = Some(yes_id.clone());
        } else {
            yes_token.is_valid = false;
        }
        if let Some(yes_outcome) = market.outcomes.get(0) {
            yes_token.outcome = Some(yes_outcome.clone());
        } else {
            yes_token.is_valid = false;
        }
        yes_token.winner = true;

        if let Some(no_id) = market.tokens.get(1) {
            no_token.token_id = Some(no_id.clone());
        } else {
            no_token.is_valid = false;
        }
        if let Some(no_outcome) = market.outcomes.get(1) {
            no_token.outcome = Some(no_outcome.clone());
        } else {
            no_token.is_valid = false;
        }
        no_token.winner = true;

        tokens.push(yes_token);
        tokens.push(no_token);
        return tokens;
    }

    pub async fn stream_crypto_tokens(&mut self) {
        let internal = std::time::Duration::from_mins(10);
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
            println!("{:?}", market_ids);
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
                            match self.subscribe_tx.send(token) {
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

    pub async fn receive_crypto_tokens(&mut self) {
        loop {
            if let Some(token) = self.subscribe_rx.recv().await {
                let mut lock = self.subscribe_tokens.lock().await;
                match token.token_id {
                    Some(id) => {
                        if !lock.contains(&id) {
                            lock.insert(id.clone());
                            println!("recv tok: {}", id);
                        }
                    }
                    None => {
                        continue;
                    }
                }
            }
        }
    }

    pub fn start(self) {
        let mut engine = self;
        tokio::spawn(async move {
            tokio::select! {
            _ = engine.stream_crypto_tokens() => {},
            _ = engine.receive_crypto_tokens() => {},
        }
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

    async fn get_crypto_tokens(&mut self) {}
}
