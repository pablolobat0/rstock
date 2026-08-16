use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::constants::{
    format_date, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_MONTH_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS,
    THREE_MONTH_DAYS, THREE_YEAR_DAYS,
};
use crate::models::{
    AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit, BondDuration, EquityStyle,
    Management,
};
use crate::services;
use crate::services::market_data::MarketData;

use super::super::display;
use super::super::output::{self, OutputFormat};
use super::super::ChartPeriod;

pub async fn get(
    db: &DatabaseConnection,
    market_data: &MarketData,
    period: ChartPeriod,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let mut result = services::portfolio::get_portfolio(db, market_data).await?;

    if output_format.is_json() {
        prepare_json_result(&mut result);
        return output::emit_json("portfolio.get", &result);
    }

    display::print_portfolio(&result);

    if result.rows.is_empty() && !result.monetary_positions.is_empty() {
        return Ok(());
    }

    let today = market_data.today();
    let today_str = format_date(today);

    let (start_date, period_label) = match period {
        ChartPeriod::OneMonth => (today - chrono::Duration::days(ONE_MONTH_DAYS), "1M"),
        ChartPeriod::ThreeMonths => (today - chrono::Duration::days(THREE_MONTH_DAYS), "3M"),
        ChartPeriod::SixMonths => (today - chrono::Duration::days(SIX_MONTH_DAYS), "6M"),
        ChartPeriod::Ytd => {
            let d = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 is always valid");
            (d, "YTD")
        }
        ChartPeriod::OneYear => (today - chrono::Duration::days(ONE_YEAR_DAYS), "1Y"),
        ChartPeriod::ThreeYears => (today - chrono::Duration::days(THREE_YEAR_DAYS), "3Y"),
        ChartPeriod::FiveYears => (today - chrono::Duration::days(FIVE_YEAR_DAYS), "5Y"),
        ChartPeriod::All => match result.inception_date.as_deref() {
            Some(date_str) => {
                let d = NaiveDate::parse_from_str(date_str, DATE_FORMAT)
                    .context("invalid inception date")?;
                (d, "All")
            }
            None => (today, "All"),
        },
    };

    let start_str = format_date(start_date);
    let snapshots = services::nav::get_ready_portfolio_history(db, &start_str, &today_str).await?;
    display::print_nav_chart(&snapshots, period_label);

    Ok(())
}

fn prepare_json_result(result: &mut crate::models::PortfolioResult) {
    result.rows.sort_by(|left, right| {
        right
            .current_value
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&left.current_value.unwrap_or(f64::NEG_INFINITY))
    });
    result.monetary_positions.sort_by(|left, right| {
        right
            .current_value
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&left.current_value.unwrap_or(f64::NEG_INFINITY))
    });
}

#[allow(clippy::too_many_arguments)]
pub async fn asset_add(
    db: &DatabaseConnection,
    ticker: String,
    name: String,
    asset_type: AssetType,
    currency: String,
    asset_class: AssetClass,
    equity_style: Option<EquityStyle>,
    bond_credit: Option<BondCredit>,
    bond_duration: Option<BondDuration>,
    management: Option<Management>,
    morningstar_code: Option<String>,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let info = AssetInfo {
        ticker: ticker.clone(),
        name,
        asset_type,
        currency,
    };
    let classification = AssetClassification {
        asset_class: Some(asset_class),
        equity_style,
        bond_credit,
        bond_duration,
        management,
    };
    let asset_id = services::assets::create_tracked_asset(
        db,
        &info,
        &classification,
        morningstar_code.as_deref(),
    )
    .await?;
    if output_format.is_json() {
        output::emit_json(
            "portfolio.asset.add",
            &CreatedAssetOutput {
                asset_id,
                ticker: &ticker,
            },
        )?;
    } else {
        println!("Added asset {ticker}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn asset_edit(
    db: &DatabaseConnection,
    ticker: String,
    name: Option<String>,
    asset_class: Option<AssetClass>,
    equity_style: Option<EquityStyle>,
    bond_credit: Option<BondCredit>,
    bond_duration: Option<BondDuration>,
    management: Option<Management>,
    morningstar_code: Option<String>,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let classification = AssetClassification {
        asset_class,
        equity_style,
        bond_credit,
        bond_duration,
        management,
    };
    if name.is_none() && morningstar_code.is_none() && classification.is_empty() {
        anyhow::bail!("at least one field must be provided");
    }
    services::assets::update_tracked_asset(
        db,
        &ticker,
        &classification,
        name.as_deref(),
        morningstar_code.as_deref(),
    )
    .await?;
    if output_format.is_json() {
        output::emit_json("portfolio.asset.edit", &AssetOutput { ticker: &ticker })?;
    } else {
        println!("Updated asset {ticker}");
    }
    Ok(())
}

#[derive(Serialize)]
struct CreatedAssetOutput<'a> {
    asset_id: i32,
    ticker: &'a str,
}

#[derive(Serialize)]
struct AssetOutput<'a> {
    ticker: &'a str,
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::{json, Value};

    use crate::cli::output;
    use crate::models::{
        AssetType, CurrentPosition, MarketDataLimitation, MarketDataLimitationClassification,
        MarketDataSubject, PortfolioResult,
    };

    use super::prepare_json_result;

    #[test]
    fn portfolio_json_preserves_domain_values_and_orders_positions() {
        let limitation = MarketDataLimitation {
            subject: MarketDataSubject::Asset {
                ticker: "XFAKE2".to_string(),
                name: "Fake Fund".to_string(),
                asset_type: AssetType::Fund,
            },
            latest_available_date: Some(NaiveDate::from_ymd_opt(2025, 1, 9).unwrap()),
            requested_end_date: NaiveDate::from_ymd_opt(2025, 1, 10).unwrap(),
            classification: MarketDataLimitationClassification::ActionableReportingLag,
        };
        let mut result = PortfolioResult {
            base_currency: "EUR".to_string(),
            rows: vec![
                position("XFAKE1", "USD", 100.0, Vec::new()),
                position("XFAKE2", "GBP", 300.0, vec![limitation.clone()]),
            ],
            monetary_positions: vec![monetary_position("XFAKEM1", Some(200.0))],
            total_monetary_value: Some(200.0),
            total_invested: Some(350.0),
            total_current_value: Some(400.0),
            total_monetary_invested: Some(200.0),
            total_value: Some(600.0),
            total_dividends: Some(0.0),
            total_monetary_dividends: Some(0.0),
            total_open_position_gain_loss: Some(50.0),
            total_open_position_gain_loss_pct: Some(14.29),
            total_monetary_open_position_gain_loss: Some(0.0),
            total_monetary_open_position_gain_loss_pct: Some(0.0),
            snapshot_date: Some("2025-01-09".to_string()),
            nav: Some(110.0),
            daily_change: None,
            daily_change_pct: None,
            inception_date: Some("2024-01-01".to_string()),
            ytd_return: None,
            one_year_return: Some(10.0),
            three_year_return: None,
            five_year_return: None,
            ytd_metrics: None,
            one_year_metrics: None,
            three_year_metrics: None,
            five_year_metrics: None,
            nav_market_data_limitations: vec![limitation],
            current_position_market_data_limitations: Vec::new(),
            monetary_market_data_limitations: Vec::new(),
        };
        result.rows[0].current_price = None;
        result.rows[0].price_date = None;
        result.rows[0].current_value = None;
        result.rows[0].open_position_gain_loss = None;
        result.rows[0].open_position_gain_loss_pct = None;
        result.total_current_value = None;
        result.total_value = None;
        prepare_json_result(&mut result);

        let mut output = Vec::new();
        output::write_json(&mut output, "portfolio.get", &result).unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();

        assert_eq!(value["command"], "portfolio.get");
        assert_eq!(value["data"]["base_currency"], "EUR");
        assert_eq!(value["data"]["positions"][0]["ticker"], "XFAKE2");
        assert_eq!(value["data"]["positions"][1]["ticker"], "XFAKE1");
        assert_eq!(value["data"]["monetary_positions"][0]["ticker"], "XFAKEM1");
        assert_eq!(value["data"]["total_monetary_value"], 200.0);
        assert_eq!(value["data"]["positions"][0]["currency"], "GBP");
        assert!(value["data"]["daily_change"].is_null());
        assert!(value["data"]["positions"][1]["current_price"].is_null());
        assert!(value["data"]["positions"][1]["price_date"].is_null());
        assert!(value["data"]["positions"][1]["current_value"].is_null());
        assert!(value["data"]["total_current_value"].is_null());
        assert!(value["data"]["total_value"].is_null());
        assert_eq!(
            value["data"]["nav_market_data_limitations"][0]["subject"],
            json!({
                "type": "asset",
                "ticker": "XFAKE2",
                "name": "Fake Fund",
                "asset_type": "fund"
            })
        );
        assert_eq!(
            value["data"]["nav_market_data_limitations"][0]["classification"],
            "actionable_reporting_lag"
        );
        assert!(value["data"].get("nav_history").is_none());
        assert!(!String::from_utf8(output).unwrap().contains("\u{1b}["));
    }

    fn position(
        ticker: &str,
        currency: &str,
        current_value: f64,
        market_data_limitations: Vec<MarketDataLimitation>,
    ) -> CurrentPosition {
        CurrentPosition {
            ticker: ticker.to_string(),
            name: format!("{ticker} name"),
            asset_type: AssetType::Fund,
            currency: currency.to_string(),
            morningstar_code: Some("F00000TEST".to_string()),
            asset_class: Some("equity".to_string()),
            equity_style: None,
            management: None,
            total_qty: 1.0,
            avg_cost: Some(100.0),
            current_price: Some(current_value),
            price_date: Some("2025-01-09".to_string()),
            total_invested: Some(100.0),
            current_value: Some(current_value),
            dividends_received: Some(0.0),
            open_position_gain_loss: Some(current_value - 100.0),
            open_position_gain_loss_pct: Some(current_value - 100.0),
            market_data_limitations,
        }
    }

    fn monetary_position(ticker: &str, current_value: Option<f64>) -> CurrentPosition {
        CurrentPosition {
            ticker: ticker.to_string(),
            name: format!("{ticker} name"),
            asset_type: AssetType::Fund,
            currency: "EUR".to_string(),
            morningstar_code: Some("F00000MONEY".to_string()),
            asset_class: Some("monetary".to_string()),
            equity_style: None,
            management: None,
            total_qty: 2.0,
            avg_cost: Some(100.0),
            current_price: current_value.map(|value| value / 2.0),
            price_date: Some("2025-01-09".to_string()),
            total_invested: Some(200.0),
            current_value,
            dividends_received: Some(0.0),
            open_position_gain_loss: current_value.map(|value| value - 200.0),
            open_position_gain_loss_pct: current_value.map(|value| (value - 200.0) / 2.0),
            market_data_limitations: Vec::new(),
        }
    }
}
