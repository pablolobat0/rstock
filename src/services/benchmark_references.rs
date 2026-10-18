use sea_orm::ConnectionTrait;
use serde::Serialize;

use crate::db::repos::{
    asset_repo, benchmark_assignment_repo, benchmark_candidate_repo, benchmark_reference_repo,
    transaction_repo,
};

pub use crate::db::repos::benchmark_reference_repo::BenchmarkReference;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundCandidateVehicle {
    Fund,
    Etf,
}

impl FundCandidateVehicle {
    fn as_subject_prefix(self) -> &'static str {
        match self {
            Self::Fund => "fund",
            Self::Etf => "etf",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogDefinition {
    #[serde(flatten)]
    pub reference: BenchmarkReference,
    pub assignment_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CandidateComparator {
    pub vehicle: String,
    pub identifier: String,
    pub display_name: String,
    pub official_comparator: Option<String>,
    pub official_provenance: String,
    pub official_document_date: String,
    pub objective_comparator: Option<String>,
    pub objective_provenance: String,
    pub objective_document_date: String,
    pub history_boundary: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssignmentView {
    pub subject_type: String,
    pub subject_id: String,
    pub reference_id: String,
    pub is_override: bool,
    pub definition: BenchmarkReference,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnassignedTrackedAsset {
    pub ticker: String,
    pub name: String,
    pub asset_type: String,
    pub has_history: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkCatalog {
    pub definitions: Vec<CatalogDefinition>,
    pub candidates: Vec<CandidateComparator>,
    pub assignments: Vec<AssignmentView>,
    pub unassigned_tracked_assets: Vec<UnassignedTrackedAsset>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssignmentOutcome {
    pub subject_type: String,
    pub subject_id: String,
    pub reference_id: String,
    pub definition: BenchmarkReference,
    pub is_override: bool,
    pub retrospective: bool,
    pub creates_holding: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RemovalOutcome {
    pub subject_type: String,
    pub subject_id: String,
    pub removed: bool,
}

pub async fn list_benchmark_catalog(db: &impl ConnectionTrait) -> anyhow::Result<BenchmarkCatalog> {
    let references = benchmark_reference_repo::find_all(db).await?;
    let candidate_records = benchmark_candidate_repo::find_all(db).await?;
    let assignments = benchmark_assignment_repo::find_all(db).await?;
    let historical_asset_ids = transaction_repo::find_distinct_asset_ids(db).await?;
    let assets = asset_repo::find_all(db).await?;

    let mut assignment_views = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        assignment_views.push(AssignmentView {
            subject_type: assignment.subject_type,
            subject_id: assignment.subject_id,
            reference_id: assignment.reference_id.clone(),
            is_override: assignment.is_override,
            definition: benchmark_reference_repo::find_by_id(db, &assignment.reference_id).await?,
        });
    }

    let definitions = references
        .into_iter()
        .map(|reference| {
            let assignment_count = assignment_views
                .iter()
                .filter(|assignment| assignment.reference_id == reference.id)
                .count();
            CatalogDefinition {
                reference,
                assignment_count,
            }
        })
        .collect();
    let candidates = candidate_records
        .into_iter()
        .map(|candidate| CandidateComparator {
            vehicle: candidate.vehicle,
            identifier: candidate.identifier,
            display_name: candidate.display_name,
            official_comparator: candidate.official_comparator,
            official_provenance: candidate.official_provenance,
            official_document_date: candidate.official_document_date,
            objective_comparator: candidate.objective_comparator,
            objective_provenance: candidate.objective_provenance,
            objective_document_date: candidate.objective_document_date,
            history_boundary: candidate.history_boundary,
        })
        .collect();
    let unassigned_tracked_assets = assets
        .into_iter()
        .filter(|asset| {
            !assignment_views.iter().any(|assignment| {
                assignment.subject_type == "tracked" && assignment.subject_id == asset.ticker
            })
        })
        .map(|asset| UnassignedTrackedAsset {
            ticker: asset.ticker,
            name: asset.name,
            asset_type: asset.asset_type.to_string(),
            has_history: historical_asset_ids.contains(&asset.id),
        })
        .collect();

    Ok(BenchmarkCatalog {
        definitions,
        candidates,
        assignments: assignment_views,
        unassigned_tracked_assets,
    })
}

pub async fn show_benchmark_reference(
    db: &impl ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<BenchmarkReference> {
    benchmark_reference_repo::find_by_id(db, reference_id).await
}

pub async fn assign_tracked_asset(
    db: &impl ConnectionTrait,
    ticker: &str,
    reference_id: &str,
    is_override: bool,
) -> anyhow::Result<AssignmentOutcome> {
    if asset_repo::find_by_ticker(db, ticker).await?.is_none() {
        anyhow::bail!("Tracked asset '{ticker}' not found");
    }
    save_assignment(db, "tracked", ticker, reference_id, is_override, false).await
}

pub async fn assign_fund_candidate(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
    reference_id: &str,
) -> anyhow::Result<AssignmentOutcome> {
    let identifier = normalized_candidate_identifier(identifier)?;
    let subject_id = format!("{}:{identifier}", vehicle.as_subject_prefix());
    save_assignment(
        db,
        "fund_candidate",
        &subject_id,
        reference_id,
        false,
        false,
    )
    .await
}

pub async fn remove_tracked_assignment(
    db: &impl ConnectionTrait,
    ticker: &str,
) -> anyhow::Result<RemovalOutcome> {
    if asset_repo::find_by_ticker(db, ticker).await?.is_none() {
        anyhow::bail!("Tracked asset '{ticker}' not found");
    }
    remove_assignment(db, "tracked", ticker).await
}

pub async fn remove_fund_candidate_assignment(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
) -> anyhow::Result<RemovalOutcome> {
    let identifier = normalized_candidate_identifier(identifier)?;
    let subject_id = format!("{}:{identifier}", vehicle.as_subject_prefix());
    remove_assignment(db, "fund_candidate", &subject_id).await
}

async fn save_assignment(
    db: &impl ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
    reference_id: &str,
    is_override: bool,
    creates_holding: bool,
) -> anyhow::Result<AssignmentOutcome> {
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    benchmark_assignment_repo::upsert(db, subject_type, subject_id, reference_id, is_override)
        .await?;
    Ok(AssignmentOutcome {
        subject_type: subject_type.to_owned(),
        subject_id: subject_id.to_owned(),
        reference_id: reference_id.to_owned(),
        definition,
        is_override,
        retrospective: true,
        creates_holding,
    })
}

async fn remove_assignment(
    db: &impl ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
) -> anyhow::Result<RemovalOutcome> {
    Ok(RemovalOutcome {
        subject_type: subject_type.to_owned(),
        subject_id: subject_id.to_owned(),
        removed: benchmark_assignment_repo::delete_by_subject(db, subject_type, subject_id).await?,
    })
}

fn normalized_candidate_identifier(identifier: &str) -> anyhow::Result<String> {
    let normalized = identifier.trim();
    if normalized.len() != 12
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        anyhow::bail!("fund candidate identity must be an exact 12-character ISIN; name aliases are not accepted");
    }
    Ok(normalized.to_ascii_uppercase())
}
