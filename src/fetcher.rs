use crate::common::{ Outcome, SPORT_URL };
use crate::bot_error::TokenError;
use polyfill_rs::math::spread_pct;
use polyfill_rs::{ ClobClient, PolyfillClient };
use std::{ collections::HashSet, sync::Arc };
use std::collections::HashMap;
use serde_json::Value;
use crate::common::{ EVENT_URL, Result };
use polyfill_rs::PolyfillError;
use crate::context::BotContext;
use anyhow::{ self, Error, Ok };

#[derive(Debug, Clone, Hash)]
pub struct Token {
    market_name: String,
    token_id: u64,
    outcome: Outcome,
}

impl Token {
    fn new(name: String, id: u64, o: Outcome) -> Self {
        Self {
            market_name: name,
            token_id: id,
            outcome: o,
        }
    }
}

pub struct TokenFetcher {
    context: BotContext,
    tokens: Vec<Token>,
}

trait TokenApi {
    async fn get_events_by_params(&self, params: HashMap<String, String>) -> Result<Value>;
    async fn get_specified_tag_ids(
        &self,
        filtered_list: Option<HashSet<String>>
    ) -> Result<Vec<String>>;
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
        let tags_set_ref = &mut tags_set;
        let filtered_list_ref = filtered_list.as_ref();
        let mut ret: Vec<String> = Vec::new();
        self.http_client
            .get(SPORT_URL)
            .send().await
            .map_err(|e| PolyfillError::network(format!("Request failed: {}", e), e))?
            .json::<Value>().await?
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                let sport_tag = entry.get("sport");
                sport_tag.map_or(false, |tag| {
                    let tag_str = tag.as_str();
                    tag_str.map_or(false, |s| filtered_list_ref.map_or(true, |set| set.contains(s)))
                })
            })
            .filter_map(|entry| entry.get("tags")?.as_str())
            .flat_map(|s| s.split(','))
            .map(|s| s.trim())
            .filter(|s| { !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) })
            .for_each(|s| {
                if tags_set_ref.insert(s.to_string()) {
                    ret.push(s.to_string());
                }
            });
        return Ok(ret);
    }
}

impl TokenFetcher {
    pub fn new(ctx: BotContext) -> Self {
        Self {
            context: ctx,
            tokens: Vec::with_capacity(200),
        }
    }

    pub async fn get_specified_tag_ids(
        &mut self,
        filtered_list: Option<HashSet<String>>
    ) -> Result<Vec<String>> {
        let res = self.context.inner.client.get_specified_tag_ids(filtered_list).await;
        return res;
    }

    pub async fn get_live_sports_tokens(&mut self) -> Result<Vec<String>> {
        let params = HashMap::from([
            ("closed".to_string(), "false".to_string()),
            ("active".to_string(), "true".to_string()),
        ]);

        let val = self.context.inner.client.get_events_by_params(params).await?;
        let events: Vec<String> = serde_json
            ::from_value(val)
            .map_err(|e| anyhow::anyhow!("here should be string of vec [{}]", e))?;
        println!("{:?}", events);
        Ok(events)
    }

    async fn get_crypto_tokens(&mut self) {}
}
