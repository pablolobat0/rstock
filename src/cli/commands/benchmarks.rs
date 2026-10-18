use std::io::{self, Write};

use serde::Serialize;

use crate::{
    cli::{output, BenchmarkCommands, CandidateVehicle},
    services::benchmark_references::{
        self, AssignmentOutcome, BenchmarkCatalog, BenchmarkReference, CandidateComparator,
        FundCandidateVehicle, RemovalOutcome,
    },
};

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

pub async fn run(
    db: &sea_orm::DatabaseConnection,
    command: BenchmarkCommands,
    format: output::OutputFormat,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    run_with_writer(db, command, format, &mut writer).await
}

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
                &reference_human(&reference),
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

fn list_human(catalog: &BenchmarkCatalog) -> String {
    let mut lines = vec![
        "Approved benchmark definitions (target-only entries are not admitted usable series):"
            .to_owned(),
    ];
    for row in &catalog.definitions {
        lines.push(reference_human(&row.reference));
    }
    lines.push(
        "Explicit primary assignments (retrospective for the requested comparison window):"
            .to_owned(),
    );
    for assignment in &catalog.assignments {
        lines.push(format!(
            "{} {} → {}{}",
            assignment.subject_type,
            assignment.subject_id,
            assignment.definition.display_name,
            if assignment.is_override {
                " (explicit override)"
            } else {
                ""
            }
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
