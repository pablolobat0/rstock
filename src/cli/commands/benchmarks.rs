use std::io::{self, Write};

use serde::Serialize;

use crate::{
    cli::{output, BenchmarkCommands, CandidateVehicle},
    services::benchmark_references::{
        self, AssignmentOutcome, BenchmarkCatalog, BenchmarkReference, BenchmarkShow,
        CandidateComparator, FundCandidateVehicle, RemovalOutcome, SecondaryAssignmentOutcome,
        SecondaryRemovalOutcome,
    },
};

pub async fn run(
    db: &sea_orm::DatabaseConnection,
    command: BenchmarkCommands,
    format: output::OutputFormat,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    run_with_writer(db, command, format, &mut writer).await
}

/// Runs one benchmark management command, writing its successful output
/// through the injected writer.
///
/// The JSON envelope and the human rendering both travel through the writer,
/// so the library dispatch of a real command value produces the production
/// document.
pub async fn run_with_writer<W: Write>(
    db: &sea_orm::DatabaseConnection,
    command: BenchmarkCommands,
    format: output::OutputFormat,
    writer: &mut W,
) -> anyhow::Result<()> {
    match command {
        BenchmarkCommands::List {} => {
            let catalog = benchmark_references::list_benchmark_catalog(db).await?;
            write_output(
                writer,
                format,
                "benchmark.list",
                &catalog,
                &list_human(&catalog),
            )
        }
        BenchmarkCommands::Show { reference_id } => {
            let reference =
                benchmark_references::show_benchmark_reference(db, &reference_id).await?;
            write_output(
                writer,
                format,
                "benchmark.show",
                &reference,
                &show_human(&reference),
            )
        }
        BenchmarkCommands::Assign {
            ticker,
            reference_id,
            override_selection,
        } => {
            let outcome = benchmark_references::assign_tracked_asset(
                db,
                &ticker,
                &reference_id,
                override_selection,
            )
            .await?;
            write_assignment(writer, format, "benchmark.assign", &outcome)
        }
        BenchmarkCommands::AssignCandidate {
            identifier,
            vehicle,
            reference_id,
        } => {
            let outcome = benchmark_references::assign_fund_candidate(
                db,
                &identifier,
                fund_vehicle(vehicle),
                &reference_id,
            )
            .await?;
            write_assignment(writer, format, "benchmark.assign_candidate", &outcome)
        }
        BenchmarkCommands::Remove { ticker } => {
            let outcome = benchmark_references::remove_tracked_assignment(db, &ticker).await?;
            write_removal(writer, format, "benchmark.remove", &outcome)
        }
        BenchmarkCommands::RemoveCandidate {
            identifier,
            vehicle,
        } => {
            let outcome = benchmark_references::remove_fund_candidate_assignment(
                db,
                &identifier,
                fund_vehicle(vehicle),
            )
            .await?;
            write_removal(writer, format, "benchmark.remove_candidate", &outcome)
        }
        BenchmarkCommands::AssignSecondary { .. }
        | BenchmarkCommands::RemoveSecondary { .. }
        | BenchmarkCommands::AssignSecondaryCandidate { .. }
        | BenchmarkCommands::RemoveSecondaryCandidate { .. } => {
            run_secondary_command(db, command, format, writer).await
        }
    }
}

/// Runs one explicit secondary assignment command.
///
/// The secondary commands are kept apart from the primary ones because a
/// secondary never replaces a primary reference and never becomes a holding.
async fn run_secondary_command<W: Write>(
    db: &sea_orm::DatabaseConnection,
    command: BenchmarkCommands,
    format: output::OutputFormat,
    writer: &mut W,
) -> anyhow::Result<()> {
    match command {
        BenchmarkCommands::AssignSecondary {
            ticker,
            reference_id,
        } => {
            let outcome =
                benchmark_references::assign_secondary_tracked_asset(db, &ticker, &reference_id)
                    .await?;
            write_secondary_assignment(writer, format, "benchmark.assign_secondary", &outcome)
        }
        BenchmarkCommands::RemoveSecondary {
            ticker,
            reference_id,
        } => {
            let outcome = benchmark_references::remove_secondary_tracked_asset(
                db,
                &ticker,
                reference_id.as_deref(),
            )
            .await?;
            write_secondary_removal(writer, format, "benchmark.remove_secondary", &outcome)
        }
        BenchmarkCommands::AssignSecondaryCandidate {
            identifier,
            vehicle,
            reference_id,
        } => {
            let outcome = benchmark_references::assign_secondary_fund_candidate(
                db,
                &identifier,
                fund_vehicle(vehicle),
                &reference_id,
            )
            .await?;
            write_secondary_assignment(
                writer,
                format,
                "benchmark.assign_secondary_candidate",
                &outcome,
            )
        }
        BenchmarkCommands::RemoveSecondaryCandidate {
            identifier,
            vehicle,
            reference_id,
        } => {
            let outcome = benchmark_references::remove_secondary_fund_candidate(
                db,
                &identifier,
                fund_vehicle(vehicle),
                reference_id.as_deref(),
            )
            .await?;
            write_secondary_removal(
                writer,
                format,
                "benchmark.remove_secondary_candidate",
                &outcome,
            )
        }
        _ => anyhow::bail!("the requested command is not a secondary assignment command"),
    }
}
fn fund_vehicle(vehicle: CandidateVehicle) -> FundCandidateVehicle {
    match vehicle {
        CandidateVehicle::Fund => FundCandidateVehicle::Fund,
        CandidateVehicle::Etf => FundCandidateVehicle::Etf,
    }
}

fn write_assignment<W: Write>(
    writer: &mut W,
    format: output::OutputFormat,
    command: &str,
    outcome: &AssignmentOutcome,
) -> anyhow::Result<()> {
    write_output(writer, format, command, outcome, &assignment_human(outcome))
}

fn write_removal<W: Write>(
    writer: &mut W,
    format: output::OutputFormat,
    command: &str,
    outcome: &RemovalOutcome,
) -> anyhow::Result<()> {
    write_output(
        writer,
        format,
        command,
        outcome,
        &format!(
            "{} primary reference assignment for {} {}",
            if outcome.removed { "Removed" } else { "No" },
            outcome.subject_type,
            outcome.subject_id
        ),
    )
}

/// Writes the outcome of one explicit secondary assignment.
///
/// The secondary keeps its own role, its own reference identity, and the
/// retrospective meaning of the explicit selection. It never replaces the
/// primary reference and never creates a holding.
fn write_secondary_assignment<W: Write>(
    writer: &mut W,
    format: output::OutputFormat,
    command: &str,
    outcome: &SecondaryAssignmentOutcome,
) -> anyhow::Result<()> {
    write_output(
        writer,
        format,
        command,
        outcome,
        &format!(
            "Assigned {} [{}] as a {} comparison of {} {} retrospectively; assignment role: {}; definition role: {}; universe: {}; return basis: {}; currency: {}; hedge policy: {}; source: {}; history: {}; verification: {}; creates holding: {}",
            outcome.definition.display_name,
            outcome.definition.id,
            outcome.role,
            outcome.subject_type,
            outcome.subject_id,
            outcome.role,
            outcome.definition.role,
            outcome.definition.universe,
            outcome.definition.return_basis_qualification,
            outcome.definition.currency,
            outcome.definition.hedge_policy,
            outcome.definition.source_provenance,
            outcome.definition.history_boundary,
            outcome.definition.verification_state,
            outcome.creates_holding,
        ),
    )
}

/// Writes the outcome of one explicit secondary removal.
fn write_secondary_removal<W: Write>(
    writer: &mut W,
    format: output::OutputFormat,
    command: &str,
    outcome: &SecondaryRemovalOutcome,
) -> anyhow::Result<()> {
    write_output(
        writer,
        format,
        command,
        outcome,
        &format!(
            "{} secondary comparison assignment(s) for {} {}: {}",
            if outcome.removed { "Removed" } else { "No" },
            outcome.subject_type,
            outcome.subject_id,
            if outcome.removed_reference_ids.is_empty() {
                "none".to_owned()
            } else {
                outcome.removed_reference_ids.join(", ")
            }
        ),
    )
}

pub fn write_output<W: Write, T: Serialize>(
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

fn list_human(catalog: &BenchmarkCatalog) -> String {
    let mut lines = vec![format!(
        "Approved benchmark definitions (target-only entries are not admitted usable series). {}",
        catalog.assignment_semantics
    )];
    for row in &catalog.definitions {
        lines.push(reference_human(&row.reference));
    }
    lines.push("Explicit primary assignments:".to_owned());
    for assignment in &catalog.assignments {
        lines.push(format!(
            "{} {} → {} (retrospective: {}; creates holding: {}){}",
            assignment.subject_type,
            assignment.subject_id,
            assignment.definition.display_name,
            assignment.retrospective,
            assignment.creates_holding,
            if assignment.is_override {
                " (explicit override)"
            } else {
                ""
            }
        ));
    }
    lines.push(format!(
        "Explicit secondary assignments ({}):",
        catalog.secondary_assignment_semantics
    ));
    for assignment in &catalog.secondary_assignments {
        lines.push(format!(
            "{} {} → {} (role: {}; retrospective: {}; creates holding: {})",
            assignment.subject_type,
            assignment.subject_id,
            assignment.definition.display_name,
            assignment.role,
            assignment.retrospective,
            assignment.creates_holding,
        ));
    }
    lines.push(
        "Exact Fund candidate comparator provenance (independent of selected primary):".to_owned(),
    );
    for candidate in &catalog.candidates {
        lines.push(candidate_human(candidate));
    }
    lines.push("Unassigned Tracked assets:".to_owned());
    for asset in &catalog.unassigned_tracked_assets {
        lines.push(format!(
            "{} ({}, {}), historical transactions: {}",
            asset.ticker, asset.name, asset.asset_type, asset.has_history
        ));
    }
    lines.join("\n")
}

fn show_human(show: &BenchmarkShow) -> String {
    format!(
        "{}; assignment semantics: {}",
        reference_human(&show.definition),
        show.assignment_semantics
    )
}

fn reference_human(reference: &BenchmarkReference) -> String {
    format!(
        "{} [{}] — role: {}; universe: {}; return basis: {}; currency: {}; hedge policy: {}; source: {}; source identifier: {}; adjustment provenance: {}; history boundary: {}; verification: {}; official comparator: {}; objective comparator: {}",
        reference.display_name,
        reference.id,
        reference.role,
        reference.universe,
        reference.return_basis_qualification,
        reference.currency,
        reference.hedge_policy,
        reference.source_provenance,
        unknown_if_empty(&reference.source_identifier),
        reference.adjustment_provenance,
        reference.history_boundary,
        reference.verification_state,
        reference.official_comparator.as_deref().unwrap_or("unknown/absent"),
        reference.objective_comparator.as_deref().unwrap_or("unknown/absent"),
    )
}

fn candidate_human(candidate: &CandidateComparator) -> String {
    format!(
        "{} {} — {}: official comparator: {}; official source/date: {} / {}; objective comparator: {}; objective source/date: {} / {}; class history: {}",
        candidate.vehicle,
        candidate.identifier,
        candidate.display_name,
        candidate.official_comparator.as_deref().unwrap_or("unknown/absent"),
        candidate.official_provenance,
        candidate.official_document_date,
        candidate.objective_comparator.as_deref().unwrap_or("unknown/absent"),
        candidate.objective_provenance,
        candidate.objective_document_date,
        candidate.history_boundary,
    )
}

fn assignment_human(outcome: &AssignmentOutcome) -> String {
    format!(
        "Assigned {} [{}] to {} {} retrospectively; role: {}; universe: {}; return basis: {}; currency: {}; hedge policy: {}; source: {}; history: {}; verification: {}; explicit override: {}; creates holding: {}",
        outcome.definition.display_name,
        outcome.definition.id,
        outcome.subject_type,
        outcome.subject_id,
        outcome.definition.role,
        outcome.definition.universe,
        outcome.definition.return_basis_qualification,
        outcome.definition.currency,
        outcome.definition.hedge_policy,
        outcome.definition.source_provenance,
        outcome.definition.history_boundary,
        outcome.definition.verification_state,
        outcome.is_override,
        outcome.creates_holding,
    )
}

fn unknown_if_empty(value: &str) -> &str {
    if value.is_empty() {
        "unknown"
    } else {
        value
    }
}
