use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, FIVE_YEAR_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS, THIRTY_DAYS, THREE_YEAR_DAYS,
};
use crate::models::CandidateCorrelationPeriod;
use crate::services;
use crate::services::market_data::MarketData;

use super::super::display;
use super::super::CorrelationPeriod;

pub async fn fund(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code: String,
    period: CorrelationPeriod,
) -> anyhow::Result<()> {
    let candidate_period = candidate_correlation_period(&period);
    let result =
        services::fund_analysis::compute_fund_analysis(db, market_data, &code, candidate_period)
            .await?;
    display::print_fund_analysis(&result);
    Ok(())
}

pub async fn composition(db: &DatabaseConnection, market_data: &MarketData) -> anyhow::Result<()> {
    services::portfolio::trigger_rebuild_if_needed(db, market_data).await?;
    let result = services::composition::compute_composition(db, market_data).await?;
    display::print_composition(&result);
    Ok(())
}

pub async fn correlation_matrix(
    db: &DatabaseConnection,
    market_data: &MarketData,
    period: CorrelationPeriod,
) -> anyhow::Result<()> {
    let (start_str, today_str, period_label) = correlation_date_range(&period);

    services::portfolio::trigger_rebuild_if_needed(db, market_data).await?;
    let matrix =
        services::analytics::compute_correlation_data(db, &start_str, &today_str, market_data)
            .await?;

    display::print_correlation_matrix(&matrix, period_label);
    Ok(())
}

pub async fn rolling_correlation(
    db: &DatabaseConnection,
    market_data: &MarketData,
    identifier_a: String,
    identifier_b: String,
    period: CorrelationPeriod,
) -> anyhow::Result<()> {
    let (start_str, today_str, period_label) = correlation_date_range(&period);

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

    display::print_rolling_correlation(&result);
    Ok(())
}

pub(crate) fn correlation_date_range(period: &CorrelationPeriod) -> (String, String, &'static str) {
    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);

    let (start_date, period_label) = match period {
        CorrelationPeriod::ThirtyDays => (today - chrono::Duration::days(THIRTY_DAYS), "30D"),
        CorrelationPeriod::SixMonths => (today - chrono::Duration::days(SIX_MONTH_DAYS), "6M"),
        CorrelationPeriod::OneYear => (today - chrono::Duration::days(ONE_YEAR_DAYS), "1Y"),
        CorrelationPeriod::ThreeYears => (today - chrono::Duration::days(THREE_YEAR_DAYS), "3Y"),
        CorrelationPeriod::FiveYears => (today - chrono::Duration::days(FIVE_YEAR_DAYS), "5Y"),
    };

    let start_str = format_date(start_date);
    (start_str, today_str, period_label)
}

fn candidate_correlation_period(period: &CorrelationPeriod) -> CandidateCorrelationPeriod {
    let days = match period {
        CorrelationPeriod::ThirtyDays => THIRTY_DAYS,
        CorrelationPeriod::SixMonths => SIX_MONTH_DAYS,
        CorrelationPeriod::OneYear => ONE_YEAR_DAYS,
        CorrelationPeriod::ThreeYears => THREE_YEAR_DAYS,
        CorrelationPeriod::FiveYears => FIVE_YEAR_DAYS,
    };
    let (_, _, label) = correlation_date_range(period);
    CandidateCorrelationPeriod { label, days }
}
