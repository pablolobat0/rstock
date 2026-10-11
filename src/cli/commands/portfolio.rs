use std::io;

use sea_orm::DatabaseConnection;

use crate::cli::adapters;
use crate::cli::{output, ChartPeriod};
use crate::models::{
    AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit, BondDuration, EquityStyle,
    Management, NavHistoryRequest,
};
use crate::services;
use crate::services::market_data::MarketData;

pub async fn get(
    db: &DatabaseConnection,
    market_data: &MarketData,
    period: ChartPeriod,
    output_format: output::OutputFormat,
) -> anyhow::Result<()> {
    let nav_history_request = nav_history_request(period);
    // One complete Portfolio view, including the requested ready NAV history,
    // is obtained before either output Adapter writes anything (ADR-0004).
    // Both dashboard aliases route through this single library dispatch path.
    let view =
        services::portfolio_view::get_portfolio_view(db, market_data, nav_history_request).await?;

    let stdout = io::stdout();
    let mut writer = stdout.lock();
    match output_format {
        output::OutputFormat::Json => adapters::write_portfolio_json(&mut writer, &view),
        output::OutputFormat::Human => {
            adapters::write_portfolio_human(&mut writer, &view, nav_history_request)
        }
    }
}

/// Maps the CLI chart period onto the presentation-neutral history request
/// the Portfolio view composer consumes. Period date calculation and history
/// reads stay inside the composer.
fn nav_history_request(period: ChartPeriod) -> NavHistoryRequest {
    match period {
        ChartPeriod::OneMonth => NavHistoryRequest::OneMonth,
        ChartPeriod::ThreeMonths => NavHistoryRequest::ThreeMonths,
        ChartPeriod::SixMonths => NavHistoryRequest::SixMonths,
        ChartPeriod::Ytd => NavHistoryRequest::Ytd,
        ChartPeriod::OneYear => NavHistoryRequest::OneYear,
        ChartPeriod::ThreeYears => NavHistoryRequest::ThreeYears,
        ChartPeriod::FiveYears => NavHistoryRequest::FiveYears,
        ChartPeriod::All => NavHistoryRequest::All,
    }
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
    output_format: output::OutputFormat,
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
    output_format: output::OutputFormat,
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

#[derive(serde::Serialize)]
struct CreatedAssetOutput<'a> {
    asset_id: i32,
    ticker: &'a str,
}

#[derive(serde::Serialize)]
struct AssetOutput<'a> {
    ticker: &'a str,
}
