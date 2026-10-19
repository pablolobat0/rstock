use sea_orm::entity::prelude::*;

/// One component of a saved Custom benchmark blend.
///
/// The component carries a convention snapshot so the saved blend keeps an
/// explicit meaning for each component. Cash is synthetic: it has no reference
/// identity and earns a 0% return.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_blend_components")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub blend_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub component_key: String,
    pub component_reference_id: Option<String>,
    pub component_display_name: String,
    /// The declared position of the component inside the blend.
    pub position: i32,
    pub target_weight: f64,
    pub is_cash: bool,
    pub component_currency: String,
    pub component_return_basis: String,
    pub component_hedge_policy: String,
    pub component_universe: String,
    pub component_source_provenance: String,
    pub component_verification_state: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
