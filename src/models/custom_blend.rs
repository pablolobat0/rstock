use serde::Serialize;

/// One declared component of a saved Custom benchmark blend.
///
/// `target_weight` is the stored fraction of the blend; `target_weight_percent`
/// is the same value in the percentage unit the user declares. The remaining
/// fields are the component's convention snapshot: the blend keeps an explicit
/// meaning for each component instead of an opaque identifier.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BlendComponent {
    pub component_key: String,
    pub component_reference_id: Option<String>,
    pub display_name: String,
    pub target_weight: f64,
    pub target_weight_percent: f64,
    pub is_cash: bool,
    pub currency: String,
    pub return_basis: String,
    pub hedge_policy: String,
    pub universe: String,
    pub source_provenance: String,
    pub verification_state: String,
}

/// One saved, immutable Custom benchmark blend definition.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CustomBlendDefinition {
    pub id: String,
    pub display_name: String,
    pub created_at: String,
    pub currency: String,
    pub reset_rule: String,
    pub weight_convention: String,
    pub cash_convention: String,
    pub weight_tolerance: f64,
    pub admitted_start_date: String,
    pub admitted_end_date: String,
    pub admission_basis: String,
    pub components: Vec<BlendComponent>,
}

/// One dated point of a rebased Custom blend wealth series.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BlendWealthPoint {
    pub date: String,
    pub wealth: f64,
}

/// One monthly target reset: the close at which targets were reset and the
/// first interval the reset governs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BlendTargetReset {
    pub reset_date: String,
    pub effective_from_date: String,
}

/// The dated, rebased preview of one saved Custom benchmark blend.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CustomBlendPreview {
    pub blend_id: String,
    pub display_name: String,
    pub currency: String,
    pub reset_rule: String,
    pub weight_convention: String,
    pub cash_convention: String,
    pub weight_tolerance: f64,
    pub admitted_start_date: String,
    pub admitted_end_date: String,
    pub admission_basis: String,
    pub components: Vec<BlendComponent>,
    pub window_start_date: String,
    pub window_end_date: String,
    pub anchor_date: String,
    pub last_observation_date: String,
    pub rebased_to: f64,
    pub cumulative_return_pct: f64,
    pub wealth: Vec<BlendWealthPoint>,
    pub target_resets: Vec<BlendTargetReset>,
    pub earlier_period_limitation: Option<String>,
    pub accounting_note: String,
}

/// The outcome of deleting one saved Custom benchmark blend.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CustomBlendDeletion {
    pub blend_id: String,
    pub display_name: String,
    pub deleted: bool,
    pub preserved_history_note: String,
}

/// A declared component of a blend as the user wrote it, before validation.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendComponentArgument {
    pub identity: String,
    pub weight_percent: f64,
}
