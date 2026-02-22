use anyhow::{Context, bail};
use yfinance_rs::{Ticker, YfClient};
use yfinance_rs::core::conversions::money_to_f64;

pub async fn get_last_price(ticker: &str) -> anyhow::Result<f64> {
    let client = YfClient::default();
    let tk = Ticker::new(&client, ticker);
    let quote = tk.quote().await.context("failed to fetch quote")?;
    let price = quote
        .price
        .as_ref()
        .context("no price available")?;
    let value = money_to_f64(price);
    if value <= 0.0 {
        bail!("invalid price for {ticker}: {value}");
    }
    Ok(value)
}
