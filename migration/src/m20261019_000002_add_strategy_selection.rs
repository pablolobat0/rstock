use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DatabaseBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The optional selected Strategy benchmark. The selection is explicit:
        // no strategy and no target allocation is ever inferred from the
        // portfolio's holdings, so the row records only the chosen definition
        // identity, when it was chosen, and why. The single selection slot is
        // the primary key, so the database itself holds at most one selected
        // Strategy benchmark.
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkStrategySelections::Table)
                    .col(
                        ColumnDef::new(BenchmarkStrategySelections::SelectionSlot)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkStrategySelections::ReferenceId)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(BenchmarkStrategySelections::SelectedAt).string().not_null())
                    .col(
                        ColumnDef::new(BenchmarkStrategySelections::SelectionBasis)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_strategy_selection_reference")
                            .from(
                                BenchmarkStrategySelections::Table,
                                BenchmarkStrategySelections::ReferenceId,
                            )
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rolling this migration back would silently drop the explicitly
        // selected Strategy benchmark, so the dashboard would stop comparing
        // against it without the user ever choosing that. Refuse the
        // destructive rollback while a selection exists instead of losing the
        // user's explicit choice.
        let selections = manager
            .get_connection()
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM benchmark_strategy_selections",
            ))
            .await?
            .map(|row| row.try_get::<i64>("", "count").unwrap_or_default())
            .unwrap_or_default();
        if selections > 0 {
            return Err(DbErr::Custom(
                "the Strategy selection migration cannot be rolled back while a Strategy benchmark is selected: \
                 unselect the Strategy benchmark first so the explicit selection is not silently lost"
                    .to_owned(),
            ));
        }
        manager
            .drop_table(
                Table::drop()
                    .table(BenchmarkStrategySelections::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum BenchmarkReferences {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum BenchmarkStrategySelections {
    Table,
    SelectionSlot,
    ReferenceId,
    SelectedAt,
    SelectionBasis,
}
