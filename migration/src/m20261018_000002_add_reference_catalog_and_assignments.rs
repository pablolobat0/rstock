use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (column, default) in [
            (BenchmarkReferences::Role, "Investment universe"),
            (BenchmarkReferences::Universe, "Unknown"),
            (BenchmarkReferences::HedgePolicy, "Unknown"),
            (BenchmarkReferences::SourceProvenance, "No production source mapping verified"),
            (BenchmarkReferences::HistoryBoundary, "Unknown"),
            (BenchmarkReferences::VerificationState, "target_only"),
        ] {
            manager.alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .add_column(ColumnDef::new(column).string().not_null().default(default))
                    .to_owned(),
            ).await?;
        }
        for column in [BenchmarkReferences::OfficialComparator, BenchmarkReferences::ObjectiveComparator] {
            manager.alter_table(
                Table::alter().table(BenchmarkReferences::Table)
                    .add_column(ColumnDef::new(column).string().null()).to_owned(),
            ).await?;
        }
        manager.create_table(
            Table::create().table(BenchmarkAssignments::Table)
                .col(ColumnDef::new(BenchmarkAssignments::SubjectType).string().not_null())
                .col(ColumnDef::new(BenchmarkAssignments::SubjectId).string().not_null())
                .col(ColumnDef::new(BenchmarkAssignments::ReferenceId).string().not_null())
                .col(ColumnDef::new(BenchmarkAssignments::IsOverride).boolean().not_null().default(false))
                .col(ColumnDef::new(BenchmarkAssignments::AssignedAt).string().not_null())
                .primary_key(Index::create().col(BenchmarkAssignments::SubjectType).col(BenchmarkAssignments::SubjectId))
                .foreign_key(ForeignKey::create().name("fk_benchmark_assignments_reference")
                    .from(BenchmarkAssignments::Table, BenchmarkAssignments::ReferenceId)
                    .to(BenchmarkReferences::Table, BenchmarkReferences::Id).on_delete(ForeignKeyAction::Restrict))
                .to_owned()
        ).await?;

        // These are approved target definitions, not admitted source series.
        // Keep all unresolved source, hedge, and history conventions explicitly unknown.
        let targets = [
            ("msci-world-net-eur-unhedged", "MSCI World", "Developed-market large/mid-cap equities", "Net total return; exact series unverified", "EUR", "Unhedged", "MSCI index definition; production source identifier unknown", "Index history boundary unknown"),
            ("msci-emerging-markets-net-eur-unhedged", "MSCI Emerging Markets", "Emerging-market equities", "Net total return; exact series unverified", "EUR", "Unhedged", "MSCI index definition; production source identifier unknown", "Index history boundary unknown"),
            ("msci-acwi-small-cap-value-net-eur", "MSCI ACWI Small Cap Value", "Global small-cap value equities", "Net total return", "EUR", "Unhedged", "MSCI index definition; production source identifier unknown", "Index history boundary unknown"),
            ("estr-plus-120bp", "€STR + 1.20%", "Euro cash-plus objective", "Objective accrual convention unknown", "EUR", "Not applicable", "DNCA B EUR class objective; exact document provenance retained in PRD", "Pre-€STR construction unknown"),
            ("ice-euro-corporate-75-high-yield-25", "75% ICE BofA Euro Corporate / 25% Euro High Yield", "Euro investment-grade and high-yield credit", "Total-return variants and reset schedule unverified", "EUR", "Unknown", "Carmignac A EUR class comparator target; source mapping unknown", "History boundaries unknown"),
            ("bloomberg-euro-aggregate-1-3y", "Bloomberg Euro Aggregate 1–3 Years", "Euro investment-grade bonds, 1–3 year", "Total return; exact class conventions unverified", "EUR", "Unknown", "Neuberger EUR I class target; source mapping unknown", "History boundaries unknown"),
            ("swiss-re-global-cat-bond-tr", "Swiss Re Global Cat Bond Total Return (SRGLTRR)", "Global catastrophe bonds", "Total return", "Unknown", "Unknown", "Swiss Re market insights, February 2025; series access unverified", "Observation cadence and history boundary unknown"),
            ("abaco-global-mixed-reference", "80% Bloomberg Global Aggregate 1–5 Year EUR-hedged / 20% MSCI ACWI Small Cap Value EUR", "Global bonds and global small-cap value equities", "Bond total return + equity net total return; monthly resets", "EUR", "Bond sleeve EUR-hedged; equity sleeve unhedged", "Ábaco prospectus/reference target; exact series unknown", "History boundary unknown"),
            ("msci-europe-small-cap-net-eur", "MSCI Europe Small Cap", "European small-cap equities (includes UK)", "Net total return", "EUR", "Unhedged", "MSCI index definition; production source identifier unknown", "Index history boundary unknown"),
            ("msci-usa-small-cap-net-eur", "MSCI USA Small Cap", "US small-cap equities", "Net total return", "EUR", "Unhedged", "MSCI index definition; EUR conversion required; production source unknown", "Index history boundary unknown"),
            ("bloomberg-global-aggregate-1-5y-eur-hedged", "Bloomberg Global Aggregate 1–5 Year EUR-hedged", "Global bonds, 1–5 year", "Total return", "EUR", "EUR-hedged", "Ábaco target; exact series mapping unknown", "History boundary unknown"),
            ("msci-europe-small-cap-value", "MSCI Europe Small Cap Value", "European small-cap value equities", "Return basis unknown", "EUR", "Unknown", "Optional Cobas secondary comparator; source mapping unknown", "History boundary unknown"),
            ("estr", "€STR", "Euro overnight money market", "Compounding convention unknown", "EUR", "Not applicable", "ECB €STR series; exact production source mapping unknown", "Pre-€STR history unavailable/unknown"),
        ];
        for (id, name, universe, basis, currency, hedge, provenance, boundary) in targets {
            manager.get_connection().execute_unprepared(&format!(
                "INSERT INTO benchmark_references (id,display_name,instrument_ticker,source,source_identifier,currency,adjustment_provenance,return_basis_qualification,role,universe,hedge_policy,source_provenance,history_boundary,verification_state) VALUES ('{id}','{name}','','unverified','','{currency}','Target definition only; no observations admitted','{basis}','Investment universe','{universe}','{hedge}','{provenance}','{boundary}','target_only')"
            )).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(BenchmarkAssignments::Table).to_owned()).await?;
        manager.get_connection().execute_unprepared(
            "DELETE FROM benchmark_references WHERE id IN (
                'msci-world-net-eur-unhedged', 'msci-emerging-markets-net-eur-unhedged',
                'msci-acwi-small-cap-value-net-eur', 'estr-plus-120bp',
                'ice-euro-corporate-75-high-yield-25', 'bloomberg-euro-aggregate-1-3y',
                'swiss-re-global-cat-bond-tr', 'abaco-global-mixed-reference',
                'msci-europe-small-cap-net-eur', 'msci-usa-small-cap-net-eur',
                'bloomberg-global-aggregate-1-5y-eur-hedged', 'msci-europe-small-cap-value', 'estr'
            )"
        ).await?;
        for column in [
            BenchmarkReferences::ObjectiveComparator, BenchmarkReferences::OfficialComparator,
            BenchmarkReferences::VerificationState, BenchmarkReferences::HistoryBoundary,
            BenchmarkReferences::SourceProvenance, BenchmarkReferences::HedgePolicy,
            BenchmarkReferences::Universe, BenchmarkReferences::Role,
        ] {
            manager.alter_table(
                Table::alter().table(BenchmarkReferences::Table).drop_column(column).to_owned(),
            ).await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)] enum BenchmarkAssignments { Table, SubjectType, SubjectId, ReferenceId, IsOverride, AssignedAt }
#[derive(DeriveIden)] enum BenchmarkReferences { Table, Id, Role, Universe, HedgePolicy, SourceProvenance, HistoryBoundary, VerificationState, OfficialComparator, ObjectiveComparator }
