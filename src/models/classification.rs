use clap::ValueEnum;

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum AssetClass {
    Equity,
    FixedIncome,
    Monetary,
    MultiAsset,
    Alternative,
    Commodity,
    RealEstate,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum EquityStyle {
    Value,
    Growth,
    Blend,
    Thematic,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum BondCredit {
    Government,
    InvestmentGrade,
    HighYield,
    InflationLinked,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum BondDuration {
    Short,
    Intermediate,
    Long,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum Management {
    Active,
    Passive,
}

#[derive(Clone, Debug, Default)]
pub struct AssetClassification {
    pub asset_class: Option<AssetClass>,
    pub equity_style: Option<EquityStyle>,
    pub bond_credit: Option<BondCredit>,
    pub bond_duration: Option<BondDuration>,
    pub management: Option<Management>,
}

impl AssetClassification {
    pub fn is_empty(&self) -> bool {
        self.asset_class.is_none()
            && self.equity_style.is_none()
            && self.bond_credit.is_none()
            && self.bond_duration.is_none()
            && self.management.is_none()
    }
}

pub fn enum_to_db<E: ValueEnum>(e: &E) -> String {
    e.to_possible_value()
        .expect("ValueEnum variant has no name")
        .get_name()
        .to_owned()
}
