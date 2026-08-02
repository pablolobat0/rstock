use std::fmt::Write;

use colored::Colorize;
use tabled::settings::object::Columns;
use tabled::settings::style::HorizontalLine;
use tabled::settings::{Alignment, Style};
use tabled::{Table, Tabled};

use crate::models::{AllocationEntry, CompositionResult};

use super::helpers::format_eu;
use super::types::TopHoldingRow;

#[derive(Tabled)]
struct BreakdownRow {
    #[tabled(rename = "Category")]
    label: String,
    #[tabled(rename = "Weight")]
    weight: String,
}

pub fn print_composition(result: &CompositionResult) {
    print!("{}", format_composition(result));
}

pub fn format_composition(result: &CompositionResult) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "\n{}\n",
        "Portfolio Composition Analysis".bold().underline()
    )
    .expect("writing to a String cannot fail");

    if result.asset_class_breakdown.is_none() {
        writeln!(output, "Value-dependent composition: unavailable\n")
            .expect("writing to a String cannot fail");
    }
    for (title, entries) in [
        ("Asset Class", &result.asset_class_breakdown),
        ("Equity Style", &result.equity_style_breakdown),
        ("Management", &result.management_breakdown),
        ("Sector Allocation", &result.sector_breakdown),
        ("Country Allocation", &result.country_breakdown),
        ("Market Cap", &result.market_cap_breakdown),
    ] {
        if let Some(entries) = entries {
            write_breakdown(&mut output, title, entries);
        }
    }

    if let Some(top_holdings) = &result.top_holdings {
        if !top_holdings.is_empty() {
            writeln!(output, "{}\n", "Top Holdings".bold())
                .expect("writing to a String cannot fail");

            let rows: Vec<TopHoldingRow> = top_holdings
                .iter()
                .map(|h| TopHoldingRow {
                    name: h.name.clone(),
                    ticker: h.ticker.clone().unwrap_or_default(),
                    weight: format_eu(&format!("{:.2}%", h.weight)),
                    country: h.country.clone().unwrap_or_default(),
                    sector: h.sector.clone().unwrap_or_default(),
                })
                .collect();

            let mut table = Table::new(&rows);
            table.with(
                Style::modern()
                    .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                    .remove_horizontal()
                    .remove_vertical(),
            );
            table.modify(Columns::single(2), Alignment::right());
            writeln!(output, "{table}\n").expect("writing to a String cannot fail");
        }
    }

    if !result.market_data_limitations.is_empty() {
        writeln!(
            output,
            "{}\n",
            "Current position market data limitations:".yellow()
        )
        .expect("writing to a String cannot fail");
        for limitation in &result.market_data_limitations {
            let warning = super::portfolio::format_market_data_limitation_warning(limitation);
            writeln!(output, "  - {warning}").expect("writing to a String cannot fail");
        }
    }

    if !result.warnings.is_empty() {
        writeln!(output, "\n{}\n", "Warnings:".yellow()).expect("writing to a String cannot fail");
        for w in &result.warnings {
            writeln!(output, "  - {w}").expect("writing to a String cannot fail");
        }
    }
    writeln!(output).expect("writing to a String cannot fail");
    output
}

fn write_breakdown(output: &mut String, title: &str, entries: &[AllocationEntry]) {
    if entries.is_empty() {
        return;
    }

    writeln!(output, "{}\n", title.bold()).expect("writing to a String cannot fail");

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
    writeln!(output, "{table}\n").expect("writing to a String cannot fail");
}
