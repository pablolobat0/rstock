use colored::Colorize;
use tabled::builder::Builder;
use tabled::settings::object::Columns;
use tabled::settings::style::HorizontalLine;
use tabled::settings::{Alignment, Style};
use tabled::Table;
use textplots::{Chart, Plot, Shape};

use crate::constants::{display_date, MIN_DATA_POINTS};
use crate::models::{
    AllocationComparison, FundComparisonResult, FundComparisonSide, FundPeriodMetrics,
};

use super::helpers::{format_eu, format_pct, format_plain, format_return_plain};

pub fn print_fund_comparison(result: &FundComparisonResult) {
    println!();
    print_identity(result);
    print_fund_info(result);
    print_performance(result);
    print_allocations("Sector Allocation", &result.sector_allocations, result);
    print_allocations("Country Allocation", &result.country_allocations, result);
    print_allocations("Currency Allocation", &result.currency_allocations, result);
    print_common_holdings(result);
    print_correlation(result);
    println!();
}

fn print_identity(result: &FundComparisonResult) {
    println!("{}", "Fund Comparison".bold().underline());
    println!(
        "{} [{}]  vs  {} [{}]",
        result.fund_a.name, result.fund_a.code, result.fund_b.name, result.fund_b.code
    );
    println!();
}

fn print_fund_info(result: &FundComparisonResult) {
    println!("{}", "Fund Info".bold());
    println!();

    let rows = vec![
        vec![
            "Currency".to_owned(),
            display_optional(result.fund_a.info.currency.as_deref()),
            display_optional(result.fund_b.info.currency.as_deref()),
        ],
        vec![
            "AUM".to_owned(),
            display_aum(&result.fund_a),
            display_aum(&result.fund_b),
        ],
        vec![
            "Inception".to_owned(),
            display_date_optional(result.fund_a.info.inception_date.as_deref()),
            display_date_optional(result.fund_b.info.inception_date.as_deref()),
        ],
        vec![
            "Total Holdings".to_owned(),
            result
                .fund_a
                .info
                .total_holdings
                .map_or_else(|| "N/A".to_owned(), |total| total.to_string()),
            result
                .fund_b
                .info
                .total_holdings
                .map_or_else(|| "N/A".to_owned(), |total| total.to_string()),
        ],
        vec![
            "Top 10 Weight".to_owned(),
            result.fund_a.info.top_10_weight.map_or_else(
                || "N/A".to_owned(),
                |weight| format_eu(&format!("{weight:.2}%")),
            ),
            result.fund_b.info.top_10_weight.map_or_else(
                || "N/A".to_owned(),
                |weight| format_eu(&format!("{weight:.2}%")),
            ),
        ],
        vec![
            "Portfolio Date".to_owned(),
            display_date_optional(result.fund_a.info.portfolio_date.as_deref()),
            display_date_optional(result.fund_b.info.portfolio_date.as_deref()),
        ],
    ];

    print_table(
        vec![
            "Field".to_owned(),
            result.fund_a.name.clone(),
            result.fund_b.name.clone(),
        ],
        rows,
        2,
    );
}

fn print_performance(result: &FundComparisonResult) {
    println!("{}", "Performance".bold());
    println!();

    let return_rows = [
        build_performance_rows("Total Return", result, |m| {
            format_return_plain(Some(m.total_return))
        }),
        build_performance_rows("CAGR", result, |m| format_return_plain(m.cagr)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    print_performance_table("Returns", return_rows, &[3]);

    let risk_rows = [
        build_performance_rows("Volatility", result, |m| format_pct(m.volatility)),
        build_performance_rows("Max DD", result, |m| format_pct(m.max_drawdown)),
        build_performance_rows("Beta", result, |m| format_plain(m.beta)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    print_performance_table("Risk", risk_rows, &[3, 5]);

    let risk_adjusted_rows = [
        build_performance_rows("Sharpe", result, |m| format_plain(m.sharpe)),
        build_performance_rows("Sortino", result, |m| format_plain(m.sortino)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    print_performance_table("Risk-Adjusted", risk_adjusted_rows, &[3]);
}

fn print_allocations(title: &str, entries: &[AllocationComparison], result: &FundComparisonResult) {
    if entries.is_empty() {
        return;
    }

    println!("{}", title.bold());
    println!();

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| {
            vec![
                entry.label.clone(),
                format_eu(&format!("{:.2}%", entry.weight_a)),
                format_eu(&format!("{:.2}%", entry.weight_b)),
                format_allocation_difference(entry),
            ]
        })
        .collect();
    print_table(
        vec![
            "Category".to_owned(),
            result.fund_a.name.clone(),
            result.fund_b.name.clone(),
            "Diff A-B".to_owned(),
        ],
        rows,
        3,
    );
}

fn print_common_holdings(result: &FundComparisonResult) {
    if result.common_holdings.is_empty() {
        return;
    }

    println!("{}", "Common Holdings".bold());
    println!();

    let rows: Vec<Vec<String>> = result
        .common_holdings
        .iter()
        .map(|holding| {
            vec![
                holding.name_a.clone(),
                display_dash(holding.ticker.as_deref()),
                format_eu(&format!("{:.2}%", holding.weight_a)),
                format_eu(&format!("{:.2}%", holding.weight_b)),
            ]
        })
        .collect();
    print_table(
        vec![
            "Holding".to_owned(),
            "Ticker".to_owned(),
            result.fund_a.name.clone(),
            result.fund_b.name.clone(),
        ],
        rows,
        3,
    );
}

fn print_correlation(result: &FundComparisonResult) {
    println!(
        "{}",
        format!(
            "Fund-To-Fund Correlation — {}",
            result.correlation.period_label
        )
        .bold()
    );
    println!();

    let Some(correlation) = result.correlation.correlation else {
        println!(
            "Correlation: N/A ({})",
            result
                .correlation
                .reason
                .as_deref()
                .unwrap_or("selected-period coverage unavailable")
        );
        println!();
        return;
    };

    println!("Correlation: {}", format_plain(Some(correlation)));
    if result.correlation.points.len() < MIN_DATA_POINTS {
        println!("Aligned return graph: N/A (not enough aligned graph data)");
        println!();
        return;
    }

    let points_a: Vec<(f32, f32)> = result
        .correlation
        .points
        .iter()
        .enumerate()
        .map(|(idx, point)| (idx as f32, point.return_a as f32))
        .collect();
    let points_b: Vec<(f32, f32)> = result
        .correlation
        .points
        .iter()
        .enumerate()
        .map(|(idx, point)| (idx as f32, point.return_b as f32))
        .collect();
    let xmax = (result.correlation.points.len() - 1) as f32;

    println!(
        "Legend: {} = line, {} = points",
        result.fund_a.name, result.fund_b.name
    );
    Chart::new(180, 40, 0.0, xmax)
        .lineplot(&Shape::Lines(&points_a))
        .lineplot(&Shape::Points(&points_b))
        .display();

    let first = &result.correlation.points[0];
    let last = &result.correlation.points[result.correlation.points.len() - 1];
    println!(
        "  Aligned period: {}  →  {}",
        display_date(&first.date),
        display_date(&last.date)
    );
    println!(
        "  {}: {}  →  {}",
        result.fund_a.name,
        format_return_plain(Some(first.return_a)),
        format_return_plain(Some(last.return_a))
    );
    println!(
        "  {}: {}  →  {}",
        result.fund_b.name,
        format_return_plain(Some(first.return_b)),
        format_return_plain(Some(last.return_b))
    );
    println!();
}

fn build_performance_rows(
    metric: &str,
    result: &FundComparisonResult,
    format_fn: impl Fn(&FundPeriodMetrics) -> String,
) -> Vec<Vec<String>> {
    vec![
        performance_row(metric, &result.fund_a, &format_fn),
        performance_row("", &result.fund_b, &format_fn),
    ]
}

fn print_performance_table(title: &str, rows: Vec<Vec<String>>, separator_lines: &[usize]) {
    println!("{}", title.bold());
    println!();

    print_table_with_separators(
        vec![
            "Metric".to_owned(),
            "Fund".to_owned(),
            "YTD".to_owned(),
            "1Y".to_owned(),
            "3Y".to_owned(),
            "5Y".to_owned(),
            "All Time".to_owned(),
        ],
        rows,
        6,
        separator_lines,
    );
}

fn performance_row(
    metric: &str,
    fund: &FundComparisonSide,
    format_fn: &impl Fn(&FundPeriodMetrics) -> String,
) -> Vec<String> {
    let value = |metrics: &Option<FundPeriodMetrics>| {
        metrics.as_ref().map_or_else(|| "N/A".to_owned(), format_fn)
    };

    vec![
        metric.to_owned(),
        fund.name.clone(),
        value(&fund.ytd),
        value(&fund.one_year),
        value(&fund.three_year),
        value(&fund.five_year),
        value(&fund.all_time),
    ]
}

fn print_table(headers: Vec<String>, rows: Vec<Vec<String>>, rightmost_column: usize) {
    print_table_with_separators(headers, rows, rightmost_column, &[]);
}

fn print_table_with_separators(
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    rightmost_column: usize,
    separator_lines: &[usize],
) {
    let mut builder = Builder::default();
    builder.push_record(headers);
    for row in rows {
        builder.push_record(row);
    }

    let mut table = builder.build();
    apply_table_style(&mut table, separator_lines);
    for col in 1..=rightmost_column {
        table.modify(Columns::single(col), Alignment::right());
    }
    println!("{table}");
    println!();
}

fn apply_table_style(table: &mut Table, separator_lines: &[usize]) {
    let header = HorizontalLine::inherit(Style::modern()).horizontal('═');
    let separator = HorizontalLine::inherit(Style::modern()).horizontal('─');

    match separator_lines {
        [] | [_, _, _, ..] => {
            table.with(
                Style::modern()
                    .horizontals([(1, header)])
                    .remove_horizontal()
                    .remove_vertical(),
            );
        }
        [first] => {
            table.with(
                Style::modern()
                    .horizontals([(1, header), (*first, separator)])
                    .remove_horizontal()
                    .remove_vertical(),
            );
        }
        [first, second] => {
            table.with(
                Style::modern()
                    .horizontals([(1, header), (*first, separator), (*second, separator)])
                    .remove_horizontal()
                    .remove_vertical(),
            );
        }
    }
}

fn display_aum(fund: &FundComparisonSide) -> String {
    match (fund.info.aum, fund.info.aum_currency.as_deref()) {
        (Some(aum), Some(currency)) => format!("{} {currency}", format_eu(&format!("{aum:.2}"))),
        (Some(aum), None) => format!("{} N/A", format_eu(&format!("{aum:.2}"))),
        (None, _) => "N/A".to_owned(),
    }
}

fn format_allocation_difference(entry: &AllocationComparison) -> String {
    let difference = entry.weight_a - entry.weight_b;
    if difference.abs() < 0.005 {
        return "0,00 pp".to_owned();
    }

    format!("{} pp", format_eu(&format!("{difference:+.2}")))
}

fn display_date_optional(value: Option<&str>) -> String {
    value.map_or_else(|| "N/A".to_owned(), display_date)
}

fn display_optional(value: Option<&str>) -> String {
    value.unwrap_or("N/A").to_owned()
}

fn display_dash(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| "—".to_string(), str::to_owned)
}
