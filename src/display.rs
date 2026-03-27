use colored::Colorize;
use tabled::settings::object::Cell;
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Color, Style};
use tabled::Table;
use textplots::{Chart, Plot, Shape};

use crate::models::{
    Asset, AssetRow, PeriodMetrics, PortfolioResult, PortfolioRow, PortfolioSnapshot,
    PortfolioSummary,
};

fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{}", qty as i64)
    } else {
        format!("{qty:.4}")
    }
}

fn color_value(value: f64, formatted: &str) -> String {
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
            let text = format!("{sign}{v:.2}%");
            color_value(v, &text)
        }
        None => "N/A".to_string(),
    }
}

fn format_pct(v: Option<f64>) -> String {
    match v {
        Some(val) => format!("{val:.2}%"),
        None => "N/A".to_string(),
    }
}

fn format_drawdown(v: Option<f64>) -> String {
    match v {
        Some(val) => {
            let text = format!("{val:.2}%");
            text.red().to_string()
        }
        None => "N/A".to_string(),
    }
}

fn format_metric(v: Option<f64>) -> String {
    match v {
        Some(val) => {
            let text = format!("{val:.2}");
            color_value(val, &text)
        }
        None => "N/A".to_string(),
    }
}

fn format_plain(v: Option<f64>) -> String {
    match v {
        Some(val) => format!("{val:.2}"),
        None => "N/A".to_string(),
    }
}

fn print_metrics_table(periods: &[(&str, Option<f64>, &Option<PeriodMetrics>)]) {
    let col_width = 12;
    let label_width = 15;

    // Header
    print!("{:label_width$}", "");
    for (name, _, _) in periods {
        print!("{name:>col_width$}");
    }
    println!();

    // Return
    print!("{:<label_width$}", "Return:");
    for (_, ret, _) in periods {
        print!("{:>col_width$}", format_return(*ret));
    }
    println!();

    // Volatility
    print!("{:<label_width$}", "Volatility:");
    for (_, _, metrics) in periods {
        let val = metrics.as_ref().and_then(|m| m.volatility);
        print!("{:>col_width$}", format_pct(val));
    }
    println!();

    // Max Drawdown
    print!("{:<label_width$}", "Max Drawdown:");
    for (_, _, metrics) in periods {
        let val = metrics.as_ref().and_then(|m| m.max_drawdown);
        print!("{:>col_width$}", format_drawdown(val));
    }
    println!();

    // Sharpe
    print!("{:<label_width$}", "Sharpe:");
    for (_, _, metrics) in periods {
        let val = metrics.as_ref().and_then(|m| m.sharpe);
        print!("{:>col_width$}", format_metric(val));
    }
    println!();

    // Beta
    print!("{:<label_width$}", "Beta:");
    for (_, _, metrics) in periods {
        let val = metrics.as_ref().and_then(|m| m.beta);
        print!("{:>col_width$}", format_plain(val));
    }
    println!();
}

#[allow(clippy::too_many_lines)]
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

                let divs_text = if r.dividends_received > 0.0 {
                    format!("{:.2}", r.dividends_received)
                } else {
                    String::new()
                };

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
                    dividends: divs_text,
                    gain_loss: gl_text,
                    gain_loss_pct: gl_pct_text,
                    weight,
                }
            })
            .collect();

        let mut table = Table::new(&display_rows);
        table.with(
            Style::modern()
                .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                .verticals([(1, VerticalLine::inherit(Style::modern()))])
                .remove_horizontal()
                .remove_vertical(),
        );
        for (i, r) in result.rows.iter().enumerate() {
            let color = if r.gain_loss >= 0.0 {
                Color::FG_GREEN
            } else {
                Color::FG_RED
            };
            // G/L = column 11, G/L % = column 12 (after Divs column)
            table.modify(Cell::new(i + 1, 11), color.clone());
            table.modify(Cell::new(i + 1, 12), color);
        }
        println!("{table}");

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
        let mut totals = format!(
            "Invested: {:.2}  Value: {:.2}",
            result.total_invested, result.total_current_value,
        );
        if result.total_dividends > 0.0 {
            totals.push_str(&format!("  Divs: {:.2}", result.total_dividends));
        }
        totals.push_str(&format!(
            "  G/L: {}",
            color_value(result.total_gain_loss, &gl_text)
        ));
        println!("{totals}");
    }

    if let Some(summary) = summary {
        println!();
        println!("As of:          {}", summary.snapshot_date);
        println!("Portfolio Value: {:.2}", summary.total_value);
        println!("NAV:            {:.2}", summary.nav);

        if let (Some(change), Some(change_pct)) = (summary.daily_change, summary.daily_change_pct) {
            let sign = if change >= 0.0 { "+" } else { "" };
            let text = format!("{sign}{change:.2} ({sign}{change_pct:.2}%)");
            println!("Daily:          {}", color_value(change, &text));
        }

        if let Some(ref inception) = summary.inception_date {
            println!("Inception:      {inception}");
        }

        let periods = [
            ("YTD", summary.ytd_return, &summary.ytd_metrics),
            ("1Y", summary.one_year_return, &summary.one_year_metrics),
            ("3Y(CAGR)", summary.three_year_return, &summary.three_year_metrics),
            ("5Y(CAGR)", summary.five_year_return, &summary.five_year_metrics),
        ];

        print_metrics_table(&periods);
    }
}

pub fn print_asset_list(assets: &[Asset]) {
    if assets.is_empty() {
        println!("No assets found.");
        return;
    }

    let rows: Vec<AssetRow> = assets
        .iter()
        .map(|a| AssetRow {
            ticker: a.ticker.clone(),
            name: a.name.clone(),
            asset_type: a.asset_type.to_string(),
            currency: a.currency.clone(),
            isin: a.isin.clone().unwrap_or_default(),
        })
        .collect();

    let mut table = Table::new(&rows);
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );
    println!("{}", table);
    println!("\nTotal: {} assets", assets.len());
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

    println!("\nNAV — {period_label}");
    Chart::new(180, 60, 0.0, xmax)
        .lineplot(&Shape::Lines(&points))
        .display();
    println!("  {first_date}  →  {last_date}");
}
