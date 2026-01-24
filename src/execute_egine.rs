use crate::types::{Config, TokenInfo};
use std::sync::Arc;
use tokio::sync::mpsc;
pub struct ExecuteEgine {
    pub notify_channel: mpsc::Receiver<TokenInfo>,
    pub config: Config,
}

impl ExecuteEgine {
    pub fn new(rx: mpsc::Receiver<TokenInfo>, cfg: Config) -> Self {
        Self {
            notify_channel: rx,
            config: cfg,
        }
    }

    pub async fn tick(&mut self) {
        while let Some(token_info) = self.notify_channel.recv().await {
            if token_info.price >= self.config.enter_price {}
        }
    }
}
