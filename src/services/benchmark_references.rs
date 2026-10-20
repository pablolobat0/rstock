use sea_orm::ConnectionTrait;
use serde::Serialize;

use crate::db::repos::{
    asset_repo, benchmark_assignment_repo, benchmark_candidate_repo, benchmark_reference_repo,
    benchmark_secondary_assignment_repo, transaction_repo,
};
use crate::models::AssetType;

pub use crate::db::repos::benchmark_reference_repo::BenchmarkReference;

/// The role a selected reference plays in a subject's comparison.
pub use crate::models::ComparisonRole;

pub const ASSIGNMENT_SEMANTICS: &str = "Explicit primary assignments apply retrospectively to the selected comparison window and do not mutate portfolio history.";

pub const SECONDARY_ASSIGNMENT_SEMANTICS: &str = "Explicit secondary assignments are diagnostic comparisons that sit alongside the primary reference; they apply retrospectively, never replace the primary, and never contribute portfolio exposure weights.";

/// The subject type of a Tracked asset's assignments.
pub const TRACKED_SUBJECT_TYPE: &str = "tracked";

/// The subject type of an exact Fund candidate's assignments.
pub const FUND_CANDIDATE_SUBJECT_TYPE: &str = "fund_candidate";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundCandidateVehicle {
    Fund,
    Etf,
}

/// The exact identity of a Fund candidate class.
///
/// The vehicle and the exact ISIN identify the class; a name alias is not an
/// identity, and one share class is never another share class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundCandidateIdentity {
    pub vehicle: FundCandidateVehicle,
    pub identifier: String,
}

/// One subject record of an explicit benchmark assignment.
///
/// A subject is a Tracked asset of this portfolio or an exact Fund candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentSubject {
    pub subject_type: String,
    pub subject_id: String,
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
    pub role: ComparisonRole,
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
    pub role: ComparisonRole,
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
            role: ComparisonRole::Secondary,
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
    save_assignment(
        db,
        &AssignmentSubject::tracked(ticker),
        reference_id,
        is_override,
        false,
    )
    .await
}

pub async fn assign_fund_candidate(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
    reference_id: &str,
) -> anyhow::Result<AssignmentOutcome> {
    let identity = FundCandidateIdentity {
        vehicle,
        identifier: normalized_candidate_identifier(identifier)?,
    };
    save_assignment(
        db,
        &AssignmentSubject::candidate(&identity),
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
    remove_assignment(db, &AssignmentSubject::tracked(ticker)).await
}

pub async fn remove_fund_candidate_assignment(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
) -> anyhow::Result<RemovalOutcome> {
    let identity = FundCandidateIdentity {
        vehicle,
        identifier: normalized_candidate_identifier(identifier)?,
    };
    remove_assignment(db, &AssignmentSubject::candidate(&identity)).await
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
    save_secondary_assignment(db, &AssignmentSubject::tracked(ticker), reference_id).await
}

/// Assigns one optional secondary reference to an exact Fund candidate
/// without creating a holding.
pub async fn assign_secondary_fund_candidate(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
    reference_id: &str,
) -> anyhow::Result<SecondaryAssignmentOutcome> {
    let identity = FundCandidateIdentity {
        vehicle,
        identifier: normalized_candidate_identifier(identifier)?,
    };
    save_secondary_assignment(db, &AssignmentSubject::candidate(&identity), reference_id).await
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
    remove_secondary_assignment(db, &AssignmentSubject::tracked(ticker), reference_id).await
}

/// Removes one optional secondary reference of an exact Fund candidate.
pub async fn remove_secondary_fund_candidate(
    db: &impl ConnectionTrait,
    identifier: &str,
    vehicle: FundCandidateVehicle,
    reference_id: Option<&str>,
) -> anyhow::Result<SecondaryRemovalOutcome> {
    let identity = FundCandidateIdentity {
        vehicle,
        identifier: normalized_candidate_identifier(identifier)?,
    };
    remove_secondary_assignment(db, &AssignmentSubject::candidate(&identity), reference_id).await
}

/// The subject records that describe one exact class.
///
/// The Tracked asset and the Fund candidate of one exact ISIN and vehicle
/// describe the same investment, so one explicit selection for one of them is
/// a selection for that exact class. A Tracked stock has no candidate
/// identity, so its exact class is the Tracked record alone.
pub async fn exact_class_subjects(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
) -> anyhow::Result<Vec<AssignmentSubject>> {
    let mut subjects = vec![subject.clone()];
    match subject.subject_type.as_str() {
        TRACKED_SUBJECT_TYPE => {
            if let Some(asset) = asset_repo::find_by_ticker(db, &subject.subject_id).await? {
                if let Some(vehicle) = FundCandidateVehicle::of_asset_type(&asset.asset_type) {
                    subjects.push(AssignmentSubject::candidate(&FundCandidateIdentity {
                        vehicle,
                        identifier: subject.subject_id.clone(),
                    }));
                }
            }
        }
        FUND_CANDIDATE_SUBJECT_TYPE => {
            if let Some(identity) = FundCandidateIdentity::from_subject_id(&subject.subject_id) {
                if let Some(asset) = asset_repo::find_by_ticker(db, &identity.identifier).await? {
                    if FundCandidateVehicle::of_asset_type(&asset.asset_type)
                        == Some(identity.vehicle)
                    {
                        subjects.push(AssignmentSubject::tracked(&identity.identifier));
                    }
                }
            }
        }
        _ => {}
    }
    Ok(subjects)
}

/// Checks that an explicit candidate identity denotes the analysed class.
///
/// The Tracked asset and the Fund candidate of one exact ISIN and vehicle
/// describe the same investment, so an identity that names another class is
/// rejected instead of mixing two classes in one comparison. An untracked
/// fund has no local identity to contradict, so the explicit identity is the
/// class.
pub async fn ensure_candidate_identity_matches(
    db: &impl ConnectionTrait,
    ticker: Option<&str>,
    candidate: Option<&FundCandidateIdentity>,
) -> anyhow::Result<()> {
    let (Some(ticker), Some(candidate)) = (ticker, candidate) else {
        return Ok(());
    };
    let Some(asset) = asset_repo::find_by_ticker(db, ticker).await? else {
        return Ok(());
    };
    let Some(vehicle) = FundCandidateVehicle::of_asset_type(&asset.asset_type) else {
        anyhow::bail!(
            "the tracked asset '{ticker}' is not a fund or ETF, so it has no exact Fund candidate identity"
        );
    };
    anyhow::ensure!(
        candidate.identifier == ticker && candidate.vehicle == vehicle,
        "the explicit candidate identity '{}' does not denote the tracked class '{ticker}'",
        candidate.subject_id()
    );
    Ok(())
}

/// Records one explicit primary assignment.
///
/// The database decides the cross-role invariant: the write is refused when
/// the reference already carries the secondary role of the exact class. The
/// decision and the write are one atomic step, so two concurrent selections
/// of one reference cannot both persist, and a refused selection leaves no
/// assignment behind.
async fn save_assignment(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: &str,
    is_override: bool,
    creates_holding: bool,
) -> anyhow::Result<AssignmentOutcome> {
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    if let Err(error) = benchmark_assignment_repo::upsert(
        db,
        &subject.subject_type,
        &subject.subject_id,
        reference_id,
        is_override,
    )
    .await
    {
        return Err(primary_role_conflict(db, subject, reference_id, error).await);
    }
    Ok(AssignmentOutcome {
        subject_type: subject.subject_type.clone(),
        subject_id: subject.subject_id.clone(),
        reference_id: reference_id.to_owned(),
        definition,
        is_override,
        retrospective: true,
        creates_holding,
    })
}

async fn remove_assignment(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
) -> anyhow::Result<RemovalOutcome> {
    Ok(RemovalOutcome {
        subject_type: subject.subject_type.clone(),
        subject_id: subject.subject_id.clone(),
        removed: benchmark_assignment_repo::delete_by_subject(
            db,
            &subject.subject_type,
            &subject.subject_id,
        )
        .await?,
    })
}

/// Records one explicit secondary assignment.
///
/// The primary assignment of the exact class is left untouched: a secondary
/// selection never replaces the primary. The same reference as the primary of
/// the exact class is refused, because one series compared with itself is not
/// a second comparison and the exact-class union would render it twice.
///
/// The database decides that refusal: the write is refused when the reference
/// already carries the primary role of the exact class, so the decision and
/// the write are one atomic step.
async fn save_secondary_assignment(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: &str,
) -> anyhow::Result<SecondaryAssignmentOutcome> {
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    if let Err(error) = benchmark_secondary_assignment_repo::upsert(
        db,
        &subject.subject_type,
        &subject.subject_id,
        reference_id,
    )
    .await
    {
        return Err(secondary_role_conflict(db, subject, reference_id, error).await);
    }
    Ok(SecondaryAssignmentOutcome {
        subject_type: subject.subject_type.clone(),
        subject_id: subject.subject_id.clone(),
        reference_id: reference_id.to_owned(),
        role: ComparisonRole::Secondary,
        definition,
        retrospective: true,
        creates_holding: false,
    })
}

/// Removes one explicit secondary assignment, or every secondary assignment of
/// the subject when no reference is named.
async fn remove_secondary_assignment(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: Option<&str>,
) -> anyhow::Result<SecondaryRemovalOutcome> {
    let removed_reference_ids = match reference_id {
        Some(reference_id) => {
            let removed = benchmark_secondary_assignment_repo::delete_by_subject_reference(
                db,
                &subject.subject_type,
                &subject.subject_id,
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
            benchmark_secondary_assignment_repo::delete_by_subject(
                db,
                &subject.subject_type,
                &subject.subject_id,
            )
            .await?
        }
    };
    Ok(SecondaryRemovalOutcome {
        subject_type: subject.subject_type.clone(),
        subject_id: subject.subject_id.clone(),
        removed: !removed_reference_ids.is_empty(),
        removed_reference_ids,
    })
}

impl FundCandidateVehicle {
    /// The subject prefix of this vehicle in a candidate subject record.
    pub fn as_subject_prefix(self) -> &'static str {
        match self {
            Self::Fund => "fund",
            Self::Etf => "etf",
        }
    }

    /// The vehicle of a Tracked asset type.
    ///
    /// A stock has no Fund candidate identity, so it has no candidate
    /// vehicle.
    pub fn of_asset_type(asset_type: &AssetType) -> Option<Self> {
        match asset_type {
            AssetType::Fund => Some(Self::Fund),
            AssetType::Etf => Some(Self::Etf),
            AssetType::Stock => None,
        }
    }
}

impl FundCandidateIdentity {
    /// Parses one explicit `<vehicle>:<ISIN>` identity.
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        let (vehicle, identifier) = value.split_once(':').ok_or_else(|| {
            anyhow::anyhow!("invalid candidate identity '{value}', expected <vehicle>:<ISIN>")
        })?;
        let vehicle = match vehicle.trim().to_ascii_lowercase().as_str() {
            "fund" => FundCandidateVehicle::Fund,
            "etf" => FundCandidateVehicle::Etf,
            other => anyhow::bail!("unknown candidate vehicle '{other}', expected fund or etf"),
        };
        Ok(Self {
            vehicle,
            identifier: normalized_candidate_identifier(identifier)?,
        })
    }

    /// Reads the identity of a candidate subject record.
    pub fn from_subject_id(subject_id: &str) -> Option<Self> {
        let (vehicle, identifier) = subject_id.split_once(':')?;
        let vehicle = match vehicle {
            "fund" => FundCandidateVehicle::Fund,
            "etf" => FundCandidateVehicle::Etf,
            _ => return None,
        };
        Some(Self {
            vehicle,
            identifier: identifier.to_owned(),
        })
    }

    /// The subject record of this exact class.
    pub fn subject_id(&self) -> String {
        format!("{}:{}", self.vehicle.as_subject_prefix(), self.identifier)
    }
}

impl AssignmentSubject {
    /// The subject record of a Tracked asset.
    pub fn tracked(ticker: &str) -> Self {
        Self {
            subject_type: TRACKED_SUBJECT_TYPE.to_owned(),
            subject_id: ticker.to_owned(),
        }
    }

    /// The subject record of an exact Fund candidate.
    pub fn candidate(identity: &FundCandidateIdentity) -> Self {
        Self {
            subject_type: FUND_CANDIDATE_SUBJECT_TYPE.to_owned(),
            subject_id: identity.subject_id(),
        }
    }

    /// The subject as it is shown to the user.
    pub fn describe(&self) -> String {
        format!("{} '{}'", self.subject_type, self.subject_id)
    }
}

/// The exact class subject whose primary reference already uses the reference.
async fn primary_reference_conflict(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: &str,
) -> anyhow::Result<Option<AssignmentSubject>> {
    for candidate in exact_class_subjects(db, subject).await? {
        let Some(primary) = benchmark_assignment_repo::find_by_subject(
            db,
            &candidate.subject_type,
            &candidate.subject_id,
        )
        .await?
        else {
            continue;
        };
        if primary.reference_id == reference_id {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// The exact class subject whose secondary comparison already uses the
/// reference.
async fn secondary_reference_conflict(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: &str,
) -> anyhow::Result<Option<AssignmentSubject>> {
    for candidate in exact_class_subjects(db, subject).await? {
        let secondaries = benchmark_secondary_assignment_repo::find_by_subject(
            db,
            &candidate.subject_type,
            &candidate.subject_id,
        )
        .await?;
        if secondaries
            .iter()
            .any(|secondary| secondary.reference_id == reference_id)
        {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// The marker of the database refusal of a cross-role write.
///
/// The database constraint raises this marker when it refuses a write that
/// would give one reference both roles of one exact class. The service
/// recognizes the refusal by this marker, so this text and the marker of the
/// migration that installs the constraint must stay the same.
const ROLE_CONFLICT_MARKER: &str = "rstock_benchmark_role_conflict";

/// Whether one error is the refusal of the cross-role constraint.
fn is_role_conflict(error: &anyhow::Error) -> bool {
    error.to_string().contains(ROLE_CONFLICT_MARKER)
        || format!("{error:?}").contains(ROLE_CONFLICT_MARKER)
}

/// The typed conflict of a refused primary assignment.
///
/// The database refuses the write, so the conflict is decided by the database
/// and not by a read that could race with another writer. The refusal is
/// explained from the state the database holds, and an unrelated database
/// error is propagated unchanged.
async fn primary_role_conflict(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: &str,
    error: anyhow::Error,
) -> anyhow::Error {
    if !is_role_conflict(&error) {
        return error;
    }
    match secondary_reference_conflict(db, subject, reference_id).await {
        Ok(Some(conflict)) => anyhow::anyhow!(
            "reference '{reference_id}' is already a secondary comparison of {}; the primary reference and the secondary comparison of one exact class must be different comparisons",
            conflict.describe()
        ),
        _ => anyhow::anyhow!(
            "reference '{reference_id}' already carries the secondary role of one exact class; the primary reference and the secondary comparison of one exact class must be different comparisons"
        ),
    }
}

/// The typed conflict of a refused secondary assignment.
///
/// The database refuses the write, so the conflict is decided by the database
/// and not by a read that could race with another writer. The refusal is
/// explained from the state the database holds, and an unrelated database
/// error is propagated unchanged.
async fn secondary_role_conflict(
    db: &impl ConnectionTrait,
    subject: &AssignmentSubject,
    reference_id: &str,
    error: anyhow::Error,
) -> anyhow::Error {
    if !is_role_conflict(&error) {
        return error;
    }
    match primary_reference_conflict(db, subject, reference_id).await {
        Ok(Some(conflict)) => anyhow::anyhow!(
            "reference '{reference_id}' is already the primary reference of {}; the primary reference and the secondary comparison of one exact class must be different comparisons",
            conflict.describe()
        ),
        _ => anyhow::anyhow!(
            "reference '{reference_id}' already carries the primary role of one exact class; the primary reference and the secondary comparison of one exact class must be different comparisons"
        ),
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
