use crate::models::{AssetType, MarketDataLimitation};

/// The state of one calculated financial fact, per the Portfolio fact
/// availability vocabulary: available with a value, not applicable for a
/// domain reason, or unavailable because a required input is missing.
///
/// Availability expresses expected domain absence only. Database failures,
/// malformed persisted data, and violated ledger invariants remain errors.
#[derive(Debug, Clone, PartialEq)]
pub enum FactAvailability<T> {
    Available(T),
    Unavailable,
    NotApplicable,
}

impl<T> FactAvailability<T> {
    /// The fact's value when it is available.
    pub fn value(&self) -> Option<&T> {
        match self {
            FactAvailability::Available(value) => Some(value),
            FactAvailability::Unavailable | FactAvailability::NotApplicable => None,
        }
    }
}

/// One asset's latest available quoted price for display, with the date of the
/// observation it came from. Its availability is independent of other facts.
#[derive(Debug, Clone, PartialEq)]
pub struct IndividualPrice {
    pub native_price: f64,
    pub price_date: String,
}

/// One currently held Tracked asset inside a Portfolio inventory section.
///
/// Descriptive metadata is plain: its absence has its own meaning and is not
/// wrapped in financial availability. Calculated financial facts carry their
/// own `FactAvailability` so a missing input hides only the dependent fact.
#[derive(Clone)]
pub struct InventoryPosition {
    pub ticker: String,
    pub name: String,
    pub asset_type: AssetType,
    pub currency: String,
    pub morningstar_code: Option<String>,
    pub asset_class: Option<String>,
    pub equity_style: Option<String>,
    pub management: Option<String>,
    /// Ledger-derived quantity; always known for an included position.
    pub quantity: f64,
    pub average_cost: FactAvailability<f64>,
    /// Remaining invested Base currency cost, unavailable when the
    /// transaction-date FX for any contributing entry is missing.
    pub invested_cost: FactAvailability<f64>,
    /// Lifetime Net dividend income, unavailable when the transaction-date FX
    /// for any dividend entry is missing.
    pub dividends: FactAvailability<f64>,
    pub individual_price: FactAvailability<IndividualPrice>,
    pub current_value: FactAvailability<f64>,
    pub open_position_gain_loss: FactAvailability<f64>,
    pub open_position_gain_loss_pct: FactAvailability<f64>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
}

/// The complete-or-unavailable aggregates of one Portfolio inventory section.
/// Known per-position facts remain visible when an aggregate is unavailable.
#[derive(Debug, Clone, PartialEq)]
pub struct InventorySectionAggregates {
    pub current_value: FactAvailability<f64>,
    pub invested_cost: FactAvailability<f64>,
    pub dividends: FactAvailability<f64>,
    pub open_position_gain_loss: FactAvailability<f64>,
    pub open_position_gain_loss_pct: FactAvailability<f64>,
}

/// One Portfolio inventory section: its positions, aggregates, and the Market
/// data limitations scoped to that section.
pub struct PortfolioInventorySection {
    pub positions: Vec<InventoryPosition>,
    pub aggregates: InventorySectionAggregates,
    pub market_data_limitations: Vec<MarketDataLimitation>,
}

/// Current Transaction ledger holdings with separately owned
/// performance-holding and Monetary-holding sections. Always returned, even
/// when every fact is unavailable. The combined informational Total value
/// stays outside portfolio performance measurement.
pub struct PortfolioInventory {
    pub base_currency: String,
    pub performance_holdings: PortfolioInventorySection,
    pub monetary_holdings: PortfolioInventorySection,
    pub total_value: FactAvailability<f64>,
}

#[cfg(test)]
#[path = "../../tests/unit/fact_availability_tests.rs"]
mod fact_availability_tests;
