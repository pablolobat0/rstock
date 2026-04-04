use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, FIVE_YEAR_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS, THIRTY_DAYS, THREE_YEAR_DAYS,
};
use crate::services;
use crate::services::price::PriceFetcher;

use super::super::display;
use super::super::{AnalysisTarget, CorrelationPeriod};

pub async fn run(
    db: &DatabaseConnection,
    fetcher: &dyn PriceFetcher,
    target: AnalysisTarget,
    period: CorrelationPeriod,
) -> anyhow::Result<()> {
    match target {
        AnalysisTarget::Portfolio => {
            let today = chrono::Local::now().date_naive();
            let today_str = format_date(today);

            let (start_date, period_label) = match period {
                CorrelationPeriod::ThirtyDays => {
                    (today - chrono::Duration::days(THIRTY_DAYS), "30D")
                }
                CorrelationPeriod::SixMonths => {
                    (today - chrono::Duration::days(SIX_MONTH_DAYS), "6M")
                }
                CorrelationPeriod::OneYear => (today - chrono::Duration::days(ONE_YEAR_DAYS), "1Y"),
                CorrelationPeriod::ThreeYears => {
                    (today - chrono::Duration::days(THREE_YEAR_DAYS), "3Y")
                }
                CorrelationPeriod::FiveYears => {
                    (today - chrono::Duration::days(FIVE_YEAR_DAYS), "5Y")
                }
            };

            let start_str = format_date(start_date);
            services::portfolio::trigger_rebuild_if_needed(db, fetcher).await?;
            let matrix =
                services::metrics::compute_correlation_matrix(db, &start_str, &today_str, fetcher)
                    .await?;

            display::print_correlation_matrix(&matrix, period_label);
            Ok(())
        }
    }
}
