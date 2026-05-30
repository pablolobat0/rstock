use std::path::Path;

use anyhow::{bail, Context};
use base64::Engine;
use chrono::{NaiveDate, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::DATE_FORMAT;
use crate::models::{FundData, FundHolding, FundQuoteMetadata};
use crate::settings::Settings;

use super::market_data_sources::sort_and_dedup_observations;
use super::SourceObservation;

pub(super) struct MorningstarAdapter {
    client: Client,
    settings: Settings,
}

#[derive(Deserialize, Serialize)]
struct CachedMorningstarToken {
    value: String,
}

impl MorningstarAdapter {
    pub(super) fn new(settings: Settings) -> Self {
        Self {
            client: Client::builder()
                .user_agent(&settings.user_agent)
                .build()
                .expect("reqwest client configuration should be valid"),
            settings,
        }
    }

    pub(super) async fn price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        let body = self
            .get_with_token_refresh(code, |token| {
                self.client
                    .get(&self.settings.chartservice_url)
                    .bearer_auth(token)
                    .header("accept", "application/json, text/plain, */*")
                    .header("origin", "https://www.morningstar.com")
                    .header("referer", "https://www.morningstar.com/")
                    .query(&[
                        ("query", format!("{code}:nav,totalReturn")),
                        ("frequency", "d".to_owned()),
                        ("startDate", start.format(DATE_FORMAT).to_string()),
                        ("endDate", end.format(DATE_FORMAT).to_string()),
                        ("trackMarketData", "3.6.5".to_owned()),
                        ("instid", "DOTCOM".to_owned()),
                    ])
            })
            .await
            .context("Morningstar chartservice request failed")?;

        let observations = parse_timeseries(&body)?;
        if observations.is_empty() {
            bail!("No NAV data found for '{code}'");
        }
        Ok(observations)
    }

    pub(super) async fn fund_data(&self, code: &str, limit: u32) -> anyhow::Result<FundData> {
        let count = limit.max(200).to_string();
        let body = self
            .client
            .get(format!("{}/{code}/data", self.settings.holdings_url))
            .header("apikey", &self.settings.sal_api_key)
            .header("accept", "application/json")
            .query(&[
                ("premiumNum", count.clone()),
                ("freeNum", count),
                ("hideesg", "false".to_owned()),
                ("locale", "en".to_owned()),
                ("clientId", "MDC".to_owned()),
                ("benchmarkId", "mstarorcat".to_owned()),
                ("version", "4.71.0".to_owned()),
            ])
            .send()
            .await
            .context("failed to send Morningstar sal-service holdings request")?
            .error_for_status()
            .context("Morningstar sal-service holdings request returned unsuccessful HTTP status")?
            .text()
            .await
            .context("failed to read Morningstar sal-service holdings response")?;

        parse_fund_data(&body, limit)
    }

    pub(super) async fn fund_quote_metadata(
        &self,
        code: &str,
    ) -> anyhow::Result<FundQuoteMetadata> {
        let body = self
            .client
            .get(format!("{}/{code}/data", self.settings.quote_url))
            .header("apikey", &self.settings.sal_api_key)
            .header("accept", "application/json")
            .query(&[
                ("locale", "en"),
                ("clientId", "MDC"),
                ("benchmarkId", "mstarorcat"),
                ("version", "4.71.0"),
            ])
            .send()
            .await
            .context("failed to send Morningstar sal-service quote request")?
            .error_for_status()
            .context("Morningstar sal-service quote request returned unsuccessful HTTP status")?
            .text()
            .await
            .context("failed to read Morningstar sal-service quote response")?;

        parse_fund_quote_metadata(&body)
    }

    async fn get_with_token_refresh(
        &self,
        code: &str,
        request: impl Fn(&str) -> reqwest::RequestBuilder,
    ) -> anyhow::Result<String> {
        let token = self.token(code, false).await?;
        let response = request(&token).send().await?;
        let response = if response.status() == StatusCode::UNAUTHORIZED {
            let token = self.token(code, true).await?;
            request(&token).send().await?
        } else {
            response
        };

        response
            .error_for_status()
            .context("Morningstar request returned unsuccessful HTTP status")?
            .text()
            .await
            .context("failed to read Morningstar response")
    }

    async fn token(&self, code: &str, force_refresh: bool) -> anyhow::Result<String> {
        if !force_refresh {
            if let Some(token) = read_cached_token(&self.settings.token_cache_path).await {
                if token_is_current(&token) {
                    return Ok(token.value);
                }
            }
        }

        let page = self
            .client
            .get(&self.settings.token_page_url)
            .query(&[("id", code)])
            .send()
            .await
            .context("failed to fetch Morningstar token page")?
            .error_for_status()
            .context("Morningstar token page request failed")?
            .text()
            .await
            .context("failed to read Morningstar token page")?;

        let token = extract_jwt(&page).context("failed to find Morningstar JWT token")?;
        if let Err(error) = write_cached_token(
            &self.settings.token_cache_path,
            &CachedMorningstarToken {
                value: token.clone(),
            },
        )
        .await
        {
            tracing::warn!(error = %error, "failed to write Morningstar token cache");
        }
        Ok(token)
    }
}

fn parse_timeseries(body: &str) -> anyhow::Result<Vec<SourceObservation>> {
    let payload: Value =
        serde_json::from_str(body).context("failed to parse Morningstar timeseries")?;
    let series = payload
        .as_array()
        .and_then(|values| values.first())
        .and_then(|value| value.get("series"))
        .and_then(Value::as_array)
        .context("Morningstar timeseries response did not contain a series")?;
    let observations = series
        .iter()
        .filter_map(|entry| {
            Some(SourceObservation {
                date: parse_date_value(entry.get("date")?)?,
                value: entry
                    .get("totalReturn")
                    .or_else(|| entry.get("nav"))
                    .and_then(Value::as_f64)?,
            })
        })
        .collect();
    Ok(sort_and_dedup_observations(observations))
}

fn parse_fund_data(body: &str, limit: u32) -> anyhow::Result<FundData> {
    let payload: Value =
        serde_json::from_str(body).context("failed to parse Morningstar fund data")?;
    let mut holdings = Vec::new();
    for page_key in ["equityHoldingPage", "boldHoldingPage", "otherHoldingPage"] {
        if let Some(values) = payload
            .get(page_key)
            .and_then(|page| page.get("holdingList"))
            .and_then(Value::as_array)
        {
            holdings.extend(parse_holdings(values, limit));
        }
    }
    holdings.sort_by(|a, b| b.weighting.total_cmp(&a.weighting));
    holdings.truncate(limit as usize);

    let portfolio_date = payload
        .get("holdingSummary")
        .and_then(|summary| summary.get("portfolioDate"))
        .and_then(Value::as_str)
        .and_then(|date| date.get(..10))
        .map(str::to_owned);

    Ok(FundData {
        fund_currency: string_field(&payload, &["baseCurrencyId"]),
        total_holdings: payload
            .get("numberOfHolding")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        portfolio_date,
        holdings,
    })
}

fn parse_fund_quote_metadata(body: &str) -> anyhow::Result<FundQuoteMetadata> {
    let payload: Value =
        serde_json::from_str(body).context("failed to parse Morningstar fund quote metadata")?;
    Ok(FundQuoteMetadata {
        name: string_field(&payload, &["investmentName"]),
        aum: numeric_field(&payload, &["tNAInShareClassCurrency"]),
        aum_currency: string_field(&payload, &["tNACurrency", "baseCurrencyId"]),
        inception_date: payload
            .get("inceptionDate")
            .and_then(parse_date_value)
            .map(|date| date.format(DATE_FORMAT).to_string()),
        quote_currency: string_field(&payload, &["baseCurrencyId"]),
    })
}

fn parse_holdings(values: &[Value], limit: u32) -> Vec<FundHolding> {
    values
        .iter()
        .filter_map(|value| {
            Some(FundHolding {
                name: string_field(value, &["securityName", "name"])?,
                weighting: numeric_field(value, &["weighting", "weight", "portfolioWeight"])
                    .unwrap_or(0.0),
                ticker: string_field(value, &["ticker", "symbol"]),
                sector: string_field(value, &["sector", "globalSector"]),
                country: string_field(value, &["country", "countryId"]),
                currency: string_field(value, &["currency", "currencyId"]),
            })
        })
        .take(limit as usize)
        .collect()
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name)?.as_str())
        .map(str::to_owned)
}

fn numeric_field(value: &Value, names: &[&str]) -> Option<f64> {
    names.iter().find_map(|name| {
        let value = value.get(*name)?;
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    })
}

fn parse_date_value(value: &Value) -> Option<NaiveDate> {
    match value {
        Value::String(text) => NaiveDate::parse_from_str(text, DATE_FORMAT)
            .ok()
            .or_else(|| {
                text.get(..10)
                    .and_then(|date| NaiveDate::parse_from_str(date, DATE_FORMAT).ok())
            })
            .or_else(|| parse_millis_date(text)),
        Value::Number(number) => number.as_i64().and_then(millis_to_date),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

fn parse_millis_date(text: &str) -> Option<NaiveDate> {
    text.parse::<i64>().ok().and_then(millis_to_date)
}

fn millis_to_date(millis: i64) -> Option<NaiveDate> {
    Utc.timestamp_millis_opt(millis)
        .single()
        .map(|dt| dt.date_naive())
}

fn extract_jwt(page: &str) -> Option<String> {
    page.split(|ch: char| ch == '"' || ch == '\'' || ch.is_whitespace())
        .find(|part| part.starts_with("eyJ") && part.matches('.').count() == 2)
        .map(str::to_owned)
}

fn token_is_current(token: &CachedMorningstarToken) -> bool {
    jwt_expiry(&token.value).is_some_and(|expiry| expiry > Utc::now().timestamp() + 60)
}

fn jwt_expiry(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value.get("exp")?.as_i64()
}

async fn read_cached_token(path: &Path) -> Option<CachedMorningstarToken> {
    let contents = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&contents).ok()
}

async fn write_cached_token(path: &Path, token: &CachedMorningstarToken) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create Morningstar token cache directory")?;
    }
    tokio::fs::write(path, serde_json::to_string(token)?)
        .await
        .context("failed to write Morningstar token cache")
}
