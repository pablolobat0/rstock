#![allow(clippy::float_cmp)]

use std::collections::BTreeMap;

use super::*;

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
        replay
            .transitions
            .iter()
            .map(|transition| transition.entry.id)
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
fn rejects_missing_required_semantic_persistence_fields() {
    let transaction = Transaction {
        id: 1,
        asset_id: 7,
        tx_type: TxType::Buy,
        date: "2025-01-01".to_owned(),
        units: Some(1.0),
        unit_price_cents: Some(100),
        dividend_amount_cents: None,
        dividend_deductions_cents: None,
        split_ratio: None,
        trade_fees_cents: None,
    };
    let error = LedgerEntry::from_transaction(&transaction).unwrap_err();
    assert_eq!(error.violated_invariant, LedgerInvariant::RequiredField);
    assert_eq!(error.attempted_effect, LedgerAttempt::Fees);
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

fn entry(id: i32, date: &str, kind: LedgerEntryKind) -> LedgerEntry {
    LedgerEntry {
        id,
        asset_id: 7,
        date: date.to_owned(),
        kind,
    }
}
