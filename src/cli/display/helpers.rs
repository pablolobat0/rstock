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

/// Rewrites the storage dates embedded in a machine reason to the
/// user-facing display format. Machine output keeps the stored ISO value.
pub(crate) fn display_reason(reason: &str) -> String {
    let bytes = reason.as_bytes();
    let mut display = String::with_capacity(reason.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some(date) = iso_date_at(reason, index) {
            display.push_str(&crate::constants::display_date(date));
            index += date.len();
        } else {
            let character = reason[index..]
                .chars()
                .next()
                .expect("a non-empty remainder has a first character");
            display.push(character);
            index += character.len_utf8();
        }
    }
    display
}

/// The complete `YYYY-MM-DD` date at `index`, when the text holds one that is
/// not part of a longer number.
fn iso_date_at(reason: &str, index: usize) -> Option<&str> {
    let bytes = reason.as_bytes();
    if index + 10 > bytes.len() {
        return None;
    }
    let window = &bytes[index..index + 10];
    let is_digit = |byte: u8| byte.is_ascii_digit();
    let shaped = is_digit(window[0])
        && is_digit(window[1])
        && is_digit(window[2])
        && is_digit(window[3])
        && window[4] == b'-'
        && is_digit(window[5])
        && is_digit(window[6])
        && window[7] == b'-'
        && is_digit(window[8])
        && is_digit(window[9]);
    if !shaped {
        return None;
    }
    if index > 0 && bytes[index - 1].is_ascii_digit() {
        return None;
    }
    if bytes.get(index + 10).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    Some(&reason[index..index + 10])
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
    use crate::models::ComparisonAvailability;

    match &comparison.availability {
        ComparisonAvailability::Available => {
            writeln!(writer, "{fund_name} assigned primary comparison: available")?;
        }
        ComparisonAvailability::Unavailable { reason } => {
            writeln!(
                writer,
                "{fund_name} assigned primary comparison: unavailable ({})",
                display_reason(reason)
            )?;
        }
        ComparisonAvailability::NotApplicable { reason } => {
            writeln!(
                writer,
                "{fund_name} assigned primary comparison: not applicable ({})",
                display_reason(reason)
            )?;
        }
    }
    if let Some(reference_name) = &comparison.reference_name {
        writeln!(
            writer,
            "{} primary reference: {} [{}]; native currency {}; wealth currency {}; basis {}; adjustment {}; hedge {}; cadence {}; source {} {}; provenance {}",
            fund_name,
            reference_name,
            comparison.reference_id.as_deref().unwrap_or("unknown id"),
            comparison.native_currency.as_deref().unwrap_or("unknown"),
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
    if let Some(purpose) = &comparison.purpose {
        write_reference_purpose(writer, purpose)?;
    }
    write_comparison_dates(writer, comparison)?;
    write_benchmark_period_rows(
        writer,
        "Fund",
        &comparison
            .standard_periods
            .iter()
            .map(crate::models::BenchmarkPeriod::from)
            .collect::<Vec<_>>(),
    )
}

/// The purpose of one selected reference definition.
///
/// A rendered comparison carries the meaning of its reference, not only an
/// identifier, so the investment universe it describes and the official or
/// objective comparator it records stay inspectable.
pub(crate) fn write_reference_purpose<W: Write>(
    writer: &mut W,
    purpose: &crate::models::ReferencePurposeInfo,
) -> anyhow::Result<()> {
    writeln!(
        writer,
        "  Reference purpose: role {}; universe {}; instrument {}; verification {}",
        purpose.role,
        purpose.universe,
        display_optional_text(Some(purpose.instrument_ticker.as_str()), "unknown/absent"),
        purpose.verification_state,
    )?;
    writeln!(
        writer,
        "  Reference history boundary: {}",
        purpose.history_boundary
    )?;
    writeln!(
        writer,
        "  Official comparator: {}; objective comparator: {}",
        display_optional_text(purpose.official_comparator.as_deref(), "unknown/absent"),
        display_optional_text(purpose.objective_comparator.as_deref(), "unknown/absent"),
    )?;
    Ok(())
}

/// The actual comparison dates and the cumulative facts of one comparison.
pub(crate) fn write_comparison_dates<W: Write>(
    writer: &mut W,
    comparison: &crate::models::FundBenchmarkComparison,
) -> anyhow::Result<()> {
    use crate::constants::display_date;

    let (Some(start), Some(end)) = (&comparison.start_date, &comparison.end_date) else {
        return Ok(());
    };
    writeln!(
        writer,
        "  Comparison dates: {} – {}; returns in %, excess in percentage points",
        display_date(start),
        display_date(end)
    )?;
    writeln!(
        writer,
        "  Fund return: {}; reference return: {}; excess: {}",
        format_return_plain(comparison.fund_return_pct),
        format_return_plain(comparison.reference_return_pct),
        match comparison.excess_return_percentage_points {
            Some(excess) => format_eu(&format!("{excess:+.2} pp")),
            None => "N/A".to_owned(),
        }
    )?;
    Ok(())
}

/// The aligned wealth graph of one comparison, rebased to 100.
pub(crate) fn write_comparison_wealth_graph<W: Write>(
    writer: &mut W,
    comparison: &crate::models::FundBenchmarkComparison,
) -> anyhow::Result<()> {
    use crate::models::DatedWealthPoint;

    if comparison.fund_wealth.len() != comparison.reference_wealth.len()
        || comparison.fund_wealth.len() < 2
    {
        return Ok(());
    }
    let points: [&[DatedWealthPoint]; 2] = [&comparison.fund_wealth, &comparison.reference_wealth];
    let (minimum, maximum) = points
        .iter()
        .flat_map(|series| series.iter().map(|point| point.wealth))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    let shades = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let render = |series: &[DatedWealthPoint]| {
        series
            .iter()
            .map(|point| {
                let fraction = if maximum > minimum {
                    (point.wealth - minimum) / (maximum - minimum)
                } else {
                    0.5
                };
                shades[(fraction * (shades.len() - 1) as f64).round() as usize]
            })
            .collect::<String>()
    };
    writeln!(
        writer,
        "  Aligned wealth graph (EUR wealth rebased to 100):"
    )?;
    writeln!(writer, "    Fund      {}", render(&comparison.fund_wealth))?;
    writeln!(
        writer,
        "    Reference {}",
        render(&comparison.reference_wealth)
    )?;
    Ok(())
}

/// The scoped Market data limitations of one comparison.
///
/// The fund scope and the reference scope stay separate, so a limitation of
/// one reference never reads as a limitation of the other.
pub(crate) fn write_comparison_limitations<W: Write>(
    writer: &mut W,
    fund_label: &str,
    reference_label: &str,
    comparison: &crate::models::FundBenchmarkComparison,
) -> anyhow::Result<()> {
    for limitation in &comparison.fund_limitations {
        writeln!(
            writer,
            "  {fund_label} limitation: {}",
            format_market_data_limitation_warning(limitation)
        )?;
    }
    for limitation in &comparison.reference_limitations {
        writeln!(
            writer,
            "  {reference_label} limitation: {}",
            format_market_data_limitation_warning(limitation)
        )?;
    }
    Ok(())
}

/// The standard comparison rows shared by the fund and exposure comparisons.
///
/// The subject label names the compared side; the reference side keeps one
/// name because every comparison measures against its reference.
pub(crate) fn write_benchmark_period_rows<W: Write>(
    writer: &mut W,
    subject_label: &str,
    periods: &[crate::models::BenchmarkPeriod],
) -> anyhow::Result<()> {
    use crate::constants::display_date;

    writeln!(writer, "  Period | Requested start; actual anchor – end | {subject_label} return | Reference return | Excess | Beta | Correlation | Tracking error (% annualized) | Information ratio")?;
    for period in periods {
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
            format_comparison_fact(&period.subject_return_pct, |value| format_pct(Some(value))),
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
        ComparisonFact::Unavailable { reason } => {
            format!("Unavailable ({})", display_reason(reason))
        }
        ComparisonFact::NotApplicable { reason } => {
            format!("N/A ({})", display_reason(reason))
        }
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

/// One descriptive text that may be absent.
///
/// The absent label states what the reader sees in place of the value.
pub(crate) fn display_optional_text(value: Option<&str>, absent: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(absent)
        .to_owned()
}

#[cfg(test)]
#[path = "../../../tests/unit/benchmark_reference_context_tests.rs"]
mod tests;
