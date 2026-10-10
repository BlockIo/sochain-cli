//! The networks SoChain serves. Codes are case-insensitive on the command line
//! and sent upper-case in API paths.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "UPPER")]
pub enum Network {
    /// Bitcoin
    Btc,
    /// Litecoin
    Ltc,
    /// Dogecoin
    Doge,
    /// Zcash (transparent addresses and transactions)
    Zec,
    /// Bitcoin testnet4
    Btctest,
    /// Litecoin testnet4
    Ltctest,
    /// Dogecoin testnet
    Dogetest,
    /// Zcash testnet
    Zectest,
}

impl Network {
    /// The code used in API paths, which is also the unit amounts are shown in.
    pub fn code(self) -> &'static str {
        match self {
            Network::Btc => "BTC",
            Network::Ltc => "LTC",
            Network::Doge => "DOGE",
            Network::Zec => "ZEC",
            Network::Btctest => "BTCTEST",
            Network::Ltctest => "LTCTEST",
            Network::Dogetest => "DOGETEST",
            Network::Zectest => "ZECTEST",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Network::Btc => "Bitcoin",
            Network::Ltc => "Litecoin",
            Network::Doge => "Dogecoin",
            Network::Zec => "Zcash",
            Network::Btctest => "Bitcoin testnet4",
            Network::Ltctest => "Litecoin testnet4",
            Network::Dogetest => "Dogecoin testnet",
            Network::Zectest => "Zcash testnet",
        }
    }

    pub fn is_testnet(self) -> bool {
        matches!(
            self,
            Network::Btctest | Network::Ltctest | Network::Dogetest | Network::Zectest
        )
    }

    pub fn is_zcash(self) -> bool {
        matches!(self, Network::Zec | Network::Zectest)
    }

    /// The unit of `network_info` fee rates.
    pub fn fee_rate_unit(self) -> &'static str {
        match self {
            Network::Btc | Network::Btctest => "sat/vB",
            Network::Ltc | Network::Ltctest => "lit/vB",
            Network::Doge | Network::Dogetest => "koinu/vB",
            Network::Zec | Network::Zectest => "zat/action",
        }
    }
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
