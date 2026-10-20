use sea_orm::ConnectionTrait;
use serde::Serialize;

use crate::db::repos::{
    asset_repo, benchmark_assignment_repo, benchmark_candidate_repo, benchmark_reference_repo,
    benchmark_secondary_assignment_repo, transaction_repo,
};

pub use crate::db::repos::benchmark_reference_repo::BenchmarkReference;

pub const ASSIGNMENT_SEMANTICS: &str = "Explicit primary assignments apply retrospectively to the selected comparison window and do not mutate portfolio history.";

pub const SECONDARY_ASSIGNMENT_SEMANTICS: &str = "Explicit secondary assignments are diagnostic comparisons that sit alongside the primary reference; they apply retrospectively, never replace the primary, and never contribute portfolio exposure weights.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundCandidateVehicle {
    Fund,
    Etf,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogDefinition {
    #[serde(flatten)]
    pub reference: BenchmarkReference,
    pub assignment_count: usize,
    /// The number of explicit secondary assignments that use this definition.
    pub secondary_assignment_count: usize,
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
    pub retrospective: bool,
    pub creates_holding: bool,
    pub definition: BenchmarkReference,
}

#[derive(Clone, Debug, Serialize)]
pub struct SecondaryAssignmentView {
    pub subject_type: String,
    pub subject_id: String,
    pub reference_id: String,
    pub role: String,
    pub retrospective: bool,
    pub creates_holding: bool,
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
    pub assignment_semantics: &'static str,
    pub secondary_assignment_semantics: &'static str,
    pub definitions: Vec<CatalogDefinition>,
    pub candidates: Vec<CandidateComparator>,
    pub assignments: Vec<AssignmentView>,
    pub secondary_assignments: Vec<SecondaryAssignmentView>,
    pub unassigned_tracked_assets: Vec<UnassignedTrackedAsset>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkShow {
    #[serde(flatten)]
    pub definition: BenchmarkReference,
    pub assignment_semantics: &'static str,
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

/// The outcome of one explicit secondary assignment.
#[derive(Clone, Debug, Serialize)]
pub struct SecondaryAssignmentOutcome {
    pub subject_type: String,
    pub subject_id: String,
    pub reference_id: String,
    /// The role of the assigned reference in this subject's comparison.
    pub role: String,
    pub definition: BenchmarkReference,
    pub retrospective: bool,
    pub creates_holding: bool,
}

/// The outcome of one explicit secondary removal.
#[derive(Clone, Debug, Serialize)]
pub struct SecondaryRemovalOutcome {
    pub subject_type: String,
    pub subject_id: String,
    pub removed: bool,
    /// The removed reference identities, so an explicit removal of one
    /// secondary never hides which assignments it dropped.
    pub removed_reference_ids: Vec<String>,
}

/// The role a secondary reference plays in a subject's comparison.
pub use crate::services::benchmark::SECONDARY_ROLE;

pub async fn list_benchmark_catalog(db: &impl ConnectionTrait) -> anyhow::Result<BenchmarkCatalog> {
    let references = benchmark_reference_repo::find_all(db).await?;
    let candidate_records = benchmark_candidate_repo::find_all(db).await?;
    let assignments = benchmark_assignment_repo::find_all(db).await?;
    let secondary_assignments = benchmark_secondary_assignment_repo::find_all(db).await?;
    let historical_asset_ids = transaction_repo::find_distinct_asset_ids(db).await?;
    let assets = asset_repo::find_all(db).await?;

    let mut assignment_views = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        assignment_views.push(AssignmentView {
            subject_type: assignment.subject_type,
            subject_id: assignment.subject_id,
            reference_id: assignment.reference_id.clone(),
            is_override: assignment.is_override,
            retrospective: true,
            creates_holding: false,
            definition: benchmark_reference_repo::find_by_id(db, &assignment.reference_id).await?,
        });
    }

    let mut secondary_assignment_views = Vec::with_capacity(secondary_assignments.len());
    for assignment in secondary_assignments {
        secondary_assignment_views.push(SecondaryAssignmentView {
            subject_type: assignment.subject_type,
            subject_id: assignment.subject_id,
            reference_id: assignment.reference_id.clone(),
            role: SECONDARY_ROLE.to_owned(),
            retrospective: true,
            creates_holding: false,
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
            let secondary_assignment_count = secondary_assignment_views
                .iter()
                .filter(|assignment| assignment.reference_id == reference.id)
                .count();
            CatalogDefinition {
                reference,
                assignment_count,
                secondary_assignment_count,
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
        assignment_semantics: ASSIGNMENT_SEMANTICS,
        secondary_assignment_semantics: SECONDARY_ASSIGNMENT_SEMANTICS,
        definitions,
        candidates,
        assignments: assignment_views,
        secondary_assignments: secondary_assignment_views,
        unassigned_tracked_assets,
    })
}

pub async fn show_benchmark_reference(
    db: &impl ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<BenchmarkShow> {
    Ok(BenchmarkShow {
        definition: benchmark_reference_repo::find_by_id(db, reference_id).await?,
        assignment_semantics: ASSIGNMENT_SEMANTICS,
    })
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

/// Assigns one optional secondary reference to a Tracked asset.
///
/// The secondary is a diagnostic comparison beside the asset's primary
/// reference. It never replaces the primary, and it never becomes a holding
/// or an exposure weight.
pub async fn assign_secondary_tracked_asset(
    db: &impl ConnectionTrait,
    ticker: &str,
    reference_id: &str,
) -> anyhow::Result<SecondaryAssignmentOutcome> {
    if asset_repo::find_by_ticker(db, ticker).await?.is_none() {
        anyhow::bail!("Tracked asset '{ticker}' not found");
    }
    save_secondary_assignment(db, "tracked", ticker, reference_id).await
}

/// Assigns one optional secondary reference to an exact Fund candidate
/// without creating a holding.
pub async fn assign_secondary_fund_candidate(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
    reference_id: &str,
) -> anyhow::Result<SecondaryAssignmentOutcome> {
    let identifier = normalized_candidate_identifier(identifier)?;
    let subject_id = format!("{}:{identifier}", vehicle.as_subject_prefix());
    save_secondary_assignment(db, "fund_candidate", &subject_id, reference_id).await
}

/// Removes one optional secondary reference of a Tracked asset.
///
/// An explicit reference removes that one assignment. Without one, every
/// secondary assignment of the subject is removed, and the removed identities
/// are reported.
pub async fn remove_secondary_tracked_asset(
    db: &impl ConnectionTrait,
    ticker: &str,
    reference_id: Option<&str>,
) -> anyhow::Result<SecondaryRemovalOutcome> {
    if asset_repo::find_by_ticker(db, ticker).await?.is_none() {
        anyhow::bail!("Tracked asset '{ticker}' not found");
    }
    remove_secondary_assignment(db, "tracked", ticker, reference_id).await
}

/// Removes one optional secondary reference of an exact Fund candidate.
pub async fn remove_secondary_fund_candidate(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
    reference_id: Option<&str>,
) -> anyhow::Result<SecondaryRemovalOutcome> {
    let identifier = normalized_candidate_identifier(identifier)?;
    let subject_id = format!("{}:{identifier}", vehicle.as_subject_prefix());
    remove_secondary_assignment(db, "fund_candidate", &subject_id, reference_id).await
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

/// Records one explicit secondary assignment.
///
/// The primary assignment of the same subject is left untouched: a secondary
/// selection never replaces the primary. The same reference as the primary is
/// rejected, because one series compared with itself is not a second
/// comparison and would read as a duplicated diagnostic.
async fn save_secondary_assignment(
    db: &impl ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
    reference_id: &str,
) -> anyhow::Result<SecondaryAssignmentOutcome> {
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    if let Some(primary) =
        benchmark_assignment_repo::find_by_subject(db, subject_type, subject_id).await?
    {
        if primary.reference_id == reference_id {
            anyhow::bail!(
                "reference '{reference_id}' is already the primary reference of {subject_type} '{subject_id}'; a secondary must be a different comparison"
            );
        }
    }
    benchmark_secondary_assignment_repo::upsert(db, subject_type, subject_id, reference_id).await?;
    Ok(SecondaryAssignmentOutcome {
        subject_type: subject_type.to_owned(),
        subject_id: subject_id.to_owned(),
        reference_id: reference_id.to_owned(),
        role: SECONDARY_ROLE.to_owned(),
        definition,
        retrospective: true,
        creates_holding: false,
    })
}

/// Removes one explicit secondary assignment, or every secondary assignment of
/// the subject when no reference is named.
async fn remove_secondary_assignment(
    db: &impl ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
    reference_id: Option<&str>,
) -> anyhow::Result<SecondaryRemovalOutcome> {
    let removed_reference_ids = match reference_id {
        Some(reference_id) => {
            let removed = benchmark_secondary_assignment_repo::delete_by_subject_reference(
                db,
                subject_type,
                subject_id,
                reference_id,
            )
            .await?;
            if removed {
                vec![reference_id.to_owned()]
            } else {
                Vec::new()
            }
        }
        None => {
            benchmark_secondary_assignment_repo::delete_by_subject(db, subject_type, subject_id)
                .await?
        }
    };
    Ok(SecondaryRemovalOutcome {
        subject_type: subject_type.to_owned(),
        subject_id: subject_id.to_owned(),
        removed: !removed_reference_ids.is_empty(),
        removed_reference_ids,
    })
}

impl FundCandidateVehicle {
    fn as_subject_prefix(self) -> &'static str {
        match self {
            Self::Fund => "fund",
            Self::Etf => "etf",
        }
    }
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
