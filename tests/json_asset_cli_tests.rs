use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

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

fn parse_json(output: Output, command: &str) -> Value {
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert_eq!(stdout.lines().count(), 1, "unexpected stdout: {stdout}");
    assert!(!stdout.contains("\u{1b}["));
    let value: Value = serde_json::from_str(&stdout).expect("stdout should be one JSON value");
    assert_eq!(value["command"], command);
    value
}

#[test]
fn asset_add_and_edit_emit_identifier_receipts() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    let isin = "IE00XFAKE001";

    let added = parse_json(
        run_success(
            home.path(),
            &[
                "portfolio",
                "asset",
                "add",
                "--ticker",
                isin,
                "--name",
                "Fake ETF",
                "--type",
                "etf",
                "--asset-class",
                "equity",
                "--morningstar-code",
                "F00000XFAKE",
                "--json",
            ],
        ),
        "portfolio.asset.add",
    );
    assert_eq!(added["data"]["asset_id"], 1);
    assert_eq!(added["data"]["ticker"], isin);

    let edited = parse_json(
        run_success(
            home.path(),
            &[
                "--json",
                "portfolio",
                "asset",
                "edit",
                "--ticker",
                isin,
                "--name",
                "Renamed Fake ETF",
            ],
        ),
        "portfolio.asset.edit",
    );
    assert_eq!(edited["data"]["ticker"], isin);
}

#[test]
fn asset_add_and_edit_human_messages_are_unchanged() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");

    let added = run_success(
        home.path(),
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
        ],
    );
    assert_eq!(
        String::from_utf8(added.stdout).expect("stdout should be UTF-8"),
        "Added asset XFAKE1\n"
    );

    let edited = run_success(
        home.path(),
        &[
            "portfolio",
            "asset",
            "edit",
            "--ticker",
            "XFAKE1",
            "--name",
            "Renamed Fake Stock",
        ],
    );
    assert_eq!(
        String::from_utf8(edited.stdout).expect("stdout should be UTF-8"),
        "Updated asset XFAKE1\n"
    );
}
