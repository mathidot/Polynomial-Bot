use polyfill_rs::ClobClient;
mod common;
use common::Result;
#[tokio::main]
async fn main() -> Result<()> {
    let client = ClobClient::new("https://clob.polymarket.com");
    Ok(())
}
