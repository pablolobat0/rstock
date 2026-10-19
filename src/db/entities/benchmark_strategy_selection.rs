use sea_orm::entity::prelude::*;

/// The explicitly selected Strategy benchmark.
///
/// A selection is a reference operation: it records which saved definition the
/// portfolio is compared against, and it never holds a target allocation, a
/// weight, or anything derived from the portfolio's holdings.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_strategy_selections")]
pub struct Model {
    /// The single selection slot. The primary key holds one row at most, so
    /// the database itself enforces that one Strategy benchmark is selected.
    #[sea_orm(primary_key, auto_increment = false)]
    pub selection_slot: String,
    /// The selected benchmark definition identity.
    pub reference_id: String,
    /// When the selection was made.
    pub selected_at: String,
    /// Why the definition is selected; the selection is always explicit.
    pub selection_basis: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
