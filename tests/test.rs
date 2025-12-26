use polynomial::{ fetcher::{ TokenFetcher }, prelude::BotContext };
#[tokio::test(flavor = "multi_thread")] // Marks the function as a test
async fn test_fetch_tokens() {
    let ctx = BotContext::new();
    let mut token_fetcher = TokenFetcher::new(ctx);
    let start_time = std::time::Instant::now();
    let result = token_fetcher.get_live_sports_tokens().await;
    let duration = start_time.elapsed();
    println!("{:?}", result);
    assert!(result.is_ok());
    let events = result.unwrap();
    assert_ne!(events.len(), 0);
    println!("get_live_sports_tokens spend {:?}", duration);
}
