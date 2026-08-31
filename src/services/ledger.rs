#![allow(dead_code)] // Public replay seam; consumers are migrated in follow-up ledger issues.

//! Pure canonical replay for one asset's transaction ledger.
//!
//! Persistence establishes transaction identities; this module establishes their
//! chronological meaning. Monetary values are in the tracked asset's native
//! currency and deliberately have no market-data or database dependency.

use std::fmt;

use crate::constants::FLOAT_EPSILON;

/// A transaction whose data shape is constrained by its type.
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerEntry {
    pub id: i32,
    pub asset_id: i32,
    pub date: String,
    pub kind: LedgerEntryKind,
}

/// The type-specific fields of a transaction ledger entry.
#[derive(Clone, Debug, PartialEq)]
pub enum LedgerEntryKind {
    Buy {
        units: f64,
        unit_price: f64,
        fees: f64,
    },
    Sell {
        units: f64,
        unit_price: f64,
        fees: f64,
    },
    Dividend {
        gross_amount: f64,
        deductions: f64,
    },
    Split {
        ratio: f64,
    },
}

impl LedgerEntryKind {
    #[must_use]
    pub fn entry_type(&self) -> LedgerEntryType {
        match self {
            Self::Buy { .. } => LedgerEntryType::Buy,
            Self::Sell { .. } => LedgerEntryType::Sell,
            Self::Dividend { .. } => LedgerEntryType::Dividend,
            Self::Split { .. } => LedgerEntryType::Split,
        }
    }
}

/// A compact, stable description of an entry's type for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerEntryType {
    Buy,
    Sell,
    Dividend,
    Split,
}

impl fmt::Display for LedgerEntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buy => write!(f, "buy"),
            Self::Sell => write!(f, "sell"),
            Self::Dividend => write!(f, "dividend"),
            Self::Split => write!(f, "split"),
        }
    }
}

/// An opaque, canonical `(date, id)` ordering for a single asset ledger.
#[derive(Clone, Debug)]
pub struct CanonicalLedger {
    asset_id: i32,
    entries: Vec<LedgerEntry>,
}

impl CanonicalLedger {
    /// Validates identity integrity and establishes the only replay order.
    pub fn new(asset_id: i32, mut entries: Vec<LedgerEntry>) -> Result<Self, LedgerError> {
        entries.sort_by(|left, right| left.date.cmp(&right.date).then(left.id.cmp(&right.id)));

        let mut seen_ids = std::collections::HashSet::new();
        for entry in &entries {
            if asset_id <= 0 {
                return Err(LedgerError::for_entry(
                    entry,
                    0.0,
                    LedgerAttempt::Identity,
                    LedgerInvariant::PositiveAssetIdentity,
                ));
            }
            if entry.asset_id != asset_id {
                return Err(LedgerError::for_entry(
                    entry,
                    0.0,
                    LedgerAttempt::Identity,
                    LedgerInvariant::MatchingAssetIdentity,
                ));
            }
            if entry.id <= 0 || !seen_ids.insert(entry.id) {
                return Err(LedgerError::for_entry(
                    entry,
                    0.0,
                    LedgerAttempt::Identity,
                    LedgerInvariant::UniquePositiveEntryIdentity,
                ));
            }
        }

        Ok(Self { asset_id, entries })
    }

    #[must_use]
    pub fn asset_id(&self) -> i32 {
        self.asset_id
    }

    #[must_use]
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Replays every prefix, returning transitions only when the entire ledger is valid.
    #[allow(clippy::too_many_lines)] // Keeping variant validation beside its transition preserves replay locality.
    pub fn replay(&self) -> Result<LedgerReplay, LedgerError> {
        let mut quantity = 0.0;
        let mut remaining_cost = 0.0;
        let mut transitions = Vec::with_capacity(self.entries.len());

        for entry in &self.entries {
            let quantity_before = quantity;
            let remaining_cost_before = remaining_cost;
            let (quantity_after, remaining_cost_after, effect) = match &entry.kind {
                LedgerEntryKind::Buy {
                    units,
                    unit_price,
                    fees,
                } => {
                    validate_positive(entry, quantity_before, *units, LedgerAttempt::Units)?;
                    validate_positive(
                        entry,
                        quantity_before,
                        *unit_price,
                        LedgerAttempt::UnitPrice,
                    )?;
                    validate_non_negative(entry, quantity_before, *fees, LedgerAttempt::Fees)?;
                    let contribution = units * unit_price + fees;
                    validate_finite(
                        entry,
                        quantity_before,
                        contribution,
                        LedgerAttempt::Contribution,
                    )?;
                    (
                        quantity_before + units,
                        remaining_cost_before + contribution,
                        LedgerEffect::Buy { contribution },
                    )
                }
                LedgerEntryKind::Sell {
                    units,
                    unit_price,
                    fees,
                } => {
                    validate_positive(entry, quantity_before, *units, LedgerAttempt::Units)?;
                    validate_positive(
                        entry,
                        quantity_before,
                        *unit_price,
                        LedgerAttempt::UnitPrice,
                    )?;
                    validate_non_negative(entry, quantity_before, *fees, LedgerAttempt::Fees)?;
                    let quantity_after = normalize_quantity(quantity_before - units);
                    if quantity_after < 0.0 {
                        return Err(LedgerError::for_entry(
                            entry,
                            quantity_before,
                            LedgerAttempt::SellUnits(*units),
                            LedgerInvariant::NonNegativeQuantity,
                        ));
                    }
                    let withdrawal = units * unit_price - fees;
                    validate_finite(
                        entry,
                        quantity_before,
                        withdrawal,
                        LedgerAttempt::Withdrawal,
                    )?;
                    let cost_removed = remaining_cost_before * (units / quantity_before);
                    validate_finite(
                        entry,
                        quantity_before,
                        cost_removed,
                        LedgerAttempt::CostRemoval,
                    )?;
                    (
                        quantity_after,
                        remaining_cost_before - cost_removed,
                        LedgerEffect::Sell {
                            withdrawal,
                            cost_removed,
                        },
                    )
                }
                LedgerEntryKind::Dividend {
                    gross_amount,
                    deductions,
                } => {
                    validate_positive(
                        entry,
                        quantity_before,
                        *gross_amount,
                        LedgerAttempt::GrossDividend,
                    )?;
                    validate_non_negative(
                        entry,
                        quantity_before,
                        *deductions,
                        LedgerAttempt::DividendDeductions(*deductions),
                    )?;
                    require_open_quantity(entry, quantity_before)?;
                    if deductions > gross_amount {
                        return Err(LedgerError::for_entry(
                            entry,
                            quantity_before,
                            LedgerAttempt::DividendDeductions(*deductions),
                            LedgerInvariant::DeductionsDoNotExceedGrossDividend,
                        ));
                    }
                    let net_income = gross_amount - deductions;
                    (
                        quantity_before,
                        remaining_cost_before,
                        LedgerEffect::Dividend { net_income },
                    )
                }
                LedgerEntryKind::Split { ratio } => {
                    validate_positive(entry, quantity_before, *ratio, LedgerAttempt::SplitRatio)?;
                    require_open_quantity(entry, quantity_before)?;
                    let quantity_after = normalize_quantity(quantity_before * ratio);
                    validate_finite(
                        entry,
                        quantity_before,
                        quantity_after,
                        LedgerAttempt::SplitResult,
                    )?;
                    (
                        quantity_after,
                        remaining_cost_before,
                        LedgerEffect::Split { ratio: *ratio },
                    )
                }
            };

            validate_finite(entry, quantity_before, quantity_after, LedgerAttempt::Units)?;
            validate_finite(
                entry,
                quantity_before,
                remaining_cost_after,
                LedgerAttempt::Contribution,
            )?;

            quantity = normalize_quantity(quantity_after);
            remaining_cost = normalize_cost(remaining_cost_after);
            transitions.push(LedgerTransition {
                entry: entry.clone(),
                quantity_before,
                quantity_after: quantity,
                remaining_cost_before,
                remaining_cost_after: remaining_cost,
                effect,
            });
        }

        Ok(LedgerReplay {
            transitions,
            final_quantity: quantity,
            remaining_cost,
        })
    }
}

/// One validated state transition and its native-currency effect.
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerTransition {
    pub entry: LedgerEntry,
    pub quantity_before: f64,
    pub quantity_after: f64,
    pub remaining_cost_before: f64,
    pub remaining_cost_after: f64,
    pub effect: LedgerEffect,
}

/// Type-specific semantic effects emitted by replay.
#[derive(Clone, Debug, PartialEq)]
pub enum LedgerEffect {
    Buy { contribution: f64 },
    Sell { withdrawal: f64, cost_removed: f64 },
    Dividend { net_income: f64 },
    Split { ratio: f64 },
}

/// A complete replay result. It is never returned for an invalid prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerReplay {
    pub transitions: Vec<LedgerTransition>,
    pub final_quantity: f64,
    pub remaining_cost: f64,
}

/// The attempted operation recorded in an actionable replay error.
#[derive(Clone, Debug, PartialEq)]
pub enum LedgerAttempt {
    Identity,
    Units,
    SellUnits(f64),
    UnitPrice,
    Fees,
    GrossDividend,
    DividendDeductions(f64),
    SplitRatio,
    Contribution,
    Withdrawal,
    CostRemoval,
    SplitResult,
}

/// The invariant violated by an invalid entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerInvariant {
    PositiveAssetIdentity,
    MatchingAssetIdentity,
    UniquePositiveEntryIdentity,
    FiniteValue,
    PositiveValue,
    NonNegativeValue,
    NonNegativeQuantity,
    OpenQuantityRequired,
    DeductionsDoNotExceedGrossDividend,
}

/// Context for the first invalid canonical prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerError {
    pub asset_id: i32,
    pub entry_id: i32,
    pub date: String,
    pub entry_type: LedgerEntryType,
    pub quantity_before: f64,
    pub attempted_effect: LedgerAttempt,
    pub violated_invariant: LedgerInvariant,
}

impl LedgerError {
    fn for_entry(
        entry: &LedgerEntry,
        quantity_before: f64,
        attempted_effect: LedgerAttempt,
        violated_invariant: LedgerInvariant,
    ) -> Self {
        Self {
            asset_id: entry.asset_id,
            entry_id: entry.id,
            date: entry.date.clone(),
            entry_type: entry.kind.entry_type(),
            quantity_before,
            attempted_effect,
            violated_invariant,
        }
    }
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ledger invariant {:?} failed for asset {} entry {} ({}) on {} with prior quantity {} while attempting {:?}",
            self.violated_invariant,
            self.asset_id,
            self.entry_id,
            self.entry_type,
            self.date,
            self.quantity_before,
            self.attempted_effect,
        )
    }
}

impl std::error::Error for LedgerError {}

fn validate_positive(
    entry: &LedgerEntry,
    quantity_before: f64,
    value: f64,
    attempted_effect: LedgerAttempt,
) -> Result<(), LedgerError> {
    if !value.is_finite() {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            attempted_effect,
            LedgerInvariant::FiniteValue,
        ))
    } else if value <= 0.0 {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            attempted_effect,
            LedgerInvariant::PositiveValue,
        ))
    } else {
        Ok(())
    }
}

fn validate_non_negative(
    entry: &LedgerEntry,
    quantity_before: f64,
    value: f64,
    attempted_effect: LedgerAttempt,
) -> Result<(), LedgerError> {
    if !value.is_finite() {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            attempted_effect,
            LedgerInvariant::FiniteValue,
        ))
    } else if value < 0.0 {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            attempted_effect,
            LedgerInvariant::NonNegativeValue,
        ))
    } else {
        Ok(())
    }
}

fn validate_finite(
    entry: &LedgerEntry,
    quantity_before: f64,
    value: f64,
    attempted_effect: LedgerAttempt,
) -> Result<(), LedgerError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            attempted_effect,
            LedgerInvariant::FiniteValue,
        ))
    }
}

fn require_open_quantity(entry: &LedgerEntry, quantity_before: f64) -> Result<(), LedgerError> {
    if quantity_before > FLOAT_EPSILON {
        Ok(())
    } else {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            LedgerAttempt::Units,
            LedgerInvariant::OpenQuantityRequired,
        ))
    }
}

fn normalize_quantity(quantity: f64) -> f64 {
    if quantity.abs() <= FLOAT_EPSILON {
        0.0
    } else {
        quantity
    }
}

fn normalize_cost(cost: f64) -> f64 {
    if cost.abs() <= FLOAT_EPSILON {
        0.0
    } else {
        cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i32, date: &str, kind: LedgerEntryKind) -> LedgerEntry {
        LedgerEntry {
            id,
            asset_id: 7,
            date: date.to_owned(),
            kind,
        }
    }

    #[test]
    fn canonicalizes_same_day_entries_and_emits_typed_effects() {
        let ledger = CanonicalLedger::new(
            7,
            vec![
                entry(
                    3,
                    "2025-01-02",
                    LedgerEntryKind::Sell {
                        units: 2.0,
                        unit_price: 20.0,
                        fees: 1.0,
                    },
                ),
                entry(2, "2025-01-01", LedgerEntryKind::Split { ratio: 2.0 }),
                entry(
                    1,
                    "2025-01-01",
                    LedgerEntryKind::Buy {
                        units: 2.0,
                        unit_price: 10.0,
                        fees: 1.0,
                    },
                ),
                entry(
                    4,
                    "2025-01-02",
                    LedgerEntryKind::Dividend {
                        gross_amount: 5.0,
                        deductions: 1.0,
                    },
                ),
            ],
        )
        .unwrap();

        let replay = ledger.replay().unwrap();
        assert_eq!(
            ledger
                .entries()
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(replay.final_quantity, 2.0);
        assert_eq!(replay.remaining_cost, 10.5);
        assert_eq!(
            replay.transitions[0].effect,
            LedgerEffect::Buy { contribution: 21.0 }
        );
        assert_eq!(
            replay.transitions[2].effect,
            LedgerEffect::Sell {
                withdrawal: 39.0,
                cost_removed: 10.5
            }
        );
        assert_eq!(
            replay.transitions[3].effect,
            LedgerEffect::Dividend { net_income: 4.0 }
        );
    }

    #[test]
    fn rejects_duplicate_entry_identities() {
        let result = CanonicalLedger::new(
            7,
            vec![
                entry(
                    1,
                    "2025-01-01",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price: 1.0,
                        fees: 0.0,
                    },
                ),
                entry(
                    1,
                    "2025-01-02",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price: 1.0,
                        fees: 0.0,
                    },
                ),
            ],
        );

        assert_eq!(
            result.unwrap_err().violated_invariant,
            LedgerInvariant::UniquePositiveEntryIdentity
        );
    }

    #[test]
    fn reports_the_first_invalid_prefix_without_a_partial_replay() {
        let ledger = CanonicalLedger::new(
            7,
            vec![
                entry(
                    1,
                    "2025-01-01",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price: 1.0,
                        fees: 0.0,
                    },
                ),
                entry(
                    2,
                    "2025-01-02",
                    LedgerEntryKind::Sell {
                        units: 2.0,
                        unit_price: 1.0,
                        fees: 0.0,
                    },
                ),
                entry(
                    3,
                    "2025-01-03",
                    LedgerEntryKind::Sell {
                        units: 1.0,
                        unit_price: 1.0,
                        fees: 0.0,
                    },
                ),
            ],
        )
        .unwrap();

        let error = ledger.replay().unwrap_err();
        assert_eq!(error.entry_id, 2);
        assert_eq!(error.asset_id, 7);
        assert_eq!(error.date, "2025-01-02");
        assert_eq!(error.entry_type, LedgerEntryType::Sell);
        assert_eq!(error.quantity_before, 1.0);
        assert_eq!(error.attempted_effect, LedgerAttempt::SellUnits(2.0));
        assert_eq!(
            error.violated_invariant,
            LedgerInvariant::NonNegativeQuantity
        );
    }

    #[test]
    fn normalizes_fractional_full_liquidation_and_allows_reopening() {
        let ledger = CanonicalLedger::new(
            7,
            vec![
                entry(
                    1,
                    "2025-01-01",
                    LedgerEntryKind::Buy {
                        units: 0.3,
                        unit_price: 10.0,
                        fees: 0.0,
                    },
                ),
                entry(
                    2,
                    "2025-01-02",
                    LedgerEntryKind::Sell {
                        units: 0.3,
                        unit_price: 10.0,
                        fees: 0.0,
                    },
                ),
                entry(
                    3,
                    "2025-01-03",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price: 5.0,
                        fees: 0.0,
                    },
                ),
            ],
        )
        .unwrap();

        let replay = ledger.replay().unwrap();
        assert_eq!(replay.transitions[1].quantity_after, 0.0);
        assert_eq!(replay.transitions[1].remaining_cost_after, 0.0);
        assert_eq!(replay.final_quantity, 1.0);
        assert_eq!(replay.remaining_cost, 5.0);
    }

    #[test]
    fn requires_an_open_position_for_dividends_and_splits() {
        for kind in [
            LedgerEntryKind::Dividend {
                gross_amount: 1.0,
                deductions: 0.0,
            },
            LedgerEntryKind::Split { ratio: 2.0 },
        ] {
            let ledger = CanonicalLedger::new(7, vec![entry(1, "2025-01-01", kind)]).unwrap();
            assert_eq!(
                ledger.replay().unwrap_err().violated_invariant,
                LedgerInvariant::OpenQuantityRequired
            );
        }
    }

    #[test]
    fn rejects_invalid_numeric_values_and_dividend_deductions() {
        let cases = [
            LedgerEntryKind::Buy {
                units: f64::NAN,
                unit_price: 1.0,
                fees: 0.0,
            },
            LedgerEntryKind::Buy {
                units: 1.0,
                unit_price: f64::INFINITY,
                fees: 0.0,
            },
            LedgerEntryKind::Buy {
                units: 1.0,
                unit_price: 1.0,
                fees: -1.0,
            },
            LedgerEntryKind::Split { ratio: 0.0 },
        ];
        for kind in cases {
            let ledger = CanonicalLedger::new(7, vec![entry(1, "2025-01-01", kind)]).unwrap();
            assert!(matches!(
                ledger.replay().unwrap_err().violated_invariant,
                LedgerInvariant::FiniteValue
                    | LedgerInvariant::PositiveValue
                    | LedgerInvariant::NonNegativeValue
            ));
        }

        let ledger = CanonicalLedger::new(
            7,
            vec![
                entry(
                    1,
                    "2025-01-01",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price: 1.0,
                        fees: 0.0,
                    },
                ),
                entry(
                    2,
                    "2025-01-02",
                    LedgerEntryKind::Dividend {
                        gross_amount: 1.0,
                        deductions: 1.1,
                    },
                ),
            ],
        )
        .unwrap();
        assert_eq!(
            ledger.replay().unwrap_err().violated_invariant,
            LedgerInvariant::DeductionsDoNotExceedGrossDividend
        );
    }
}
