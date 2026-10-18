use std::io::{self, Write};

use sea_orm::{EntityTrait, QueryOrder, QuerySelect};
use serde::Serialize;

use crate::cli::{output, BenchmarkCommands, CandidateVehicle};
use crate::db::{
    entities::{benchmark_assignment, benchmark_candidate},
    repos::{asset_repo, benchmark_reference_repo},
};

#[derive(Serialize)]
struct CatalogRow {
    #[serde(flatten)]
    reference: benchmark_reference_repo::BenchmarkReference,
    assignment_count: usize,
}

#[derive(Serialize)]
struct AssignmentRow {
    subject_type: String,
    subject_id: String,
    reference_id: String,
    is_override: bool,
    definition: Option<benchmark_reference_repo::BenchmarkReference>,
}

#[derive(Serialize)]
struct ListOutput {
    definitions: Vec<CatalogRow>,
    candidates: Vec<CandidateOutput>,
    assignments: Vec<AssignmentRow>,
    unassigned_tracked_assets: Vec<TrackedOutput>,
}

#[derive(Serialize)]
struct CandidateOutput {
    vehicle: String,
    identifier: String,
    display_name: String,
    official_comparator: Option<String>,
    official_provenance: String,
    objective_comparator: Option<String>,
    objective_provenance: String,
}

#[derive(Serialize)]
struct TrackedOutput {
    ticker: String,
    name: String,
    asset_type: String,
    has_history: bool,
}

#[allow(clippy::too_many_lines)]
pub async fn run(
    db: &sea_orm::DatabaseConnection,
    command: BenchmarkCommands,
    format: output::OutputFormat,
) -> anyhow::Result<()> {
    match command {
        BenchmarkCommands::List {} => {
            let definitions = benchmark_reference_repo::find_all(db).await?;
            let candidates = benchmark_candidate::Entity::find()
                .order_by_asc(benchmark_candidate::Column::Identifier)
                .all(db)
                .await?
                .into_iter()
                .map(|item| CandidateOutput {
                    vehicle: item.vehicle,
                    identifier: item.identifier,
                    display_name: item.display_name,
                    official_comparator: item.official_comparator,
                    official_provenance: item.official_provenance,
                    objective_comparator: item.objective_comparator,
                    objective_provenance: item.objective_provenance,
                })
                .collect();
            let rows = benchmark_assignment::Entity::find()
                .order_by_asc(benchmark_assignment::Column::SubjectType)
                .all(db)
                .await?;
            let mut assignments = Vec::new();
            for row in rows {
                let definition =
                    Some(benchmark_reference_repo::find_by_id(db, &row.reference_id).await?);
                assignments.push(AssignmentRow {
                    subject_type: row.subject_type,
                    subject_id: row.subject_id,
                    reference_id: row.reference_id,
                    is_override: row.is_override,
                    definition,
                });
            }
            let assets = asset_repo::find_all(db).await?;
            let historical_ids: Vec<i32> = crate::db::entities::transaction::Entity::find()
                .select_only()
                .column(crate::db::entities::transaction::Column::AssetId)
                .distinct()
                .into_tuple::<i32>()
                .all(db)
                .await?;
            let unassigned_tracked_assets = assets
                .into_iter()
                .filter(|item| {
                    !assignments
                        .iter()
                        .any(|a| a.subject_type == "tracked" && a.subject_id == item.ticker)
                })
                .map(|item| TrackedOutput {
                    ticker: item.ticker,
                    name: item.name,
                    asset_type: item.asset_type.to_string(),
                    has_history: historical_ids.contains(&item.id),
                })
                .collect();
            let data = ListOutput {
                definitions: definitions
                    .into_iter()
                    .map(|reference| {
                        let assignment_count = assignments
                            .iter()
                            .filter(|a| a.reference_id == reference.id)
                            .count();
                        CatalogRow {
                            reference,
                            assignment_count,
                        }
                    })
                    .collect(),
                candidates,
                assignments,
                unassigned_tracked_assets,
            };
            let human = list_human(&data);
            write_list(format, &data, &human)
        }
        BenchmarkCommands::Show { reference_id } => {
            let reference = benchmark_reference_repo::find_by_id(db, &reference_id).await?;
            emit(
                format,
                "benchmark.show",
                &reference,
                &format!("{} [{}]", reference.display_name, reference.id),
            )
        }
        BenchmarkCommands::Assign {
            ticker,
            reference_id,
            override_selection,
        } => {
            if asset_repo::find_by_ticker(db, &ticker).await?.is_none() {
                anyhow::bail!("Tracked asset '{ticker}' not found");
            }
            benchmark_reference_repo::put_assignment(
                db,
                "tracked",
                &ticker,
                &reference_id,
                override_selection,
            )
            .await?;
            let definition = benchmark_reference_repo::find_by_id(db, &reference_id).await?;
            let data = serde_json::json!({"subject_type":"tracked", "subject_id":ticker, "reference_id":reference_id, "definition":definition, "is_override":override_selection, "retrospective":true});
            let human = format!(
                "Assigned {} [{}] to {ticker} retrospectively ({}; {})",
                definition.display_name,
                definition.id,
                definition.universe,
                definition.verification_state
            );
            emit(format, "benchmark.assign", &data, &human)
        }
        BenchmarkCommands::AssignCandidate {
            identifier,
            vehicle,
            reference_id,
        } => {
            validate_candidate_identifier(&identifier)?;
            let subject = candidate_subject(&identifier, vehicle);
            benchmark_reference_repo::put_assignment(
                db,
                "fund_candidate",
                &subject,
                &reference_id,
                false,
            )
            .await?;
            let definition = benchmark_reference_repo::find_by_id(db, &reference_id).await?;
            let data = serde_json::json!({"subject_type":"fund_candidate", "subject_id":subject, "reference_id":reference_id, "definition":definition, "retrospective":true, "creates_holding":false});
            let human = format!("Assigned {} [{}] to exact candidate {subject} retrospectively ({}; {}); no holding created", definition.display_name, definition.id, definition.universe, definition.verification_state);
            emit(format, "benchmark.assign_candidate", &data, &human)
        }
        BenchmarkCommands::Remove { ticker } => {
            if asset_repo::find_by_ticker(db, &ticker).await?.is_none() {
                anyhow::bail!("Tracked asset '{ticker}' not found");
            }
            let removed =
                benchmark_reference_repo::remove_assignment(db, "tracked", &ticker).await?;
            emit(
                format,
                "benchmark.remove",
                &serde_json::json!({"subject_type":"tracked","subject_id":ticker,"removed":removed}),
                &format!("Removed primary reference assignment for {ticker}"),
            )
        }
        BenchmarkCommands::RemoveCandidate {
            identifier,
            vehicle,
        } => {
            validate_candidate_identifier(&identifier)?;
            let subject = candidate_subject(&identifier, vehicle);
            let removed =
                benchmark_reference_repo::remove_assignment(db, "fund_candidate", &subject).await?;
            emit(
                format,
                "benchmark.remove_candidate",
                &serde_json::json!({"subject_type":"fund_candidate","subject_id":subject,"removed":removed}),
                &format!("Removed primary reference assignment for {subject}"),
            )
        }
    }
}

fn list_human(data: &ListOutput) -> String {
    let mut lines = vec![
        "Approved benchmark definitions (target-only entries are not admitted usable series):"
            .to_owned(),
    ];
    for row in &data.definitions {
        let reference = &row.reference;
        lines.push(format!("{} [{}] — role: {}; universe: {}; basis: {}; currency: {}; hedge: {}; source: {}; history: {}; verification: {}; official comparator: {}; objective comparator: {}",
            reference.display_name, reference.id, reference.role, reference.universe, reference.return_basis_qualification,
            reference.currency, reference.hedge_policy, reference.source_provenance, reference.history_boundary,
            reference.verification_state, reference.official_comparator.as_deref().unwrap_or("unknown"),
            reference.objective_comparator.as_deref().unwrap_or("unknown")));
    }
    lines.push("Explicit assignments:".to_owned());
    for assignment in &data.assignments {
        lines.push(format!(
            "{} {} → {}{}",
            assignment.subject_type,
            assignment.subject_id,
            assignment.definition.as_ref().map_or_else(
                || assignment.reference_id.clone(),
                |d| format!("{} [{}]", d.display_name, d.id),
            ),
            if assignment.is_override {
                " (explicit override; retrospective)"
            } else {
                " (retrospective)"
            }
        ));
    }
    lines.push(
        "Exact Fund candidate comparator provenance (separate from selected primary):".to_owned(),
    );
    for candidate in &data.candidates {
        lines.push(format!(
            "{} {} — {}: official comparator {}; objective comparator {}",
            candidate.vehicle,
            candidate.identifier,
            candidate.display_name,
            candidate
                .official_comparator
                .as_deref()
                .unwrap_or("unknown/absent"),
            candidate
                .objective_comparator
                .as_deref()
                .unwrap_or("unknown/absent")
        ));
    }
    lines.push("Unassigned Tracked assets:".to_owned());
    for asset in &data.unassigned_tracked_assets {
        lines.push(format!(
            "{} ({}, {}), historical transactions: {}",
            asset.ticker, asset.name, asset.asset_type, asset.has_history
        ));
    }
    lines.join("\n")
}

fn write_list(format: output::OutputFormat, data: &ListOutput, human: &str) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    write_output(&mut writer, format, "benchmark.list", data, human)
}

fn candidate_subject(identifier: &str, vehicle: CandidateVehicle) -> String {
    format!(
        "{}:{}",
        match vehicle {
            CandidateVehicle::Fund => "fund",
            CandidateVehicle::Etf => "etf",
        },
        identifier.trim().to_ascii_uppercase()
    )
}

fn validate_candidate_identifier(identifier: &str) -> anyhow::Result<()> {
    let normalized = identifier.trim();
    if normalized.len() != 12
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        anyhow::bail!("fund candidate identity must be an exact 12-character ISIN; name aliases are not accepted");
    }
    Ok(())
}

fn emit<T: Serialize>(
    format: output::OutputFormat,
    command: &str,
    data: &T,
    human: &str,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    write_output(&mut writer, format, command, data, human)
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
