use anyhow::Context;
use chrono::{NaiveDate, TimeZone, Utc};
use yfinance_rs::core::conversions::money_to_f64;
use yfinance_rs::history::HistoryBuilder;
use yfinance_rs::profile::{self, Profile};
use yfinance_rs::ticker::Ticker;
use yfinance_rs::YfClient;

use crate::models::StockInfo;

use super::market_data_sources::sort_and_dedup_observations;
use super::SourceObservation;

pub(super) struct YahooFinanceAdapter;

impl YahooFinanceAdapter {
    pub(super) async fn price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        let start_dt =
            Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).expect("valid HMS constant"));
        let end_dt =
            Utc.from_utc_datetime(&end.and_hms_opt(23, 59, 59).expect("valid HMS constant"));

        let client = YfClient::default();
        let candles = HistoryBuilder::new(&client, ticker)
            .between(start_dt, end_dt)
            .fetch()
            .await
            .context(format!("failed to fetch historical prices for {ticker}"))?;

        Ok(sort_and_dedup_observations(
            candles
                .iter()
                .map(|candle| SourceObservation {
                    date: candle.ts.date_naive(),
                    value: money_to_f64(&candle.close),
                })
                .collect(),
        ))
    }

    pub(super) async fn exchange_rate_history(
        &self,
        from: &str,
        to: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.price_history(&format!("{from}{to}=X"), start, end)
            .await
    }

    pub(super) async fn stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo> {
        let client = YfClient::default();
        let t = Ticker::new(&client, ticker);
        let info = t
            .info()
            .await
            .context(format!("failed to fetch info for {ticker}"))?;

        let profile_result = profile::load_profile(&client, ticker).await.ok();
        let (sector, country, name_from_profile) = match &profile_result {
            Some(Profile::Company(c)) => (
                c.sector.clone(),
                c.address.as_ref().and_then(|a| a.country.clone()),
                Some(c.name.clone()),
            ),
            Some(Profile::Fund(f)) => (None, None, Some(f.name.clone())),
            None => (None, None, None),
        };

        Ok(StockInfo {
            name: info.name.clone().or(name_from_profile),
            market_cap: info.market_cap.as_ref().map(money_to_f64),
            sector,
            country,
        })
    }
}
