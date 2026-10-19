//! The explicit Strategy benchmark selection commands.
//!
//! The commands are a reference operation: they record which saved definition
//! the portfolio is compared against and nothing else. They never infer a
//! strategy from the holdings, and they create no transaction, weight, or NAV
//! change.

use std::io::Write;

use anyhow::Context;

use crate::{
    cli::{display::helpers::display_timestamp, output, StrategyCommands},
    models::StrategySelectionOutcome,
    services::{market_data::MarketData, strategy},
};

/// Runs one Strategy selection command through the injected writer.
///
/// The library-owned dispatch is the only entry point, so the production
/// command and the tests observe exactly the same behaviour.
pub async fn run_with_writer<W: Write>(
    db: &sea_orm::DatabaseConnection,
    market_data: &MarketData,
    command: StrategyCommands,
    format: output::OutputFormat,
    writer: &mut W,
) -> anyhow::Result<()> {
    match command {
        StrategyCommands::Select { reference_id } => {
            let outcome = strategy::select_strategy(db, market_data, &reference_id).await?;
            output::write_command_output(
                writer,
                format,
                "strategy.select",
                &outcome,
                &selection_human(&outcome, "Selected"),
            )
        }
        StrategyCommands::Unselect {} => {
            let outcome = strategy::unselect_strategy(db).await?;
            output::write_command_output(
                writer,
                format,
                "strategy.unselect",
                &outcome,
                &selection_human(&outcome, "Unselected"),
            )
        }
        StrategyCommands::Show {} => {
            let outcome = current_selection(db).await?;
            output::write_command_output(
                writer,
                format,
                "strategy.show",
                &outcome,
                &selection_human(&outcome, "Selected"),
            )
        }
    }
}

/// The current selection state, reported like a selection receipt.
async fn current_selection(
    db: &sea_orm::DatabaseConnection,
) -> anyhow::Result<StrategySelectionOutcome> {
    let Some(selection) = strategy::selected_strategy(db).await? else {
        return Ok(StrategySelectionOutcome {
            reference_id: None,
            display_name: None,
            selected: false,
            selected_at: None,
            selection_basis: None,
            replaced_reference_id: None,
            replaced_display_name: None,
            note: "No Strategy benchmark is selected".to_owned(),
        });
    };
    // The selected definition is a required lookup: a database failure and a
    // missing selected definition are both hard failures, never a silent
    // "nothing is selected".
    let definition =
        crate::db::repos::benchmark_reference_repo::find_by_id(db, &selection.reference_id)
            .await
            .with_context(|| {
                format!(
                    "the selected Strategy benchmark '{}' is not a known benchmark definition",
                    selection.reference_id
                )
            })?;
    Ok(StrategySelectionOutcome {
        reference_id: Some(selection.reference_id),
        display_name: Some(definition.display_name),
        selected: true,
        selected_at: Some(selection.selected_at),
        selection_basis: Some(selection.selection_basis),
        replaced_reference_id: None,
        replaced_display_name: None,
        note: "Strategy benchmark selected explicitly; it is compared independently of the portfolio's actual allocation"
            .to_owned(),
    })
}

/// The human rendering of one selection receipt.
fn selection_human(outcome: &StrategySelectionOutcome, verb: &str) -> String {
    let mut lines = Vec::new();
    match (
        outcome.selected,
        outcome.display_name.as_ref(),
        outcome.reference_id.as_ref(),
    ) {
        (true, Some(name), Some(reference_id)) => {
            lines.push(format!(
                "{verb} Strategy benchmark '{name}' [{reference_id}]"
            ));
        }
        (true, _, Some(reference_id)) => {
            lines.push(format!("{verb} Strategy benchmark [{reference_id}]"));
        }
        (true, _, None) => lines.push(format!("{verb} Strategy benchmark")),
        (false, _, _) => lines.push("No Strategy benchmark is selected".to_owned()),
    }
    if let Some(selected_at) = outcome.selected_at.as_ref() {
        lines.push(format!(
            "  Selected at: {}; basis: {}",
            display_timestamp(selected_at),
            outcome.selection_basis.as_deref().unwrap_or("unknown")
        ));
    }
    if let (Some(reference_id), Some(name)) = (
        outcome.replaced_reference_id.as_ref(),
        outcome.replaced_display_name.as_ref(),
    ) {
        lines.push(format!(
            "  Replaced the previous Strategy benchmark '{name}' [{reference_id}]"
        ));
    }
    lines.push(format!("  {}", outcome.note));
    lines.join("\n")
}
