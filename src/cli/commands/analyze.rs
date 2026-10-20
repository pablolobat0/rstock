use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, FIVE_YEAR_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS, THIRTY_DAYS, THREE_YEAR_DAYS,
};
use crate::models::CandidateCorrelationPeriod;
use crate::services;
use crate::services::benchmark_references::FundCandidateIdentity;
use crate::services::market_data::MarketData;

use super::super::display;
use super::super::output::{self, OutputFormat};
use super::super::CorrelationPeriod;

pub async fn fund(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code: String,
    period: CorrelationPeriod,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    fund_with_writer(
        db,
        market_data,
        code,
        period,
        None,
        output_format,
        &mut std::io::stdout().lock(),
    )
    .await
}

/// The same command, writing its successful output through the injected
/// writer.
///
/// The JSON envelope is written through the writer, so a caller that captures
/// it observes exactly the production document. The human renderer keeps its
/// existing successful-output behaviour, because the chart library prints to
/// the process stdout.
pub async fn fund_with_writer<W: std::io::Write>(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code: String,
    period: CorrelationPeriod,
    candidate: Option<&FundCandidateIdentity>,
    output_format: OutputFormat,
    writer: &mut W,
) -> anyhow::Result<()> {
    let candidate_period = candidate_correlation_period(&period);
    let result = services::fund_analysis::compute_fund_analysis_with_candidate(
        db,
        market_data,
        &code,
        candidate_period,
        candidate,
    )
    .await?;

    if output_format.is_json() {
        return output::write_json(writer, "analyze.fund", &result);
    }

    display::print_fund_analysis(&result);
    Ok(())
}

pub async fn composition(
    db: &DatabaseConnection,
    market_data: &MarketData,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let result = services::composition::compute_composition(db, market_data).await?;

    if output_format.is_json() {
        return output::emit_json("analyze.composition", &result);
    }

    display::print_composition(&result);
    Ok(())
}

pub async fn correlation_matrix(
    db: &DatabaseConnection,
    market_data: &MarketData,
    period: CorrelationPeriod,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let (start_str, today_str, period_label) = correlation_date_range(&period, market_data);

    let matrix =
        services::analytics::compute_correlation_data(db, &start_str, &today_str, market_data)
            .await?;

    if output_format.is_json() {
        return output::emit_json("analyze.correlation.matrix", &matrix);
    }

    display::print_correlation_matrix(&matrix, period_label);
    Ok(())
}

pub async fn rolling_correlation(
    db: &DatabaseConnection,
    market_data: &MarketData,
    identifier_a: String,
    identifier_b: String,
    period: CorrelationPeriod,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let (start_str, today_str, period_label) = correlation_date_range(&period, market_data);

    let result = services::analytics::compute_rolling_correlation_data(
        db,
        &start_str,
        &today_str,
        &identifier_a,
        &identifier_b,
        period_label,
        market_data,
    )
    .await?;

    if output_format.is_json() {
        return output::emit_json("analyze.correlation.rolling", &result);
    }

    display::print_rolling_correlation(&result);
    Ok(())
}

pub(crate) fn correlation_date_range(
    period: &CorrelationPeriod,
    market_data: &MarketData,
) -> (String, String, &'static str) {
    let today = market_data.today();
    let today_str = format_date(today);

    let (days, period_label) = correlation_period_metadata(period);
    let start_date = today - chrono::Duration::days(days);

    let start_str = format_date(start_date);
    (start_str, today_str, period_label)
}

fn correlation_period_metadata(period: &CorrelationPeriod) -> (i64, &'static str) {
    match period {
        CorrelationPeriod::ThirtyDays => (THIRTY_DAYS, "30D"),
        CorrelationPeriod::SixMonths => (SIX_MONTH_DAYS, "6M"),
        CorrelationPeriod::OneYear => (ONE_YEAR_DAYS, "1Y"),
        CorrelationPeriod::ThreeYears => (THREE_YEAR_DAYS, "3Y"),
        CorrelationPeriod::FiveYears => (FIVE_YEAR_DAYS, "5Y"),
    }
}

fn candidate_correlation_period(period: &CorrelationPeriod) -> CandidateCorrelationPeriod {
    let (days, label) = correlation_period_metadata(period);
    CandidateCorrelationPeriod { label, days }
}
