use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_assignments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub subject_type: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub subject_id: String,
    pub reference_id: String,
    pub is_override: bool,
    pub assigned_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
