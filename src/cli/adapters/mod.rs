//! Portfolio CLI output Adapters (ADR-0004).
//!
//! Separate human and JSON Adapters present one complete, presentation-neutral
//! [`PortfolioView`] through an injected `Write` target. Presentation choices
//! such as sorting, field names, labels, tables, and the NAV chart stay here;
//! the domain outcome carries no serialization.

pub mod human;
pub mod json;

pub use human::write_portfolio_human;
pub use json::write_portfolio_json;

/// Orders position rows by current value with unvalued positions last. The
/// ordering is an Adapter choice shared by both output formats so they never
/// drift apart.
pub(super) fn sort_position_refs_by_current_value(
    positions: &mut [&crate::models::InventoryPosition],
) {
    positions.sort_by(|left, right| {
        right
            .current_value
            .value()
            .copied()
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(
                &left
                    .current_value
                    .value()
                    .copied()
                    .unwrap_or(f64::NEG_INFINITY),
            )
    });
}
