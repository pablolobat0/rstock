use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DatabaseBackend, Statement};

/// The explicit secondary benchmark assignments.
///
/// A secondary assignment is a diagnostic comparison that sits alongside the
/// subject's primary reference. It is stored apart from the primary
/// assignments so that a secondary can never replace a primary, so that one
/// subject can hold several secondaries, and so that the exposure-matched
/// portfolio benchmark — which reads only the primary assignments — can never
/// pick up a secondary weight.
#[derive(DeriveMigrationName)]
pub struct Migration;

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
            .await
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
