//! The parts of API replies the text output reads. --json output never goes
//! through these types, so a field missing here is never lost there.
//!
//! Optional and nullable fields are `Option` or defaulted, so a reply that
//! leaves something out still prints. scripts/check-api-contract.py checks
//! these against the published OpenAPI description.

use serde::Deserialize;

use crate::amount::Amount;

#[derive(Debug, Deserialize)]
pub struct Price {
    pub value: Amount,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
pub struct Height {
    pub height: u64,
    pub time: i64,
}

#[derive(Debug, Deserialize)]
pub struct Balance {
    pub confirmed: Amount,
    #[serde(default)]
    pub unconfirmed: Option<Amount>,
    /// Set for a balance at a past height or time.
    #[serde(default)]
    pub height: Option<u64>,
    #[serde(default)]
    pub time: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AddressSummary {
    pub txs_sent: u64,
    pub txs_received: u64,
    pub txs_total: u64,
    pub confirmed_balance: Amount,
    pub unconfirmed_balance: Amount,
    pub confirmed_received: Amount,
    #[serde(default)]
    pub first_active: Option<Height>,
    #[serde(default)]
    pub last_active: Option<Height>,
}

#[derive(Debug, Deserialize)]
pub struct AddressTransactions {
    pub transactions: Vec<AddressTransaction>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub reorged: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AddressTransaction {
    pub hash: String,
    pub balance_change: Amount,
    pub time: i64,
    pub block: Option<u64>,
    #[serde(default)]
    pub price: Option<Price>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionCounts {
    pub sent: u64,
    pub received: u64,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct UnspentOutputs {
    pub outputs: Vec<UnspentOutput>,
}

#[derive(Debug, Deserialize)]
pub struct UnspentOutput {
    pub hash: String,
    pub index: u32,
    pub value: Amount,
    pub block: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct IsValid {
    pub is_valid: bool,
}

#[derive(Debug, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub confirmations: u64,
    pub size: u64,
    #[serde(default)]
    pub vsize: Option<u64>,
    pub block_height: Option<u64>,
    pub time: i64,
    pub fee: Amount,
    #[serde(default)]
    pub price: Option<Price>,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    /// Zcash: what each shielded pool released into (+) or took from (−)
    /// the transparent side.
    #[serde(default)]
    pub shielded: Option<serde_json::Map<String, serde_json::Value>>,
    /// What kind of transaction it is, such as `coinjoin`; null when not
    /// classified.
    #[serde(default)]
    pub kind: Option<String>,
    /// True when the kind is a guess.
    #[serde(default)]
    pub heuristic: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TxInput {
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub value: Option<Amount>,
    /// Null for a coinbase input, which spends no earlier output.
    #[serde(default)]
    pub previous_output: Option<serde_json::Value>,
    /// Litecoin: `pool` when the input spends the MWEB pool, `pegin` when it
    /// spends a peg-in.
    #[serde(default)]
    pub mweb: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TxOutput {
    #[serde(default)]
    pub address: Option<String>,
    pub value: Amount,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// Litecoin: `pegin`, `pool` or `pegout` for outputs touching MWEB.
    #[serde(default)]
    pub mweb: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionStatus {
    pub status: String,
    pub block_height: Option<u64>,
    pub confirmations: Option<u64>,
    pub replaced_by: Option<String>,
    pub replaced_by_status: Option<String>,
    pub replaced_by_height: Option<u64>,
    pub replaced_ancestor: Option<String>,
    pub removed_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Block {
    pub height: u64,
    pub hash: String,
    pub time: i64,
    /// -1 for a block that is no longer part of the main chain.
    pub confirmations: i64,
    pub num_txs: u64,
    pub size: u64,
    pub previous_block_hash: Option<String>,
    pub next_block_hash: Option<String>,
    #[serde(default)]
    pub is_orphan: bool,
    pub difficulty: String,
    pub reward: Amount,
    pub fees: Amount,
    pub value_out: Amount,
    #[serde(default)]
    pub price: Option<Price>,
    /// Litecoin: the MWEB summary. value_out includes the MWEB pool carried
    /// over from block to block.
    #[serde(default)]
    pub mweb: Option<BlockMweb>,
}

#[derive(Debug, Deserialize)]
pub struct BlockMweb {
    pub pool: Amount,
}

#[derive(Debug, Deserialize)]
pub struct LatestBlocks {
    pub blocks: Vec<LatestBlock>,
}

#[derive(Debug, Deserialize)]
pub struct LatestBlock {
    pub height: u64,
    pub time: i64,
    pub num_txs: u64,
    #[serde(default)]
    pub miner: Option<String>,
    pub reward_and_fees: Amount,
}

#[derive(Debug, Deserialize)]
pub struct BestBlockHash {
    pub hash: String,
}

#[derive(Debug, Deserialize)]
pub struct NetworkInfo {
    pub block_count: u64,
    pub best_block_hash: String,
    pub hashrate: String,
    pub mempool: Mempool,
}

#[derive(Debug, Deserialize)]
pub struct Mempool {
    pub mempool_txs: u64,
    pub mempool_size: u64,
    /// True while SoChain is still reading the mempool in; counts can be low.
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub blocks: Vec<ProjectedBlock>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectedBlock {
    pub block_num: u32,
    pub num_txs: u64,
    #[serde(default)]
    pub min_fee_rate_precise: Option<String>,
    #[serde(default)]
    pub median_fee_rate_precise: Option<String>,
    #[serde(default)]
    pub max_fee_rate_precise: Option<String>,
    pub min_fee_rate: f64,
    pub median_fee_rate: f64,
    pub max_fee_rate: f64,
    /// Zcash (ZIP 317): the fee per logical action, and the actions every
    /// transaction gets for free.
    #[serde(default)]
    pub marginal_fee: Option<u64>,
    #[serde(default)]
    pub grace_actions: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PriceData {
    pub value: Amount,
    pub currency: String,
    #[serde(default)]
    pub as_of: Option<i64>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub fx_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RichList {
    pub height: u64,
    pub supply: Amount,
    pub page: u32,
    pub page_size: u32,
    pub total_rows: u64,
    pub rows: Vec<RichRow>,
}

#[derive(Debug, Deserialize)]
pub struct RichRow {
    pub rank: u64,
    pub address: Option<String>,
    pub balance: Amount,
    pub share_of_supply: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct AddressRank {
    pub address: String,
    pub balance: Amount,
    pub rank: Option<u64>,
    #[serde(default)]
    pub rank_limit: Option<u64>,
    pub share_of_supply: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct BalanceHistory {
    pub interval: String,
    pub points: Vec<BalancePoint>,
}

#[derive(Debug, Deserialize)]
pub struct BalancePoint {
    pub t: i64,
    pub v: Amount,
    pub received: Amount,
    pub sent: Amount,
    #[serde(default)]
    pub tx_in: Option<u64>,
    #[serde(default)]
    pub tx_out: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct AddressPoisoning {
    pub address: String,
    #[serde(default)]
    pub building: bool,
    pub findings: Vec<PoisoningFinding>,
}

#[derive(Debug, Deserialize)]
pub struct PoisoningFinding {
    pub sender: String,
    pub imitates: String,
    #[serde(default)]
    pub amount: Option<Amount>,
    #[serde(default)]
    pub dust_txid: Option<String>,
    #[serde(default)]
    pub receiver_paid: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Counterparties {
    pub address: String,
    #[serde(default)]
    pub building: bool,
    #[serde(default)]
    pub window_blocks: Option<u64>,
    pub counterparties: Vec<Counterparty>,
}

#[derive(Debug, Deserialize)]
pub struct Counterparty {
    pub address: String,
    pub direction: String,
    pub amount: Amount,
    pub txid: String,
    pub height: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct Chart {
    pub series: String,
    pub interval: String,
    pub points: Vec<ChartPoint>,
}

#[derive(Debug, Deserialize)]
pub struct ChartPoint {
    pub t: i64,
    pub v: serde_json::Value,
    #[serde(default)]
    pub height: Option<u64>,
}
