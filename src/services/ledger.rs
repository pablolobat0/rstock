//! Pure canonical replay for one asset's transaction ledger.
//!
//! Persistence establishes transaction identities; this module establishes their
//! chronological meaning. Monetary values are in the tracked asset's native
//! currency and deliberately have no market-data or database dependency.

#![allow(dead_code)] // Keep the pure replay surface available to downstream consumers and tests.

use std::collections::BTreeMap;
use std::fmt;

use chrono::NaiveDate;

use crate::constants::{DATE_FORMAT, FLOAT_EPSILON, MONETARY_MULTIPLIER};
use crate::models::{Transaction, TxType};

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
        unit_price_cents: i64,
        fees_cents: i64,
    },
    Sell {
        units: f64,
        unit_price_cents: i64,
        fees_cents: i64,
    },
    Dividend {
        gross_amount_cents: i64,
        deductions_cents: i64,
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

impl LedgerEntry {
    /// Converts the persisted transaction representation into the typed replay
    /// representation.  The database encoding is intentionally kept at this
    /// boundary; consumers never need to reinterpret dividend or split fields.
    #[must_use]
    pub fn from_transaction(transaction: &Transaction) -> Self {
        let kind = match transaction.tx_type {
            TxType::Buy => LedgerEntryKind::Buy {
                units: transaction.quantity,
                unit_price_cents: transaction.price_cents,
                fees_cents: transaction.fees_cents,
            },
            TxType::Sell => LedgerEntryKind::Sell {
                units: transaction.quantity,
                unit_price_cents: transaction.price_cents,
                fees_cents: transaction.fees_cents,
            },
            TxType::Dividend => LedgerEntryKind::Dividend {
                gross_amount_cents: transaction.price_cents,
                deductions_cents: transaction.fees_cents,
            },
            TxType::Split => LedgerEntryKind::Split {
                ratio: transaction.quantity,
            },
        };
        Self {
            id: transaction.id,
            asset_id: transaction.asset_id,
            date: transaction.date.clone(),
            kind,
        }
    }
}

/// A compact, stable description of an entry's type for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerEntryType {
    Ledger,
    Buy,
    Sell,
    Dividend,
    Split,
}

impl fmt::Display for LedgerEntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ledger => write!(f, "ledger"),
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
    /// Builds a canonical ledger from persisted transactions.
    pub fn from_transactions(
        asset_id: i32,
        transactions: &[Transaction],
    ) -> Result<Self, LedgerError> {
        Self::new(
            asset_id,
            transactions
                .iter()
                .map(LedgerEntry::from_transaction)
                .collect(),
        )
    }

    /// Validates identity integrity and establishes the only replay order.
    pub fn new(asset_id: i32, mut entries: Vec<LedgerEntry>) -> Result<Self, LedgerError> {
        if asset_id <= 0 {
            return Err(LedgerError::for_ledger(
                asset_id,
                LedgerInvariant::PositiveAssetIdentity,
            ));
        }
        entries.sort_by(|left, right| left.date.cmp(&right.date).then(left.id.cmp(&right.id)));

        let mut seen_ids = std::collections::HashSet::new();
        for entry in &entries {
            if entry.asset_id != asset_id {
                return Err(LedgerError::for_ledger_entry(
                    asset_id,
                    entry,
                    0.0,
                    LedgerAttempt::Identity,
                    LedgerInvariant::MatchingAssetIdentity,
                ));
            }
            if entry.id <= 0 || !seen_ids.insert(entry.id) {
                return Err(LedgerError::for_ledger_entry(
                    asset_id,
                    entry,
                    0.0,
                    LedgerAttempt::Identity,
                    LedgerInvariant::UniquePositiveEntryIdentity,
                ));
            }
            if !is_canonical_date(&entry.date) {
                return Err(LedgerError::for_ledger_entry(
                    asset_id,
                    entry,
                    0.0,
                    LedgerAttempt::Identity,
                    LedgerInvariant::ValidDate,
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
                    unit_price_cents,
                    fees_cents,
                } => {
                    validate_positive(entry, quantity_before, *units, LedgerAttempt::Units)?;
                    validate_positive(
                        entry,
                        quantity_before,
                        *unit_price_cents as f64,
                        LedgerAttempt::UnitPrice,
                    )?;
                    validate_non_negative(
                        entry,
                        quantity_before,
                        *fees_cents as f64,
                        LedgerAttempt::Fees,
                    )?;
                    let contribution = units * *unit_price_cents as f64 + *fees_cents as f64;
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
                    unit_price_cents,
                    fees_cents,
                } => {
                    validate_positive(entry, quantity_before, *units, LedgerAttempt::Units)?;
                    validate_positive(
                        entry,
                        quantity_before,
                        *unit_price_cents as f64,
                        LedgerAttempt::UnitPrice,
                    )?;
                    validate_non_negative(
                        entry,
                        quantity_before,
                        *fees_cents as f64,
                        LedgerAttempt::Fees,
                    )?;
                    let raw_quantity_after = quantity_before - units;
                    if raw_quantity_after < -FLOAT_EPSILON {
                        return Err(LedgerError::for_entry(
                            entry,
                            quantity_before,
                            LedgerAttempt::SellUnits(*units),
                            LedgerInvariant::NonNegativeQuantity,
                        ));
                    }
                    let quantity_after = normalize_quantity(raw_quantity_after);
                    let withdrawal = units * *unit_price_cents as f64 - *fees_cents as f64;
                    validate_finite(
                        entry,
                        quantity_before,
                        withdrawal,
                        LedgerAttempt::Withdrawal,
                    )?;
                    let cost_removed = if quantity_after == 0.0 {
                        remaining_cost_before
                    } else {
                        remaining_cost_before * (units / quantity_before)
                    };
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
                    gross_amount_cents,
                    deductions_cents,
                } => {
                    validate_positive(
                        entry,
                        quantity_before,
                        *gross_amount_cents as f64,
                        LedgerAttempt::GrossDividend,
                    )?;
                    validate_non_negative(
                        entry,
                        quantity_before,
                        *deductions_cents as f64,
                        LedgerAttempt::DividendDeductions(*deductions_cents as f64),
                    )?;
                    require_open_quantity(entry, quantity_before, LedgerAttempt::GrossDividend)?;
                    if deductions_cents > gross_amount_cents {
                        return Err(LedgerError::for_entry(
                            entry,
                            quantity_before,
                            LedgerAttempt::DividendDeductions(*deductions_cents as f64),
                            LedgerInvariant::DeductionsDoNotExceedGrossDividend,
                        ));
                    }
                    let net_income = (*gross_amount_cents - *deductions_cents) as f64;
                    (
                        quantity_before,
                        remaining_cost_before,
                        LedgerEffect::Dividend { net_income },
                    )
                }
                LedgerEntryKind::Split { ratio } => {
                    validate_positive(entry, quantity_before, *ratio, LedgerAttempt::SplitRatio)?;
                    require_open_quantity(entry, quantity_before, LedgerAttempt::SplitRatio)?;
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
            remaining_cost = if quantity == 0.0 {
                0.0
            } else {
                normalize_cost(remaining_cost_after)
            };
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

/// Base-currency effects for one complete, valid ledger replay.  Missing FX
/// only removes effects that depend on it; quantity remains available.
#[derive(Clone, Debug, PartialEq)]
pub struct EnrichedLedgerReplay {
    pub transitions: Vec<EnrichedLedgerTransition>,
    pub final_quantity: f64,
    pub remaining_cost: Option<f64>,
    pub dividends: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnrichedLedgerTransition {
    pub transition: LedgerTransition,
    pub buy_contribution: Option<f64>,
    pub sell_withdrawal: Option<f64>,
    pub cost_removed: Option<f64>,
    pub dividend_income: Option<f64>,
}

/// Applies transaction-date FX to native-currency semantic effects.
///
/// `rates` contains prepared historical rates.  The lookup deliberately uses
/// the latest rate on or before each entry date and never a later/current rate.
pub fn enrich_replay(
    replay: &LedgerReplay,
    currency: &str,
    base_currency: &str,
    rates: &BTreeMap<NaiveDate, f64>,
) -> Result<EnrichedLedgerReplay, LedgerError> {
    let mut remaining_cost = Some(0.0);
    let mut dividends = Some(0.0);
    let mut transitions = Vec::with_capacity(replay.transitions.len());

    for transition in &replay.transitions {
        let date =
            NaiveDate::parse_from_str(&transition.entry.date, DATE_FORMAT).map_err(|_| {
                LedgerError::for_entry(
                    &transition.entry,
                    transition.quantity_before,
                    LedgerAttempt::Identity,
                    LedgerInvariant::ValidDate,
                )
            })?;
        let rate = if currency == base_currency {
            Some(1.0)
        } else {
            rates.range(..=date).next_back().map(|(_, rate)| *rate)
        };

        let (buy_contribution, sell_withdrawal, cost_removed, dividend_income) =
            match &transition.effect {
                LedgerEffect::Buy { contribution } => (
                    rate.map(|rate| *contribution * rate / MONETARY_MULTIPLIER),
                    None,
                    None,
                    None,
                ),
                LedgerEffect::Sell { withdrawal, .. } => {
                    let cost_removed = remaining_cost.map(|cost| {
                        if transition.quantity_after == 0.0 {
                            cost
                        } else {
                            cost * (transition.quantity_before - transition.quantity_after)
                                / transition.quantity_before
                        }
                    });
                    if let (Some(cost), Some(removed)) = (remaining_cost, cost_removed) {
                        remaining_cost = Some(cost - removed);
                    } else {
                        remaining_cost = None;
                    }
                    (
                        None,
                        rate.map(|rate| *withdrawal * rate / MONETARY_MULTIPLIER),
                        cost_removed,
                        None,
                    )
                }
                LedgerEffect::Dividend { net_income } => {
                    let income = rate.map(|rate| *net_income * rate / MONETARY_MULTIPLIER);
                    dividends = dividends.zip(income).map(|(total, income)| total + income);
                    (None, None, None, income)
                }
                LedgerEffect::Split { .. } => (None, None, None, None),
            };

        if let Some(contribution) = buy_contribution {
            remaining_cost = remaining_cost.map(|cost| cost + contribution);
        } else if matches!(&transition.effect, LedgerEffect::Buy { .. }) {
            remaining_cost = None;
        }

        transitions.push(EnrichedLedgerTransition {
            transition: transition.clone(),
            buy_contribution,
            sell_withdrawal,
            cost_removed,
            dividend_income,
        });
    }

    Ok(EnrichedLedgerReplay {
        transitions,
        final_quantity: replay.final_quantity,
        remaining_cost,
        dividends,
    })
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
    ValidDate,
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
    fn for_ledger(asset_id: i32, violated_invariant: LedgerInvariant) -> Self {
        Self {
            asset_id,
            entry_id: 0,
            date: String::new(),
            entry_type: LedgerEntryType::Ledger,
            quantity_before: 0.0,
            attempted_effect: LedgerAttempt::Identity,
            violated_invariant,
        }
    }

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

    fn for_ledger_entry(
        ledger_asset_id: i32,
        entry: &LedgerEntry,
        quantity_before: f64,
        attempted_effect: LedgerAttempt,
        violated_invariant: LedgerInvariant,
    ) -> Self {
        Self {
            asset_id: ledger_asset_id,
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

fn require_open_quantity(
    entry: &LedgerEntry,
    quantity_before: f64,
    attempted_effect: LedgerAttempt,
) -> Result<(), LedgerError> {
    if quantity_before > FLOAT_EPSILON {
        Ok(())
    } else {
        Err(LedgerError::for_entry(
            entry,
            quantity_before,
            attempted_effect,
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

fn is_canonical_date(date: &str) -> bool {
    NaiveDate::parse_from_str(date, DATE_FORMAT)
        .is_ok_and(|parsed| parsed.format(DATE_FORMAT).to_string() == date)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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
                        unit_price_cents: 20,
                        fees_cents: 1,
                    },
                ),
                entry(2, "2025-01-01", LedgerEntryKind::Split { ratio: 2.0 }),
                entry(
                    1,
                    "2025-01-01",
                    LedgerEntryKind::Buy {
                        units: 2.0,
                        unit_price_cents: 10,
                        fees_cents: 1,
                    },
                ),
                entry(
                    4,
                    "2025-01-02",
                    LedgerEntryKind::Dividend {
                        gross_amount_cents: 5,
                        deductions_cents: 1,
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
                        unit_price_cents: 1,
                        fees_cents: 0,
                    },
                ),
                entry(
                    1,
                    "2025-01-02",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price_cents: 1,
                        fees_cents: 0,
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
                        unit_price_cents: 1,
                        fees_cents: 0,
                    },
                ),
                entry(
                    2,
                    "2025-01-02",
                    LedgerEntryKind::Sell {
                        units: 2.0,
                        unit_price_cents: 1,
                        fees_cents: 0,
                    },
                ),
                entry(
                    3,
                    "2025-01-03",
                    LedgerEntryKind::Sell {
                        units: 1.0,
                        unit_price_cents: 1,
                        fees_cents: 0,
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
                        unit_price_cents: 10,
                        fees_cents: 0,
                    },
                ),
                entry(
                    2,
                    "2025-01-02",
                    LedgerEntryKind::Sell {
                        units: 0.3,
                        unit_price_cents: 10,
                        fees_cents: 0,
                    },
                ),
                entry(
                    3,
                    "2025-01-03",
                    LedgerEntryKind::Buy {
                        units: 1.0,
                        unit_price_cents: 5,
                        fees_cents: 0,
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
                gross_amount_cents: 1,
                deductions_cents: 0,
            },
            LedgerEntryKind::Split { ratio: 2.0 },
        ] {
            let attempted_effect = match &kind {
                LedgerEntryKind::Dividend { .. } => LedgerAttempt::GrossDividend,
                LedgerEntryKind::Split { .. } => LedgerAttempt::SplitRatio,
                _ => unreachable!("test only contains dividend and split entries"),
            };
            let ledger = CanonicalLedger::new(7, vec![entry(1, "2025-01-01", kind)]).unwrap();
            let error = ledger.replay().unwrap_err();
            assert_eq!(error.attempted_effect, attempted_effect);
            assert_eq!(
                error.violated_invariant,
                LedgerInvariant::OpenQuantityRequired
            );
        }
    }

    #[test]
    fn rejects_invalid_numeric_values_and_dividend_deductions() {
        let cases = [
            LedgerEntryKind::Buy {
                units: f64::NAN,
                unit_price_cents: 1,
                fees_cents: 0,
            },
            LedgerEntryKind::Buy {
                units: f64::INFINITY,
                unit_price_cents: 1,
                fees_cents: 0,
            },
            LedgerEntryKind::Buy {
                units: 1.0,
                unit_price_cents: 1,
                fees_cents: -1,
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
                        unit_price_cents: 1,
                        fees_cents: 0,
                    },
                ),
                entry(
                    2,
                    "2025-01-02",
                    LedgerEntryKind::Dividend {
                        gross_amount_cents: 10,
                        deductions_cents: 11,
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

    #[test]
    fn enriches_native_effects_with_prior_fx_and_keeps_missing_facts_independent() {
        let ledger = CanonicalLedger::new(
            7,
            vec![
                entry(
                    1,
                    "2025-01-02",
                    LedgerEntryKind::Buy {
                        units: 2.0,
                        unit_price_cents: 1_000,
                        fees_cents: 100,
                    },
                ),
                entry(
                    2,
                    "2025-01-03",
                    LedgerEntryKind::Dividend {
                        gross_amount_cents: 300,
                        deductions_cents: 100,
                    },
                ),
                entry(
                    3,
                    "2025-01-04",
                    LedgerEntryKind::Sell {
                        units: 1.0,
                        unit_price_cents: 1_200,
                        fees_cents: 50,
                    },
                ),
            ],
        )
        .unwrap();
        let replay = ledger.replay().unwrap();
        let mut rates = BTreeMap::new();
        rates.insert(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(), 0.8);
        let enriched = enrich_replay(&replay, "USD", "EUR", &rates).unwrap();

        assert_eq!(enriched.final_quantity, 1.0);
        assert_eq!(enriched.remaining_cost, Some(0.084));
        assert_eq!(enriched.dividends, Some(0.016));
        assert_eq!(enriched.transitions[0].buy_contribution, Some(0.168));
        assert_eq!(enriched.transitions[1].dividend_income, Some(0.016));
        assert_eq!(enriched.transitions[2].sell_withdrawal, Some(0.092));
        assert_eq!(enriched.transitions[2].cost_removed, Some(0.084));

        let missing = enrich_replay(&replay, "USD", "EUR", &BTreeMap::new()).unwrap();
        assert_eq!(missing.final_quantity, 1.0);
        assert_eq!(missing.remaining_cost, None);
        assert_eq!(missing.dividends, None);
        assert_eq!(missing.transitions[2].sell_withdrawal, None);
    }
}
