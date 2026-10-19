use serde::Serialize;

/// One declared component of a saved Custom benchmark blend.
///
/// `target_weight` is the stored fraction of the blend; `target_weight_percent`
/// is the same value in the percentage unit the user declares. The remaining
/// fields are the component's convention snapshot: the exact source and lookup
/// identity, the adjustment basis, the observation cadence, the currency, the
/// return basis, and the hedge policy the blend was admitted with. A saved
/// blend keeps this meaning instead of an opaque identifier.
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
    pub adjustment: String,
    pub hedge_policy: String,
    pub universe: String,
    pub source: String,
    pub source_identifier: String,
    pub cadence: String,
    pub source_provenance: String,
    pub verification_state: String,
}

/// One saved, immutable Custom benchmark blend definition.
///
/// The requested dates are the comparison boundary the blend was asked for;
/// the admitted dates are the actual anchor and end closes it was admitted
/// at. Both are persisted so a saved definition never claims a window it did
/// not compute.
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
    pub requested_start_date: String,
    pub requested_end_date: String,
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
    /// The comparison start the saved definition was admitted at.
    pub admitted_start_date: String,
    /// The comparison end the saved definition was admitted at.
    pub admitted_end_date: String,
    pub admission_basis: String,
    pub components: Vec<BlendComponent>,
    /// The start of the previewed window.
    pub requested_start_date: String,
    /// The end of the previewed window.
    pub requested_end_date: String,
    /// The actual anchor close the preview starts at.
    pub anchor_date: String,
    /// The actual last close the preview reaches.
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
