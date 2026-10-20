pub mod adapters;
pub mod commands;
mod dispatch;
pub mod display;
pub mod output;

pub use dispatch::{
    run_benchmark_command_with_writer, run_blend_command_with_writer, run_command,
    run_command_with_writer,
};

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::constants::DISPLAY_DATE_FORMAT;
use crate::models::{
    AssetClass, AssetType, BlendComponentArgument, BondCredit, BondDuration, EquityStyle,
    Management,
};

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    let date = NaiveDate::parse_from_str(s, DISPLAY_DATE_FORMAT)
        .map_err(|_| format!("invalid date '{s}', expected DD-MM-YYYY format"))?;
    let today = chrono::Local::now().date_naive();
    if date > today {
        return Err(format!("date cannot be in the future: {s}"));
    }
    Ok(date)
}

fn parse_positive_f64(s: &str) -> Result<f64, String> {
    let val: f64 = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid number"))?;

    if val > 0.0 {
        Ok(val)
    } else {
        Err("value must be greater than 0".to_string())
    }
}

fn parse_non_negative_f64(s: &str) -> Result<f64, String> {
    let val: f64 = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid number"))?;

    if val >= 0.0 {
        Ok(val)
    } else {
        Err("value must be greater than or equal to 0".to_string())
    }
}

#[derive(Parser)]
#[command(
    name = "rstock",
    version,
    about = "Personal investment portfolio manager",
    long_about = "Personal investment portfolio manager.\n\n\
        Track purchases, sales, dividends, and splits. View portfolio \
        performance and analyze portfolio-relevant data.",
    after_help = "Use 'rstock <command> --help' for more information about a specific command."
)]
pub struct Cli {
    /// Increase logging verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Emit successful command output as JSON
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CorrelationPeriod {
    #[value(name = "30d")]
    ThirtyDays,
    #[value(name = "6m")]
    SixMonths,
    #[value(name = "1y")]
    OneYear,
    #[value(name = "3y")]
    ThreeYears,
    #[value(name = "5y")]
    FiveYears,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ChartPeriod {
    #[value(name = "1m")]
    OneMonth,
    #[value(name = "3m")]
    ThreeMonths,
    #[value(name = "6m")]
    SixMonths,
    Ytd,
    #[value(name = "1y")]
    OneYear,
    #[value(name = "3y")]
    ThreeYears,
    #[value(name = "5y")]
    FiveYears,
    All,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show portfolio overview with NAV chart and key metrics (shortcut for `portfolio get`)
    Get {
        /// Time period for the NAV chart
        #[arg(short, long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },

    /// Portfolio dashboard and asset metadata management
    Portfolio(PortfolioArgs),

    /// Maintain the Transaction ledger
    Transaction(TransactionArgs),

    /// Analyze portfolio-relevant data: composition, fund candidates, correlations
    Analyze(AnalyzeArgs),

    /// Compare portfolio-relevant candidates side by side
    Compare(CompareArgs),
    /// Browse benchmark definitions and manage explicit primary assignments
    Benchmark(BenchmarkArgs),
    /// Create, inspect and delete saved Custom benchmark blends
    Blend(BlendArgs),
    /// Select, unselect and inspect the portfolio's Strategy benchmark
    Strategy(StrategyArgs),
}

#[derive(Debug, Args)]
pub struct BenchmarkArgs {
    #[command(subcommand)]
    pub command: BenchmarkCommands,
}

#[derive(Debug, Subcommand)]
pub enum BenchmarkCommands {
    /// List approved reference definitions and tracked-asset assignment state
    List {},
    /// Show one stable reference definition
    Show { reference_id: String },
    /// Explicitly assign or replace a Tracked asset's primary reference
    Assign {
        #[arg(short = 't', long = "ticker")]
        ticker: String,
        reference_id: String,
        #[arg(long)]
        override_selection: bool,
    },
    /// Explicitly assign a reference to an exact Fund candidate without creating a holding
    AssignCandidate {
        #[arg(long)]
        identifier: String,
        #[arg(long, value_enum)]
        vehicle: CandidateVehicle,
        reference_id: String,
    },
    /// Remove a Tracked asset's primary reference
    Remove {
        #[arg(short = 't', long = "ticker")]
        ticker: String,
    },
    /// Remove an exact Fund candidate's primary reference
    RemoveCandidate {
        #[arg(long)]
        identifier: String,
        #[arg(long, value_enum)]
        vehicle: CandidateVehicle,
    },
    /// Explicitly assign an optional secondary reference to a Tracked asset
    AssignSecondary {
        #[arg(short = 't', long = "ticker")]
        ticker: String,
        reference_id: String,
    },
    /// Remove an optional secondary reference of a Tracked asset
    RemoveSecondary {
        #[arg(short = 't', long = "ticker")]
        ticker: String,
        /// The secondary reference to remove; every secondary of the asset when absent
        #[arg(long)]
        reference_id: Option<String>,
    },
    /// Explicitly assign an optional secondary reference to an exact Fund candidate
    AssignSecondaryCandidate {
        #[arg(long)]
        identifier: String,
        #[arg(long, value_enum)]
        vehicle: CandidateVehicle,
        reference_id: String,
    },
    /// Remove an optional secondary reference of an exact Fund candidate
    RemoveSecondaryCandidate {
        #[arg(long)]
        identifier: String,
        #[arg(long, value_enum)]
        vehicle: CandidateVehicle,
        /// The secondary reference to remove; every secondary of the candidate when absent
        #[arg(long)]
        reference_id: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CandidateVehicle {
    Fund,
    Etf,
}

#[derive(Debug, Args)]
pub struct BlendArgs {
    #[command(subcommand)]
    pub command: BlendCommands,
}

/// The grouped Strategy benchmark selection commands.
#[derive(Debug, Args)]
pub struct StrategyArgs {
    #[command(subcommand)]
    pub command: StrategyCommands,
}

/// The explicit Strategy benchmark selection commands.
///
/// A selection is a reference operation: it records which saved definition the
/// portfolio is compared against and never infers a strategy from the
/// holdings.
#[derive(Clone, Debug, Subcommand)]
pub enum StrategyCommands {
    /// Explicitly select a saved definition as the portfolio's Strategy benchmark
    Select {
        /// The saved benchmark definition identity to select
        reference_id: String,
    },
    /// Explicitly remove the portfolio's Strategy benchmark selection
    Unselect {},
    /// Show the current Strategy benchmark selection
    Show {},
}

#[derive(Debug, Subcommand)]
pub enum BlendCommands {
    /// List saved Custom benchmark blends
    List {},
    /// Create an immutable Custom benchmark blend from explicit components
    Create {
        /// Display name of the blend; its stable identity is derived from it
        #[arg(short = 'n', long = "name")]
        name: String,

        /// Component as <reference-id|cash>:<weight-percent>; repeat per component
        #[arg(short = 'c', long = "component", value_parser = parse_blend_component)]
        component: Vec<BlendComponentArgument>,

        /// Explicit later comparison start (DD-MM-YYYY); defaults to the full comparison history
        #[arg(long = "start", value_parser = parse_date)]
        start: Option<NaiveDate>,
    },
    /// Show one saved Custom benchmark blend with its component conventions
    Show { blend_id: String },
    /// Preview the dated rebased wealth series of one saved Custom benchmark blend
    Preview { blend_id: String },
    /// Delete one unused saved Custom benchmark blend
    Delete { blend_id: String },
}

/// Parses one declared component as `<reference-id|cash>:<weight-percent>`.
fn parse_blend_component(value: &str) -> Result<BlendComponentArgument, String> {
    let (identity, weight) = value.split_once(':').ok_or_else(|| {
        format!("invalid component '{value}', expected <reference-id|cash>:<weight-percent>")
    })?;
    let identity = identity.trim();
    if identity.is_empty() {
        return Err(format!(
            "invalid component '{value}', the component identity is empty"
        ));
    }
    let weight_percent: f64 = weight
        .trim()
        .parse()
        .map_err(|_| format!("invalid component '{value}', the weight must be a number"))?;
    if !weight_percent.is_finite() {
        return Err(format!(
            "invalid component '{value}', the weight must be a finite number"
        ));
    }
    Ok(BlendComponentArgument {
        identity: identity.to_owned(),
        weight_percent,
    })
}

#[derive(Debug, Args)]
pub struct CompareArgs {
    #[command(subcommand)]
    pub command: CompareCommands,
}

#[derive(Debug, Subcommand)]
pub enum CompareCommands {
    /// Compare two Morningstar fund codes
    Funds {
        /// First Morningstar security code
        #[arg(long = "code-a")]
        code_a: String,

        /// Second Morningstar security code
        #[arg(long = "code-b")]
        code_b: String,

        /// Period reserved for comparison correlation and graph sections
        #[arg(short, long, value_enum, default_value = "1y")]
        period: CorrelationPeriod,
    },
}

#[derive(Debug, Args)]
pub struct AnalyzeArgs {
    #[command(subcommand)]
    pub command: AnalyzeCommands,
}

#[derive(Debug, Subcommand)]
pub enum AnalyzeCommands {
    /// Portfolio composition: fund type, sector, country, and market cap breakdown
    Composition {},

    /// Deep-dive analysis of a fund: performance, holdings, sector/country/currency breakdowns
    Fund {
        /// Morningstar security code (e.g. F00000YN5R)
        #[arg(short, long)]
        code: String,

        /// Time period for candidate correlation calculation
        #[arg(short, long, value_enum, default_value = "1y")]
        period: CorrelationPeriod,
    },

    /// Correlation analysis: static matrix and rolling pair correlation
    Correlation(CorrelationArgs),
}

#[derive(Debug, Args)]
pub struct CorrelationArgs {
    #[command(subcommand)]
    pub command: CorrelationCommands,
}

#[derive(Debug, Subcommand)]
pub enum CorrelationCommands {
    /// Static correlation matrix between portfolio assets
    Matrix {
        /// Time period for correlation calculation
        #[arg(short, long, value_enum, default_value = "1y")]
        period: CorrelationPeriod,
    },

    /// 60-day rolling correlation for two Tracked assets
    Rolling {
        /// First Tracked asset ticker/ISIN
        identifier_a: String,

        /// Second Tracked asset ticker/ISIN
        identifier_b: String,

        /// Time period for correlation calculation
        #[arg(short, long, value_enum, default_value = "1y")]
        period: CorrelationPeriod,
    },
}

#[derive(Debug, Args)]
pub struct PortfolioArgs {
    #[command(subcommand)]
    pub command: PortfolioCommands,
}

#[derive(Debug, Subcommand)]
pub enum PortfolioCommands {
    /// Show portfolio overview with NAV chart and key metrics
    Get {
        /// Time period for the NAV chart
        #[arg(short, long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },

    /// Manage assets: add or edit asset metadata and classification
    Asset(AssetArgs),
}

#[derive(Debug, Args)]
pub struct AssetArgs {
    #[command(subcommand)]
    pub command: AssetCommands,
}

#[derive(Debug, Subcommand)]
pub enum AssetCommands {
    /// Create an asset with its classification
    Add {
        /// Ticker symbol (stocks) or ISIN (funds/ETFs)
        #[arg(short, long)]
        ticker: String,

        /// Full name of the asset
        #[arg(short, long)]
        name: String,

        /// Vehicle type
        #[arg(short = 'T', long = "type", value_enum)]
        asset_type: AssetType,

        /// Trading denomination (GBP = pounds; GBX or `GBp` = pence)
        #[arg(short, long, default_value = "EUR")]
        currency: String,

        /// Asset class (top-level classification)
        #[arg(long = "asset-class", value_enum)]
        asset_class: AssetClass,

        /// Equity style (for equity assets)
        #[arg(long = "equity-style", value_enum)]
        equity_style: Option<EquityStyle>,

        /// Bond credit quality (for fixed-income assets)
        #[arg(long = "bond-credit", value_enum)]
        bond_credit: Option<BondCredit>,

        /// Bond duration bucket (for fixed-income assets)
        #[arg(long = "bond-duration", value_enum)]
        bond_duration: Option<BondDuration>,

        /// Management style (active vs passive)
        #[arg(long, value_enum)]
        management: Option<Management>,

        /// Morningstar code (for funds/ETFs needing the price scraper)
        #[arg(long = "morningstar-code")]
        morningstar_code: Option<String>,
    },

    /// Update an existing asset's metadata or classification
    Edit {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// New name
        #[arg(short, long)]
        name: Option<String>,

        /// Asset class
        #[arg(long = "asset-class", value_enum)]
        asset_class: Option<AssetClass>,

        /// Equity style
        #[arg(long = "equity-style", value_enum)]
        equity_style: Option<EquityStyle>,

        /// Bond credit quality
        #[arg(long = "bond-credit", value_enum)]
        bond_credit: Option<BondCredit>,

        /// Bond duration bucket
        #[arg(long = "bond-duration", value_enum)]
        bond_duration: Option<BondDuration>,

        /// Management style
        #[arg(long, value_enum)]
        management: Option<Management>,

        /// Morningstar code
        #[arg(long = "morningstar-code")]
        morningstar_code: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct TransactionArgs {
    #[command(subcommand)]
    pub command: TransactionCommands,
}

#[derive(Debug, Subcommand)]
pub enum TransactionCommands {
    /// List transactions with IDs for edit/delete workflows
    List {},

    /// Record a buy transaction for an existing asset
    Buy {
        /// Ticker symbol (asset must already exist; create with `portfolio asset add`)
        #[arg(short, long)]
        ticker: String,

        /// Purchase date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Number of shares/units
        #[arg(short, long, value_parser = parse_positive_f64)]
        quantity: f64,

        /// Price per unit in the asset denomination (GBX = pence)
        #[arg(short, long, value_parser = parse_positive_f64)]
        price: f64,

        /// Broker commission and fees in the asset denomination (GBX = pence)
        #[arg(short, long, default_value = "0", value_parser = parse_non_negative_f64)]
        fees: f64,
    },

    /// Record a sell transaction for an existing asset
    Sell {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// Sale date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Number of shares/units to sell
        #[arg(short, long, value_parser = parse_positive_f64)]
        quantity: f64,

        /// Sale price per unit in the asset denomination (GBX = pence)
        #[arg(short, long, value_parser = parse_positive_f64)]
        price: f64,

        /// Broker commission and fees in the asset denomination (GBX = pence)
        #[arg(short, long, default_value = "0", value_parser = parse_non_negative_f64)]
        fees: f64,
    },

    /// Record a dividend payment for an existing asset
    Dividend {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// Ex-dividend date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Total dividend amount in the asset denomination (GBX = pence)
        #[arg(short, long, value_parser = parse_positive_f64)]
        amount: f64,

        /// Withholding tax or fees in the asset denomination (GBX = pence)
        #[arg(short, long, default_value = "0", value_parser = parse_non_negative_f64)]
        fees: f64,
    },

    /// Record a stock split or reverse split for an existing asset
    Split {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// Split date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Split ratio: new shares per old share (e.g. 2 for 2:1 split, 0.25 for 1:4 reverse split)
        #[arg(short, long, value_parser = parse_positive_f64)]
        ratio: f64,
    },

    /// Edit an existing transaction (date, quantity, price, fees)
    Edit {
        /// Transaction ID
        id: i32,

        /// New date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: Option<NaiveDate>,

        /// New quantity
        #[arg(short, long, value_parser = parse_positive_f64)]
        quantity: Option<f64>,

        /// New price per unit
        #[arg(short, long, value_parser = parse_positive_f64)]
        price: Option<f64>,

        /// New fees
        #[arg(short, long, value_parser = parse_non_negative_f64)]
        fees: Option<f64>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Delete an existing transaction
    Delete {
        /// Transaction ID
        id: i32,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Export all transactions to a CSV file
    Export {
        /// Output file path (e.g. transactions.csv)
        #[arg(long, short)]
        output: String,
    },

    /// Import transactions from a CSV file
    Import {
        /// Input CSV file path (e.g. transactions.csv)
        #[arg(long, short)]
        input: String,
    },
}
