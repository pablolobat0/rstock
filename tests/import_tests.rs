mod common;

use std::io::Write;

use rstock::db::repos::transaction_repo;
use rstock::models::TxType;
use rstock::services::export::export_transactions_csv;
use rstock::services::import::import_transactions_csv;
use tempfile::NamedTempFile;

fn write_csv(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("failed to write CSV");
    file.flush().expect("failed to flush");
    file
}

#[tokio::test]
async fn test_import_buy_sell_dividend_split() {
    let db = common::setup_test_db().await;

    let csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,buy,10,100.00,5.00\n\
         15-01-2025,XFAKE1,,,EUR,sell,2,110.00,3.00\n\
         20-01-2025,XFAKE1,,,EUR,dividend,1,50.00,5.00\n\
         25-01-2025,XFAKE1,,,EUR,split,2,0.00,0.00\n",
    );

    let count = import_transactions_csv(&db, csv.path().to_str().unwrap())
        .await
        .expect("import should succeed");
    assert_eq!(count, 4);

    let txns = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .expect("failed to query transactions");
    assert_eq!(txns.len(), 4);

    assert_eq!(txns[0].tx_type, TxType::Buy);
    assert_eq!(txns[0].quantity, 10.0);

    assert_eq!(txns[1].tx_type, TxType::Sell);
    assert_eq!(txns[1].quantity, 2.0);

    assert_eq!(txns[2].tx_type, TxType::Dividend);

    assert_eq!(txns[3].tx_type, TxType::Split);
    assert_eq!(txns[3].quantity, 2.0);
}

#[tokio::test]
async fn test_import_invalid_date() {
    let db = common::setup_test_db().await;

    let csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         2025-01-01,XFAKE1,Fake Stock,stock,EUR,buy,10,100.00,5.00\n",
    );

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("row 2"),
        "error should mention row number: {err}"
    );
}

#[tokio::test]
async fn test_import_sell_nonexistent_asset() {
    let db = common::setup_test_db().await;

    let csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         01-01-2025,XFAKE1,,,EUR,sell,5,100.00,0.00\n",
    );

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_import_buy_missing_name() {
    let db = common::setup_test_db().await;

    let csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         01-01-2025,XFAKE1,,stock,EUR,buy,10,100.00,5.00\n",
    );

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Name"), "error should mention Name: {err}");
}

#[tokio::test]
async fn test_import_buy_missing_asset_type() {
    let db = common::setup_test_db().await;

    let csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         01-01-2025,XFAKE1,Fake Stock,,EUR,buy,10,100.00,5.00\n",
    );

    let result = import_transactions_csv(&db, csv.path().to_str().unwrap()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("AssetType"),
        "error should mention AssetType: {err}"
    );
}

#[tokio::test]
async fn test_import_export_roundtrip() {
    let db1 = common::setup_test_db().await;

    let buy_csv = write_csv(
        "Date,Ticker,Name,AssetType,Currency,Type,Quantity,Price,Fees\n\
         01-01-2025,XFAKE1,Fake Stock,stock,EUR,buy,10,100.00,5.00\n\
         10-01-2025,XFAKE1,,,EUR,sell,3,120.00,2.00\n",
    );

    import_transactions_csv(&db1, buy_csv.path().to_str().unwrap())
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

    let export_file2 = NamedTempFile::new().expect("failed to create temp file");
    let export_path2 = export_file2.path().to_str().unwrap();
    export_transactions_csv(&db2, export_path2)
        .await
        .expect("second export should succeed");

    let csv1 = std::fs::read_to_string(export_path).expect("failed to read first export");
    let csv2 = std::fs::read_to_string(export_path2).expect("failed to read second export");
    assert_eq!(csv1, csv2, "roundtrip CSVs should be identical");
}
