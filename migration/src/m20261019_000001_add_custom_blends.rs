use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A saved Custom blend is a benchmark definition with a distinct
        // identity: synthetic reference wealth is never presented as official
        // index data, an adjusted fund return, or a raw price.
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .add_column(
                        ColumnDef::new(BenchmarkReferences::Kind)
                            .string()
                            .not_null()
                            .default("reference"),
                    )
                    .to_owned(),
            )
            .await?;

        // Blend-specific definition facts. The definition row itself stays in
        // the reference catalog so a blend keeps one stable identity that
        // assignments, Strategy selection, and deletion guards can reference.
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkBlends::Table)
                    .col(
                        ColumnDef::new(BenchmarkBlends::ReferenceId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BenchmarkBlends::CreatedAt).string().not_null())
                    .col(ColumnDef::new(BenchmarkBlends::ResetRule).string().not_null())
                    .col(
                        ColumnDef::new(BenchmarkBlends::WeightConvention)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::CashConvention)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BenchmarkBlends::WeightTolerance).double().not_null())
                    .col(
                        ColumnDef::new(BenchmarkBlends::AdmittedStartDate)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::AdmittedEndDate)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::AdmissionBasis)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_blends_reference")
                            .from(BenchmarkBlends::Table, BenchmarkBlends::ReferenceId)
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Component convention snapshots keep a blend's meaning explicit and
        // fixed. Cash is synthetic: it carries no reference identity and earns
        // a 0% return.
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkBlendComponents::Table)
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::BlendId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentKey)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentReferenceId)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentDisplayName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::Position)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::TargetWeight)
                            .double()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::IsCash)
                            .boolean()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentCurrency)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentReturnBasis)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentHedgePolicy)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentUniverse)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentSourceProvenance)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentVerificationState)
                            .string()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(BenchmarkBlendComponents::BlendId)
                            .col(BenchmarkBlendComponents::ComponentKey),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_blend_components_blend")
                            .from(BenchmarkBlendComponents::Table, BenchmarkBlendComponents::BlendId)
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_blend_components_reference")
                            .from(
                                BenchmarkBlendComponents::Table,
                                BenchmarkBlendComponents::ComponentReferenceId,
                            )
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        // A selected Strategy is one optional reference to a saved definition.
        // The row exists so a selected definition can never be deleted out from
        // under that selection.
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkStrategySelection::Table)
                    .col(
                        ColumnDef::new(BenchmarkStrategySelection::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkStrategySelection::ReferenceId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkStrategySelection::SelectedAt)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_strategy_selection_reference")
                            .from(
                                BenchmarkStrategySelection::Table,
                                BenchmarkStrategySelection::ReferenceId,
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
        manager
            .drop_table(
                Table::drop()
                    .table(BenchmarkStrategySelection::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(BenchmarkBlendComponents::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(BenchmarkBlends::Table).to_owned())
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .drop_column(BenchmarkReferences::Kind)
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
    Kind,
}

#[derive(DeriveIden)]
enum BenchmarkBlends {
    Table,
    ReferenceId,
    CreatedAt,
    ResetRule,
    WeightConvention,
    CashConvention,
    WeightTolerance,
    AdmittedStartDate,
    AdmittedEndDate,
    AdmissionBasis,
}

#[derive(DeriveIden)]
enum BenchmarkBlendComponents {
    Table,
    BlendId,
    ComponentKey,
    ComponentReferenceId,
    ComponentDisplayName,
    Position,
    TargetWeight,
    IsCash,
    ComponentCurrency,
    ComponentReturnBasis,
    ComponentHedgePolicy,
    ComponentUniverse,
    ComponentSourceProvenance,
    ComponentVerificationState,
}

#[derive(DeriveIden)]
enum BenchmarkStrategySelection {
    Table,
    Id,
    ReferenceId,
    SelectedAt,
}
