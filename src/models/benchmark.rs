use serde::Serialize;

/// The availability of one comparison outcome. Available, unavailable, and not
/// applicable are explicit states rather than nullable placeholders over
/// shared fields.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ComparisonAvailability {
    Available,
    Unavailable { reason: String },
    NotApplicable { reason: String },
}

/// One dated point of an aligned wealth series.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DatedWealthPoint {
    pub date: String,
    pub wealth: f64,
}

/// The purpose of one selected reference definition.
///
/// A rendered comparison carries the meaning of its selected definition, not
/// only an opaque identifier, so that the reference stays inspectable: which
/// investment universe it describes, which official or objective comparator it
/// records, and which history boundary and production mapping it was verified
/// against.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReferencePurposeInfo {
    /// The financial role the definition plays, for example "Investment universe".
    pub role: String,
    /// The investment universe the definition describes.
    pub universe: String,
    /// The instrument the definition tracks, when it tracks one.
    pub instrument_ticker: String,
    /// The explicit history boundary of the definition.
    pub history_boundary: String,
    /// The production mapping verification state of the definition.
    pub verification_state: String,
    /// The official comparator the definition records, when it records one.
    pub official_comparator: Option<String>,
    /// The stated objective comparator the definition records, when it has one.
    pub objective_comparator: Option<String>,
}

/// The role a selected reference plays in one subject's comparison.
///
/// The role is a closed vocabulary: a reference is either the primary
/// reference of its subject or an optional secondary comparison beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonRole {
    /// The reference the subject is measured against.
    Primary,
    /// An optional diagnostic comparison beside the primary reference. It
    /// never replaces the primary reference and never contributes a portfolio
    /// exposure weight.
    Secondary,
}

/// The exact Fund candidate class of an analysed fund.
///
/// The vehicle and the exact ISIN identify the candidate class, so the
/// comparison states which candidate assignment produced it. It is a
/// research record of an exact class, not a holding: the tracked identity of
/// a class this portfolio does not track stays absent, and this identity
/// never borrows another class's identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CandidateClassIdentity {
    /// The vehicle of the exact candidate class, for example "fund".
    pub vehicle: String,
    /// The exact ISIN of the candidate class.
    pub identifier: String,
}

impl ComparisonRole {
    /// The role as it is shown to the user and serialized.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }
}

impl std::fmt::Display for ComparisonRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The standard comparison periods shared by every subject/reference
/// comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkComparisonPeriod {
    Ytd,
    OneYear,
    ThreeYear,
    FiveYear,
    SinceComparisonStart,
}

impl BenchmarkComparisonPeriod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ytd => "YTD",
            Self::OneYear => "1Y",
            Self::ThreeYear => "3Y",
            Self::FiveYear => "5Y",
            Self::SinceComparisonStart => "Since Comparison Start",
        }
    }

    /// The standard periods every comparison reports, in presentation order.
    pub fn standard_periods() -> [Self; 5] {
        [
            Self::Ytd,
            Self::OneYear,
            Self::ThreeYear,
            Self::FiveYear,
            Self::SinceComparisonStart,
        ]
    }
}

/// One calculated comparison fact. Available keeps the computed value,
/// unavailable states a missing required input, and not applicable states a
/// domain reason such as insufficient history or undefined mathematics.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ComparisonFact<T> {
    Available { value: T },
    Unavailable { reason: String },
    NotApplicable { reason: String },
}

impl<T> ComparisonFact<T> {
    pub fn available(value: T) -> Self {
        Self::Available { value }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn not_applicable(reason: impl Into<String>) -> Self {
        Self::NotApplicable {
            reason: reason.into(),
        }
    }
}

/// One standard comparison period's return and risk facts over the aligned
/// subject/reference series.
///
/// The subject is the compared portfolio or fund; the reference is the
/// benchmark it is measured against. Every calculated fact keeps its
/// independent availability: insufficient applicable history and
/// mathematically undefined metrics are not applicable, while a missing
/// required input makes only the dependent fact unavailable.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BenchmarkPeriod {
    pub period: BenchmarkComparisonPeriod,
    /// The date the period requested, which a trailing window may not reach.
    pub requested_start_date: Option<String>,
    /// The actual anchor observation the period opened at.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub subject_return_pct: ComparisonFact<f64>,
    pub reference_return_pct: ComparisonFact<f64>,
    pub excess_return_percentage_points: ComparisonFact<f64>,
    pub beta: ComparisonFact<f64>,
    pub correlation: ComparisonFact<f64>,
    /// Sample deviation of arithmetic active returns, annualized and expressed in percent.
    pub tracking_error_annualized_pct: ComparisonFact<f64>,
    /// Annualized information ratio from arithmetic daily active returns.
    pub information_ratio: ComparisonFact<f64>,
}

impl BenchmarkPeriod {
    pub fn not_applicable(period: BenchmarkComparisonPeriod, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            period,
            requested_start_date: None,
            start_date: None,
            end_date: None,
            subject_return_pct: ComparisonFact::not_applicable(reason.clone()),
            reference_return_pct: ComparisonFact::not_applicable(reason.clone()),
            excess_return_percentage_points: ComparisonFact::not_applicable(reason.clone()),
            beta: ComparisonFact::not_applicable(reason.clone()),
            correlation: ComparisonFact::not_applicable(reason.clone()),
            tracking_error_annualized_pct: ComparisonFact::not_applicable(reason.clone()),
            information_ratio: ComparisonFact::not_applicable(reason),
        }
    }

    pub fn unavailable(period: BenchmarkComparisonPeriod, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            period,
            requested_start_date: None,
            start_date: None,
            end_date: None,
            subject_return_pct: ComparisonFact::unavailable(reason.clone()),
            reference_return_pct: ComparisonFact::unavailable(reason.clone()),
            excess_return_percentage_points: ComparisonFact::unavailable(reason.clone()),
            beta: ComparisonFact::unavailable(reason.clone()),
            correlation: ComparisonFact::unavailable(reason.clone()),
            tracking_error_annualized_pct: ComparisonFact::unavailable(reason.clone()),
            information_ratio: ComparisonFact::unavailable(reason),
        }
    }
}
