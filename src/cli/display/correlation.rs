use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Color, Style};

use crate::models::CorrelationMatrix;

use super::helpers::format_eu;

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
                Some(v) if v.abs() > 0.7 => Color::FG_RED,
                Some(v) if v.abs() >= 0.3 => Color::FG_YELLOW,
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
}
