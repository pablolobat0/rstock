pub mod common;

use std::io::Write;

use rstock::db::repos::{asset_repo, transaction_repo};
use rstock::models::TxType;
use rstock::services::export::export_transactions_csv;
use rstock::services::import::import_transactions_csv;
use tempfile::NamedTempFile;

const CSV_HEADER: &str = "Date,Ticker,Name,AssetType,Currency,MorningstarCode,AssetClass,EquityStyle,BondCredit,BondDuration,Management,Type,Quantity,Price,Fees\n";

fn write_csv(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("failed to write CSV");
    file.flush().expect("failed to flush");
    file
}

fn classified_stock_row(tx_type: &str, quantity: &str, price: &str, fees: &str) -> String {
    format!(
        "01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,{tx_type},{quantity},{price},{fees}\n"
    )
}

#[tokio::test]
async fn test_import_buy_sell_dividend_split() {
    let db = common::setup_test_db().await;

    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n\
         15-01-2025,XFAKE1,,,EUR,,,,,,,sell,2,110.00,3.00\n\
         20-01-2025,XFAKE1,,,EUR,,,,,,,dividend,1,50.00,5.00\n\
         25-01-2025,XFAKE1,,,EUR,,,,,,,split,2,0.00,0.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("import should succeed");
    assert_eq!(result.count, 4);
    assert_eq!(result.transaction_receipts.len(), 4);

    let txns = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .expect("failed to query transactions");
    assert_eq!(txns.len(), 4);

    assert_eq!(txns[0].tx_type, TxType::Buy);
    assert_eq!(txns[0].units, Some(10.0));
    assert_eq!(txns[1].tx_type, TxType::Sell);
    assert_eq!(txns[1].units, Some(2.0));
    assert_eq!(txns[2].tx_type, TxType::Dividend);
    assert_eq!(txns[3].tx_type, TxType::Split);
    assert_eq!(txns[3].split_ratio, Some(2.0));
}

#[tokio::test]
async fn import_receipts_use_persisted_replay_precision_and_generated_ids() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,200,1.000049,0.000149\n\
         02-01-2025,XFAKE1,,,EUR,,,,,,,dividend,1,1.23456,0.12345\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("sub-cent semantic amounts should import");
    let transactions = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap();

    assert_eq!(result.transaction_receipts.len(), 2);
    assert_eq!(
        result.transaction_receipts[0].transaction_id,
        transactions[0].id
    );
    assert_eq!(
        result.transaction_receipts[1].transaction_id,
        transactions[1].id
    );
    assert_eq!(transactions[0].unit_price_cents, Some(10_000));
    assert_eq!(transactions[0].trade_fees_cents, Some(1));
    assert_eq!(transactions[1].dividend_amount_cents, Some(12_346));
    assert_eq!(transactions[1].dividend_deductions_cents, Some(1_235));
    assert_eq!(
        result.transaction_receipts[0].summary,
        "Bought 200 units of Fake Stock (XFAKE1) at 1.00 EUR on 01-01-2025. Total: 200.00 EUR"
    );
    assert_eq!(
        result.transaction_receipts[1].summary,
        "Dividend for Fake Stock (XFAKE1): 1.23 (fees: 0.12, net: 1.11) on 02-01-2025"
    );
}

#[tokio::test]
async fn test_import_invalid_date() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}2025-01-01,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("row 2"),
        "error should mention row number: {err}"
    );
}

#[tokio::test]
async fn test_import_rejects_old_schema() {
    let db = common::setup_test_db().await;
    let csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,buy,10,100.00,5.00\n",
    );

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("classified schema"),
        "error should mention classified schema: {err}"
    );
}

#[tokio::test]
async fn test_import_sell_nonexistent_asset() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,XFAKE1,,,EUR,,,,,,,sell,5,100.00,0.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_import_replays_existing_ledger_before_commit() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEIMPORT", "Import Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-01", 1.0, 100.0, 0.0).await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("sell", "2", "100.00", "0.00").replace("XFAKE1", "XFAKEIMPORT")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("row 2"));
    assert_eq!(
        transaction_repo::find_all_ordered_by_date(&db, None, None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_import_replay_error_identifies_the_imported_row() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEIMPORT", "Import Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-01", 1.0, 100.0, 0.0).await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         02-01-2025,XFAKEIMPORT,,,EUR,,,,,,,sell,2,100.00,0.00\n"
    ));

    let error = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect_err("oversell should be rejected");
    let message = error.to_string();
    assert!(
        message.contains("row 2"),
        "error should identify CSV row: {message}"
    );
    assert!(
        message.contains("XFAKEIMPORT"),
        "error should identify asset: {message}"
    );
    assert!(
        message.contains("NonNegativeQuantity"),
        "error should identify invariant: {message}"
    );
    assert_eq!(
        transaction_repo::find_all_ordered_by_date(&db, None, None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_import_replay_error_identifies_row_causing_existing_suffix_failure() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKESUFFIX", "Suffix Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-01", 1.0, 100.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-01-04", 1.0, 100.0, 0.0).await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         02-01-2025,XFAKESUFFIX,,,EUR,,,,,,,sell,0.5,100.00,0.00\n\
         03-01-2025,XFAKESUFFIX,,,EUR,,,,,,,dividend,1,10.00,0.00\n"
    ));

    let error = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect_err("an imported sell should invalidate the existing suffix");
    let message = error.to_string();
    assert!(
        message.contains("row 2"),
        "error should identify the imported row causing the suffix failure: {message}"
    );
    assert_eq!(
        transaction_repo::find_all_ordered_by_date(&db, None, None)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn test_import_unsorted_dates_replay_in_canonical_order() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         10-01-2025,XFAKE1,,,EUR,,,,,,,sell,2,110.00,0.00\n\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,3,100.00,0.00\n"
    ));

    import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("dates should be canonicalized before replay");
    let transactions = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap();
    assert_eq!(transactions.len(), 2);
    assert_eq!(transactions[0].tx_type, TxType::Buy);
    assert_eq!(transactions[1].tx_type, TxType::Sell);
}

#[tokio::test]
async fn test_import_rolls_back_all_assets_when_one_replay_fails() {
    let db = common::setup_test_db().await;
    let first_asset = common::insert_asset(&db, "XFAKE1", "First Stock", "stock", "EUR").await;
    let second_asset = common::insert_asset(&db, "XFAKE2", "Second Stock", "stock", "EUR").await;
    common::insert_transaction(&db, first_asset, "2025-01-01", 1.0, 100.0, 0.0).await;
    common::insert_transaction(&db, second_asset, "2025-01-01", 1.0, 100.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-01-01", 100.0, 1.0).await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         02-01-2025,XFAKE1,,,EUR,,,,,,,sell,1,110.00,0.00\n\
         02-01-2025,XFAKE2,,,EUR,,,,,,,sell,2,110.00,0.00\n"
    ));

    let error = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect_err("one invalid affected ledger should reject the complete import");
    assert!(error.to_string().contains("row 3"));
    assert_eq!(
        transaction_repo::find_all_ordered_by_date(&db, None, None)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(common::get_portfolio_snapshot(&db, "2025-01-01")
        .await
        .is_some());
}

#[tokio::test]
async fn test_import_rejects_non_finite_numeric_values() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("buy", "NaN", "100.00", "0.00")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.unwrap_err().to_string().contains("finite"));

    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("buy", "1", "1e20", "0.00")
    ));
    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.unwrap_err().to_string().contains("precision"));
}

#[tokio::test]
async fn test_import_buy_missing_name() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,XFAKE1,,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Name"), "error should mention Name: {err}");
}

#[tokio::test]
async fn test_import_buy_missing_asset_type() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,XFAKE1,Fake Stock,,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("AssetType"),
        "error should mention AssetType: {err}"
    );
}

#[tokio::test]
async fn test_import_rejects_missing_classification_for_new_asset() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,XFAKE1,Fake Stock,stock,EUR,,,,,,,buy,10,100.00,5.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("classification"),
        "error should mention classification: {err}"
    );
}

#[tokio::test]
async fn test_import_rejects_inconsistent_classification_for_new_asset() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,XFAKE1,Fake Stock,stock,EUR,,monetary,blend,,,passive,buy,10,100.00,5.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("equity style"),
        "error should mention equity style: {err}"
    );
}

#[tokio::test]
async fn test_import_rejects_fund_without_morningstar_code() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,IE00FAKE,Fake Fund,fund,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Morningstar code"),
        "error should mention Morningstar code: {err}"
    );
}

#[tokio::test]
async fn test_import_creates_classified_asset_with_morningstar_code() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}01-01-2025,IE00FAKE,Fake ETF,etf,EUR,F000FAKE,equity,blend,,,passive,buy,10,100.00,5.00\n"
    ));

    import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("import should succeed");

    let asset = asset_repo::find_by_ticker(&db, "IE00FAKE")
        .await
        .expect("asset query should succeed")
        .expect("asset should exist");
    assert_eq!(asset.morningstar_code.as_deref(), Some("F000FAKE"));
    assert_eq!(asset.asset_class.as_deref(), Some("equity"));
    assert_eq!(asset.equity_style.as_deref(), Some("blend"));
    assert_eq!(asset.management.as_deref(), Some("passive"));
}

#[tokio::test]
async fn test_import_rejects_non_positive_buy_quantity() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("buy", "0", "100.00", "5.00")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("quantity"),
        "error should mention quantity: {err}"
    );
}

#[tokio::test]
async fn test_import_rejects_non_positive_buy_price() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("buy", "1", "0", "5.00")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("price"), "error should mention price: {err}");
}

#[tokio::test]
async fn test_import_rejects_positive_price_that_rounds_to_zero() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("buy", "1", "0.000049", "0.00")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    let error = result.expect_err("positive sub-unit price must not be silently quantized away");
    assert!(error.to_string().contains("supported cents precision"));
    assert!(asset_repo::find_all(&db).await.unwrap().is_empty());
    assert!(transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn test_import_compares_dividend_fields_after_quantization() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,1,100.00,0.00\n\
         02-01-2025,XFAKE1,,,EUR,,,,,,,dividend,1,1.00004,1.000049\n"
    ));

    import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("encoded-equal dividend fields should be accepted");
    let transactions = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap();
    assert_eq!(transactions[1].dividend_amount_cents, Some(10_000));
    assert_eq!(transactions[1].dividend_deductions_cents, Some(10_000));
}

#[tokio::test]
async fn test_import_rejects_non_positive_dividend_amount() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("dividend", "1", "0", "0")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("dividend amount"),
        "error should mention amount: {err}"
    );
}

#[tokio::test]
async fn test_import_rejects_non_positive_split_ratio() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("split", "0", "0", "0")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("split ratio"),
        "error should mention ratio: {err}"
    );
}

#[tokio::test]
async fn import_rejects_nonzero_split_monetary_placeholders_atomically() {
    for (price, fees, field) in [("0.01", "0.00", "Price"), ("0.00", "0.01", "Fees")] {
        let db = common::setup_test_db().await;
        let csv = write_csv(&format!(
            "{CSV_HEADER}\
             01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,1,100.00,0.00\n\
             02-01-2025,XFAKE1,,,EUR,,,,,,,split,2,{price},{fees}\n"
        ));

        let error = import_transactions_csv(&db, csv.path().to_str().unwrap())
            .await
            .expect_err("noncanonical split monetary placeholders must be rejected");

        assert!(error.to_string().contains("row 3"));
        assert!(error.to_string().contains(field));
        assert!(asset_repo::find_all(&db).await.unwrap().is_empty());
        assert!(transaction_repo::find_all_ordered_by_date(&db, None, None)
            .await
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn test_import_rejects_negative_fees() {
    let db = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}{}",
        classified_stock_row("buy", "1", "100", "-1")
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("fees"), "error should mention fees: {err}");
}

#[tokio::test]
async fn test_import_export_roundtrip() {
    let db1 = common::setup_test_db().await;
    let buy_csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n\
         10-01-2025,XFAKE1,,,EUR,,,,,,,sell,3,120.00,2.00\n"
    ));

    import_transactions_csv(&db1, buy_csv.path().to_str().unwrap())
        .await
        .expect("initial import should succeed");

    let export_file = NamedTempFile::new().expect("failed to create temp file");
    let export_path = export_file.path().to_str().unwrap();
    export_transactions_csv(&db1, export_path)
        .await
        .expect("export should succeed");

    let exported = std::fs::read_to_string(export_path).expect("failed to read export");
    assert!(exported.starts_with(CSV_HEADER));
    assert!(exported.contains("equity,blend,,,passive"));

    let db2 = common::setup_test_db().await;
    import_transactions_csv(&db2, export_path)
        .await
        .expect("roundtrip import should succeed");

    let export_file2 = NamedTempFile::new().expect("failed to create temp file");
    let export_path2 = export_file2.path().to_str().unwrap();
    export_transactions_csv(&db2, export_path2)
        .await
        .expect("second export should succeed");

    let csv1 = std::fs::read_to_string(export_path).expect("failed to read first export");
    let csv2 = std::fs::read_to_string(export_path2).expect("failed to read second export");
    assert_eq!(csv1, csv2, "roundtrip CSVs should be identical");
}

#[tokio::test]
async fn test_import_export_roundtrip_preserves_all_transaction_types() {
    let db1 = common::setup_test_db().await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n\
         10-01-2025,XFAKE1,,,EUR,,,,,,,dividend,1,50.00,5.00\n\
         15-01-2025,XFAKE1,,,EUR,,,,,,,split,2,0.00,0.00\n\
         20-01-2025,XFAKE1,,,EUR,,,,,,,sell,3,120.00,2.00\n"
    ));
    import_transactions_csv(&db1, csv.path().to_str().unwrap())
        .await
        .expect("initial import should succeed");

    let export_file = NamedTempFile::new().expect("failed to create temp file");
    let export_path = export_file.path().to_str().unwrap();
    export_transactions_csv(&db1, export_path)
        .await
        .expect("export should succeed");

    let db2 = common::setup_test_db().await;
    import_transactions_csv(&db2, export_path)
        .await
        .expect("roundtrip import should succeed");
    let original = transaction_repo::find_all_ordered_by_date(&db1, None, None)
        .await
        .unwrap();
    let roundtripped = transaction_repo::find_all_ordered_by_date(&db2, None, None)
        .await
        .unwrap();
    assert_eq!(original.len(), roundtripped.len());
    for (original, roundtripped) in original.iter().zip(roundtripped.iter()) {
        assert_eq!(original.tx_type, roundtripped.tx_type);
        assert!((original.display_quantity() - roundtripped.display_quantity()).abs() < 1e-12);
        assert_eq!(
            original.display_price_cents(),
            roundtripped.display_price_cents()
        );
        assert_eq!(
            original.display_fees_cents(),
            roundtripped.display_fees_cents()
        );
    }
}

#[tokio::test]
async fn test_import_rejects_late_row_without_persisting_anything() {
    let db = common::setup_test_db().await;
    common::insert_portfolio_snapshot(&db, "2024-12-31", 100.0, 1.0).await;
    common::insert_portfolio_snapshot(&db, "2025-01-01", 101.0, 1.0).await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n\
         02-01-2025,XFAKE2,,,EUR,,,,,,,sell,1,100.00,0.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;

    assert!(result.is_err());
    assert!(asset_repo::find_all(&db).await.unwrap().is_empty());
    assert!(transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap()
        .is_empty());
    assert!(common::get_portfolio_snapshot(&db, "2025-01-01")
        .await
        .is_some());
}

#[tokio::test]
async fn test_import_preserves_same_day_source_order_and_invalidates_once() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    common::insert_portfolio_snapshot(&db, "2024-12-31", 100.0, 1.0).await;
    common::insert_portfolio_snapshot(&db, "2025-01-01", 101.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-01-01", asset_id, 10.0, 100.0, 1000.0, 1.0)
        .await;
    let csv = write_csv(&format!(
        "{CSV_HEADER}\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,,equity,blend,,,passive,buy,10,100.00,5.00\n\
         01-01-2025,XFAKE1,,,EUR,,,,,,,split,2,0.00,0.00\n\
         01-01-2025,XFAKE1,,,EUR,,,,,,,sell,5,110.00,3.00\n"
    ));

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("import should succeed");
    let transactions = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap();

    assert_eq!(result.transaction_receipts.len(), 3);
    assert_eq!(transactions.len(), 3);
    assert!(transactions[0].id < transactions[1].id);
    assert!(transactions[1].id < transactions[2].id);
    assert_eq!(transactions[0].tx_type, TxType::Buy);
    assert_eq!(transactions[1].tx_type, TxType::Split);
    assert_eq!(transactions[2].tx_type, TxType::Sell);
    assert_eq!(asset_repo::find_all(&db).await.unwrap().len(), 1);
    assert!(common::get_portfolio_snapshot(&db, "2024-12-31")
        .await
        .is_some());
    assert!(common::get_portfolio_snapshot(&db, "2025-01-01")
        .await
        .is_none());
    assert!(common::get_asset_snapshots(&db, "2025-01-01")
        .await
        .is_empty());
}
