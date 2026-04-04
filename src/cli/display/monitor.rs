use std::collections::HashMap;

use colored::Colorize;
use textplots::{Chart, Plot, Shape};

use crate::constants::{display_date, RSI_OVERBOUGHT, RSI_OVERSOLD};
use crate::models::monitor::{MomentumIndicators, MonitorReport};

use super::helpers::{color_value, format_eu};

fn format_volume(v: u64) -> String {
    if v >= 1_000_000_000 {
        format_eu(&format!("{:.2}B", v as f64 / 1_000_000_000.0))
    } else if v >= 1_000_000 {
        format_eu(&format!("{:.2}M", v as f64 / 1_000_000.0))
    } else if v >= 1_000 {
        format_eu(&format!("{:.2}K", v as f64 / 1_000.0))
    } else {
        format!("{v}")
    }
}

fn format_market_cap(v: f64) -> String {
    if v >= 1e12 {
        format_eu(&format!("{:.2}T", v / 1e12))
    } else if v >= 1e9 {
        format_eu(&format!("{:.2}B", v / 1e9))
    } else if v >= 1e6 {
        format_eu(&format!("{:.2}M", v / 1e6))
    } else {
        format_eu(&format!("{v:.2}"))
    }
}

fn rsi_label(rsi: f64) -> String {
    if rsi > RSI_OVERBOUGHT {
        "Overbought".red().to_string()
    } else if rsi < RSI_OVERSOLD {
        "Oversold".green().to_string()
    } else {
        "Neutral".to_string()
    }
}

fn print_momentum_section(label: &str, m: &MomentumIndicators) {
    println!("\n{label}");

    if let Some(rsi) = m.rsi_14 {
        println!(
            "  RSI(14):    {:>8}         [{}]",
            format_eu(&format!("{rsi:.2}")),
            rsi_label(rsi)
        );
    } else {
        println!("  RSI(14):         N/A");
    }

    if let Some(sma) = m.sma_50 {
        let signal = m.sma_50_signal.as_deref().unwrap_or("N/A");
        let arrow = if signal == "Above" { " ↑" } else { " ↓" };
        println!(
            "  SMA(50):    {:>8}       Price {signal}{arrow}",
            format_eu(&format!("{sma:.2}"))
        );
    } else {
        println!("  SMA(50):         N/A");
    }

    if let Some(sma) = m.sma_200 {
        let signal = m.sma_200_signal.as_deref().unwrap_or("N/A");
        let arrow = if signal == "Above" { " ↑" } else { " ↓" };
        println!(
            "  SMA(200):   {:>8}       Price {signal}{arrow}",
            format_eu(&format!("{sma:.2}"))
        );
    } else {
        println!("  SMA(200):        N/A");
    }

    if let Some(ref cross) = m.golden_death_cross {
        println!("  SMA Signal: {cross}");
    }

    if let (Some(ml), Some(ms), Some(mh)) = (m.macd_line, m.macd_signal, m.macd_histogram) {
        let signal_text = m.macd_signal_text.as_deref().unwrap_or("N/A");
        let sign = if mh >= 0.0 { "+" } else { "" };
        println!(
            "  MACD:       {:>8}  Signal: {}  Hist: {}  [{signal_text}]",
            format_eu(&format!("{ml:.2}")),
            format_eu(&format!("{ms:.2}")),
            format_eu(&format!("{sign}{mh:.2}")),
        );
    } else {
        println!("  MACD:            N/A");
    }
}

pub fn print_monitor_report(report: &MonitorReport) {
    let info = &report.stock_info;

    let name = info.name.as_deref().unwrap_or("Unknown");
    println!("\n══ {} — {} ══", info.ticker, name);

    let sector_str = info.sector.as_deref().unwrap_or("N/A");
    let industry_str = info.industry.as_deref().unwrap_or("N/A");
    println!("Sector: {sector_str}  |  Industry: {industry_str}");

    println!();
    let price_str = info
        .current_price
        .map_or("N/A".to_string(), |p| format_eu(&format!("{p:.2}")));
    let prev_str = info
        .previous_close
        .map_or("N/A".to_string(), |p| format_eu(&format!("{p:.2}")));

    let change_str = match (info.current_price, info.previous_close) {
        (Some(cur), Some(prev)) if prev > 0.0 => {
            let diff = cur - prev;
            let pct = diff / prev * 100.0;
            let sign = if diff >= 0.0 { "+" } else { "" };
            let text = format!(
                "{} ({})",
                format_eu(&format!("{sign}{diff:.2}")),
                format_eu(&format!("{sign}{pct:.2}%")),
            );
            color_value(diff, &text)
        }
        _ => "N/A".to_string(),
    };

    let currency = info.currency.as_deref().unwrap_or("");
    println!("  Price: {price_str} {currency}  Prev Close: {prev_str}  Change: {change_str}");

    if let Some((lo, hi)) = info.day_range {
        print!(
            "  Day Range: {} – {}",
            format_eu(&format!("{lo:.2}")),
            format_eu(&format!("{hi:.2}"))
        );
    }
    if let Some((lo, hi)) = info.fifty_two_week_range {
        print!(
            "    52W Range: {} – {}",
            format_eu(&format!("{lo:.2}")),
            format_eu(&format!("{hi:.2}"))
        );
    }
    println!();

    let vol_str = info.volume.map_or("N/A".to_string(), format_volume);
    let avg_vol_str = info.avg_volume.map_or("N/A".to_string(), format_volume);
    print!("  Volume: {vol_str}  Avg Volume: {avg_vol_str}");

    if let Some(mc) = info.market_cap {
        print!("  Market Cap: {}", format_market_cap(mc));
    }
    println!();

    let pe_str = info
        .pe_ttm
        .map_or("N/A".to_string(), |v| format_eu(&format!("{v:.2}")));
    let eps_str = info
        .eps_ttm
        .map_or("N/A".to_string(), |v| format_eu(&format!("{v:.2}")));
    let div_str = info.dividend_yield.map_or("N/A".to_string(), |v| {
        format_eu(&format!("{:.2}%", v * 100.0))
    });
    println!("  P/E: {pe_str}  EPS: {eps_str}  Div Yield: {div_str}");

    let stock_label = format!("── Momentum: {} ──────────────────────", info.ticker);
    print_momentum_section(&stock_label, &report.stock_momentum);

    let sector_label = format!(
        "── Momentum: {} (Sector ETF) ─────────",
        report.sector_etf_ticker
    );
    print_momentum_section(&sector_label, &report.sector_momentum);

    println!(
        "\n── {} vs {} Relationship ────────────",
        info.ticker, report.sector_etf_ticker
    );
    let rel = &report.relationship;
    if let Some(rs) = rel.relative_strength_current {
        let change_str = rel.relative_strength_change.map_or(String::new(), |c| {
            let sign = if c >= 0.0 { "+" } else { "" };
            format!("  ({}% over period)", format_eu(&format!("{sign}{c:.2}")))
        });
        println!(
            "  Relative Strength: {}{}",
            format_eu(&format!("{rs:.2}")),
            change_str
        );
    }
    if let Some(beta) = rel.beta_vs_sector {
        println!("  Beta vs Sector:    {}", format_eu(&format!("{beta:.2}")));
    }
    if let Some(corr) = rel.correlation {
        println!("  Correlation:       {}", format_eu(&format!("{corr:.2}")));
    }

    print_normalized_chart(
        &report.stock_prices,
        &report.sector_prices,
        &info.ticker,
        &report.sector_etf_ticker,
        &report.period_label,
    );
}

fn print_normalized_chart(
    stock_prices: &[(String, f64)],
    sector_prices: &[(String, f64)],
    stock_ticker: &str,
    sector_ticker: &str,
    period_label: &str,
) {
    if stock_prices.len() < 2 || sector_prices.len() < 2 {
        println!("\nNot enough data to display chart.");
        return;
    }

    let sector_map: HashMap<&str, f64> = sector_prices
        .iter()
        .map(|(d, p)| (d.as_str(), *p))
        .collect();

    let mut aligned: Vec<(&str, f64, f64)> = Vec::new();
    for (date, sp) in stock_prices {
        if let Some(&ep) = sector_map.get(date.as_str()) {
            aligned.push((date.as_str(), *sp, ep));
        }
    }

    if aligned.len() < 2 {
        println!("\nNot enough aligned data to display chart.");
        return;
    }

    let base_stock = aligned[0].1;
    let base_sector = aligned[0].2;

    if base_stock <= 0.0 || base_sector <= 0.0 {
        println!("\nInvalid base prices for normalization.");
        return;
    }

    let stock_points: Vec<(f32, f32)> = aligned
        .iter()
        .enumerate()
        .map(|(i, (_, sp, _))| (i as f32, (sp / base_stock * 100.0) as f32))
        .collect();

    let sector_points: Vec<(f32, f32)> = aligned
        .iter()
        .enumerate()
        .map(|(i, (_, _, ep))| (i as f32, (ep / base_sector * 100.0) as f32))
        .collect();

    let xmax = (aligned.len() - 1) as f32;
    let first_date = aligned[0].0;
    let last_date = aligned.last().expect("at least 2 aligned").0;

    println!("\n── Performance (normalized to 100) — {period_label} ──");
    Chart::new(180, 60, 0.0, xmax)
        .lineplot(&Shape::Lines(&stock_points))
        .lineplot(&Shape::Lines(&sector_points))
        .display();
    println!("  {stock_ticker}: ——  {sector_ticker}: ··");
    println!(
        "  {}  →  {}",
        display_date(first_date),
        display_date(last_date)
    );
}
