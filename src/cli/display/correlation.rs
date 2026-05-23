use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Color, Style};
use textplots::{Chart, Plot, Shape};

use crate::constants::display_date;
use crate::models::{CorrelationMatrix, RollingCorrelationResult};

use super::helpers::{color_for_value, format_eu, format_plain};
use super::portfolio::format_market_data_limitation_warning;

pub fn print_correlation_matrix(matrix: &CorrelationMatrix, period_label: &str) {
    if matrix.names.is_empty() {
        println!("No assets found for correlation analysis.");
        return;
    }

    println!("\nCorrelation Matrix — {period_label}\n");

    let n = matrix.names.len();

    // Build table with Builder for dynamic columns
    let mut builder = Builder::default();

    // Header row: empty cell + all names
    let mut header = vec![String::new()];
    header.extend(matrix.names.iter().cloned());
    builder.push_record(header);

    // Data rows
    for i in 0..n {
        let mut row = vec![matrix.names[i].clone()];
        for j in 0..n {
            let cell = match matrix.matrix[i][j] {
                Some(v) => format_eu(&format!("{v:.2}")),
                None => "N/A".to_string(),
            };
            row.push(cell);
        }
        builder.push_record(row);
    }

    let mut table = builder.build();
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );

    // Right-align all number columns (skip column 0 which is the name label)
    for col in 1..=n {
        table.modify(Columns::single(col), Alignment::right());
    }

    // Apply color coding: <0.3 green (diversified), 0.3–0.7 neutral, >0.7 red (concentrated)
    for i in 0..n {
        for j in 0..n {
            let color = match matrix.matrix[i][j] {
                None => Color::FG_BRIGHT_BLACK,
                Some(_) if i == j => Color::FG_WHITE,
                Some(v) if v > 0.7 => Color::FG_RED,
                Some(v) if v >= 0.3 => Color::FG_YELLOW,
                Some(v) if v >= -0.3 => Color::FG_GREEN,
                Some(v) if v >= -0.7 => Color::FG_YELLOW,
                Some(_) => Color::FG_GREEN,
            };
            // +1 for header row, +1 for name column
            table.modify(Cell::new(i + 1, j + 1), color);
        }
    }

    println!("{table}");

    if !matrix.warnings.is_empty() {
        println!(
            "\nNote: insufficient data for {period_label}: {}",
            matrix.warnings.join(", ")
        );
    }

    print_market_data_limitations(&matrix.market_data_limitations);
}

pub fn print_rolling_correlation(result: &RollingCorrelationResult) {
    println!(
        "\nRolling Correlation — {} vs {}  [{} | {}]\n",
        result.left_name, result.right_name, result.period_label, result.window_label
    );

    if result.points.is_empty() {
        println!(
            "Not enough aligned data to compute {} rolling correlation.",
            result.window_label
        );
        print_market_data_limitations(&result.market_data_limitations);
        return;
    }

    let points: Vec<(f32, f32)> = result
        .points
        .iter()
        .enumerate()
        .map(|(i, (_, value))| (i as f32, *value as f32))
        .collect();

    let first_date = &result.points[0].0;
    let last_date = &result.points[result.points.len() - 1].0;
    let xmax = (points.len() - 1) as f32;

    Chart::new(180, 60, 0.0, xmax)
        .lineplot(&Shape::Lines(&points))
        .display();
    println!(
        "  Requested period: {}  →  {}",
        display_date(&result.requested_start_date),
        display_date(&result.requested_end_date)
    );
    println!(
        "  Rolling series:    {}  →  {}",
        display_date(first_date),
        display_date(last_date)
    );

    let mut builder = Builder::default();
    builder.push_record(["Metric", "Value"]);
    builder.push_record(["Latest", &format_plain(result.latest)]);
    builder.push_record(["Min", &format_plain(result.min)]);
    builder.push_record(["Max", &format_plain(result.max)]);
    builder.push_record(["Average", &format_plain(result.average)]);

    let mut table = builder.build();
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );
    table.modify(Columns::single(1), Alignment::right());

    for (row, value) in [result.latest, result.min, result.max, result.average]
        .iter()
        .enumerate()
    {
        if let Some(value) = value {
            table.modify(Cell::new(row + 1, 1), color_for_value(*value));
        }
    }

    println!("\n{table}");
    print_market_data_limitations(&result.market_data_limitations);
}

fn print_market_data_limitations(limitations: &[crate::models::MarketDataLimitation]) {
    if limitations.is_empty() {
        return;
    }

    println!();
    println!("Market data limitations:");
    for limitation in limitations {
        let warning = format_market_data_limitation_warning(limitation);
        println!("- {warning}");
    }
}
