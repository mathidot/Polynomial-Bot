use std::collections::HashSet;

use polynomial::{ fetcher::{ TokenFetcher }, prelude::BotContext };
#[tokio::test(flavor = "multi_thread")] // Marks the function as a test
async fn test_fetch_tokens() {
    let ctx = BotContext::new();
    let mut token_fetcher = TokenFetcher::new();
    let start_time = std::time::Instant::now();
    let markets = token_fetcher.run_crypto().await;
    println!("{:?}", markets);
    assert!(false);
    let duration = start_time.elapsed();
    println!("time spend {:?}", duration);
}
