#![allow(missing_docs)]
//! Instrument-master download types.

use serde::{Deserialize, Serialize};

/// Exchange segment accepted by Dhan's segment-wise instrument CSV endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstrumentSegment {
    #[serde(rename = "IDX_I")]
    Index,
    #[serde(rename = "NSE_EQ")]
    NseEq,
    #[serde(rename = "NSE_FNO")]
    NseFno,
    #[serde(rename = "NSE_CURRENCY")]
    NseCurrency,
    #[serde(rename = "BSE_EQ")]
    BseEq,
    #[serde(rename = "MCX_COMM")]
    McxComm,
    #[serde(rename = "BSE_CURRENCY")]
    BseCurrency,
    #[serde(rename = "BSE_FNO")]
    BseFno,
}

impl InstrumentSegment {
    /// Wire value used as the endpoint path segment.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "IDX_I",
            Self::NseEq => "NSE_EQ",
            Self::NseFno => "NSE_FNO",
            Self::NseCurrency => "NSE_CURRENCY",
            Self::BseEq => "BSE_EQ",
            Self::McxComm => "MCX_COMM",
            Self::BseCurrency => "BSE_CURRENCY",
            Self::BseFno => "BSE_FNO",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_path_values_match_annexure() {
        assert_eq!(InstrumentSegment::Index.as_str(), "IDX_I");
        assert_eq!(InstrumentSegment::NseEq.as_str(), "NSE_EQ");
        assert_eq!(InstrumentSegment::NseFno.as_str(), "NSE_FNO");
        assert_eq!(InstrumentSegment::NseCurrency.as_str(), "NSE_CURRENCY");
        assert_eq!(InstrumentSegment::BseEq.as_str(), "BSE_EQ");
        assert_eq!(InstrumentSegment::McxComm.as_str(), "MCX_COMM");
        assert_eq!(InstrumentSegment::BseCurrency.as_str(), "BSE_CURRENCY");
        assert_eq!(InstrumentSegment::BseFno.as_str(), "BSE_FNO");
    }
}
