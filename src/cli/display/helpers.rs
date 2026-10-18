use colored::Colorize;
use std::io::Write;
use tabled::settings::Color;

/// Converts a formatted number string to EU format: `,` as decimal separator,
/// `.` as thousands separator. Preserves leading sign (+/-) and trailing suffixes (%, K, B, T).
/// Numbers with decimals are displayed with 2 digits; numbers without stay as-is.
pub(crate) fn format_eu(s: &str) -> String {
    let (sign, rest) = if let Some(stripped) = s.strip_prefix('-') {
        ("-", stripped)
    } else if let Some(stripped) = s.strip_prefix('+') {
        ("+", stripped)
    } else {
        ("", s)
    };

    // Separate trailing non-numeric suffix (e.g. %, K, B, T, M)
    let suffix_start = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(rest.len());
    let num = &rest[..suffix_start];
    let suffix = &rest[suffix_start..];

    let (integer, decimal) = match num.split_once('.') {
        Some((int, dec)) => (int, Some(dec)),
        None => (num, None),
    };

    // Add thousands separator (.)
    let mut reversed = String::new();
    for (i, c) in integer.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            reversed.push('.');
        }
        reversed.push(c);
    }
    let integer_formatted: String = reversed.chars().rev().collect();

    let mut output = format!("{sign}{integer_formatted}");
    if let Some(dec) = decimal {
        output.push(',');
        output.push_str(dec);
    }
    output.push_str(suffix);
    output
}

pub(crate) fn color_value(value: f64, formatted: &str) -> String {
    if value >= 0.0 {
        formatted.green().to_string()
    } else {
        formatted.red().to_string()
    }
}

pub(super) fn format_pct(v: Option<f64>) -> String {
    match v {
        Some(val) => format_eu(&format!("{val:.2}%")),
        None => "N/A".to_string(),
    }
}

pub(super) fn format_plain(v: Option<f64>) -> String {
    match v {
        Some(val) => format_eu(&format!("{val:.2}")),
        None => "N/A".to_string(),
    }
}

pub(super) fn format_return_plain(r: Option<f64>) -> String {
    match r {
        Some(v) => {
            let sign = if v >= 0.0 { "+" } else { "" };
            format_eu(&format!("{sign}{v:.2}%"))
        }
        None => "N/A".to_string(),
    }
}

pub(in crate::cli) fn color_for_value(v: f64) -> Color {
    if v >= 0.0 {
        Color::FG_GREEN
    } else {
        Color::FG_RED
    }
}

pub(crate) fn write_benchmark_reference_context<W: Write>(
    writer: &mut W,
    reference: &crate::models::BenchmarkReferenceInfo,
    limitations: &[crate::models::MarketDataLimitation],
) -> anyhow::Result<()> {
    for line in benchmark_reference_context_lines(reference, limitations) {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

pub(crate) fn write_fund_benchmark_periods<W: Write>(
    writer: &mut W,
    fund_name: &str,
    comparison: &crate::models::FundBenchmarkComparison,
) -> anyhow::Result<()> {
    use crate::constants::display_date;
    use crate::models::ComparisonAvailability;

    match &comparison.availability {
        ComparisonAvailability::Available => {
            writeln!(writer, "{fund_name} assigned primary comparison: available")?;
        }
        ComparisonAvailability::Unavailable { reason } => {
            writeln!(
                writer,
                "{fund_name} assigned primary comparison: unavailable ({reason})"
            )?;
        }
        ComparisonAvailability::NotApplicable { reason } => {
            writeln!(
                writer,
                "{fund_name} assigned primary comparison: not applicable ({reason})"
            )?;
        }
    }
    if let Some(reference_name) = &comparison.reference_name {
        writeln!(
            writer,
            "{} primary reference: {} [{}]; currency {}; basis {}; adjustment {}; hedge {}; cadence {}; source {} {}; provenance {}",
            fund_name,
            reference_name,
            comparison.reference_id.as_deref().unwrap_or("unknown id"),
            comparison.currency,
            comparison.return_basis.as_deref().unwrap_or("unknown"),
            comparison.adjustment.as_deref().unwrap_or("unknown"),
            comparison.hedge_policy.as_deref().unwrap_or("unknown"),
            comparison.cadence.as_deref().unwrap_or("unknown"),
            comparison.source.as_deref().unwrap_or("unknown"),
            comparison.source_identifier.as_deref().unwrap_or("unknown"),
            comparison.source_provenance.as_deref().unwrap_or("unknown"),
        )?;
    }
    writeln!(writer, "  Period | Requested start; actual anchor – end | Fund return | Reference return | Excess | Beta | Correlation | Tracking error (% annualized) | Information ratio")?;
    for period in &comparison.standard_periods {
        let dates = match (&period.start_date, &period.end_date) {
            (Some(start), Some(end)) => format!(
                "{}; {} – {}",
                period
                    .requested_start_date
                    .as_deref()
                    .map_or_else(|| "N/A".to_owned(), display_date),
                display_date(start),
                display_date(end)
            ),
            _ => format!(
                "{}; actual anchor/end N/A",
                period
                    .requested_start_date
                    .as_deref()
                    .map_or_else(|| "N/A".to_owned(), display_date)
            ),
        };
        writeln!(
            writer,
            "  {} | {} | {} | {} | {} | {} | {} | {} | {}",
            period.period.label(),
            dates,
            format_comparison_fact(&period.fund_return_pct, |value| format_pct(Some(value))),
            format_comparison_fact(&period.reference_return_pct, |value| format_pct(Some(
                value
            ))),
            format_comparison_fact(&period.excess_return_percentage_points, |value| {
                format_eu(&format!("{value:+.2} pp"))
            }),
            format_comparison_fact(&period.beta, |value| format_plain(Some(value))),
            format_comparison_fact(&period.correlation, |value| format_plain(Some(value))),
            format_comparison_fact(&period.tracking_error_annualized_pct, |value| {
                format_pct(Some(value))
            }),
            format_comparison_fact(&period.information_ratio, |value| format_plain(Some(value))),
        )?;
    }
    Ok(())
}

pub(crate) fn write_risk_free_assumption<W: Write>(
    writer: &mut W,
    assumption: &crate::models::RiskFreeAssumption,
) -> anyhow::Result<()> {
    writeln!(
        writer,
        "Risk-free assumption: {:.2}% annual for {}; not a substitute for {} and not applied to {}.",
        assumption.annual_rate_pct,
        assumption.applies_to.join("/"),
        assumption.not_applied_to.first().map_or("€STR", String::as_str),
        assumption
            .not_applied_to
            .get(1)
            .map_or("retained cash", String::as_str),
    )?;
    Ok(())
}

fn format_comparison_fact(
    fact: &crate::models::ComparisonFact<f64>,
    format_value: impl FnOnce(f64) -> String,
) -> String {
    use crate::models::ComparisonFact;

    match fact {
        ComparisonFact::Available { value } => format_value(*value),
        ComparisonFact::Unavailable { reason } => format!("Unavailable ({reason})"),
        ComparisonFact::NotApplicable { reason } => format!("N/A ({reason})"),
    }
}

pub(crate) fn benchmark_reference_context_lines(
    reference: &crate::models::BenchmarkReferenceInfo,
    limitations: &[crate::models::MarketDataLimitation],
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Beta reference: {} [{}]",
            reference.display_name, reference.id
        ),
        format!(
            "  Source: {} {} ({})",
            reference.source, reference.source_identifier, reference.currency
        ),
        format!("  Adjustment: {}", reference.adjustment_provenance),
        format!("  Return basis: {}", reference.return_basis_qualification),
    ];
    lines.extend(
        limitations
            .iter()
            .map(format_market_data_limitation_warning),
    );
    lines
}

/// The user-facing Market data limitation wording shared by the Portfolio
/// view human Adapter and the correlation and composition renderers.
pub(crate) fn format_market_data_limitation_warning(
    limitation: &crate::models::MarketDataLimitation,
) -> String {
    use crate::constants::{display_date, format_date};
    use crate::models::MarketDataSubject;

    let requested_end_date = display_date(&format_date(limitation.requested_end_date));
    let latest_available_date = limitation
        .latest_available_date
        .map(|date| display_date(&format_date(date)));

    match (&limitation.subject, latest_available_date) {
        (
            MarketDataSubject::Asset {
                ticker,
                name,
                asset_type,
            },
            None,
        ) => format!(
            "Market data limitation: {asset_type} {ticker} ({name}) has no available price through {requested_end_date}."
        ),
        (
            MarketDataSubject::Asset {
                ticker,
                name,
                asset_type,
            },
            Some(latest_available_date),
        ) => format!(
            "Market data limitation: {asset_type} {ticker} ({name}) has latest price from {latest_available_date}; requested through {requested_end_date}."
        ),
        (MarketDataSubject::BenchmarkReference { id, display_name }, None) => format!(
            "Market data limitation: benchmark reference {display_name} [{id}] has no available price through {requested_end_date}."
        ),
        (
            MarketDataSubject::BenchmarkReference { id, display_name },
            Some(latest_available_date),
        ) => format!(
            "Market data limitation: benchmark reference {display_name} [{id}] has latest price from {latest_available_date}; requested through {requested_end_date}."
        ),
        (MarketDataSubject::FxRate { currency }, None) => format!(
            "Market data limitation: FX rate {currency} has no available rate through {requested_end_date}."
        ),
        (MarketDataSubject::FxRate { currency }, Some(latest_available_date)) => format!(
            "Market data limitation: FX rate {currency} has latest rate from {latest_available_date}; requested through {requested_end_date}."
        ),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/benchmark_reference_context_tests.rs"]
mod tests;
