use colored::Colorize;
use tabled::settings::Color;

/// Converts a formatted number string to EU format: `,` as decimal separator,
/// `.` as thousands separator. Preserves leading sign (+/-) and trailing suffixes (%, K, B, T).
/// Numbers with decimals are displayed with 2 digits; numbers without stay as-is.
pub(super) fn format_eu(s: &str) -> String {
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

pub(super) fn color_value(value: f64, formatted: &str) -> String {
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

pub(super) fn color_for_value(v: f64) -> Color {
    if v >= 0.0 {
        Color::FG_GREEN
    } else {
        Color::FG_RED
    }
}
