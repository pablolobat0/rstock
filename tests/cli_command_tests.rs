use clap::Parser;
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
