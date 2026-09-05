use clap::{CommandFactory, Parser};
use rstock::cli::{
    AnalyzeCommands, Cli, Commands, CompareCommands, CorrelationPeriod, PortfolioCommands,
    TransactionCommands,
};

#[test]
fn get_and_portfolio_get_parse() {
    let cli = Cli::try_parse_from(["rstock", "get"]).expect("get should parse");
    assert!(matches!(cli.command, Commands::Get { .. }));

    let cli =
        Cli::try_parse_from(["rstock", "portfolio", "get"]).expect("portfolio get should parse");
    assert!(matches!(
        cli.command,
        Commands::Portfolio(args) if matches!(args.command, PortfolioCommands::Get { .. })
    ));
}

#[test]
fn json_is_global_and_has_no_short_alias() {
    let cli = Cli::try_parse_from(["rstock", "--json", "get"])
        .expect("JSON should parse before the command");
    assert!(cli.json);

    let cli = Cli::try_parse_from(["rstock", "get", "--json"])
        .expect("JSON should parse after the top-level command");
    assert!(cli.json);

    let cli = Cli::try_parse_from(["rstock", "portfolio", "--json", "get"])
        .expect("JSON should parse within a nested command path");
    assert!(cli.json);

    let cli = Cli::try_parse_from(["rstock", "portfolio", "get", "--json"])
        .expect("JSON should parse after a nested command path");
    assert!(cli.json);

    assert!(Cli::try_parse_from(["rstock", "-j", "get"]).is_err());
}

#[test]
fn every_application_command_leaf_accepts_json() {
    let cases: &[&[&str]] = &[
        &["rstock", "get", "--json"],
        &["rstock", "portfolio", "get", "--json"],
        &[
            "rstock",
            "portfolio",
            "asset",
            "add",
            "-t",
            "XFAKE1",
            "-n",
            "Fake",
            "-T",
            "stock",
            "--asset-class",
            "equity",
            "--json",
        ],
        &[
            "rstock",
            "portfolio",
            "asset",
            "edit",
            "-t",
            "XFAKE1",
            "-n",
            "Fake",
            "--json",
        ],
        &["rstock", "transaction", "list", "--json"],
        &[
            "rstock",
            "transaction",
            "buy",
            "-t",
            "XFAKE1",
            "-d",
            "01-01-2025",
            "-q",
            "1",
            "-p",
            "10",
            "--json",
        ],
        &[
            "rstock",
            "transaction",
            "sell",
            "-t",
            "XFAKE1",
            "-d",
            "01-01-2025",
            "-q",
            "1",
            "-p",
            "10",
            "--json",
        ],
        &[
            "rstock",
            "transaction",
            "dividend",
            "-t",
            "XFAKE1",
            "-d",
            "01-01-2025",
            "-a",
            "1",
            "--json",
        ],
        &[
            "rstock",
            "transaction",
            "split",
            "-t",
            "XFAKE1",
            "-d",
            "01-01-2025",
            "-r",
            "2",
            "--json",
        ],
        &[
            "rstock",
            "transaction",
            "edit",
            "1",
            "--quantity",
            "2",
            "--yes",
            "--json",
        ],
        &["rstock", "transaction", "delete", "1", "--yes", "--json"],
        &["rstock", "transaction", "export", "-o", "tx.csv", "--json"],
        &["rstock", "transaction", "import", "-i", "tx.csv", "--json"],
        &["rstock", "analyze", "composition", "--json"],
        &[
            "rstock",
            "analyze",
            "fund",
            "--code",
            "F00000TEST",
            "--json",
        ],
        &["rstock", "analyze", "correlation", "matrix", "--json"],
        &[
            "rstock",
            "analyze",
            "correlation",
            "rolling",
            "XFAKE1",
            "XFAKE2",
            "--json",
        ],
        &[
            "rstock", "compare", "funds", "--code-a", "F00000A", "--code-b", "F00000B", "--json",
        ],
    ];

    for args in cases {
        let cli = Cli::try_parse_from(*args).unwrap_or_else(|error| {
            panic!("JSON should parse for {args:?}: {error}");
        });
        assert!(cli.json, "JSON was not enabled for {args:?}");
    }
}

#[test]
fn application_command_surface_matches_the_dispatch_audit() {
    fn collect_leaves(command: &clap::Command, prefix: &str, leaves: &mut Vec<String>) {
        for child in command
            .get_subcommands()
            .filter(|child| child.get_name() != "help")
        {
            let path = if prefix.is_empty() {
                child.get_name().to_string()
            } else {
                format!("{prefix} {}", child.get_name())
            };
            if child.get_subcommands().next().is_some() {
                collect_leaves(child, &path, leaves);
            } else {
                leaves.push(path);
            }
        }
    }

    let mut actual = Vec::new();
    collect_leaves(&Cli::command(), "", &mut actual);
    actual.sort();

    let mut expected = vec![
        "analyze composition",
        "analyze correlation matrix",
        "analyze correlation rolling",
        "analyze fund",
        "compare funds",
        "get",
        "portfolio asset add",
        "portfolio asset edit",
        "portfolio get",
        "transaction buy",
        "transaction delete",
        "transaction dividend",
        "transaction edit",
        "transaction export",
        "transaction import",
        "transaction list",
        "transaction sell",
        "transaction split",
    ];
    expected.sort();

    assert_eq!(actual, expected);
}

#[test]
fn removed_command_paths_do_not_parse() {
    assert!(Cli::try_parse_from(["rstock", "buy", "-t", "XFAKE1"]).is_err());
    assert!(Cli::try_parse_from(["rstock", "portfolio", "list"]).is_err());
    assert!(Cli::try_parse_from(["rstock", "data", "export", "-o", "tx.csv"]).is_err());
    assert!(Cli::try_parse_from(["rstock", "monitor", "list"]).is_err());
}

#[test]
fn analysis_commands_parse() {
    let cli = Cli::try_parse_from(["rstock", "analyze", "fund", "--code", "F00000TEST"])
        .expect("analyze fund should parse");
    assert!(matches!(
        cli.command,
        Commands::Analyze(args) if matches!(args.command, AnalyzeCommands::Fund { period: CorrelationPeriod::OneYear, .. })
    ));

    let cli = Cli::try_parse_from([
        "rstock",
        "analyze",
        "fund",
        "--code",
        "F00000TEST",
        "--period",
        "30d",
    ])
    .expect("analyze fund with period should parse");
    assert!(matches!(
        cli.command,
        Commands::Analyze(args) if matches!(args.command, AnalyzeCommands::Fund { period: CorrelationPeriod::ThirtyDays, .. })
    ));

    let cli = Cli::try_parse_from([
        "rstock",
        "analyze",
        "correlation",
        "rolling",
        "XFAKE1",
        "IE00XFAKE2",
    ])
    .expect("rolling correlation should parse");
    assert!(matches!(cli.command, Commands::Analyze(_)));
}

#[test]
fn compare_funds_command_parses() {
    let cli = Cli::try_parse_from([
        "rstock", "compare", "funds", "--code-a", "F00000A", "--code-b", "F00000B", "--period",
        "6m",
    ])
    .expect("compare funds should parse");
    assert!(matches!(
        cli.command,
        Commands::Compare(args) if matches!(args.command, CompareCommands::Funds { .. })
    ));

    let cli = Cli::try_parse_from([
        "rstock", "compare", "funds", "--code-a", "F00000A", "--code-b", "F00000B",
    ])
    .expect("compare funds should parse with default period");
    assert!(matches!(cli.command, Commands::Compare(_)));
}

#[test]
fn transaction_commands_parse() {
    let cli = Cli::try_parse_from(["rstock", "transaction", "list"])
        .expect("transaction list should parse");
    assert!(matches!(
        cli.command,
        Commands::Transaction(args) if matches!(args.command, TransactionCommands::List {})
    ));

    let cli = Cli::try_parse_from([
        "rstock",
        "transaction",
        "buy",
        "-t",
        "XFAKE1",
        "-d",
        "01-01-2025",
        "-q",
        "1",
        "-p",
        "10",
    ])
    .expect("transaction buy should parse");
    assert!(matches!(
        cli.command,
        Commands::Transaction(args) if matches!(args.command, TransactionCommands::Buy { .. })
    ));

    let cli = Cli::try_parse_from(["rstock", "transaction", "export", "-o", "tx.csv"])
        .expect("transaction export should parse");
    assert!(matches!(
        cli.command,
        Commands::Transaction(args) if matches!(args.command, TransactionCommands::Export { .. })
    ));

    let cli = Cli::try_parse_from(["rstock", "transaction", "import", "-i", "tx.csv"])
        .expect("transaction import should parse");
    assert!(matches!(
        cli.command,
        Commands::Transaction(args) if matches!(args.command, TransactionCommands::Import { .. })
    ));
}

#[test]
fn transaction_cli_rejects_invalid_numeric_values() {
    assert!(Cli::try_parse_from([
        "rstock",
        "transaction",
        "buy",
        "-t",
        "XFAKE1",
        "-d",
        "01-01-2025",
        "-q",
        "0",
        "-p",
        "10",
    ])
    .is_err());

    assert!(Cli::try_parse_from([
        "rstock",
        "transaction",
        "dividend",
        "-t",
        "XFAKE1",
        "-d",
        "01-01-2025",
        "-a",
        "0",
    ])
    .is_err());

    assert!(Cli::try_parse_from([
        "rstock",
        "transaction",
        "split",
        "-t",
        "XFAKE1",
        "-d",
        "01-01-2025",
        "-r",
        "0",
    ])
    .is_err());

    assert!(Cli::try_parse_from([
        "rstock",
        "transaction",
        "sell",
        "-t",
        "XFAKE1",
        "-d",
        "01-01-2025",
        "-q",
        "1",
        "-p",
        "10",
        "-f",
        "-1",
    ])
    .is_err());

    assert!(Cli::try_parse_from([
        "rstock",
        "transaction",
        "edit",
        "1",
        "--date",
        "01-01-2025x",
    ])
    .is_err());

    assert!(Cli::try_parse_from([
        "rstock",
        "transaction",
        "split",
        "-t",
        "XFAKE1",
        "-d",
        "01-01-2025",
        "-r",
        "2x",
    ])
    .is_err());
}

#[test]
fn asset_edit_does_not_accept_identity_type_or_currency_changes() {
    assert!(Cli::try_parse_from([
        "rstock",
        "portfolio",
        "asset",
        "edit",
        "-t",
        "XFAKE1",
        "--type",
        "fund",
    ])
    .is_err());

    assert!(Cli::try_parse_from([
        "rstock",
        "portfolio",
        "asset",
        "edit",
        "-t",
        "XFAKE1",
        "--currency",
        "USD",
    ])
    .is_err());
}
