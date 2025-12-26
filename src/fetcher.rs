use crate::common::Outcome;
use crate::bot_error::TokenError;
use polyfill_rs::{ ClobClient, PolyfillClient };
use std::sync::Arc;
use std::collections::HashMap;
use serde_json::Value;
use crate::common::{ EVENT_URL, Result };
use polyfill_rs::PolyfillError;
use crate::context::BotContext;
use anyhow;

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
}

impl TokenFetcher {
    pub fn new(ctx: BotContext) -> Self {
        Self {
            context: ctx,
            tokens: Vec::with_capacity(200),
        }
    }

    pub async fn get_live_sports_tokens(&mut self) -> Result<Vec<String>> {
        let params = HashMap::from([
            ("closed".to_string(), "false".to_string()),
            ("live".to_string(), "true".to_string()),
        ]);

        let val = self.context.inner.client.get_events_by_params(params).await?;
        let events: Vec<String> = serde_json
            ::from_value(val)
            .map_err(|e| anyhow::anyhow!("here should be string of vec [{}]", e))?;

        Ok(events)
    }

    async fn get_crypto_tokens(&mut self) {}
}
