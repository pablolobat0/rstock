use colored::Colorize;
use tabled::settings::Color;

use crate::constants::MONETARY_MULTIPLIER;

pub(super) fn format_price(price: f64) -> String {
    let decimals = (MONETARY_MULTIPLIER as u64).trailing_zeros() as usize;
    format!("{price:.decimals$}")
}

pub(super) fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{}", qty as i64)
    } else {
        format!("{qty:.4}")
    }
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
        Some(val) => format!("{val:.2}%"),
        None => "N/A".to_string(),
    }
}

pub(super) fn format_plain(v: Option<f64>) -> String {
    match v {
        Some(val) => format!("{val:.2}"),
        None => "N/A".to_string(),
    }
}

pub(super) fn format_return_plain(r: Option<f64>) -> String {
    match r {
        Some(v) => {
            let sign = if v >= 0.0 { "+" } else { "" };
            format!("{sign}{v:.2}%")
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
