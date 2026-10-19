use sea_orm::{sea_query::OnConflict, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

use crate::db::entities::benchmark_assignment;

#[derive(Clone, Debug)]
pub struct PrimaryAssignment {
    pub subject_type: String,
    pub subject_id: String,
    pub reference_id: String,
    pub is_override: bool,
    /// When the explicit assignment was recorded. The assignment applies
    /// retrospectively to the whole comparison window.
    pub assigned_at: String,
}

pub async fn find_all(
    db: &impl sea_orm::ConnectionTrait,
) -> anyhow::Result<Vec<PrimaryAssignment>> {
    Ok(benchmark_assignment::Entity::find()
        .order_by_asc(benchmark_assignment::Column::SubjectType)
        .order_by_asc(benchmark_assignment::Column::SubjectId)
        .all(db)
        .await?
        .into_iter()
        .map(assignment_from_row)
        .collect())
}

pub async fn find_by_subject(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
) -> anyhow::Result<Option<PrimaryAssignment>> {
    Ok(
        benchmark_assignment::Entity::find_by_id((subject_type.to_owned(), subject_id.to_owned()))
            .one(db)
            .await?
            .map(assignment_from_row),
    )
}

/// Reads the explicit primary assignments of the given subjects in one query.
pub async fn find_by_subjects(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_ids: &[String],
) -> anyhow::Result<Vec<PrimaryAssignment>> {
    if subject_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(benchmark_assignment::Entity::find()
        .filter(benchmark_assignment::Column::SubjectType.eq(subject_type))
        .filter(benchmark_assignment::Column::SubjectId.is_in(subject_ids.to_vec()))
        .order_by_asc(benchmark_assignment::Column::SubjectId)
        .all(db)
        .await?
        .into_iter()
        .map(assignment_from_row)
        .collect())
}

fn assignment_from_row(row: benchmark_assignment::Model) -> PrimaryAssignment {
    PrimaryAssignment {
        subject_type: row.subject_type,
        subject_id: row.subject_id,
        reference_id: row.reference_id,
        is_override: row.is_override,
        assigned_at: row.assigned_at,
    }
}

pub async fn upsert(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
    reference_id: &str,
    is_override: bool,
) -> anyhow::Result<()> {
    benchmark_assignment::Entity::insert(benchmark_assignment::ActiveModel {
        subject_type: Set(subject_type.to_owned()),
        subject_id: Set(subject_id.to_owned()),
        reference_id: Set(reference_id.to_owned()),
        is_override: Set(is_override),
        assigned_at: Set(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
    })
    .on_conflict(
        OnConflict::columns([
            benchmark_assignment::Column::SubjectType,
            benchmark_assignment::Column::SubjectId,
        ])
        .update_columns([
            benchmark_assignment::Column::ReferenceId,
            benchmark_assignment::Column::IsOverride,
            benchmark_assignment::Column::AssignedAt,
        ])
        .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    Ok(())
}

pub async fn delete_by_subject(
    db: &impl sea_orm::ConnectionTrait,
    subject_type: &str,
    subject_id: &str,
) -> anyhow::Result<bool> {
    let result = benchmark_assignment::Entity::delete_by_id((
        subject_type.to_owned(),
        subject_id.to_owned(),
    ))
    .exec(db)
    .await?;
    Ok(result.rows_affected > 0)
}
