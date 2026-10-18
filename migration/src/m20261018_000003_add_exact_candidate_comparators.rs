use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)] pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(Table::create().table(BenchmarkCandidates::Table)
            .col(ColumnDef::new(BenchmarkCandidates::Vehicle).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::Identifier).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::DisplayName).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::OfficialComparator).string().null())
            .col(ColumnDef::new(BenchmarkCandidates::OfficialProvenance).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::OfficialDocumentDate).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveComparator).string().null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveProvenance).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveDocumentDate).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::HistoryBoundary).string().not_null())
            .primary_key(Index::create().col(BenchmarkCandidates::Vehicle).col(BenchmarkCandidates::Identifier))
            .to_owned()).await?;
        let candidates = [
            ("fund","IE000ZYRH0Q7","iShares Developed World S EUR Acc",Some("MSCI World Index Net (EUR); exact official tracking target"),"FUND-EXAMPLES.md §2; W2 factsheet September 2026; W3 Key Facts dated 2026-10-15","2026-09 factsheet; 2026-10-15 key facts",None,"Not specified in the exact-class evidence summarized here","Unknown","Class history starts 2025-08-21; no pre-class history authorized"),
            ("fund","IE00B03HD191","Vanguard Global Stock Index Fund, EUR Accumulation Shares",Some("MSCI World"),"Exact-class PRIIPs KID identifies this passive tracking target; exact net/gross return variant unknown","KID dated 2026-09-29; accessed 2026-10-17",None,"No separate official objective comparator established","Unknown","Exact class history boundary unknown"),
            ("fund","IE000QAZP7L2","iShares Emerging Markets S EUR Acc",Some("MSCI Emerging Markets, Net Returns (EUR); exact official tracking target"),"FUND-EXAMPLES.md §3; E2 factsheet September 2026; E3 Key Facts","2026-09 factsheet; 2026-10-15 key facts",None,"Not specified in the exact-class evidence summarized here","Unknown","Class history starts 2025-08-21; no pre-class history authorized"),
            ("fund","IE0031786696","Vanguard Emerging Markets Stock Index Fund, EUR Accumulation Shares",Some("MSCI Emerging Markets"),"Exact-class PRIIPs KID identifies this passive tracking target; exact net/gross return variant unknown","KID dated 2026-09-29; accessed 2026-10-17",None,"No separate official objective comparator established","Unknown","Exact class history boundary unknown"),
            ("fund","LU1598719752","Cobas International P Acc EUR",Some("MSCI Europe Total Net Return; exact series code/currency convention unreconciled"),"FUND-EXAMPLES.md §4; L1 KID 2023-01-01; L2 prospectus 2021-06 (visa 2021-07-01); L3 factsheet 2026-08","2026-08 factsheet; legal docs 2021/2023",None,"No manager objective comparator reported separately in cited evidence","Unknown","Exact class return history boundary not established by these sources"),
            ("fund","ES0119199018","Cobas Internacional FI D",Some("Bloomberg Europe Developed Markets Large & Mid Cap Net Return Index EUR (EDMNE Index); net-of-tax dividend reinvestment; comparison only"),"FUND-EXAMPLES.md §5; C1 prospectus updated 2026-06-22; C2 KID 2025-12-31","2026-06-22 prospectus; 2025-12-31 KID",None,"No separate manager objective comparator reported","Unknown","Manager class performance dates from 2021; index-change effective date/history unknown"),
            ("fund","LU3038481936","Hamco Global Value R EUR",None,"Official benchmark: none. Exact-class KID says active without an index; August 2026 prospectus explicitly says the Sub-Fund does not use a benchmark (FUND-EXAMPLES.md §6, H1/H2)","KID dated 2025-12-27; prospectus August 2026 (CSSF visa 2026-07-06); accessed 2026-10-17",None,"No separate official objective comparator established; do not treat the high-water mark as an index hurdle","Unknown","Exact class boundary unknown; no predecessor history implied"),
            ("fund","LU1694789535","DNCA Invest Alpha Bonds B EUR",None,"Official comparator not established here","Unknown",Some("€STR + 1.20%"),"Official exact B Share-EUR monthly management report: objective net of fees above €STR + 1.20% over recommended period >3 years; reference chart names same hurdle. Discretionary management, not passive global-bond tracking. Spread accrual/compounding/day-count unverified.","September 2026 report; data as of 2026-09-30; accessed 2026-10-17","Pre-€STR construction and history unknown"),
            ("fund","LU1623762843","Carmignac Portfolio Credit A EUR Acc",Some("75% ICE BofA Euro Corporate Index + 25% ICE BofA Euro High Yield Index"),"Official Q3 2026 letter specifically cites LU1623762843; exact component series variants, reset/rebalance conventions and feed availability unverified. Primary publisher text inspected via search; direct document fetch unavailable.","Published 2026-10-13; snapshot data 2026-09-30; accessed 2026-10-17",None,"No separate objective comparator established in this evidence","Unknown","Exact-class and composite history boundaries unknown"),
            ("fund","IE00BFZMJT78","Neuberger Short Duration Euro Bond EUR I Acc",None,"Official comparator exact-class convention not established here","Unknown",None,"No objective comparator recorded in this ticket evidence","Unknown","Exact class history boundary unknown"),
            ("fund","IE00BD2B9710","Twelve Cat Bond P EUR Acc",None,"Official comparator class/variant not verified here","Unknown",None,"No objective comparator recorded in this ticket evidence","Unknown","Class inception and observation-frequency boundary unknown"),
            ("fund","ES0140072028","Ábaco Renta Fija Mixta Global C",None,"Official comparator variants not reconciled here","Unknown",None,"No separate manager objective recorded here","Unknown","Exact class and component history boundaries unknown"),
        ];
        for (vehicle, identifier, name, official, official_source, official_date, objective, objective_source, objective_date, history) in candidates {
            let official_sql = official.map(|value| format!("'{value}'")).unwrap_or_else(|| "NULL".to_owned());
            let objective_sql = objective.map(|value| format!("'{value}'")).unwrap_or_else(|| "NULL".to_owned());
            manager.get_connection().execute_unprepared(&format!("INSERT INTO benchmark_candidates (vehicle,identifier,display_name,official_comparator,official_provenance,official_document_date,objective_comparator,objective_provenance,objective_document_date,history_boundary) VALUES ('{vehicle}','{identifier}','{name}',{official_sql},'{official_source}','{official_date}',{objective_sql},'{objective_source}','{objective_date}','{history}')")).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(BenchmarkCandidates::Table).to_owned()).await
    }
}

#[derive(DeriveIden)] enum BenchmarkCandidates { Table, Vehicle, Identifier, DisplayName, OfficialComparator, OfficialProvenance, OfficialDocumentDate, ObjectiveComparator, ObjectiveProvenance, ObjectiveDocumentDate, HistoryBoundary }
