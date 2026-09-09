#![allow(clippy::float_cmp)]

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rstock::models::{Transaction, TxType};
use rstock::services::ledger::{
    enrich_replay, CanonicalLedger, LedgerAttempt, LedgerEffect, LedgerEntry, LedgerEntryKind,
    LedgerInvariant,
};

fn entry(id: i32, date: &str, kind: LedgerEntryKind) -> LedgerEntry {
    LedgerEntry {
        id,
        asset_id: 42,
        date: date.to_owned(),
        kind,
    }
}

#[test]
fn public_replay_canonicalizes_mixed_same_day_entries_and_split_effects() {
    let ledger = CanonicalLedger::new(
        42,
        vec![
            entry(
                3,
                "2025-01-01",
                LedgerEntryKind::Sell {
                    units: 1.0,
                    unit_price_cents: 200,
                    fees_cents: 0,
                },
            ),
            entry(2, "2025-01-01", LedgerEntryKind::Split { ratio: 2.0 }),
            entry(
                1,
                "2025-01-01",
                LedgerEntryKind::Buy {
                    units: 1.0,
                    unit_price_cents: 100,
                    fees_cents: 5,
                },
            ),
            entry(
                4,
                "2025-01-02",
                LedgerEntryKind::Dividend {
                    gross_amount_cents: 12,
                    deductions_cents: 2,
                },
            ),
        ],
    )
    .unwrap();

    let replay = ledger.replay().unwrap();
    assert_eq!(
        replay
            .transitions
            .iter()
            .map(|transition| transition.entry.id)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(replay.final_quantity, 1.0);
    assert_eq!(replay.transitions[1].quantity_before, 1.0);
    assert_eq!(replay.transitions[1].quantity_after, 2.0);
    assert_eq!(
        replay.transitions[2].effect,
        LedgerEffect::Sell {
            withdrawal: 200.0,
            cost_removed: 52.5,
        }
    );
    assert_eq!(
        replay.transitions[3].effect,
        LedgerEffect::Dividend { net_income: 10.0 }
    );
}

#[test]
fn public_replay_normalizes_fractional_liquidation_and_allows_reopening() {
    let ledger = CanonicalLedger::new(
        42,
        vec![
            entry(
                1,
                "2025-01-01",
                LedgerEntryKind::Buy {
                    units: 0.3,
                    unit_price_cents: 100,
                    fees_cents: 0,
                },
            ),
            entry(
                2,
                "2025-01-02",
                LedgerEntryKind::Sell {
                    units: 0.3 + 5e-10,
                    unit_price_cents: 100,
                    fees_cents: 0,
                },
            ),
            entry(
                3,
                "2025-01-03",
                LedgerEntryKind::Buy {
                    units: 1.0,
                    unit_price_cents: 50,
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
    assert_eq!(replay.remaining_cost, 50.0);
}

#[test]
fn public_replay_returns_the_first_invalid_prefix_with_context() {
    let ledger = CanonicalLedger::new(
        42,
        vec![
            entry(
                1,
                "2025-01-01",
                LedgerEntryKind::Buy {
                    units: 1.0,
                    unit_price_cents: 100,
                    fees_cents: 0,
                },
            ),
            entry(
                2,
                "2025-01-02",
                LedgerEntryKind::Sell {
                    units: 2.0,
                    unit_price_cents: 100,
                    fees_cents: 0,
                },
            ),
        ],
    )
    .unwrap();

    let error = ledger.replay().unwrap_err();
    assert_eq!(error.asset_id, 42);
    assert_eq!(error.entry_id, 2);
    assert_eq!(error.date, "2025-01-02");
    assert_eq!(error.quantity_before, 1.0);
    assert_eq!(error.attempted_effect, LedgerAttempt::SellUnits(2.0));
    assert_eq!(
        error.violated_invariant,
        LedgerInvariant::NonNegativeQuantity
    );
}

#[test]
fn public_constructor_requires_a_positive_identity_and_canonical_date() {
    let error = CanonicalLedger::new(0, vec![]).unwrap_err();
    assert_eq!(
        error.violated_invariant,
        LedgerInvariant::PositiveAssetIdentity
    );

    let error = CanonicalLedger::new(
        42,
        vec![entry(
            1,
            "2025-2-01",
            LedgerEntryKind::Buy {
                units: 1.0,
                unit_price_cents: 100,
                fees_cents: 0,
            },
        )],
    )
    .unwrap_err();
    assert_eq!(error.violated_invariant, LedgerInvariant::ValidDate);
}

#[test]
fn transaction_constructor_reports_first_malformed_entry_in_canonical_order() {
    let later_valid = Transaction {
        id: 2,
        asset_id: 42,
        tx_type: TxType::Buy,
        date: "2025-01-02".to_owned(),
        units: Some(1.0),
        unit_price_cents: Some(100),
        dividend_amount_cents: None,
        dividend_deductions_cents: None,
        split_ratio: None,
        trade_fees_cents: Some(0),
    };
    let earlier_missing_fees = Transaction {
        id: 1,
        asset_id: 42,
        tx_type: TxType::Buy,
        date: "2025-01-01".to_owned(),
        units: Some(1.0),
        unit_price_cents: Some(100),
        dividend_amount_cents: None,
        dividend_deductions_cents: None,
        split_ratio: None,
        trade_fees_cents: None,
    };

    let error =
        CanonicalLedger::from_transactions(42, &[later_valid, earlier_missing_fees]).unwrap_err();
    assert_eq!(error.entry_id, 1);
    assert_eq!(error.violated_invariant, LedgerInvariant::RequiredField);
    assert_eq!(error.attempted_effect, LedgerAttempt::Fees);
}

#[test]
fn eur_enrichment_resets_cost_on_split_closure_and_keeps_lifetime_dividends() {
    let ledger = CanonicalLedger::new(
        42,
        vec![
            entry(
                1,
                "2025-01-01",
                LedgerEntryKind::Buy {
                    units: 1.0,
                    unit_price_cents: 1_000,
                    fees_cents: 100,
                },
            ),
            entry(
                2,
                "2025-01-02",
                LedgerEntryKind::Dividend {
                    gross_amount_cents: 300,
                    deductions_cents: 50,
                },
            ),
            entry(3, "2025-01-03", LedgerEntryKind::Split { ratio: 5e-10 }),
            entry(
                4,
                "2025-01-04",
                LedgerEntryKind::Buy {
                    units: 2.0,
                    unit_price_cents: 500,
                    fees_cents: 0,
                },
            ),
        ],
    )
    .unwrap();

    let enriched =
        enrich_replay(&ledger.replay().unwrap(), "EUR", "EUR", &BTreeMap::new()).unwrap();

    assert_eq!(enriched.transitions[2].transition.quantity_after, 0.0);
    assert_eq!(enriched.remaining_cost, Some(0.1));
    assert_eq!(enriched.dividends, Some(0.025));
}

#[test]
fn missing_fx_cost_recovers_after_split_closure_and_reopening() {
    let ledger = CanonicalLedger::new(
        42,
        vec![
            entry(
                1,
                "2025-01-01",
                LedgerEntryKind::Buy {
                    units: 1.0,
                    unit_price_cents: 1_000,
                    fees_cents: 0,
                },
            ),
            entry(
                2,
                "2025-01-02",
                LedgerEntryKind::Dividend {
                    gross_amount_cents: 100,
                    deductions_cents: 0,
                },
            ),
            entry(3, "2025-01-03", LedgerEntryKind::Split { ratio: 5e-10 }),
            entry(
                4,
                "2025-01-04",
                LedgerEntryKind::Buy {
                    units: 2.0,
                    unit_price_cents: 500,
                    fees_cents: 0,
                },
            ),
        ],
    )
    .unwrap();
    let mut rates = BTreeMap::new();
    rates.insert(NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(), 0.8);

    let enriched = enrich_replay(&ledger.replay().unwrap(), "USD", "EUR", &rates).unwrap();

    assert_eq!(enriched.remaining_cost, Some(0.08));
    assert_eq!(enriched.dividends, None);
}
