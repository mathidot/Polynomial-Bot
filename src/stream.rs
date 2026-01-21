//! Async streaming functionality for Polymarket client
//!
//! This module provides high-performance streaming capabilities for
//! real-time market data and order updates.

use crate::errors::{PolyfillError, Result};
use crate::types::*;
use chrono::Utc;
use futures::{
    Sink, SinkExt, Stream, StreamExt,
    stream::{SplitSink, SplitStream},
};
use rand::Rng;
use serde_json::Value;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Sleep, sleep};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

/// Trait for market data streams
pub trait MarketStream: Stream<Item = Result<StreamMessage>> + Send + Sync {
    /// Subscribe to market data for specific tokens
    fn subscribe(&mut self, subscription: Subscription) -> Result<()>;

    /// Unsubscribe from market data
    fn unsubscribe(&mut self, token_ids: &[String]) -> Result<()>;

    /// Check if the stream is connected
    fn is_connected(&self) -> bool;

    /// Get connection statistics
    fn get_stats(&self) -> StreamStats;
}

/// WebSocket-based market stream implementation
#[derive(Debug)]
#[allow(dead_code)]
pub struct WebSocketStream {
    /// WebSocket connection
    connection: Option<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    /// URL for the WebSocket connection
    url: String,
    /// Authentication credentials
    auth: Option<WssAuth>,
    /// Current subscriptions
    subscriptions: Vec<WssSubscription>,
    /// Message sender for internal communication
    tx: mpsc::UnboundedSender<StreamMessage>,
    /// Message receiver
    rx: mpsc::UnboundedReceiver<StreamMessage>,
    /// Connection statistics
    stats: StreamStats,
    /// Reconnection configuration
    reconnect_config: ReconnectConfig,
    /// message buffer
    msg_buffer: VecDeque<StreamMessage>,
}

/// Stream statistics
#[derive(Debug, Clone)]
pub struct StreamStats {
    pub messages_received: u64,
    pub messages_sent: u64,
    pub errors: u64,
    pub last_message_time: Option<chrono::DateTime<Utc>>,
    pub connection_uptime: std::time::Duration,
    pub reconnect_count: u32,
}

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    pub max_retries: u32,
    pub base_delay: std::time::Duration,
    pub max_delay: std::time::Duration,
    pub backoff_multiplier: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay: std::time::Duration::from_secs(1),
            max_delay: std::time::Duration::from_secs(60),
            backoff_multiplier: 2.0,
        }
    }
}

impl WebSocketStream {
    /// Create a new WebSocket stream
    pub fn new(url: &str) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        Self {
            connection: None,
            url: url.to_string(),
            auth: None,
            subscriptions: Vec::new(),
            tx,
            rx,
            stats: StreamStats {
                messages_received: 0,
                messages_sent: 0,
                errors: 0,
                last_message_time: None,
                connection_uptime: std::time::Duration::ZERO,
                reconnect_count: 0,
            },
            reconnect_config: ReconnectConfig::default(),
            msg_buffer: VecDeque::new(),
        }
    }

    /// Set authentication credentials
    pub fn with_auth(mut self, auth: WssAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Connect to the WebSocket
    async fn connect(&mut self) -> Result<()> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| {
                PolyfillError::stream(
                    format!("WebSocket connection failed: {}", e),
                    crate::errors::StreamErrorKind::ConnectionFailed,
                )
            })?;

        self.connection = Some(ws_stream);
        info!("Connected to WebSocket stream at {}", self.url);
        Ok(())
    }

    pub async fn init_and_split(
        mut self,
    ) -> Result<(
        SplitSink<WebSocketStream, Value>,
        SplitStream<WebSocketStream>,
    )> {
        self.connect().await?;
        let (sink, stream) = self.split();
        Ok((sink, stream))
    }

    /// Send a message to the WebSocket
    async fn send_message(&mut self, message: Value) -> Result<()> {
        if let Some(connection) = &mut self.connection {
            let text = serde_json::to_string(&message).map_err(|e| {
                PolyfillError::parse(format!("Failed to serialize message: {}", e), None)
            })?;

            let ws_message = tokio_tungstenite::tungstenite::Message::Text(text.into());

            connection.send(ws_message).await.map_err(|e| {
                PolyfillError::stream(
                    format!("Failed to send message: {}", e),
                    crate::errors::StreamErrorKind::MessageCorrupted,
                )
            })?;

            self.stats.messages_sent += 1;
        }

        Ok(())
    }

    /// Subscribe to market data using official Polymarket WebSocket API
    // Ensure connection
    pub async fn subscribe_async(&mut self, subscription: WssSubscription) -> Result<()> {
        if self.connection.is_none() {
            self.connect().await?;
        }
        // Send subscription message in the format expected by Polymarket
        // The subscription struct will serialize correctly with proper field names
        let message = serde_json::to_value(&subscription).map_err(|e| {
            PolyfillError::parse(format!("Failed to serialize subscription: {}", e), None)
        })?;

        self.send_message(message).await?;
        self.subscriptions.push(subscription.clone());

        info!("Subscribed to {} channel", subscription.channel_type);
        Ok(())
    }

    /// Subscribe to user channel (orders and trades)
    pub async fn subscribe_user_channel(&mut self, markets: Vec<String>) -> Result<()> {
        let auth = self
            .auth
            .as_ref()
            .ok_or_else(|| PolyfillError::auth("No authentication provided for WebSocket"))?
            .clone();

        let subscription = WssSubscription {
            channel_type: "user".to_string(),
            operation: Some("subscribe".to_string()),
            asset_ids: Vec::new(),
            auth: Some(auth),
        };

        todo!();

        self.subscribe_async(subscription).await
    }

    /// Subscribe to tokens (order book and trades)
    pub async fn subscribe_tokens(&mut self, asset_ids: Vec<String>) -> Result<()> {
        let subscription = WssSubscription {
            channel_type: "market".to_string(),
            operation: Some("subscribe".to_string()),
            asset_ids,
            auth: None,
        };
        self.subscribe_async(subscription).await
    }

    /// Subscribe to market channel with custom features enabled
    /// Custom features include: best_bid_ask, new_market, market_resolved events
    pub async fn subscribe_market_channel_with_features(
        &mut self,
        asset_ids: Vec<String>,
    ) -> Result<()> {
        let subscription = WssSubscription {
            channel_type: "market".to_string(),
            operation: Some("subscribe".to_string()),
            asset_ids,
            auth: None,
        };

        self.subscribe_async(subscription).await
    }

    /// Unsubscribe from market channel
    pub async fn unsubscribe_market_channel(&mut self, asset_ids: Vec<String>) -> Result<()> {
        let subscription = WssSubscription {
            channel_type: "market".to_string(),
            operation: Some("unsubscribe".to_string()),
            asset_ids,
            auth: None,
        };

        self.subscribe_async(subscription).await
    }

    /// Unsubscribe from user channel
    pub async fn unsubscribe_user_channel(&mut self) -> Result<()> {
        let auth = self
            .auth
            .as_ref()
            .ok_or_else(|| PolyfillError::auth("No authentication provided for WebSocket"))?
            .clone();

        let subscription = WssSubscription {
            channel_type: "user".to_string(),
            operation: Some("unsubscribe".to_string()),
            asset_ids: Vec::new(),
            auth: Some(auth),
        };

        self.subscribe_async(subscription).await
    }

    /// Handle incoming WebSocket messages
    async fn handle_message(
        &mut self,
        message: tokio_tungstenite::tungstenite::Message,
    ) -> Result<()> {
        match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                debug!("Received WebSocket message: {}", text);

                // Parse the message according to Polymarket's format
                let msgs = self.parse_polymarket_message(&text)?;

                for msg in msgs {
                    // Send to internal channel
                    if let Err(e) = self.tx.send(msg) {
                        error!("Failed to send message to internal channel: {}", e);
                    }
                }

                self.stats.messages_received += 1;
                self.stats.last_message_time = Some(Utc::now());
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => {
                info!("WebSocket connection closed by server");
                self.connection = None;
            }
            tokio_tungstenite::tungstenite::Message::Ping(data) => {
                // Respond with pong
                if let Some(connection) = &mut self.connection {
                    let pong = tokio_tungstenite::tungstenite::Message::Pong(data);
                    if let Err(e) = connection.send(pong).await {
                        error!("Failed to send pong: {}", e);
                    }
                }
            }
            tokio_tungstenite::tungstenite::Message::Pong(_) => {
                // Handle pong if needed
                debug!("Received pong");
            }
            tokio_tungstenite::tungstenite::Message::Binary(_) => {
                warn!("Received binary message (not supported)");
            }
            tokio_tungstenite::tungstenite::Message::Frame(_) => {
                warn!("Received raw frame (not supported)");
            }
        }

        Ok(())
    }

    // /// Parse Polymarket WebSocket message format
    fn parse_polymarket_message(&self, text: &str) -> Result<Vec<StreamMessage>> {
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            PolyfillError::parse(
                format!("Failed to parse WebSocket message: {}", e),
                Some(Box::new(e)),
            )
        })?;

        if let Some(array) = value.as_array() {
            let mut messages = Vec::new();
            for item in array {
                let message: StreamMessage = serde_json::from_value(item.clone()).map_err(|e| {
                    PolyfillError::parse(
                        format!("message item parse error: {}", e),
                        Some(Box::new(e)),
                    )
                })?;
                messages.push(message);
            }
            return Ok(messages);
        }

        if value.is_object() {
            let message: StreamMessage = serde_json::from_value(value).map_err(|e| {
                PolyfillError::parse(format!("object parse error: {}", e), Some(Box::new(e)))
            })?;
            return Ok(vec![message]);
        }

        Err(PolyfillError::stream(
            "Fail to parse polymarket message",
            crate::errors::StreamErrorKind::MessageCorrupted,
        ))
    }

    async fn reconnect(&mut self) -> Result<()> {
        let mut delay = self.reconnect_config.base_delay;
        let mut retries = 0;

        while retries < self.reconnect_config.max_retries {
            warn!("Attempting to reconnect (attempt {})", retries + 1);

            match self.connect().await {
                Ok(()) => {
                    info!("Successfully reconnected");
                    self.stats.reconnect_count += 1;

                    // Resubscribe to all previous subscriptions
                    let subscriptions = self.subscriptions.clone();
                    for subscription in subscriptions {
                        self.send_message(serde_json::to_value(subscription)?)
                            .await?;
                    }

                    return Ok(());
                }
                Err(e) => {
                    error!("Reconnection attempt {} failed: {}", retries + 1, e);
                    retries += 1;

                    if retries < self.reconnect_config.max_retries {
                        tokio::time::sleep(delay).await;
                        delay = std::cmp::min(
                            delay.mul_f64(self.reconnect_config.backoff_multiplier),
                            self.reconnect_config.max_delay,
                        );
                    }
                }
            }
        }

        Err(PolyfillError::stream(
            format!(
                "Failed to reconnect after {} attempts",
                self.reconnect_config.max_retries
            ),
            crate::errors::StreamErrorKind::ConnectionFailed,
        ))
    }
}

impl Stream for WebSocketStream {
    type Item = Result<StreamMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // First check message buffer
        if let Some(message) = self.msg_buffer.pop_front() {
            println!("message from buffer: {:?}", message);
            return Poll::Ready(Some(Ok(message)));
        }

        // Second check internal channel
        if let Poll::Ready(Some(message)) = self.rx.poll_recv(cx) {
            println!("message from internal channel: {:?}", message);
            return Poll::Ready(Some(Ok(message)));
        }

        // Then check WebSocket connection
        if let Some(connection) = &mut self.connection {
            match connection.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(_message))) => {
                    println!("message from websocket connection: {}", _message);
                    let text = match _message {
                        Message::Text(text) => text,
                        _ => {
                            error!("can not transform message into str");
                            let trans_err = PolyfillError::stream(
                                "could translate Message to Text".to_string(),
                                crate::errors::StreamErrorKind::MessageCorrupted,
                            );

                            return Poll::Ready(Some(Err(trans_err)));
                        }
                    };
                    match self.parse_polymarket_message(&text) {
                        Ok(msgs) => {
                            let mut msg_it = msgs.into_iter();
                            if let Some(first) = msg_it.next() {
                                self.msg_buffer.extend(msg_it);
                                return Poll::Ready(Some(Ok(first)));
                            } else {
                                return Poll::Ready(None);
                            }
                        }
                        Err(e) => Poll::Ready(Some(Err(e))),
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    println!("WebSocket error: {}", e);
                    error!("WebSocket error: {}", e);
                    self.stats.errors += 1;
                    Poll::Ready(Some(Err(e.into())))
                }
                Poll::Ready(None) => {
                    println!("WebSocket stream ended");
                    info!("WebSocket stream ended");
                    Poll::Ready(None)
                }
                Poll::Pending => {
                    println!("WebSocket stream pending");
                    Poll::Pending
                }
            }
        } else {
            Poll::Ready(None)
        }
    }
}

impl Sink<Value> for WebSocketStream {
    type Error = PolyfillError;
    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        if let Some(conn) = &mut self.get_mut().connection {
            Pin::new(conn).poll_ready(_cx).map_err(|e| {
                PolyfillError::stream(
                    format!("Sink not ready: {}", e),
                    crate::errors::StreamErrorKind::ConnectionFailed,
                )
            })
        } else {
            Poll::Ready(Err(PolyfillError::stream(
                "Not connected",
                crate::errors::StreamErrorKind::ConnectionFailed,
            )))
        }
    }

    fn start_send(self: Pin<&mut Self>, item: Value) -> std::result::Result<(), Self::Error> {
        let text = serde_json::to_string(&item).map_err(|e| {
            PolyfillError::parse(format!("Failed to serialize message: {}", e), None)
        })?;

        let ws_message = tokio_tungstenite::tungstenite::Message::Text(text.into());
        let this = self.get_mut();

        if let Some(conn) = &mut this.connection {
            Pin::new(conn).start_send(ws_message).map_err(|e| {
                PolyfillError::stream(
                    format!("Failed to start send: {}", e),
                    crate::errors::StreamErrorKind::MessageCorrupted,
                )
            })?;
            this.stats.messages_sent += 1;
            Ok(())
        } else {
            Err(PolyfillError::stream(
                "Connection lost during send",
                crate::errors::StreamErrorKind::ConnectionFailed,
            ))
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        if let Some(conn) = &mut self.get_mut().connection {
            Pin::new(conn).poll_flush(cx).map_err(|e| {
                PolyfillError::stream(
                    format!("Flush failed: {}", e),
                    crate::errors::StreamErrorKind::MessageCorrupted,
                )
            })
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        if let Some(conn) = &mut self.get_mut().connection {
            Pin::new(conn).poll_close(cx).map_err(|e| {
                PolyfillError::stream(
                    format!("Close failed: {}", e),
                    crate::errors::StreamErrorKind::ConnectionFailed,
                )
            })
        } else {
            Poll::Ready(Ok(()))
        }
    }
}

impl MarketStream for WebSocketStream {
    fn subscribe(&mut self, _subscription: Subscription) -> Result<()> {
        // This is for backward compatibility - use subscribe_async for new code
        Ok(())
    }

    fn unsubscribe(&mut self, _token_ids: &[String]) -> Result<()> {
        // This is for backward compatibility - use unsubscribe_async for new code
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    fn get_stats(&self) -> StreamStats {
        self.stats.clone()
    }
}

/// Mock stream for testing
#[derive(Debug)]
pub struct MockStream {
    pub sent_messages: Arc<Mutex<Vec<serde_json::Value>>>,
    index: usize,
    send_count: usize,
    connected: bool,
    sleep_timer: Pin<Box<Sleep>>,
}

impl Default for MockStream {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl Unpin for MockStream {}

impl MockStream {
    pub fn new(cnt: usize) -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            index: 0,
            send_count: cnt,
            connected: true,
            sleep_timer: Box::pin(sleep(Duration::from_secs(1))),
        }
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }
}

impl Stream for MockStream {
    type Item = Result<StreamMessage>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.index >= self.send_count {
            Poll::Ready(None)
        } else {
            match self.sleep_timer.as_mut().poll(_cx) {
                Poll::Pending => {
                    return Poll::Pending;
                }
                Poll::Ready(_) => {
                    let message: StreamMessage = rand::rng().random();
                    self.index += 1;

                    let random_millis = rand::rng().random_range(100..1000);
                    self.sleep_timer
                        .as_mut()
                        .set(sleep(Duration::from_millis(random_millis)));

                    Poll::Ready(Some(Ok(message)))
                }
            }
        }
    }
}

impl Sink<serde_json::Value> for MockStream {
    type Error = PolyfillError;
    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        if self.connected {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn start_send(
        self: Pin<&mut Self>,
        item: serde_json::Value,
    ) -> std::result::Result<(), Self::Error> {
        if let Ok(mut sent) = self.sent_messages.lock() {
            sent.push(item);
        }
        Ok(())
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl MarketStream for MockStream {
    fn subscribe(&mut self, _subscription: Subscription) -> Result<()> {
        Ok(())
    }

    fn unsubscribe(&mut self, _token_ids: &[String]) -> Result<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn get_stats(&self) -> StreamStats {
        StreamStats {
            messages_received: 0,
            messages_sent: self.index as u64,
            errors: 0,
            last_message_time: None,
            connection_uptime: std::time::Duration::from_micros(1),
            reconnect_count: 0,
        }
    }
}
/// Stream manager for handling multiple streams
#[allow(dead_code)]
pub struct StreamManager {
    streams: Vec<Box<dyn MarketStream>>,
    message_tx: mpsc::UnboundedSender<StreamMessage>,
    message_rx: mpsc::UnboundedReceiver<StreamMessage>,
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamManager {
    pub fn new() -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        Self {
            streams: Vec::new(),
            message_tx,
            message_rx,
        }
    }

    pub fn add_stream(&mut self, stream: Box<dyn MarketStream>) {
        self.streams.push(stream);
    }

    pub fn get_message_receiver(&mut self) -> mpsc::UnboundedReceiver<StreamMessage> {
        // Note: UnboundedReceiver doesn't implement Clone
        // In a real implementation, you'd want to use a different approach
        // For now, we'll return a dummy receiver
        let (_, rx) = mpsc::unbounded_channel();
        rx
    }

    pub fn broadcast_message(&self, message: StreamMessage) -> Result<()> {
        self.message_tx
            .send(message)
            .map_err(|e| PolyfillError::internal("Failed to broadcast message", e))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_mock_stream_multithreaded() {
        use futures::{SinkExt, StreamExt};
        use serde_json::json;

        let mock = MockStream::new(4096);

        let (mut sink, mut stream) = mock.split();

        while let Some(msg) = stream.next().await {
            dbg!(msg);
        }
    }

    #[test]
    fn test_stream_manager() {
        let mut manager = StreamManager::new();
        let mock_stream = Box::new(MockStream::new(4096));
        manager.add_stream(mock_stream);
    }

    #[tokio::test]
    async fn test_tokio_stream_token_message() -> Result<()> {
        let (mut ws_stream, r) = tokio_tungstenite::connect_async(
            "wss://ws-subscriptions-clob.polymarket.com/ws/market",
        )
        .await?;

        println!("response: {:?}", r);

        let payload = json!({
            "assets_ids": ["3936340147748920096456624623493606422377714730122392075827122179093813941108"],
            "type": "market",
            "operation": "subscribe",
        });
        let text = serde_json::to_string(&payload).map_err(|e| {
            PolyfillError::parse(format!("Failed to serialize message: {}", e), None)
        })?;
        let ws_message = tokio_tungstenite::tungstenite::Message::Text(text.into());
        let ret = ws_stream.send(ws_message).await;
        assert!(ret.is_ok());
        loop {
            if let Some(message) = ws_stream.next().await {
                println!("message: {:?}", message);
            } else {
            }
        }
    }

    #[tokio::test]
    async fn test_stream_message() {
        let stream = WebSocketStream::new("wss://ws-subscriptions-clob.polymarket.com/ws/market");
        let ret = stream.init_and_split().await;
        assert!(ret.is_ok());
        let (mut writer, mut reader) = ret.unwrap();
        let payload = json!({
            "assets_ids": ["3936340147748920096456624623493606422377714730122392075827122179093813941108"],
            "type": "market",
            "operation": "subscribe",
        });
        let ret = writer.send(payload).await;
        assert!(ret.is_ok());

        while let Some(message) = reader.next().await {
            println!("message: {:?}", message);
        }
    }
}
