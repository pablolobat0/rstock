use colored::Colorize;
use tabled::Table;
use textplots::{Chart, Plot, Shape};

use crate::models::{PortfolioResult, PortfolioRow, PortfolioSnapshot, PortfolioSummary};

fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{}", qty as i64)
    } else {
        format!("{:.4}", qty)
    }
}

fn color_value(value: f64, formatted: String) -> String {
    if value >= 0.0 {
        formatted.green().to_string()
    } else {
        formatted.red().to_string()
    }
}

fn format_return(r: Option<f64>) -> String {
    match r {
        Some(v) => {
            let sign = if v >= 0.0 { "+" } else { "" };
            let text = format!("{}{:.2}%", sign, v);
            color_value(v, text)
        }
        None => "N/A".to_string(),
    }
}

pub fn print_portfolio(result: &PortfolioResult, summary: Option<&PortfolioSummary>) {
    if result.rows.is_empty() {
        println!("No positions found.");
    } else {
        let total_current_value = result.total_current_value;
        let display_rows: Vec<PortfolioRow> = result
            .rows
            .iter()
            .map(|r| {
                let sign = if r.gain_loss >= 0.0 { "+" } else { "" };
                let weight = if total_current_value > 0.0 {
                    format!("{:.1}%", (r.current_value / total_current_value) * 100.0)
                } else {
                    "0.0%".to_string()
                };

                let gl_text = format!("{}{:.2}", sign, r.gain_loss);
                let gl_pct_text = format!("{}{:.2}%", sign, r.gain_loss_pct);

                PortfolioRow {
                    ticker: r.ticker.clone(),
                    name: r.name.clone(),
                    asset_type: r.asset_type.to_string(),
                    currency: r.currency.clone(),
                    quantity: format_qty(r.total_qty),
                    avg_cost: format!("{:.2}", r.avg_cost),
                    current_price: format!("{:.2}", r.current_price),
                    price_date: r.price_date.clone(),
                    total_invested: format!("{:.2}", r.total_invested),
                    current_value: format!("{:.2}", r.current_value),
                    gain_loss: color_value(r.gain_loss, gl_text),
                    gain_loss_pct: color_value(r.gain_loss_pct, gl_pct_text),
                    weight,
                }
            })
            .collect();
        println!("{}", Table::new(&display_rows));

        let sign = if result.total_gain_loss >= 0.0 {
            "+"
        } else {
            ""
        };
        let gl_text = format!(
            "{}{:.2} ({}{:.2}%)",
            sign, result.total_gain_loss, sign, result.total_gain_loss_pct
        );
        println!();
        println!(
            "Invested: {:.2}  Value: {:.2}  G/L: {}",
            result.total_invested,
            result.total_current_value,
            color_value(result.total_gain_loss, gl_text),
        );
    }

    if let Some(summary) = summary {
        println!();
        println!("As of:          {}", summary.snapshot_date);
        println!("Portfolio Value: {:.2}", summary.total_value);
        println!("NAV:            {:.2}", summary.nav);

        if let (Some(change), Some(change_pct)) =
            (summary.daily_change, summary.daily_change_pct)
        {
            let sign = if change >= 0.0 { "+" } else { "" };
            let text = format!("{}{:.2} ({}{:.2}%)", sign, change, sign, change_pct);
            println!("Daily:          {}", color_value(change, text));
        }

        if let Some(ref inception) = summary.inception_date {
            println!("Inception:      {}", inception);
        }

        println!(
            "YTD: {}  1Y: {}  3Y(CAGR): {}  5Y(CAGR): {}",
            format_return(summary.ytd_return),
            format_return(summary.one_year_return),
            format_return(summary.three_year_return),
            format_return(summary.five_year_return),
        );
    }
}

pub fn print_nav_chart(snapshots: &[PortfolioSnapshot], period_label: &str) {
    if snapshots.len() < 2 {
        println!("\nNot enough data to display NAV chart.");
        return;
    }

    let first_date = &snapshots[0].date;
    let last_date = &snapshots[snapshots.len() - 1].date;

    // Convert to (f32, f32) points: x = day index, y = nav
    let points: Vec<(f32, f32)> = snapshots
        .iter()
        .enumerate()
        .map(|(i, s)| (i as f32, s.nav as f32))
        .collect();

    let xmax = (snapshots.len() - 1) as f32;

    println!("\nNAV — {}", period_label);
    Chart::new(180, 60, 0.0, xmax)
        .lineplot(&Shape::Lines(&points))
        .display();
    println!("  {}  →  {}", first_date, last_date);
}
