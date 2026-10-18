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
