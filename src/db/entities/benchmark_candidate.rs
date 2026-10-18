use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_candidates")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub vehicle: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub identifier: String,
    pub display_name: String,
    pub official_comparator: Option<String>,
    pub official_provenance: String,
    pub objective_comparator: Option<String>,
    pub objective_provenance: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
