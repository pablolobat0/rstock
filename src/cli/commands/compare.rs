use std::io::Write;

use sea_orm::DatabaseConnection;

use crate::constants::{
    FIVE_YEAR_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS, THIRTY_DAYS, THREE_YEAR_DAYS,
};
use crate::models::FundComparisonPeriod;
use crate::services;
use crate::services::market_data::MarketData;

use super::super::display;
use super::super::output::{self, OutputFormat};
use super::super::CorrelationPeriod;

pub async fn funds(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code_a: String,
    code_b: String,
    period: CorrelationPeriod,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    funds_with_writer(
        db,
        market_data,
        code_a,
        code_b,
        period,
        output_format,
        &mut writer,
    )
    .await
}

/// The same command, writing its successful output through the injected writer.
///
/// The JSON envelope is written through the writer, so a caller that captures
/// it observes exactly the production document. The human renderer keeps its
/// existing successful-output behaviour, because the chart library prints the
/// correlation graph to the process stdout.
pub async fn funds_with_writer<W: Write>(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code_a: String,
    code_b: String,
    period: CorrelationPeriod,
    output_format: OutputFormat,
    writer: &mut W,
) -> anyhow::Result<()> {
    let result = services::fund_comparison::compare_funds(
        db,
        market_data,
        &code_a,
        &code_b,
        fund_comparison_period(&period),
    )
    .await?;

    if output_format.is_json() {
        return output::write_json(writer, "compare.funds", &result);
    }

    display::print_fund_comparison(&result);
    Ok(())
}

fn fund_comparison_period(period: &CorrelationPeriod) -> FundComparisonPeriod {
    match period {
        CorrelationPeriod::ThirtyDays => FundComparisonPeriod {
            label: "30D",
            days: THIRTY_DAYS,
        },
        CorrelationPeriod::SixMonths => FundComparisonPeriod {
            label: "6M",
            days: SIX_MONTH_DAYS,
        },
        CorrelationPeriod::OneYear => FundComparisonPeriod {
            label: "1Y",
            days: ONE_YEAR_DAYS,
        },
        CorrelationPeriod::ThreeYears => FundComparisonPeriod {
            label: "3Y",
            days: THREE_YEAR_DAYS,
        },
        CorrelationPeriod::FiveYears => FundComparisonPeriod {
            label: "5Y",
            days: FIVE_YEAR_DAYS,
        },
    }
}
