use std::io::{self, Write};

use serde::Serialize;

use crate::{
    cli::{output, BlendCommands},
    constants::display_date,
    models::{CustomBlendDefinition, CustomBlendDeletion, CustomBlendPreview},
    services::custom_blends::{self, BlendCreateRequest},
    services::market_data::MarketData,
};

/// Runs one Custom blend command against the application graph.
pub async fn run(
    db: &sea_orm::DatabaseConnection,
    market_data: &MarketData,
    command: BlendCommands,
    format: output::OutputFormat,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    run_with_writer(db, market_data, command, format, &mut writer).await
}

/// Runs one Custom blend command through the injected writer.
pub async fn run_with_writer<W: Write>(
    db: &sea_orm::DatabaseConnection,
    market_data: &MarketData,
    command: BlendCommands,
    format: output::OutputFormat,
    writer: &mut W,
) -> anyhow::Result<()> {
    match command {
        BlendCommands::List {} => {
            let blends = custom_blends::list_custom_blends(db).await?;
            write_output(
                writer,
                format,
                "blend.list",
                &BlendList { blends: &blends },
                &list_human(&blends),
            )
        }
        BlendCommands::Create {
            name,
            component,
            start,
        } => {
            let definition = custom_blends::create_custom_blend(
                db,
                market_data,
                BlendCreateRequest {
                    name,
                    components: component,
                    start_date: start,
                },
            )
            .await?;
            write_output(
                writer,
                format,
                "blend.create",
                &definition,
                &definition_human(&definition, "Saved Custom blend"),
            )
        }
        BlendCommands::Show { blend_id } => {
            let definition = custom_blends::show_custom_blend(db, &blend_id).await?;
            write_output(
                writer,
                format,
                "blend.show",
                &definition,
                &definition_human(&definition, "Custom blend"),
            )
        }
        BlendCommands::Preview { blend_id } => {
            let preview = custom_blends::preview_custom_blend(db, market_data, &blend_id).await?;
            write_output(
                writer,
                format,
                "blend.preview",
                &preview,
                &preview_human(&preview),
            )
        }
        BlendCommands::Delete { blend_id } => {
            let deletion = custom_blends::delete_custom_blend(db, &blend_id).await?;
            write_output(
                writer,
                format,
                "blend.delete",
                &deletion,
                &deletion_human(&deletion),
            )
        }
    }
}

/// The saved blends of one catalog listing.
#[derive(Serialize)]
struct BlendList<'a> {
    blends: &'a [CustomBlendDefinition],
}

fn write_output<W: Write, T: Serialize>(
    writer: &mut W,
    format: output::OutputFormat,
    command: &str,
    data: &T,
    human: &str,
) -> anyhow::Result<()> {
    match format {
        output::OutputFormat::Json => output::write_json(writer, command, data),
        output::OutputFormat::Human => writeln!(writer, "{human}").map_err(Into::into),
    }
}

fn list_human(blends: &[CustomBlendDefinition]) -> String {
    let mut lines = vec![
        "Saved Custom benchmark blends (immutable definitions; a changed strategy is a new definition):"
            .to_owned(),
    ];
    if blends.is_empty() {
        lines.push("No saved Custom blends.".to_owned());
        return lines.join("\n");
    }
    for blend in blends {
        lines.push(format!(
            "{} [{}] — {} component(s); requested {} through {}; admitted {} through {}; basis: {}",
            blend.display_name,
            blend.id,
            blend.components.len(),
            display_date(&blend.requested_start_date),
            display_date(&blend.requested_end_date),
            display_date(&blend.admitted_start_date),
            display_date(&blend.admitted_end_date),
            blend.admission_basis
        ));
    }
    lines.join("\n")
}

fn definition_human(definition: &CustomBlendDefinition, heading: &str) -> String {
    let mut lines = vec![format!(
        "{heading} '{}' [{}]",
        definition.display_name, definition.id
    )];
    lines.push(format!(
        "  Created: {}; currency: {}; reset rule: {}",
        display_created_at(&definition.created_at),
        definition.currency,
        definition.reset_rule
    ));
    lines.push(format!(
        "  Weight convention: {}",
        definition.weight_convention
    ));
    lines.push(format!("  Cash convention: {}", definition.cash_convention));
    lines.push(format!(
        "  Weight tolerance: {} percentage points",
        definition.weight_tolerance * 100.0
    ));
    lines.push(format!(
        "  Requested window: {} through {}; basis: {}",
        display_date(&definition.requested_start_date),
        display_date(&definition.requested_end_date),
        definition.admission_basis
    ));
    lines.push(format!(
        "  Admitted window: {} through {}",
        display_date(&definition.admitted_start_date),
        display_date(&definition.admitted_end_date)
    ));
    lines.push("  Components:".to_owned());
    for component in &definition.components {
        lines.push(component_human(component));
    }
    lines.join("\n")
}

fn component_human(component: &crate::models::BlendComponent) -> String {
    format!(
        "    - {} [{}] — target weight: {:.2}%; return basis: {}; adjustment: {}; currency: {}; hedge policy: {}; universe: {}; source: {}; source identifier: {}; cadence: {}; provenance: {}; verification: {}",
        component.display_name,
        component.component_key,
        component.target_weight_percent,
        component.return_basis,
        component.adjustment,
        component.currency,
        component.hedge_policy,
        component.universe,
        component.source,
        component.source_identifier,
        component.cadence,
        component.source_provenance,
        component.verification_state
    )
}

fn preview_human(preview: &CustomBlendPreview) -> String {
    let mut lines = vec![format!(
        "Custom blend preview '{}' [{}]",
        preview.display_name, preview.blend_id
    )];
    lines.push(format!(
        "  Requested window: {} through {}; actual anchor {}; actual end {}; rebased to {}",
        display_date(&preview.requested_start_date),
        display_date(&preview.requested_end_date),
        display_date(&preview.anchor_date),
        display_date(&preview.last_observation_date),
        preview.rebased_to
    ));
    lines.push(format!(
        "  Admitted window: {} through {}; basis: {}",
        display_date(&preview.admitted_start_date),
        display_date(&preview.admitted_end_date),
        preview.admission_basis
    ));
    lines.push(format!(
        "  Cumulative return: {:.4}%",
        preview.cumulative_return_pct
    ));
    lines.push(format!("  Reset rule: {}", preview.reset_rule));
    lines.push(format!(
        "  Weight convention: {}",
        preview.weight_convention
    ));
    lines.push(format!("  Cash convention: {}", preview.cash_convention));
    lines.push(format!("  Accounting: {}", preview.accounting_note));
    if let Some(limitation) = preview.earlier_period_limitation.as_ref() {
        lines.push(format!(
            "  Earlier-period limitation: {}",
            crate::cli::display::helpers::display_reason(limitation)
        ));
    }
    lines.push("  Components:".to_owned());
    for component in &preview.components {
        lines.push(component_human(component));
    }
    lines.push(
        "  Monthly target resets (close immediately before each new calendar month):".to_owned(),
    );
    if preview.target_resets.is_empty() {
        lines.push("    - none in the previewed window".to_owned());
    }
    for reset in &preview.target_resets {
        lines.push(format!(
            "    - reset at the {} close, governing the interval from {}",
            display_date(&reset.reset_date),
            display_date(&reset.effective_from_date)
        ));
    }
    lines.push("  Dated rebased wealth series:".to_owned());
    for point in &preview.wealth {
        lines.push(format!(
            "    {}  {:.4}",
            display_date(&point.date),
            point.wealth
        ));
    }
    lines.join("\n")
}

fn deletion_human(deletion: &CustomBlendDeletion) -> String {
    format!(
        "{} Custom blend '{}' [{}]; {}",
        if deletion.deleted {
            "Deleted"
        } else {
            "Did not delete"
        },
        deletion.display_name,
        deletion.blend_id,
        deletion.preserved_history_note
    )
}

/// The creation date of a saved blend, displayed in the user-facing format.
fn display_created_at(created_at: &str) -> String {
    created_at
        .split('T')
        .next()
        .map_or_else(|| created_at.to_owned(), display_date)
}
