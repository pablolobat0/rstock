use sea_orm::{sea_query::OnConflict, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

use crate::db::entities::benchmark_secondary_assignment;

/// One explicit secondary assignment of a reference to a subject.
#[derive(Clone, Debug, PartialEq)]
pub struct SecondaryAssignment {
    pub subject_type: String,
    pub subject_id: String,
    pub reference_id: String,
    /// When the explicit secondary assignment was recorded. The assignment
    /// applies retrospectively to the whole comparison window.
    pub assigned_at: String,
}

/// Every explicit secondary assignment.
pub async fn find_all(
    db: &impl sea_orm::ConnectionTrait,
) -> anyhow::Result<Vec<SecondaryAssignment>> {
    Ok(benchmark_secondary_assignment::Entity::find()
        .order_by_asc(benchmark_secondary_assignment::Column::SubjectType)
        .order_by_asc(benchmark_secondary_assignment::Column::SubjectId)
        .order_by_asc(benchmark_secondary_assignment::Column::ReferenceId)
        .all(db)
        .await?
        .into_iter()
        .map(assignment_from_row)
        .collect())
}

/// The secondary assignments of one subject.
pub async fn find_by_subject(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
) -> anyhow::Result<Vec<SecondaryAssignment>> {
    Ok(benchmark_secondary_assignment::Entity::find()
        .filter(benchmark_secondary_assignment::Column::SubjectType.eq(subject_type))
        .filter(benchmark_secondary_assignment::Column::SubjectId.eq(subject_id))
        .order_by_asc(benchmark_secondary_assignment::Column::ReferenceId)
        .all(db)
        .await?
        .into_iter()
        .map(assignment_from_row)
        .collect())
}

/// The secondary assignments of several subjects in one query.
pub async fn find_by_subjects(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_ids: &[String],
) -> anyhow::Result<Vec<SecondaryAssignment>> {
    if subject_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(benchmark_secondary_assignment::Entity::find()
        .filter(benchmark_secondary_assignment::Column::SubjectType.eq(subject_type))
        .filter(benchmark_secondary_assignment::Column::SubjectId.is_in(subject_ids.to_vec()))
        .order_by_asc(benchmark_secondary_assignment::Column::SubjectId)
        .order_by_asc(benchmark_secondary_assignment::Column::ReferenceId)
        .all(db)
        .await?
        .into_iter()
        .map(assignment_from_row)
        .collect())
}

/// The active secondary assignments that point at one definition.
pub async fn find_by_reference(
    db: &impl sea_orm::ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<Vec<SecondaryAssignment>> {
    Ok(benchmark_secondary_assignment::Entity::find()
        .filter(benchmark_secondary_assignment::Column::ReferenceId.eq(reference_id))
        .order_by_asc(benchmark_secondary_assignment::Column::SubjectType)
        .order_by_asc(benchmark_secondary_assignment::Column::SubjectId)
        .all(db)
        .await?
        .into_iter()
        .map(assignment_from_row)
        .collect())
}

/// Records one explicit secondary assignment.
///
/// The write is idempotent: the same subject and reference keep one row, and
/// the recorded moment follows the latest explicit act.
pub async fn upsert(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
    reference_id: &str,
) -> anyhow::Result<()> {
    benchmark_secondary_assignment::Entity::insert(benchmark_secondary_assignment::ActiveModel {
        subject_type: Set(subject_type.to_owned()),
        subject_id: Set(subject_id.to_owned()),
        reference_id: Set(reference_id.to_owned()),
        assigned_at: Set(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
    })
    .on_conflict(
        OnConflict::columns([
            benchmark_secondary_assignment::Column::SubjectType,
            benchmark_secondary_assignment::Column::SubjectId,
            benchmark_secondary_assignment::Column::ReferenceId,
        ])
        .update_columns([benchmark_secondary_assignment::Column::AssignedAt])
        .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    Ok(())
}

/// Removes one secondary assignment of one subject.
pub async fn delete_by_subject_reference(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
    reference_id: &str,
) -> anyhow::Result<bool> {
    let result = benchmark_secondary_assignment::Entity::delete_many()
        .filter(benchmark_secondary_assignment::Column::SubjectType.eq(subject_type))
        .filter(benchmark_secondary_assignment::Column::SubjectId.eq(subject_id))
        .filter(benchmark_secondary_assignment::Column::ReferenceId.eq(reference_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

/// Removes every secondary assignment of one subject and reports the removed
/// reference identities.
pub async fn delete_by_subject(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
) -> anyhow::Result<Vec<String>> {
    let removed = find_by_subject(db, subject_type, subject_id).await?;
    let result = benchmark_secondary_assignment::Entity::delete_many()
        .filter(benchmark_secondary_assignment::Column::SubjectType.eq(subject_type))
        .filter(benchmark_secondary_assignment::Column::SubjectId.eq(subject_id))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return Ok(Vec::new());
    }
    Ok(removed
        .into_iter()
        .map(|assignment| assignment.reference_id)
        .collect())
}

fn assignment_from_row(row: benchmark_secondary_assignment::Model) -> SecondaryAssignment {
    SecondaryAssignment {
        subject_type: row.subject_type,
        subject_id: row.subject_id,
        reference_id: row.reference_id,
        assigned_at: row.assigned_at,
    }
}
