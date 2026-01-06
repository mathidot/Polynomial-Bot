use crate::common::{ MARKET_URL, Outcome, SLUG_URL, SPORT_URL };
use crate::bot_error::TokenError;
use chrono::Datelike;
use polyfill_rs::decode::deserializers;
use polyfill_rs::math::spread_pct;
use polyfill_rs::{ ClobClient, PolyfillClient };
use tokio::task::JoinSet;
use std::{ collections::HashSet, sync::Arc };
use std::collections::HashMap;
use serde_json::Value;
use crate::common::{ EVENT_URL, Result };
use crate::common::CRYPTO_PATTERNS;
use polyfill_rs::PolyfillError;
use crate::context::BotContext;
use std::result::Result::Ok;
use anyhow::{ anyhow };
use futures::future;
use polyfill_rs::types::{ Token, Market };

trait TokenApi {
    async fn get_events_by_params(&self, params: HashMap<String, String>) -> Result<Value>;
    async fn get_specified_tag_ids(
        &self,
        filtered_list: Option<HashSet<String>>
    ) -> Result<Vec<String>>;

    async fn get_market_id_by_slug(&self, event_slug: String) -> Result<Vec<String>>;
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
}

pub struct TokenFetcher {
    client: Arc<ClobClient>,
    tokens: HashSet<String>,
}

impl TokenFetcher {
    pub fn new() -> Self {
        Self {
            client: Arc::new(ClobClient::new_internet("https://clob.polymarket.com")),
            tokens: HashSet::new(),
        }
    }

    pub async fn run_crypto(&mut self) -> Result<Vec<Market>> {
        let market_ids = self.get_crypto_markets_id().await?;
        let mut set = JoinSet::new();
        for id in market_ids {
            let client = self.client.clone();
            set.spawn(async move { client.get_market(&id).await });
        }
        let mut markets: Vec<Market> = Vec::with_capacity(512);
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(market)) => markets.push(market),
                Ok(Err(e)) => eprintln!("API 请求失败: {:?}", e), // 捕获业务错误
                Err(e) => eprintln!("任务执行崩溃: {:?}", e), // 捕获 JoinError (如 panic)
            }
        }
        Ok(markets)
    }

    async fn get_crypto_markets_id(&self) -> Result<Vec<String>> {
        let slugs = TokenFetcher::generate_crypto_slugs();
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
