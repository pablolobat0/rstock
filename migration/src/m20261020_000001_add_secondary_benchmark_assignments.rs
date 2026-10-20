use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

/// The explicit secondary benchmark assignments.
///
/// A secondary assignment is a diagnostic comparison that sits alongside the
/// subject's primary reference. It is stored apart from the primary
/// assignments so that a secondary can never replace a primary, so that one
/// subject can hold several secondaries, and so that the exposure-matched
/// portfolio benchmark — which reads only the primary assignments — can never
/// pick up a secondary weight.
///
/// The migration also installs the database constraints that keep one
/// reference out of both roles of one exact class. The constraint is the
/// decision, not a read that could race: two concurrent selections of one
/// reference cannot both persist, because the second write is refused by the
/// database itself and leaves no partial write behind.
#[derive(DeriveMigrationName)]
pub struct Migration;

/// The marker of the role conflict refusal.
///
/// The service recognizes the refusal of the database constraint by this
/// marker, so the two copies of the text must stay the same.
pub const ROLE_CONFLICT_MARKER: &str = "rstock_benchmark_role_conflict";

/// The trigger that refuses a write giving one reference the other role of
/// one exact class.
///
/// The Tracked asset and the Fund candidate of one exact ISIN and vehicle
/// describe the same investment, so the constraint compares a subject with
/// its exact-class counterpart in both directions. The check runs inside the
/// write, which is what makes the decision and the write one atomic step.
fn role_conflict_trigger(
    name: &str,
    table: &str,
    timing: &str,
    other_table: &str,
    other_role: &str,
) -> String {
    format!(
        "CREATE TRIGGER {name}
BEFORE {timing} ON {table}
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM {other_table} AS other_assignment
    WHERE other_assignment.reference_id = NEW.reference_id
      AND (
        (other_assignment.subject_type = NEW.subject_type
         AND other_assignment.subject_id = NEW.subject_id)
        OR (NEW.subject_type = 'tracked'
            AND other_assignment.subject_type = 'fund_candidate'
            AND other_assignment.subject_id = (
                SELECT candidate.asset_type || ':' || candidate.ticker
                FROM assets AS candidate
                WHERE candidate.ticker = NEW.subject_id
                  AND candidate.asset_type IN ('fund', 'etf')))
        OR (NEW.subject_type = 'fund_candidate'
            AND other_assignment.subject_type = 'tracked'
            AND other_assignment.subject_id = substr(NEW.subject_id, instr(NEW.subject_id, ':') + 1)
            AND EXISTS (
                SELECT 1
                FROM assets AS candidate
                WHERE candidate.ticker = substr(NEW.subject_id, instr(NEW.subject_id, ':') + 1)
                  AND candidate.asset_type = substr(NEW.subject_id, 1, instr(NEW.subject_id, ':') - 1)))
      )
)
BEGIN
    SELECT RAISE(ABORT, '{ROLE_CONFLICT_MARKER}: the reference already carries the {other_role} role of this exact class');
END;"
    )
}

/// The triggers of the cross-role constraint.
fn role_conflict_triggers() -> Vec<String> {
    vec![
        role_conflict_trigger(
            "benchmark_assignments_role_conflict_insert",
            "benchmark_assignments",
            "INSERT",
            "benchmark_secondary_assignments",
            "secondary",
        ),
        role_conflict_trigger(
            "benchmark_assignments_role_conflict_update",
            "benchmark_assignments",
            "UPDATE",
            "benchmark_secondary_assignments",
            "secondary",
        ),
        role_conflict_trigger(
            "benchmark_secondary_assignments_role_conflict_insert",
            "benchmark_secondary_assignments",
            "INSERT",
            "benchmark_assignments",
            "primary",
        ),
        role_conflict_trigger(
            "benchmark_secondary_assignments_role_conflict_update",
            "benchmark_secondary_assignments",
            "UPDATE",
            "benchmark_assignments",
            "primary",
        ),
    ]
}

/// The statements that remove the cross-role constraint.
fn role_conflict_trigger_drops() -> Vec<String> {
    vec![
        "DROP TRIGGER IF EXISTS benchmark_assignments_role_conflict_insert".to_owned(),
        "DROP TRIGGER IF EXISTS benchmark_assignments_role_conflict_update".to_owned(),
        "DROP TRIGGER IF EXISTS benchmark_secondary_assignments_role_conflict_insert".to_owned(),
        "DROP TRIGGER IF EXISTS benchmark_secondary_assignments_role_conflict_update".to_owned(),
    ]
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkSecondaryAssignments::Table)
                    .col(ColumnDef::new(BenchmarkSecondaryAssignments::SubjectType).string().not_null())
                    .col(ColumnDef::new(BenchmarkSecondaryAssignments::SubjectId).string().not_null())
                    .col(ColumnDef::new(BenchmarkSecondaryAssignments::ReferenceId).string().not_null())
                    .col(ColumnDef::new(BenchmarkSecondaryAssignments::AssignedAt).string().not_null())
                    .primary_key(Index::create()
                        .col(BenchmarkSecondaryAssignments::SubjectType)
                        .col(BenchmarkSecondaryAssignments::SubjectId)
                        .col(BenchmarkSecondaryAssignments::ReferenceId))
                    .foreign_key(ForeignKey::create().name("fk_benchmark_secondary_assignments_reference")
                        .from(BenchmarkSecondaryAssignments::Table, BenchmarkSecondaryAssignments::ReferenceId)
                        .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                        .on_delete(ForeignKeyAction::Restrict))
                    .to_owned(),
            )
            .await?;
        for statement in role_conflict_triggers() {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    DatabaseBackend::Sqlite,
                    statement,
                ))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rolling this migration back would drop the explicitly recorded
        // secondary assignments, so the diagnostic comparisons would silently
        // disappear. Refuse the destructive rollback while secondary
        // assignments exist instead of losing the user's explicit choices.
        //
        // The count is read as an invariant: a row that cannot be read or
        // decoded is an error, never an absent count, because a silently
        // empty count would drop the table and the assignments with it.
        let row = manager
            .get_connection()
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM benchmark_secondary_assignments",
            ))
            .await?
            .ok_or_else(|| {
                DbErr::Custom(
                    "the secondary assignment count could not be read, so the destructive rollback is refused"
                        .to_owned(),
                )
            })?;
        let assignments: i64 = row.try_get("", "count").map_err(|error| {
            DbErr::Custom(format!(
                "the secondary assignment count could not be decoded ({error}), so the destructive rollback is refused"
            ))
        })?;
        if assignments > 0 {
            return Err(DbErr::Custom(
                "the secondary benchmark assignment migration cannot be rolled back while secondary \
                 assignments exist: remove them first so no explicit secondary comparison is silently lost"
                    .to_owned(),
            ));
        }
        // The constraint that reads the secondary table is removed before the
        // table itself, so no trigger is left pointing at a missing table.
        for statement in role_conflict_trigger_drops() {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    DatabaseBackend::Sqlite,
                    statement,
                ))
                .await?;
        }
        manager
            .drop_table(
                Table::drop()
                    .table(BenchmarkSecondaryAssignments::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BenchmarkSecondaryAssignments {
    Table,
    SubjectType,
    SubjectId,
    ReferenceId,
    AssignedAt,
}
#[derive(DeriveIden)]
enum BenchmarkReferences {
    Table,
    Id,
}
