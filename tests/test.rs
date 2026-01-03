use std::collections::HashSet;

use polynomial::{ fetcher::{ TokenFetcher }, prelude::BotContext };
#[tokio::test(flavor = "multi_thread")] // Marks the function as a test
async fn test_fetch_tokens() {
    let ctx = BotContext::new();
    let mut token_fetcher = TokenFetcher::new(ctx);
    let start_time = std::time::Instant::now();
    // let result = token_fetcher.get_live_sports_tokens().await;
    let mut sports_tags = HashSet::new();
    sports_tags.insert("nba".to_string());
    let tags = token_fetcher.get_specified_tag_ids(Some(sports_tags)).await;
    let duration = start_time.elapsed();
    println!("{:?}", tags);
    assert!(tags.is_ok());
    assert!(false);
    // assert!(result.is_ok());
    // let events = result.unwrap();
    // assert_ne!(events.len(), 0);
    println!("get_live_sports_tokens spend {:?}", duration);
}
