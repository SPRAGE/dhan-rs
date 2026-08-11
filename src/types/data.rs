#![allow(missing_docs)]
//! Types for rolling expired-options, technical metrics, market movers, and
//! company-information Data APIs.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! string_wire_enum {
    ($name:ident { $($variant:ident => $wire:literal,)+ }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant,)+
            /// Forward-compatible value not known by this crate version.
            Other(String),
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value,
                }
            }

            fn from_wire(value: String) -> Self {
                match value.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Other(value),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                String::deserialize(deserializer).map(Self::from_wire)
            }
        }
    };
}

fn null_to_default<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollingExchangeSegment {
    #[serde(rename = "NSE_EQ")]
    NseEq,
    #[serde(rename = "NSE_FNO")]
    NseFno,
    #[serde(rename = "BSE_EQ")]
    BseEq,
    #[serde(rename = "BSE_FNO")]
    BseFno,
    #[serde(rename = "MCX_COMM")]
    McxComm,
    #[serde(rename = "IDX_I")]
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollingInterval {
    #[serde(rename = "1")]
    OneMinute,
    #[serde(rename = "5")]
    FiveMinutes,
    #[serde(rename = "15")]
    FifteenMinutes,
    #[serde(rename = "25")]
    TwentyFiveMinutes,
    #[serde(rename = "60")]
    SixtyMinutes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollingInstrument {
    Index,
    Futidx,
    Optidx,
    Equity,
    Futstk,
    Optstk,
    Futcom,
    Optfut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollingExpiryFlag {
    Month,
    Week,
}

/// Near-to-far rolling expiry selector. Dhan encodes this as a JSON number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RollingExpiryCode {
    First = 1,
    Second = 2,
    Third = 3,
}

impl Serialize for RollingExpiryCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for RollingExpiryCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match u8::deserialize(deserializer)? {
            1 => Ok(Self::First),
            2 => Ok(Self::Second),
            3 => Ok(Self::Third),
            value => Err(serde::de::Error::custom(format!(
                "invalid rolling expiry code {value}; expected 1, 2, or 3"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollingOptionType {
    Call,
    Put,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RollingDataField {
    Open,
    High,
    Low,
    Close,
    Iv,
    Volume,
    Strike,
    Oi,
    Spot,
}

/// Request for `POST /v2/charts/rollingoption`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollingOptionRequest {
    pub exchange_segment: RollingExchangeSegment,
    pub interval: RollingInterval,
    pub security_id: u64,
    pub instrument: RollingInstrument,
    pub expiry_flag: RollingExpiryFlag,
    pub expiry_code: RollingExpiryCode,
    /// Relative strike expression such as `ATM` or `ATM+1`.
    pub strike: String,
    pub drv_option_type: RollingOptionType,
    pub required_data: Vec<RollingDataField>,
    /// Start date in `YYYY-MM-DD` format.
    pub from_date: String,
    /// End date in `YYYY-MM-DD` format.
    pub to_date: String,
}

impl RollingOptionRequest {
    pub(crate) fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.required_data.is_empty() {
            return Err("rolling option required_data cannot be empty");
        }
        if self.strike.trim().is_empty()
            || self.from_date.trim().is_empty()
            || self.to_date.trim().is_empty()
        {
            return Err("rolling option strike and date fields cannot be empty");
        }
        Ok(())
    }
}

/// Parallel arrays returned for one side of a rolling option chart.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RollingOptionSeries {
    #[serde(default, deserialize_with = "null_to_default")]
    pub iv: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub oi: Vec<i64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub strike: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub spot: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub open: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub high: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub low: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub close: Vec<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub volume: Vec<i64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub timestamp: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RollingOptionData {
    #[serde(default)]
    pub ce: Option<RollingOptionSeries>,
    #[serde(default)]
    pub pe: Option<RollingOptionSeries>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RollingOptionResponse {
    pub data: RollingOptionData,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TechnicalExchangeSegment {
    #[serde(rename = "NSE_EQ")]
    NseEq,
    #[serde(rename = "IDX_I")]
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TechnicalInstrument {
    Index,
    Equity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TechnicalTimeframe {
    #[serde(rename = "1")]
    OneMinute,
    #[serde(rename = "5")]
    FiveMinutes,
    #[serde(rename = "15")]
    FifteenMinutes,
    #[serde(rename = "D")]
    Daily,
}

string_wire_enum!(TechnicalIndicator {
    Sma5 => "SMA_5",
    Sma10 => "SMA_10",
    Sma20 => "SMA_20",
    Sma50 => "SMA_50",
    Sma100 => "SMA_100",
    Sma200 => "SMA_200",
    Ema5 => "EMA_5",
    Ema10 => "EMA_10",
    Ema20 => "EMA_20",
    Ema50 => "EMA_50",
    Ema100 => "EMA_100",
    Ema200 => "EMA_200",
    Rsi14 => "RSI_14",
    MacdHist => "MACD_HIST",
    Stoch => "STOCH",
    StochRsi14 => "STOCHRSI_14",
    Atr14 => "ATR_14",
    Adx14 => "ADX_14",
    UltimateOscillator => "UO",
    RateOfChange => "ROC",
    WilliamsR => "WILLR",
    PivotClassic => "PIVOT_CLASSIC",
    PivotFibonacci => "PIVOT_FIBONACCI",
    PivotCamarilla => "PIVOT_CAMARILLA",
});

/// Request for point-in-time technical metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalMetricsRequest {
    pub security_id: String,
    pub exchange_segment: TechnicalExchangeSegment,
    pub instrument: TechnicalInstrument,
    pub timeframe: TechnicalTimeframe,
    pub indicators: Vec<TechnicalIndicator>,
}

impl TechnicalMetricsRequest {
    pub(crate) fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.security_id.trim().is_empty() {
            return Err("technical metrics security_id cannot be empty");
        }
        if self.indicators.is_empty()
            || self
                .indicators
                .iter()
                .any(|value| value.as_str().trim().is_empty())
        {
            return Err("technical metrics indicators must contain non-empty values");
        }
        Ok(())
    }
}

/// Technical metrics are keyed by requested indicator and intentionally use a
/// dynamic value because each indicator has a distinct documented shape.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalMetricsResponse {
    pub security_id: String,
    pub timeframe: String,
    #[serde(default)]
    pub data: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketMoverExchangeSegment {
    #[serde(rename = "NSE_FNO")]
    NseFno,
    #[serde(rename = "BSE_FNO")]
    BseFno,
    #[serde(rename = "NSE_COMM")]
    NseComm,
    #[serde(rename = "MCX_COMM")]
    McxComm,
    #[serde(rename = "NSE_EQ")]
    NseEq,
    #[serde(rename = "BSE_EQ")]
    BseEq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketMoverInstrument {
    Optidx,
    Optstk,
    Optfut,
    Futidx,
    Futstk,
    Futcom,
    Equity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketMoverCategory {
    HighestOi,
    OiGainers,
    OiLosers,
    TopVolume,
    PriceGainers,
    PriceLosers,
}

string_wire_enum!(MarketMoverUniverse {
    All => "ALL",
    FnoStocks => "FNO_STOCKS",
    Nifty50 => "NIFTY_50",
    NiftyBank => "NIFTY_BANK",
    Finnifty => "FINNIFTY",
    IndiaVix => "INDIA_VIX",
    NiftyMidcap => "NIFTY_MIDCAP",
    NiftyNext50 => "NIFTY_NEXT_50",
    NiftySmallcap50 => "NIFTY_SMALLCAP_50",
    NiftyMidCap50 => "NIFTY_MID_CAP_50",
    Nifty100 => "NIFTY_100",
    Nifty200 => "NIFTY_200",
    Nifty500 => "NIFTY_500",
    NiftyMidcap100 => "NIFTY_MIDCAP_100",
    NiftyMidcap150 => "NIFTY_MIDCAP_150",
    NiftySmallcap100 => "NIFTY_SMALLCAP_100",
    NiftySmallcap250 => "NIFTY_SMALLCAP_250",
    NiftyMicrocap250 => "NIFTY_MICROCAP_250",
    NiftyAuto => "NIFTY_AUTO",
    NiftyPrivateBank => "NIFTY_PRIVATE_BANK",
    NiftyFmcg => "NIFTY_FMCG",
    NiftyEnergy => "NIFTY_ENERGY",
    NiftyInfra => "NIFTY_INFRA",
    NiftyIt => "NIFTY_IT",
    NiftyMedia => "NIFTY_MEDIA",
    NiftyMetal => "NIFTY_METAL",
    NiftyMnc => "NIFTY_MNC",
    NiftyPharma => "NIFTY_PHARMA",
    NiftyPsuBank => "NIFTY_PSU_BANK",
    NiftyRealty => "NIFTY_REALTY",
    NiftyServiceSector => "NIFTY_SERVICE_SECTOR",
    NiftyConsumption => "NIFTY_CUNSUMPTION",
    GiftNifty => "GIFT_NIFTY",
    Sensex => "SENSEX",
    Bse100 => "BSE_100",
    Bse200 => "BSE_200",
    Bse500 => "BSE_500",
    Bse150Midcap => "BSE_150_MIDCAP",
    Bse250Smallcap => "BSE_250_SMALLCAP",
    Bse250LargeMid => "BSE_250_LARGE_MID",
    Bse400MidSmall => "BSE_400_MID_SMALL",
    BseBankex => "BSE_BANKEX",
    BseAuto => "BSE_AUTO",
    BseCapitalGoods => "BSE_CAPITAL_GOODS",
    BseConsumerDurables => "BSE_CONSUMER_DURABLES",
    BseEnergy => "BSE_ENERGY",
    BseFinance => "BSE_FINANCE",
    BseFmcg => "BSE_FMCG",
    BseHealthcare => "BSE_HEALTHCARE",
    BseIndiaMfg => "BSE_INDIA_MFG",
    BseIndustrials => "BSE_INDUSTRIALS",
    BseIpo => "BSE_IPO",
    BseIt => "BSE_IT",
    BseMetals => "BSE_METALS",
    BseOilAndGas => "BSE_OIL_AND_GAS",
    BsePower => "BSE_POWER",
    BsePsu => "BSE_PSU",
    BseTelecom => "BSE_TELECOM",
});

/// Request for ranked market movers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketMoversRequest {
    pub exchange_segment: MarketMoverExchangeSegment,
    pub instrument: Vec<MarketMoverInstrument>,
    pub category: MarketMoverCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub universe: Option<MarketMoverUniverse>,
    /// Documented range: 1 through 100.
    pub limit: u8,
}

impl MarketMoversRequest {
    pub(crate) fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.instrument.is_empty() {
            return Err("market movers instrument list cannot be empty");
        }
        if !(1..=100).contains(&self.limit) {
            return Err("market movers limit must be between 1 and 100");
        }
        let group = self.instrument[0].group();
        if self.instrument.iter().any(|value| value.group() != group) {
            return Err("market movers instrument values must belong to one instrument group");
        }
        match group {
            MarketMoverInstrumentGroup::Equity if self.universe.is_none() => {
                return Err("market movers equity requests require universe");
            }
            MarketMoverInstrumentGroup::Options | MarketMoverInstrumentGroup::Futures
                if self
                    .expiry
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty()) =>
            {
                return Err("market movers derivative requests require expiry");
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarketMoverInstrumentGroup {
    Options,
    Futures,
    Equity,
}

impl MarketMoverInstrument {
    fn group(self) -> MarketMoverInstrumentGroup {
        match self {
            Self::Optidx | Self::Optstk | Self::Optfut => MarketMoverInstrumentGroup::Options,
            Self::Futidx | Self::Futstk | Self::Futcom => MarketMoverInstrumentGroup::Futures,
            Self::Equity => MarketMoverInstrumentGroup::Equity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketMoverInstrumentDetail {
    #[serde(default)]
    pub security_id: Option<String>,
    #[serde(default)]
    pub exchange_segment: Option<String>,
    #[serde(default)]
    pub trading_symbol: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub instrument: Option<String>,
    #[serde(default)]
    pub expiry: Option<String>,
    #[serde(default)]
    pub strike_price: Option<f64>,
    #[serde(default)]
    pub tick_size: Option<f64>,
    #[serde(default)]
    pub lot_size: Option<i32>,
    #[serde(default)]
    pub ltp: Option<f64>,
    #[serde(default)]
    pub change: Option<f64>,
    #[serde(default)]
    pub change_percent: Option<f64>,
    #[serde(default)]
    pub volume: Option<i64>,
    #[serde(default)]
    pub traded_value: Option<f64>,
    #[serde(default)]
    pub underlying_security_id: Option<String>,
    #[serde(default)]
    pub underlying_ltp: Option<f64>,
    #[serde(default)]
    pub premium_discount: Option<f64>,
    #[serde(default)]
    pub premium_discount_percent: Option<f64>,
    #[serde(default)]
    pub open_interest: Option<i64>,
    #[serde(default)]
    pub open_interest_change: Option<i64>,
    #[serde(default)]
    pub open_interest_change_percent: Option<f64>,
    #[serde(default)]
    pub put_call_ratio: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketMoversResponse {
    pub exchange_segment: String,
    pub category: String,
    #[serde(default)]
    pub data: Vec<MarketMoverInstrumentDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FundamentalExchangeSegment {
    #[serde(rename = "NSE_EQ")]
    NseEq,
    #[serde(rename = "BSE_EQ")]
    BseEq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FundamentalMetricSection {
    Co,
    Ratios,
    Shp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompanyInstrument {
    Equity,
}

/// Request for company overview, ratios, or shareholding data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyInfoRequest {
    pub security_id: String,
    pub exchange_segment: FundamentalExchangeSegment,
    pub instrument: CompanyInstrument,
    pub metrics: Vec<FundamentalMetricSection>,
}

impl CompanyInfoRequest {
    pub(crate) fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.security_id.trim().is_empty() {
            return Err("company info security_id cannot be empty");
        }
        if self.metrics.is_empty() {
            return Err("company info metrics cannot be empty");
        }
        Ok(())
    }
}

/// Company metric sections contain heterogeneous values and are therefore
/// retained as section-keyed JSON objects without losing newly added fields.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyInfoResponse {
    pub security_id: String,
    #[serde(default)]
    pub data: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_option_request_matches_documented_wire_names() {
        let request = RollingOptionRequest {
            exchange_segment: RollingExchangeSegment::NseFno,
            interval: RollingInterval::OneMinute,
            security_id: 13,
            instrument: RollingInstrument::Optidx,
            expiry_flag: RollingExpiryFlag::Month,
            expiry_code: RollingExpiryCode::First,
            strike: "ATM".into(),
            drv_option_type: RollingOptionType::Call,
            required_data: vec![RollingDataField::Open, RollingDataField::Iv],
            from_date: "2026-01-01".into(),
            to_date: "2026-01-31".into(),
        };

        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["exchangeSegment"], "NSE_FNO");
        assert_eq!(value["interval"], "1");
        assert_eq!(value["instrument"], "OPTIDX");
        assert_eq!(value["expiryFlag"], "MONTH");
        assert_eq!(value["drvOptionType"], "CALL");
        assert_eq!(value["requiredData"], serde_json::json!(["open", "iv"]));
    }

    #[test]
    fn rolling_series_tolerates_omitted_and_null_arrays() {
        let response: RollingOptionResponse = serde_json::from_value(serde_json::json!({
            "data": {
                "ce": { "open": [1.0], "iv": null },
                "pe": null
            }
        }))
        .unwrap();

        let ce = response.data.ce.unwrap();
        assert_eq!(ce.open, vec![1.0]);
        assert!(ce.iv.is_empty());
        assert!(ce.close.is_empty());
        assert!(response.data.pe.is_none());
    }

    #[test]
    fn data_requests_serialize_exact_enums() {
        let technical = TechnicalMetricsRequest {
            security_id: "1333".into(),
            exchange_segment: TechnicalExchangeSegment::NseEq,
            instrument: TechnicalInstrument::Equity,
            timeframe: TechnicalTimeframe::Daily,
            indicators: vec![TechnicalIndicator::Rsi14],
        };
        assert_eq!(
            serde_json::to_value(technical).unwrap(),
            serde_json::json!({
                "securityId": "1333",
                "exchangeSegment": "NSE_EQ",
                "instrument": "EQUITY",
                "timeframe": "D",
                "indicators": ["RSI_14"]
            })
        );

        let movers = MarketMoversRequest {
            exchange_segment: MarketMoverExchangeSegment::NseFno,
            instrument: vec![MarketMoverInstrument::Optidx],
            category: MarketMoverCategory::HighestOi,
            expiry: Some("2026-08-27".into()),
            universe: Some(MarketMoverUniverse::Nifty50),
            limit: 20,
        };
        let movers = serde_json::to_value(movers).unwrap();
        assert_eq!(movers["category"], "HIGHEST_OI");
        assert_eq!(movers["instrument"], serde_json::json!(["OPTIDX"]));
        assert_eq!(movers["expiry"], "2026-08-27");
    }

    #[test]
    fn request_validation_rejects_documented_limit_violations() {
        let invalid = MarketMoversRequest {
            exchange_segment: MarketMoverExchangeSegment::NseEq,
            instrument: vec![MarketMoverInstrument::Equity],
            category: MarketMoverCategory::TopVolume,
            expiry: None,
            universe: Some(MarketMoverUniverse::All),
            limit: 0,
        };
        assert!(invalid.validate().is_err());

        let invalid = TechnicalMetricsRequest {
            security_id: "1333".into(),
            exchange_segment: TechnicalExchangeSegment::NseEq,
            instrument: TechnicalInstrument::Equity,
            timeframe: TechnicalTimeframe::Daily,
            indicators: Vec::new(),
        };
        assert!(invalid.validate().is_err());
    }
}
