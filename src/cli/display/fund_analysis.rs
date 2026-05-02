use chrono::Local;
use colored::Colorize;
use tabled::settings::object::Columns;
use tabled::settings::style::HorizontalLine;
use tabled::settings::{Alignment, Style};
use tabled::{Table, Tabled};

use crate::constants::{display_date, format_date};
use crate::models::{AllocationEntry, FundAnalysisResult, HoldingChangeType};

use super::helpers::{format_eu, format_pct, format_plain, format_return_plain};

pub fn print_fund_analysis(result: &FundAnalysisResult) {
    println!();
    print_header(result);
    print_performance(result);
    print_top_holdings(result);
    print_breakdown("Sector Allocation", &result.sector_breakdown);
    print_breakdown("Country Allocation", &result.country_breakdown);
    print_breakdown("Currency Allocation", &result.currency_breakdown);
    print_snapshot_diff(result);
    println!();
}

fn print_header(result: &FundAnalysisResult) {
    let name = result.name.as_deref().unwrap_or("Unknown Fund");
    println!("{}  [{}]", name.bold().underline(), result.ms_code,);

    let mut info_parts = Vec::new();
    if let Some(ref currency) = result.fund_currency {
        info_parts.push(format!("Currency: {currency}"));
    }
    if let Some(total) = result.total_holdings {
        info_parts.push(format!("Total Holdings: {total}"));
    }
    if let Some(weight) = result.top_10_weight {
        info_parts.push(format!(
            "Top 10 Weight: {}",
            format_eu(&format!("{weight:.2}%"))
        ));
    }
    if let Some(ref date) = result.portfolio_date {
        info_parts.push(format!("Portfolio Date: {}", display_date(date)));
    }
    if !info_parts.is_empty() {
        println!("{}", info_parts.join("  |  "));
    }
    println!();
}

fn print_performance(result: &FundAnalysisResult) {
    println!("{}", "Performance".bold());
    println!();

    let periods = [
        ("YTD", &result.ytd),
        ("1Y", &result.one_year),
        ("3Y", &result.three_year),
        ("5Y", &result.five_year),
        ("All Time", &result.all_time),
    ];

    let rows = vec![
        build_metric_row("Total Return", &periods, |m| {
            format_return_plain(Some(m.total_return))
        }),
        build_metric_row("CAGR", &periods, |m| format_return_plain(m.cagr)),
        build_metric_row("Volatility", &periods, |m| format_pct(m.volatility)),
        build_metric_row("Sharpe", &periods, |m| format_plain(m.sharpe)),
        build_metric_row("Sortino", &periods, |m| format_plain(m.sortino)),
        build_metric_row("Max DD", &periods, |m| format_pct(m.max_drawdown)),
        build_metric_row("Beta", &periods, |m| format_plain(m.beta)),
    ];

    let mut table = Table::new(&rows);
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .remove_horizontal()
            .remove_vertical(),
    );
    for col in 1..=5 {
        table.modify(Columns::single(col), Alignment::right());
    }
    println!("{table}");
    println!();
}

fn print_top_holdings(result: &FundAnalysisResult) {
    if result.top_holdings.is_empty() {
        return;
    }

    let header = match &result.portfolio_date {
        Some(date) => format!(
            "Top {} Holdings  (as of {})",
            result.top_holdings.len(),
            display_date(date)
        ),
        None => format!("Top {} Holdings", result.top_holdings.len()),
    };
    println!("{}", header.bold());
    println!();

    let rows: Vec<HoldingRow> = result
        .top_holdings
        .iter()
        .enumerate()
        .map(|(i, h)| HoldingRow {
            rank: format!("{}", i + 1),
            name: h.name.clone(),
            ticker: display_optional(h.ticker.as_deref()),
            weight: format_eu(&format!("{:.2}%", h.weighting)),
            sector: display_optional(h.sector.as_deref()),
            country: display_optional(h.country.as_deref()),
        })
        .collect();

    let mut table = Table::new(&rows);
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .remove_horizontal()
            .remove_vertical(),
    );
    table.modify(Columns::single(0), Alignment::right());
    table.modify(Columns::single(3), Alignment::right());
    println!("{table}");
    println!();
}

fn print_breakdown(title: &str, entries: &[AllocationEntry]) {
    if entries.is_empty() {
        return;
    }

    println!("{}", title.bold());
    println!();

    let rows: Vec<BreakdownRow> = entries
        .iter()
        .map(|e| BreakdownRow {
            label: e.label.clone(),
            weight: format_eu(&format!("{:.2}%", e.weight)),
        })
        .collect();

    let mut table = Table::new(&rows);
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .remove_horizontal()
            .remove_vertical(),
    );
    table.modify(Columns::single(1), Alignment::right());
    println!("{table}");
    println!();
}

fn print_snapshot_diff(result: &FundAnalysisResult) {
    println!("{}", "Holdings Snapshot".bold());
    println!();

    let today = display_date(&format_date(Local::now().date_naive()));

    match &result.last_snapshot_date {
        Some(date) => {
            println!(
                "  Checked on: {}  |  Compared with: {}  |  {}",
                today,
                display_date(date),
                if result.holdings_changed {
                    "Changes detected!".yellow().to_string()
                } else {
                    "No changes".to_string()
                },
            );
        }
        None => {
            println!("  Checked on: {today}  |  First snapshot recorded.");
        }
    }

    if !result.holding_diff.is_empty() {
        println!();
        for change in &result.holding_diff {
            match change.change_type {
                HoldingChangeType::Added => {
                    let w = change
                        .new_weight
                        .map_or(String::new(), |w| format!("{w:.2}%"));
                    println!(
                        "  {} {:<35} {}",
                        "+".green(),
                        change.name,
                        format_eu(&w).green()
                    );
                }
                HoldingChangeType::Removed => {
                    let w = change
                        .old_weight
                        .map_or(String::new(), |w| format!("{w:.2}%"));
                    println!(
                        "  {} {:<35} {}",
                        "-".red(),
                        change.name,
                        format_eu(&w).red()
                    );
                }
                HoldingChangeType::WeightChanged => {
                    let old = change
                        .old_weight
                        .map_or("?".to_string(), |w| format_eu(&format!("{w:.2}%")));
                    let new = change
                        .new_weight
                        .map_or("?".to_string(), |w| format_eu(&format!("{w:.2}%")));
                    println!("  {} {:<35} {} -> {}", "~".yellow(), change.name, old, new);
                }
            }
        }
    }
}

fn display_optional(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| "—".to_string(), str::to_owned)
}

fn build_metric_row(
    label: &str,
    periods: &[(&str, &Option<crate::models::FundPeriodMetrics>); 5],
    format_fn: impl Fn(&crate::models::FundPeriodMetrics) -> String,
) -> PerformanceRow {
    let extract = |period: &Option<crate::models::FundPeriodMetrics>| -> String {
        match period {
            Some(m) => format_fn(m),
            None => "N/A".to_string(),
        }
    };

    PerformanceRow {
        metric: label.to_string(),
        ytd: extract(periods[0].1),
        one_year: extract(periods[1].1),
        three_year: extract(periods[2].1),
        five_year: extract(periods[3].1),
        all_time: extract(periods[4].1),
    }
}

#[derive(Tabled)]
struct PerformanceRow {
    #[tabled(rename = "")]
    metric: String,
    #[tabled(rename = "YTD")]
    ytd: String,
    #[tabled(rename = "1Y")]
    one_year: String,
    #[tabled(rename = "3Y")]
    three_year: String,
    #[tabled(rename = "5Y")]
    five_year: String,
    #[tabled(rename = "All Time")]
    all_time: String,
}

#[derive(Tabled)]
struct HoldingRow {
    #[tabled(rename = "#")]
    rank: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Weight")]
    weight: String,
    #[tabled(rename = "Sector")]
    sector: String,
    #[tabled(rename = "Country")]
    country: String,
}

#[derive(Tabled)]
struct BreakdownRow {
    #[tabled(rename = "Category")]
    label: String,
    #[tabled(rename = "Weight")]
    weight: String,
}
