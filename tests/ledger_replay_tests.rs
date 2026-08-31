use rstock::services::ledger::{
    CanonicalLedger, LedgerAttempt, LedgerEffect, LedgerEntry, LedgerEntryKind, LedgerInvariant,
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
        ledger
            .entries()
            .iter()
            .map(|entry| entry.id)
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
