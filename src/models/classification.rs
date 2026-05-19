use clap::ValueEnum;

use super::AssetType;

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

    pub fn validate_for_asset(
        &self,
        asset_type: &AssetType,
        morningstar_code: Option<&str>,
    ) -> anyhow::Result<()> {
        let asset_class = self
            .asset_class
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("asset classification is required"))?;

        if self.equity_style.is_some() && asset_class != &AssetClass::Equity {
            anyhow::bail!("equity style is only valid for equity assets");
        }

        if self.bond_credit.is_some() && asset_class != &AssetClass::FixedIncome {
            anyhow::bail!("bond credit is only valid for fixed-income assets");
        }

        if self.bond_duration.is_some() && asset_class != &AssetClass::FixedIncome {
            anyhow::bail!("bond duration is only valid for fixed-income assets");
        }

        if matches!(asset_type, AssetType::Fund | AssetType::Etf)
            && morningstar_code.is_none_or(|code| code.trim().is_empty())
        {
            anyhow::bail!("fund and ETF assets require a Morningstar code");
        }

        Ok(())
    }
}

pub fn enum_to_db<E: ValueEnum>(e: &E) -> String {
    e.to_possible_value()
        .expect("ValueEnum variant has no name")
        .get_name()
        .to_owned()
}
