use sea_orm::entity::prelude::*;

/// The single optional Strategy benchmark selection.
///
/// The row exists so a definition that is selected as a Strategy can never be
/// deleted out from under that selection.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_strategy_selection")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub reference_id: String,
    pub selected_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
