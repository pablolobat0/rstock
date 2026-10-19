use serde::Serialize;

use super::benchmark::{
    BenchmarkComparisonPeriod, BenchmarkPeriod, ComparisonAvailability, DatedWealthPoint,
};
use super::MarketDataLimitation;

/// The one contributing primary reference of an exposure-matched comparison.
///
/// Several held assets may share one reference: their weights are aggregated
/// into a single contribution instead of being counted twice.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExposureBenchmarkReference {
    pub reference_id: String,
    pub display_name: String,
    /// The tracked assets whose historical exposure this reference carries.
    pub assets: Vec<String>,
    /// The aggregated portfolio weight of this reference at the comparison
    /// anchor, in percent of the complete NAV scope.
    pub first_weight_pct: f64,
    /// The aggregated portfolio weight of this reference at the comparison end,
    /// in percent of the complete NAV scope.
    pub last_weight_pct: f64,
    /// The highest aggregated weight this reference reached in the window.
    pub max_weight_pct: f64,
    /// The currency of the persisted reference prices before conversion.
    pub native_currency: String,
    /// The currency of both compared series and returns.
    pub comparison_currency: String,
    pub return_basis: String,
    pub adjustment: String,
    pub hedge_policy: String,
    pub cadence: Option<String>,
    pub source: String,
    pub source_identifier: String,
    pub source_provenance: String,
    /// The verification state of the reference definition.
    pub verification_state: String,
    /// When the explicit assignment was recorded. The assignment applies
    /// retrospectively to the whole comparison window.
    pub assigned_at: Option<String>,
}

/// The seed-to-first-close interval that the comparison omits.
///
/// NAV is unitized at the first deposit, so the seed closing snapshot that
/// precedes the first invested close carries no comparable value. The
/// comparison anchors at the first invested close and discloses the omitted
/// interval instead of extending the comparison backwards over it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OmittedSeedInterval {
    /// The zero-value seed closing snapshot.
    pub seed_date: String,
    /// The first invested closing snapshot that anchors the comparison.
    pub anchor_date: String,
}

/// The exposure-matched portfolio comparison outcome.
///
/// The comparison is the primary benchmark of the Portfolio dashboard. It
/// combines the explicitly assigned primary reference of every held asset
/// through the EUR exposures of the preceding Complete NAV snapshot, so
/// today's holdings are never projected backwards over past performance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExposureBenchmarkComparison {
    pub availability: ComparisonAvailability,
    /// The comparison role: the primary exposure-matched benchmark.
    pub role: String,
    /// The calculation method of this comparison.
    pub method: String,
    /// The subject of the comparison: the portfolio itself.
    pub subject_label: String,
    /// The reference of the comparison: the exposure-matched benchmark.
    pub reference_label: String,
    /// The currency of both compared series and returns.
    pub currency: String,
    /// The first invested closing snapshot that anchors both series at 100.
    pub start_date: Option<String>,
    /// The last Complete NAV snapshot of the comparison.
    pub end_date: Option<String>,
    /// The earliest Complete NAV snapshot, which bounds NAV's own inception.
    pub inception_date: Option<String>,
    /// The seed-to-first-close interval the comparison omits.
    pub omitted_seed_interval: Option<OmittedSeedInterval>,
    /// Explicit assignments apply to the whole retrospective window; no
    /// effective-dated chain or splice is used.
    pub retrospective: bool,
    pub subject_return_pct: Option<f64>,
    pub reference_return_pct: Option<f64>,
    pub excess_return_percentage_points: Option<f64>,
    /// The portfolio NAV rebased to 100 at the comparison anchor.
    pub subject_wealth: Vec<DatedWealthPoint>,
    /// The exposure-weighted reference rebased to 100 at the comparison anchor.
    pub reference_wealth: Vec<DatedWealthPoint>,
    /// The YTD, 1Y, 3Y, 5Y, and since-comparison-start rows, independent of
    /// the graph period.
    pub standard_periods: Vec<BenchmarkPeriod>,
    /// The contributing references with their provenance and weights.
    pub references: Vec<ExposureBenchmarkReference>,
    /// Market data limitations scoped to the reference series.
    pub reference_limitations: Vec<MarketDataLimitation>,
}

impl ExposureBenchmarkComparison {
    /// The comparison outcome for a portfolio that cannot carry one: an empty
    /// or Monetary-only portfolio has no invested closing snapshot.
    pub fn not_applicable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            availability: ComparisonAvailability::NotApplicable {
                reason: reason.clone(),
            },
            role: "primary".to_owned(),
            method: "exposure_matched".to_owned(),
            subject_label: "Portfolio".to_owned(),
            reference_label: "Exposure-matched benchmark".to_owned(),
            currency: crate::constants::BASE_CURRENCY.to_owned(),
            start_date: None,
            end_date: None,
            inception_date: None,
            omitted_seed_interval: None,
            retrospective: true,
            subject_return_pct: None,
            reference_return_pct: None,
            excess_return_percentage_points: None,
            subject_wealth: Vec::new(),
            reference_wealth: Vec::new(),
            standard_periods: BenchmarkComparisonPeriod::standard_periods()
                .into_iter()
                .map(|period| BenchmarkPeriod::not_applicable(period, reason.clone()))
                .collect(),
            references: Vec::new(),
            reference_limitations: Vec::new(),
        }
    }

    /// The comparison outcome when a required benchmark input is missing or a
    /// benchmark failure isolates the comparison from the rest of the
    /// Portfolio view.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            availability: ComparisonAvailability::Unavailable {
                reason: reason.clone(),
            },
            role: "primary".to_owned(),
            method: "exposure_matched".to_owned(),
            subject_label: "Portfolio".to_owned(),
            reference_label: "Exposure-matched benchmark".to_owned(),
            currency: crate::constants::BASE_CURRENCY.to_owned(),
            start_date: None,
            end_date: None,
            inception_date: None,
            omitted_seed_interval: None,
            retrospective: true,
            subject_return_pct: None,
            reference_return_pct: None,
            excess_return_percentage_points: None,
            subject_wealth: Vec::new(),
            reference_wealth: Vec::new(),
            standard_periods: BenchmarkComparisonPeriod::standard_periods()
                .into_iter()
                .map(|period| BenchmarkPeriod::unavailable(period, reason.clone()))
                .collect(),
            references: Vec::new(),
            reference_limitations: Vec::new(),
        }
    }
}
