use tabled::builder::Builder;
use tabled::settings::object::Cell;
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Color, Style};

use crate::models::CorrelationMatrix;

pub fn print_correlation_matrix(matrix: &CorrelationMatrix, period_label: &str) {
    if matrix.labels.is_empty() {
        println!("No assets found for correlation analysis.");
        return;
    }

    println!("\nCorrelation Matrix — {period_label}\n");

    let n = matrix.labels.len();

    // Build table with Builder for dynamic columns
    let mut builder = Builder::default();

    // Header row: empty cell + all tickers
    let mut header = vec![String::new()];
    header.extend(matrix.labels.iter().cloned());
    builder.push_record(header);

    // Data rows
    for i in 0..n {
        let mut row = vec![matrix.labels[i].clone()];
        for j in 0..n {
            let cell = match matrix.matrix[i][j] {
                Some(v) => format!("{v:.2}"),
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

    // Apply color coding per cell
    for i in 0..n {
        for j in 0..n {
            let color = match matrix.matrix[i][j] {
                None => Color::FG_BRIGHT_BLACK,
                Some(_) if i == j => Color::FG_WHITE,
                Some(v) if v.abs() > 0.7 => Color::FG_GREEN,
                Some(v) if v.abs() >= 0.3 => Color::FG_YELLOW,
                Some(_) => Color::FG_RED,
            };
            // +1 for header row, +1 for label column
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
