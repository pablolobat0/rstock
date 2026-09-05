use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

fn run(home: &Path, args: &[&str]) -> Output {
    command(home)
        .args(args)
        .output()
        .expect("rstock should run")
}

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rstock"));
    command
        .env("HOME", home)
        .env(
            "RSTOCK_SOURCE_TOKEN_PAGE_URL",
            "https://example.invalid/token",
        )
        .env(
            "RSTOCK_SOURCE_CHARTSERVICE_URL",
            "https://example.invalid/chart",
        )
        .env(
            "RSTOCK_SOURCE_HOLDINGS_URL",
            "https://example.invalid/holdings",
        )
        .env("RSTOCK_SOURCE_QUOTE_URL", "https://example.invalid/quote")
        .env("RSTOCK_SOURCE_SAL_API_KEY", "test")
        .env("RSTOCK_SOURCE_USER_AGENT", "rstock-test")
        .env(
            "RSTOCK_SOURCE_TOKEN_CACHE_PATH",
            home.join("token-cache.json"),
        );
    command
}

fn run_success(home: &Path, args: &[&str]) -> Output {
    let output = run(home, args);
    assert!(
        output.status.success(),
        "rstock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_with_input(home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = command(home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rstock should start");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input)
        .expect("input should be written");
    child.wait_with_output().expect("rstock should finish")
}

fn run_json(home: &Path, args: &[&str], command: &str) -> Value {
    let output = run_success(home, args);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert_eq!(stdout.lines().count(), 1, "unexpected stdout: {stdout}");
    assert!(!stdout.contains("\u{1b}["));
    let value: Value = serde_json::from_str(&stdout).expect("stdout should be one JSON value");
    assert_eq!(value["command"], command);
    value
}

fn add_asset(home: &Path) {
    run_success(
        home,
        &[
            "portfolio",
            "asset",
            "add",
            "--ticker",
            "XFAKE1",
            "--name",
            "Fake Stock",
            "--type",
            "stock",
            "--asset-class",
            "equity",
            "--equity-style",
            "blend",
            "--management",
            "passive",
        ],
    );
}

fn buy(home: &Path, json: bool) -> Output {
    let mut args = vec![
        "transaction",
        "buy",
        "--ticker",
        "XFAKE1",
        "--date",
        "01-01-2025",
        "--quantity",
        "10",
        "--price",
        "12.3456",
        "--fees",
        "0.1234",
    ];
    if json {
        args.push("--json");
    }
    run_success(home, &args)
}

#[test]
fn empty_transaction_list_uses_the_normal_json_schema() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    let value = run_json(
        home.path(),
        &["--json", "transaction", "list"],
        "transaction.list",
    );

    assert_eq!(value["data"]["transactions"], json!([]));
    assert_eq!(value["data"]["count"], 0);
}

#[test]
fn transaction_workflow_emits_decimal_rows_and_id_receipts() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    add_asset(home.path());

    let buy_output = buy(home.path(), true);
    let stdout = String::from_utf8(buy_output.stdout).expect("stdout should be UTF-8");
    let buy_value: Value = serde_json::from_str(&stdout).expect("buy output should be JSON");
    assert_eq!(buy_value["command"], "transaction.buy");
    assert_eq!(buy_value["data"]["transaction_id"], 1);

    let list = run_json(
        home.path(),
        &["transaction", "list", "--json"],
        "transaction.list",
    );
    assert_eq!(list["data"]["count"], 1);
    let row = &list["data"]["transactions"][0];
    assert_eq!(row["id"], 1);
    assert_eq!(row["date"], "01-01-2025");
    assert_eq!(row["tx_type"], "buy");
    assert_eq!(row["ticker"], "XFAKE1");
    assert_eq!(row["asset_name"], "Fake Stock");
    assert_eq!(row["quantity"], 10.0);
    assert_eq!(row["price"], 12.3456);
    assert_eq!(row["fees"], 0.1234);
    assert!(row.get("asset_id").is_none());
    assert!(row.get("price_cents").is_none());

    let cases: &[(&[&str], &str, i64)] = &[
        (
            &[
                "transaction",
                "sell",
                "-t",
                "XFAKE1",
                "-d",
                "02-01-2025",
                "-q",
                "1",
                "-p",
                "13",
                "--json",
            ],
            "transaction.sell",
            2,
        ),
        (
            &[
                "transaction",
                "dividend",
                "-t",
                "XFAKE1",
                "-d",
                "03-01-2025",
                "-a",
                "2",
                "--json",
            ],
            "transaction.dividend",
            3,
        ),
        (
            &[
                "transaction",
                "split",
                "-t",
                "XFAKE1",
                "-d",
                "04-01-2025",
                "-r",
                "2",
                "--json",
            ],
            "transaction.split",
            4,
        ),
        (
            &[
                "transaction",
                "edit",
                "1",
                "--quantity",
                "11",
                "--yes",
                "--json",
            ],
            "transaction.edit",
            1,
        ),
        (
            &["transaction", "delete", "2", "--yes", "--json"],
            "transaction.delete",
            2,
        ),
    ];

    for (args, command, id) in cases {
        let value = run_json(home.path(), args, command);
        assert_eq!(value["data"]["transaction_id"], *id);
    }
}

#[test]
fn json_edit_and_delete_require_explicit_consent_without_mutating() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    add_asset(home.path());
    buy(home.path(), true);

    for args in [
        vec!["transaction", "edit", "1", "--quantity", "99", "--json"],
        vec!["transaction", "delete", "1", "--json"],
    ] {
        let output = run(home.path(), &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--yes is required"));
    }

    let list = run_json(
        home.path(),
        &["transaction", "list", "--json"],
        "transaction.list",
    );
    assert_eq!(list["data"]["count"], 1);
    assert_eq!(list["data"]["transactions"][0]["quantity"], 10.0);
}

#[test]
fn edit_and_delete_use_confirmation_and_leave_cancelled_transactions_unchanged() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    add_asset(home.path());
    buy(home.path(), false);

    let confirmation_cases: &[&[&str]] = &[
        &["transaction", "edit", "1", "--quantity", "99"],
        &["transaction", "delete", "1"],
    ];
    for args in confirmation_cases {
        let output = run_with_input(home.path(), &args, b"n\n");
        assert!(
            output.status.success(),
            "cancelled command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("Cancelled."));
    }

    let list = run_json(
        home.path(),
        &["transaction", "list", "--json"],
        "transaction.list",
    );
    assert_eq!(list["data"]["count"], 1);
    assert_eq!(list["data"]["transactions"][0]["quantity"], 10.0);
}

#[test]
fn edit_replays_later_entries_and_reports_nonexistent_ids_without_mutation() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    add_asset(home.path());
    buy(home.path(), true);
    run_success(
        home.path(),
        &[
            "transaction",
            "sell",
            "--ticker",
            "XFAKE1",
            "--date",
            "03-01-2025",
            "--quantity",
            "10",
            "--price",
            "12",
            "--json",
        ],
    );

    let invalid_edit = run(
        home.path(),
        &[
            "transaction",
            "edit",
            "1",
            "--quantity",
            "9",
            "--yes",
            "--json",
        ],
    );
    assert!(!invalid_edit.status.success());
    assert!(String::from_utf8_lossy(&invalid_edit.stderr).contains("ledger invariant"));

    let nonexistent_cases: &[&[&str]] = &[
        &[
            "transaction",
            "edit",
            "999",
            "--quantity",
            "1",
            "--yes",
            "--json",
        ],
        &["transaction", "delete", "999", "--yes", "--json"],
    ];
    for args in nonexistent_cases {
        let output = run(home.path(), &args);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("not found"));
    }

    let list = run_json(
        home.path(),
        &["transaction", "list", "--json"],
        "transaction.list",
    );
    assert_eq!(list["data"]["count"], 2);
    assert_eq!(list["data"]["transactions"][0]["quantity"], 10.0);
}

#[test]
fn import_and_export_emit_count_and_path_without_service_output() {
    let source_home = tempfile::tempdir().expect("temporary HOME should be created");
    add_asset(source_home.path());
    buy(source_home.path(), true);

    let files = tempfile::tempdir().expect("temporary file directory should be created");
    let export_path = files.path().join("transactions.csv");
    let export_path_text = export_path.to_string_lossy().into_owned();
    let export = run_json(
        source_home.path(),
        &[
            "transaction",
            "export",
            "--output",
            &export_path_text,
            "--json",
        ],
        "transaction.export",
    );
    assert_eq!(export["data"]["count"], 1);
    assert_eq!(export["data"]["path"], export_path_text);

    let target_home = tempfile::tempdir().expect("temporary HOME should be created");
    let import = run_json(
        target_home.path(),
        &[
            "transaction",
            "import",
            "--input",
            &export_path_text,
            "--json",
        ],
        "transaction.import",
    );
    assert_eq!(import["data"]["count"], 1);
    assert_eq!(import["data"]["path"], export_path_text);

    let human_home = tempfile::tempdir().expect("temporary HOME should be created");
    let human = run_success(
        human_home.path(),
        &["transaction", "import", "--input", &export_path_text],
    );
    let stdout = String::from_utf8(human.stdout).expect("stdout should be UTF-8");
    assert!(stdout.starts_with(
        "Bought 10 units of Fake Stock (XFAKE1) at 12.35 EUR on 01-01-2025. Total: 123.58 EUR\nTransaction ID: 1\n"
    ));
    assert!(stdout.ends_with(&format!(
        "Imported 1 transactions from {export_path_text}\n"
    )));
}

#[test]
fn human_buy_summary_is_unchanged() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    add_asset(home.path());

    let output = buy(home.path(), false);
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be UTF-8"),
        "Bought 10 units of Fake Stock (XFAKE1) at 12.35 EUR on 01-01-2025. Total: 123.58 EUR\nTransaction ID: 1\n"
    );
}
