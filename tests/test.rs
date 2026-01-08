use std::collections::HashSet;

use polynomial::{ fetcher::{ DataEngine }, prelude::BotContext };
#[tokio::test(flavor = "multi_thread")] // Marks the function as a test
async fn test_fetch_tokens() {
    let mut token_fetcher = DataEngine::new();
    let start_time = std::time::Instant::now();
    assert!(false);
    let duration = start_time.elapsed();
    println!("time spend {:?}", duration);
}
