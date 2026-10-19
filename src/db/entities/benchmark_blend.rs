use sea_orm::entity::prelude::*;

/// One saved Custom benchmark blend definition.
///
/// A blend is synthetic reference wealth: a flat, long-only, fully allocated
/// target definition that is immutable once saved. The definition identity
/// lives in the reference catalog; these are the blend-specific facts.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_blends")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub reference_id: String,
    pub created_at: String,
    pub reset_rule: String,
    pub weight_convention: String,
    pub cash_convention: String,
    pub weight_tolerance: f64,
    /// The comparison start the blend was requested against.
    pub requested_start_date: String,
    /// The comparison end the blend was requested against.
    pub requested_end_date: String,
    /// The actual anchor the blend was admitted at.
    pub admitted_start_date: String,
    /// The actual end the blend was admitted at.
    pub admitted_end_date: String,
    pub admission_basis: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
