//! Pure canonical replay for one asset's transaction ledger.
//!
//! Persistence establishes transaction identities; this module establishes their
//! chronological meaning. Monetary values are in the tracked asset's native
//! currency and deliberately have no market-data or database dependency.

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
    pub fn from_transaction(transaction: &Transaction) -> Result<Self, LedgerError> {
        let kind = match &transaction.tx_type {
            TxType::Buy => LedgerEntryKind::Buy {
                units: required_field(transaction.units, transaction, LedgerAttempt::Units)?,
                unit_price_cents: required_field(
                    transaction.unit_price_cents,
                    transaction,
                    LedgerAttempt::UnitPrice,
                )?,
                fees_cents: required_field(
                    transaction.trade_fees_cents,
                    transaction,
                    LedgerAttempt::Fees,
                )?,
            },
            TxType::Sell => LedgerEntryKind::Sell {
                units: required_field(transaction.units, transaction, LedgerAttempt::Units)?,
                unit_price_cents: required_field(
                    transaction.unit_price_cents,
                    transaction,
                    LedgerAttempt::UnitPrice,
                )?,
                fees_cents: required_field(
                    transaction.trade_fees_cents,
                    transaction,
                    LedgerAttempt::Fees,
                )?,
            },
            TxType::Dividend => LedgerEntryKind::Dividend {
                gross_amount_cents: required_field(
                    transaction.dividend_amount_cents,
                    transaction,
                    LedgerAttempt::GrossDividend,
                )?,
                deductions_cents: required_field(
                    transaction.dividend_deductions_cents,
                    transaction,
                    LedgerAttempt::DividendDeductions(0.0),
                )?,
            },
            TxType::Split => LedgerEntryKind::Split {
                ratio: required_field(
                    transaction.split_ratio,
                    transaction,
                    LedgerAttempt::SplitRatio,
                )?,
            },
        };
        Ok(Self {
            id: transaction.id,
            asset_id: transaction.asset_id,
            date: transaction.date.clone(),
            kind,
        })
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
    entries: Vec<LedgerEntry>,
}

impl CanonicalLedger {
    /// Builds a canonical ledger from persisted transactions.
    pub fn from_transactions(
        asset_id: i32,
        transactions: &[Transaction],
    ) -> Result<Self, LedgerError> {
        // Sort references before conversion so malformed-field errors retain
        // canonical first-error ordering without cloning the transaction list.
        let mut ordered: Vec<&Transaction> = transactions.iter().collect();
        ordered.sort_by(|left, right| left.date.cmp(&right.date).then(left.id.cmp(&right.id)));
        let entries = ordered
            .into_iter()
            .map(LedgerEntry::from_transaction)
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_sorted_entries(asset_id, entries)
    }

    /// Validates identity integrity and establishes the only replay order.
    pub fn new(asset_id: i32, mut entries: Vec<LedgerEntry>) -> Result<Self, LedgerError> {
        entries.sort_by(|left, right| left.date.cmp(&right.date).then(left.id.cmp(&right.id)));
        Self::from_sorted_entries(asset_id, entries)
    }

    fn from_sorted_entries(asset_id: i32, entries: Vec<LedgerEntry>) -> Result<Self, LedgerError> {
        if asset_id <= 0 {
            return Err(LedgerError::for_ledger(
                asset_id,
                LedgerInvariant::PositiveAssetIdentity,
            ));
        }
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

        Ok(Self { entries })
    }

    /// Replays every prefix, returning transitions only when the entire ledger is valid.
    #[allow(clippy::too_many_lines)] // Keeping variant validation beside its transition preserves replay locality.
    pub fn replay(&self) -> Result<LedgerReplay, LedgerError> {
        self.replay_from_state(0.0, 0.0)
    }

    /// Replays a suffix from a trusted NAV checkpoint state. The ledger still
    /// validates every suffix prefix; the seed only avoids rereading history
    /// that the checkpoint already represents.
    #[allow(clippy::too_many_lines)]
    pub fn replay_from_state(
        &self,
        initial_quantity: f64,
        initial_cost: f64,
    ) -> Result<LedgerReplay, LedgerError> {
        let mut quantity = normalize_quantity(initial_quantity);
        let mut remaining_cost = normalize_cost(initial_cost);
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
                    validate_positive_cents(
                        entry,
                        quantity_before,
                        *unit_price_cents,
                        LedgerAttempt::UnitPrice,
                    )?;
                    validate_non_negative_cents(
                        entry,
                        quantity_before,
                        *fees_cents,
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
                    validate_positive_cents(
                        entry,
                        quantity_before,
                        *unit_price_cents,
                        LedgerAttempt::UnitPrice,
                    )?;
                    validate_non_negative_cents(
                        entry,
                        quantity_before,
                        *fees_cents,
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
                    validate_positive_cents(
                        entry,
                        quantity_before,
                        *gross_amount_cents,
                        LedgerAttempt::GrossDividend,
                    )?;
                    validate_non_negative_cents(
                        entry,
                        quantity_before,
                        *deductions_cents,
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

/// Canonicalizes and replays persisted entries through the authoritative ledger
/// boundary in one operation.
pub fn replay_transactions(
    asset_id: i32,
    transactions: &[Transaction],
) -> Result<LedgerReplay, LedgerError> {
    CanonicalLedger::from_transactions(asset_id, transactions)?.replay()
}

/// Canonicalizes and replays a transaction suffix from a trusted quantity.
pub fn replay_transactions_from_state(
    asset_id: i32,
    transactions: &[Transaction],
    initial_quantity: f64,
) -> Result<LedgerReplay, LedgerError> {
    CanonicalLedger::from_transactions(asset_id, transactions)?
        .replay_from_state(initial_quantity, 0.0)
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

impl EnrichedLedgerTransition {
    #[must_use]
    pub fn entry_type(&self) -> LedgerEntryType {
        self.transition.entry.kind.entry_type()
    }
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
                    if transition.quantity_after == 0.0 {
                        // A fully liquidated position no longer depends on an
                        // unavailable historical cost for its next opening.
                        remaining_cost = Some(0.0);
                    } else if let (Some(cost), Some(removed)) = (remaining_cost, cost_removed) {
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
        if transition.quantity_after == 0.0 {
            // Canonical replay owns epsilon closure for every transition type.
            // A later opening must not inherit cost from closed inventory.
            remaining_cost = Some(0.0);
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
    RequiredField,
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
    fn missing_field(transaction: &Transaction, attempted_effect: LedgerAttempt) -> Self {
        Self {
            asset_id: transaction.asset_id,
            entry_id: transaction.id,
            date: transaction.date.clone(),
            entry_type: match &transaction.tx_type {
                TxType::Buy => LedgerEntryType::Buy,
                TxType::Sell => LedgerEntryType::Sell,
                TxType::Dividend => LedgerEntryType::Dividend,
                TxType::Split => LedgerEntryType::Split,
            },
            quantity_before: 0.0,
            attempted_effect,
            violated_invariant: LedgerInvariant::RequiredField,
        }
    }

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

fn required_field<T>(
    value: Option<T>,
    transaction: &Transaction,
    attempted_effect: LedgerAttempt,
) -> Result<T, LedgerError> {
    value.ok_or_else(|| LedgerError::missing_field(transaction, attempted_effect))
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

fn validate_positive_cents(
    entry: &LedgerEntry,
    quantity_before: f64,
    value: i64,
    attempted_effect: LedgerAttempt,
) -> Result<(), LedgerError> {
    if value <= 0 {
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

fn validate_non_negative_cents(
    entry: &LedgerEntry,
    quantity_before: f64,
    value: i64,
    attempted_effect: LedgerAttempt,
) -> Result<(), LedgerError> {
    if value < 0 {
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
#[path = "../../tests/unit/ledger_tests.rs"]
mod tests;
