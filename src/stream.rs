//! Async streaming functionality for Polymarket client
//!
//! This module provides high-performance streaming capabilities for
//! real-time market data and order updates.

use crate::errors::{PolyfillError, Result, StreamErrorKind};
use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;
use futures::{
    Sink, SinkExt, Stream, StreamExt,
    stream::{SplitSink, SplitStream},
};
use rand::Rng;
use serde_json::Value;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Sleep, sleep};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

/// Trait for market data streams
#[async_trait]
pub trait MarketStream:
    Stream<Item = Result<StreamMessage>> + Sink<Value, Error = PolyfillError> + Send + Unpin + 'static
{
    /// Subscribe to market data for specific tokens
    fn subscribe(&mut self, subscription: Subscription) -> Result<()>;

    /// Unsubscribe from market data
    fn unsubscribe(&mut self, token_ids: &[String]) -> Result<()>;

    /// Check if the stream is connected
    fn is_connected(&self) -> bool;

    /// Get connection statistics
    fn get_stats(&self) -> StreamStats;

    /// Connect
    async fn connect(&mut self) -> Result<()>;
}

/// Internal commands for the WebSocket actor
#[derive(Debug)]
enum Command {
    Subscribe(WssSubscription),
    Unsubscribe(WssSubscription), // Using WssSubscription to carry the unsubscription payload
    SendMessage(Value),
    Close,
}

/// WebSocket-based market stream implementation
#[derive(Debug)]
pub struct WebSocketStream {
    /// Channel to send commands to the background task
    command_tx: mpsc::UnboundedSender<Command>,
    /// Channel to receive messages from the background task
    message_rx: mpsc::UnboundedReceiver<Result<StreamMessage>>,
    /// Connection statistics (shared with background task)
    stats: Arc<Mutex<StreamStats>>,
    /// URL for the WebSocket connection
    url: String,
    /// Authentication credentials
    auth: Option<WssAuth>,
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
    pub connected: bool,
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
            max_retries: 100, // Increased retries for robustness
            base_delay: std::time::Duration::from_secs(1),
            max_delay: std::time::Duration::from_secs(60),
            backoff_multiplier: 1.5,
        }
    }
}

impl WebSocketStream {
    /// Create a new WebSocket stream
    pub fn new(url: &str) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        let stats = Arc::new(Mutex::new(StreamStats {
            messages_received: 0,
            messages_sent: 0,
            errors: 0,
            last_message_time: None,
            connection_uptime: std::time::Duration::ZERO,
            reconnect_count: 0,
            connected: false,
        }));

        let stream = Self {
            command_tx,
            message_rx,
            stats: stats.clone(),
            url: url.to_string(),
            auth: None,
        };

        // Spawn background connection manager
        let url_clone = url.to_string();
        tokio::spawn(async move {
            Self::connection_manager(url_clone, command_rx, message_tx, stats).await;
        });

        stream
    }

    /// Background task to manage WebSocket connection
    async fn connection_manager(
        url: String,
        mut command_rx: mpsc::UnboundedReceiver<Command>,
        message_tx: mpsc::UnboundedSender<Result<StreamMessage>>,
        stats: Arc<Mutex<StreamStats>>,
    ) {
        let mut subscriptions: Vec<WssSubscription> = Vec::new();
        let reconnect_config = ReconnectConfig::default();
        let mut retry_count = 0;
        let mut current_delay = reconnect_config.base_delay;

        'connection_loop: loop {
            // 1. Attempt to connect
            info!("Connecting to WebSocket at {}...", url);

            let connect_result = tokio_tungstenite::connect_async(&url).await;

            let (mut ws_stream, _) = match connect_result {
                Ok((ws, resp)) => {
                    info!("Connected to WebSocket. Response: {:?}", resp);
                    {
                        let mut stats_lock = stats.lock().unwrap();
                        stats_lock.connected = true;
                        stats_lock.reconnect_count += 1;
                    }
                    retry_count = 0;
                    current_delay = reconnect_config.base_delay;
                    (ws, resp)
                }
                Err(e) => {
                    error!("WebSocket connection failed: {}", e);
                    {
                        let mut stats_lock = stats.lock().unwrap();
                        stats_lock.errors += 1;
                        stats_lock.connected = false;
                    }

                    // Handle commands while disconnected (to queue subscriptions)
                    // We check if there are pending commands, but don't block indefinitely
                    // We only process Subscribe/Unsubscribe/Close. SendMessage might be dropped or queued.
                    // For now, let's just sleep and retry, but check for Close command.

                    tokio::select! {
                        cmd = command_rx.recv() => {
                            match cmd {
                                Some(Command::Subscribe(sub)) => subscriptions.push(sub),
                                Some(Command::Unsubscribe(sub)) => {
                                    // Remove matching subscriptions
                                    subscriptions.retain(|s| s.channel_type != sub.channel_type || s.asset_ids != sub.asset_ids);
                                }
                                Some(Command::Close) => break 'connection_loop,
                                _ => {} // Drop other messages while disconnected
                            }
                        }
                        _ = sleep(current_delay) => {
                            // Backoff
                            current_delay = std::cmp::min(
                                current_delay.mul_f64(reconnect_config.backoff_multiplier),
                                reconnect_config.max_delay,
                            );
                        }
                    }
                    continue 'connection_loop;
                }
            };

            // 2. Resubscribe
            for sub in &subscriptions {
                info!("Resubscribing to {}", sub.channel_type);
                match serde_json::to_string(sub) {
                    Ok(text) => {
                        if let Err(e) = ws_stream.send(Message::Text(text.into())).await {
                            error!("Failed to resend subscription: {}", e);
                            // If sending fails immediately, break to outer loop to reconnect
                            continue 'connection_loop;
                        }
                    }
                    Err(e) => error!("Failed to serialize subscription: {}", e),
                }
            }

            // 3. Main processing loop
            loop {
                tokio::select! {
                    // Handle incoming WebSocket messages
                    msg = ws_stream.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                debug!("Received WebSocket message: {}", text);
                                {
                                    let mut stats_lock = stats.lock().unwrap();
                                    stats_lock.messages_received += 1;
                                    stats_lock.last_message_time = Some(Utc::now());
                                }

                                match Self::parse_polymarket_message(&text) {
                                    Ok(parsed_msgs) => {
                                        for m in parsed_msgs {
                                            if let Err(_) = message_tx.send(Ok(m)) {
                                                break 'connection_loop; // Receiver dropped
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse message: {}", e);
                                        // Don't disconnect on parse error
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(data))) => {
                                if let Err(e) = ws_stream.send(Message::Pong(data)).await {
                                    error!("Failed to send Pong: {}", e);
                                    break 'connection_loop;
                                }
                            }
                            Some(Ok(Message::Pong(_))) => {
                                debug!("Received Pong");
                            }
                            Some(Ok(Message::Close(_))) => {
                                info!("WebSocket closed by server");
                                break 'connection_loop;
                            }
                            Some(Err(e)) => {
                                error!("WebSocket stream error: {}", e);
                                break 'connection_loop;
                            }
                            None => {
                                info!("WebSocket stream ended");
                                break 'connection_loop;
                            }
                            _ => {} // Ignore Binary/Frame
                        }
                    }

                    // Handle outgoing commands
                    cmd = command_rx.recv() => {
                        match cmd {
                            Some(Command::Subscribe(sub)) => {
                                subscriptions.push(sub.clone());
                                if let Ok(text) = serde_json::to_string(&sub) {
                                    if let Err(e) = ws_stream.send(Message::Text(text.into())).await {
                                        error!("Failed to send subscription: {}", e);
                                        break 'connection_loop;
                                    }
                                }
                            }
                            Some(Command::Unsubscribe(sub)) => {
                                // Remove from local list
                                subscriptions.retain(|s| s.channel_type != sub.channel_type || s.asset_ids != sub.asset_ids);
                                // Send unsubscribe message
                                if let Ok(text) = serde_json::to_string(&sub) {
                                    if let Err(e) = ws_stream.send(Message::Text(text.into())).await {
                                        error!("Failed to send unsubscription: {}", e);
                                        break 'connection_loop;
                                    }
                                }
                            }
                            Some(Command::SendMessage(val)) => {
                                if let Ok(text) = serde_json::to_string(&val) {
                                    if let Err(e) = ws_stream.send(Message::Text(text.into())).await {
                                        error!("Failed to send message: {}", e);
                                        break 'connection_loop;
                                    }
                                    let mut stats_lock = stats.lock().unwrap();
                                    stats_lock.messages_sent += 1;
                                }
                            }
                            Some(Command::Close) => {
                                info!("Closing WebSocket connection manager");
                                let _ = ws_stream.close(None).await;
                                break 'connection_loop;
                            }
                            None => {
                                // Command channel closed
                                break 'connection_loop;
                            }
                        }
                    }
                }
            }

            // Connection lost, mark as disconnected
            {
                let mut stats_lock = stats.lock().unwrap();
                stats_lock.connected = false;
            }
            // Add a small delay before reconnecting to prevent tight loops
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// Parse Polymarket WebSocket message format
    fn parse_polymarket_message(text: &str) -> Result<Vec<StreamMessage>> {
        let value: Value = serde_json::from_str(text).map_err(|e| {
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

    /// Set authentication credentials
    pub fn with_auth(mut self, auth: WssAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub async fn init_and_split(
        self,
    ) -> Result<(
        SplitSink<WebSocketStream, Value>,
        SplitStream<WebSocketStream>,
    )> {
        // self.connect().await?; // Connect is handled by manager
        let (sink, stream) = self.split();
        Ok((sink, stream))
    }

    /// Subscribe to market data using official Polymarket WebSocket API
    pub async fn subscribe_async(&mut self, subscription: WssSubscription) -> Result<()> {
        self.command_tx
            .send(Command::Subscribe(subscription.clone()))
            .map_err(|e| {
                PolyfillError::stream(
                    format!("Failed to send subscribe command: {}", e),
                    StreamErrorKind::ConnectionFailed,
                )
            })?;
        info!("Subscribed to {} channel", subscription.channel_type);
        Ok(())
    }

    /// Subscribe to user channel (orders and trades)
    pub async fn subscribe_user_channel(&mut self, _markets: Vec<String>) -> Result<()> {
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

        self.command_tx
            .send(Command::Unsubscribe(subscription))
            .map_err(|e| {
                PolyfillError::stream(
                    format!("Failed to send unsubscribe command: {}", e),
                    StreamErrorKind::ConnectionFailed,
                )
            })?;
        Ok(())
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

        self.command_tx
            .send(Command::Unsubscribe(subscription))
            .map_err(|e| {
                PolyfillError::stream(
                    format!("Failed to send unsubscribe command: {}", e),
                    StreamErrorKind::ConnectionFailed,
                )
            })?;
        Ok(())
    }
}

impl Stream for WebSocketStream {
    type Item = Result<StreamMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.message_rx.poll_recv(cx)
    }
}

impl Sink<Value> for WebSocketStream {
    type Error = PolyfillError;
    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        // We are always ready to accept commands to the unbounded channel
        // If the manager is dead, start_send will catch it.
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Value) -> std::result::Result<(), Self::Error> {
        // Check if this is a subscription message to handle it properly
        // This is a bit of a hack because Sink<Value> is generic
        // Ideally we should have Sink<Command> or similar
        // But the current architecture uses Sink<Value>

        // Try to parse as subscription to see if we should treat it as Command::Subscribe
        if let Ok(sub) = serde_json::from_value::<WssSubscription>(item.clone()) {
            // Check operation type
            if let Some(op) = &sub.operation {
                if op == "subscribe" {
                    self.command_tx.send(Command::Subscribe(sub)).map_err(|e| {
                        PolyfillError::stream(
                            format!("Failed to send subscribe command: {}", e),
                            StreamErrorKind::ConnectionFailed,
                        )
                    })?;
                    return Ok(());
                } else if op == "unsubscribe" {
                    self.command_tx
                        .send(Command::Unsubscribe(sub))
                        .map_err(|e| {
                            PolyfillError::stream(
                                format!("Failed to send unsubscribe command: {}", e),
                                StreamErrorKind::ConnectionFailed,
                            )
                        })?;
                    return Ok(());
                }
            }
        }

        // Default: Send as raw message
        self.command_tx
            .send(Command::SendMessage(item))
            .map_err(|e| {
                PolyfillError::stream(
                    format!("Failed to send message command: {}", e),
                    StreamErrorKind::ConnectionFailed,
                )
            })?;
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
        let _ = self.command_tx.send(Command::Close);
        Poll::Ready(Ok(()))
    }
}

#[async_trait]
impl MarketStream for WebSocketStream {
    fn subscribe(&mut self, subscription: Subscription) -> Result<()> {
        // Convert Subscription (old type?) to WssSubscription if possible, or just use subscribe_async logic
        // The trait uses `Subscription` but the implementation uses `WssSubscription`
        // Looking at `types.rs`, `Subscription` might be an alias or different struct.
        // Let's check imports.
        // `use crate::types::*;`
        // Based on original code:
        // fn subscribe(&mut self, _subscription: Subscription) -> Result<()> { Ok(()) }
        // It was a no-op for backward compatibility.
        // I'll keep it as no-op or implement it if I can convert.
        Ok(())
    }

    fn unsubscribe(&mut self, _token_ids: &[String]) -> Result<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.stats.lock().unwrap().connected
    }

    fn get_stats(&self) -> StreamStats {
        self.stats.lock().unwrap().clone()
    }

    async fn connect(&mut self) -> Result<()> {
        // Manager handles connection
        Ok(())
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

                    let random_millis = rand::rng().random_range(1..10);
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

#[async_trait]
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
            connected: self.connected,
        }
    }

    async fn connect(&mut self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl MarketStream for Box<dyn MarketStream> {
    /// Subscribe to market data for specific tokens
    fn subscribe(&mut self, subscription: Subscription) -> Result<()> {
        (**self).subscribe(subscription)
    }

    /// Unsubscribe from market data
    fn unsubscribe(&mut self, token_ids: &[String]) -> Result<()> {
        (**self).unsubscribe(token_ids)
    }

    /// Check if the stream is connected
    fn is_connected(&self) -> bool {
        (**self).is_connected()
    }

    /// Get connection statistics
    fn get_stats(&self) -> StreamStats {
        (**self).get_stats()
    }

    /// Connect
    async fn connect(&mut self) -> Result<()> {
        (**self).connect().await
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
    async fn test_mock_stream_multithreaded() {}

    #[test]
    fn test_stream_manager() {
        let mut manager = StreamManager::new();
        let mock_stream = Box::new(MockStream::new(4096));
        manager.add_stream(mock_stream);
    }

    // Tests requiring network access are commented out or modified to be safe
    // #[tokio::test]
    // async fn test_tokio_stream_token_message() -> Result<()> { ... }

    #[tokio::test]
    async fn test_mock_stream() {
        let stream = MockStream::new(10_0000);
        let mut msgs: Vec<StreamMessage> = Vec::new();
        let (_, mut r) = stream.split();

        for _ in 1..=1000 {
            if let Some(Ok(msg)) = r.next().await {
                // println!("{:?}", msg.clone());
                msgs.push(msg);
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        assert_eq!(msgs.len(), 10_0000);
    }
}
