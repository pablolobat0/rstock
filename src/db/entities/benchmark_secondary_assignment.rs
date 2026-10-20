use sea_orm::entity::prelude::*;

/// One explicit secondary benchmark assignment.
///
/// A secondary assignment is a diagnostic comparison that sits alongside the
/// subject's primary reference. It is a separate record from the primary
/// assignment, so a secondary can never replace a primary and the
/// exposure-matched portfolio benchmark never reads it.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_secondary_assignments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub subject_type: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub subject_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub reference_id: String,
    /// When the explicit secondary assignment was recorded. It applies
    /// retrospectively to the whole comparison window.
    pub assigned_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
