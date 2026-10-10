//! Command-line surface. Command names, flag names and environment variable
//! names are public interface: renaming or removing any of them is a
//! breaking change.

use clap::builder::FalseyValueParser;
use clap::{ArgAction, ArgGroup, Parser, Subcommand, ValueEnum};

use crate::network::Network;

#[derive(Debug, Parser)]
#[command(
    name = "sochain",
    version,
    about = "Bitcoin, Litecoin, Dogecoin and Zcash blockchain data from your terminal, via SoChain",
    long_about = "Bitcoin, Litecoin, Dogecoin and Zcash blockchain data from your terminal, via SoChain.\n\n\
        Every lookup names its network first: BTC, LTC, DOGE, ZEC, or a testnet (BTCTEST, \
        LTCTEST, DOGETEST, ZECTEST), in any case.\n\n\
        Output is plain English by default. Pass --json (or set SOCHAIN_JSON=1) for the \
        API's JSON reply, unchanged; errors are then JSON on stderr.\n\n\
        Lookups work without an API key (at least 2,500 a day per IP address). With a key, your \
        plan's limits apply: run `sochain auth login`, or set SOCHAIN_API_KEY.\n\n\
        Exit codes: 0 ok, 1 other error, 2 bad input, 3 not found, 4 SoChain unavailable, \
        5 rate limited, 6 API key missing, invalid or not allowed.",
    after_help = "Examples:\n  \
        sochain balance BTC bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh\n  \
        sochain transaction LTC <txid>\n  \
        sochain block DOGE latest\n  \
        sochain fees BTC\n  \
        sochain transactions BTC <address> --from-time 2025-01-01 --direction in --json\n\n\
        Run `sochain guide` for a guide aimed at scripts and AI agents.\n\n\
        Data comes from SoChain and is subject to its Terms of Service: https://sochain.com/terms"
)]
pub struct Cli {
    /// Print the API's JSON reply instead of text (errors as JSON on stderr)
    #[arg(
        long,
        global = true,
        env = "SOCHAIN_JSON",
        action = ArgAction::SetTrue,
        value_parser = FalseyValueParser::new()
    )]
    pub json: bool,

    /// Give up on a request after this many seconds
    #[arg(
        long,
        global = true,
        env = "SOCHAIN_TIMEOUT",
        default_value_t = 30,
        value_name = "SECS"
    )]
    pub timeout: u64,

    /// Show prices in this currency instead of US dollars (EUR, GBP, JPY, …)
    #[arg(
        long,
        global = true,
        env = "SOCHAIN_CURRENCY",
        value_name = "CODE",
        value_parser = parse_currency
    )]
    pub currency: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Address balance: confirmed and unconfirmed, or the confirmed balance at a past block or time
    #[command(
        after_help = "Examples:\n  sochain balance BTC bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh\n  \
            sochain balance DOGE <address> --time 2025-12-31\n  sochain balance LTC <address> --height 2800000"
    )]
    #[command(group(ArgGroup::new("at").args(["height", "time"])))]
    Balance {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,

        /// The confirmed balance at the end of this block
        #[arg(long, value_name = "N")]
        height: Option<u64>,

        /// The confirmed balance at this time: unix seconds, a UTC date (the
        /// end of that day) or an RFC 3339 timestamp
        #[arg(long, value_name = "TIME")]
        time: Option<String>,
    },

    /// Address summary: balance, totals received, transaction counts, first and last activity
    #[command(visible_alias = "address")]
    AddressSummary {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,
    },

    /// Address transactions: 10 per page, newest first, or filtered by time, height and direction
    #[command(
        visible_alias = "txs",
        after_help = "Without filters, pages are numbered (--page). With any filter, pages are \
            linked by a cursor: pass the printed --cursor to get the next one.\n\n\
            Examples:\n  sochain transactions BTC <address>\n  sochain transactions BTC <address> --page 2\n  \
            sochain transactions LTC <address> --from-time 2025-01-01 --to-time 2025-12-31 --direction in\n  \
            sochain transactions DOGE <address> --from-time 2025-01-01 --csv > history.csv"
    )]
    Transactions {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,

        #[command(flatten)]
        filters: HistoryFilters,
    },

    /// Address transaction counts: sent, received and total
    #[command(visible_alias = "counts")]
    TransactionCounts {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,
    },

    /// Address unspent outputs: spendable outputs, 10 per page, oldest first
    #[command(visible_aliases = ["utxos", "unspent"])]
    UnspentOutputs {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,

        /// Page to show, starting at 1
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=999_999))]
        page: u32,
    },

    /// Is valid address: whether an address is valid on a network
    #[command(visible_alias = "validate")]
    IsValidAddress {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,
    },

    /// Transaction: confirmations, fee, inputs and outputs
    #[command(visible_alias = "tx")]
    Transaction {
        #[arg(ignore_case = true)]
        network: Network,
        /// Transaction id (64 hex characters)
        hash: String,
    },

    /// Transaction status: pending, mined, replaced, dropped, expired or unknown
    #[command(visible_alias = "status")]
    TransactionStatus {
        #[arg(ignore_case = true)]
        network: Network,
        /// Transaction id (64 hex characters)
        hash: String,
    },

    /// Block: header, reward, fees and transaction ids
    #[command(
        after_help = "Examples:\n  sochain block BTC latest\n  sochain block LTC 2800000\n  sochain block DOGE <hash> --json"
    )]
    Block {
        #[arg(ignore_case = true)]
        network: Network,
        /// Block height, block hash, or `latest`
        id: String,
    },

    /// Latest blocks: the 10 newest, with miner and reward
    #[command(visible_alias = "blocks")]
    LatestBlocks {
        #[arg(ignore_case = true)]
        network: Network,
    },

    /// Best block hash: the hash of the newest block
    BestBlockHash {
        #[arg(ignore_case = true)]
        network: Network,
    },

    /// Network info: block count, hashrate, the mempool and projected blocks
    #[command(visible_alias = "network")]
    NetworkInfo {
        #[arg(ignore_case = true)]
        network: Network,
    },

    /// Fee rates for the next blocks, from the mempool
    Fees {
        #[arg(ignore_case = true)]
        network: Network,
    },

    /// Price: the latest, or a UTC day's opening price
    #[command(
        after_help = "Examples:\n  sochain price BTC\n  sochain price LTC --time 2024-01-01\n  sochain price DOGE --currency EUR"
    )]
    Price {
        #[arg(ignore_case = true)]
        network: Network,

        /// That UTC day's opening price: unix seconds or a date (YYYY-MM-DD)
        #[arg(long, value_name = "TIME")]
        time: Option<String>,
    },

    /// Rich list: the top 1,000 holders
    #[command(visible_aliases = ["richlist", "top"])]
    RichList {
        #[arg(ignore_case = true)]
        network: Network,

        /// Page to show, from 1
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=40))]
        page: u32,

        /// Holders per page: 25, 50 or 100
        #[arg(long, default_value_t = 100, value_parser = parse_page_size)]
        page_size: u32,
    },

    /// Address rank among the largest holders, and its share of the supply
    #[command(visible_alias = "rank")]
    AddressRank {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,
    },

    /// Balance history: confirmed balance per day, week or month, with amounts in and out
    #[command(
        after_help = "Examples:\n  sochain balance-history BTC <address> --interval month\n  \
        sochain balance-history DOGE <address> --from 2025-01-01 --to 2025-06-30 --json"
    )]
    BalanceHistory {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,

        /// One point per day, week (Monday to Sunday, UTC) or month
        #[arg(long, value_enum, default_value_t = HistoryInterval::Day)]
        interval: HistoryInterval,

        /// Start: unix seconds, a UTC date or an RFC 3339 timestamp
        #[arg(long, value_name = "TIME")]
        from: Option<String>,

        /// End, in the same forms (a date means its last second)
        #[arg(long, value_name = "TIME")]
        to: Option<String>,

        /// Text output shows only the most recent N points (0 for all).
        /// `--json` always returns every point.
        #[arg(long, value_name = "N", default_value_t = 30)]
        last: usize,
    },

    /// Address poisoning: look-alike addresses that sent dust imitating a real counterparty
    #[command(visible_alias = "poisoning")]
    AddressPoisoning {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,

        /// Rows to show, 1 to 100, newest first
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
    },

    /// Address counterparties: who it paid and was paid by lately
    #[command(visible_alias = "counterparties")]
    AddressCounterparties {
        #[arg(ignore_case = true)]
        network: Network,
        address: String,

        /// Rows to show, 1 to 100, newest first
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
    },

    /// Chart: a series as a table of values
    #[command(
        after_help = "Series: tx_count, tx_per_min, fees_total, fees_median, block_size, block_interval, \
            difficulty, hashrate, supply, price_usd, pool_share, active_addresses, new_addresses, \
            transfers, mempool_txs, mempool_bytes, holder_share, lookalikes; on LTC mweb_pool and \
            mweb_flows; on ZEC zcash_pools. Live reference: https://sochain.com/docs/charts/series\n\n\
            Intervals: block (last 1,000 blocks; --from/--to are heights), hour (last 90 days), \
            day (whole chain), minute (last 60 minutes; tx_per_min, mempool_txs, mempool_bytes).\n\n\
            Examples:\n  sochain chart BTC tx_count\n  sochain chart LTC hashrate --interval hour --last 48\n  \
            sochain chart DOGE tx_per_min --interval minute --json"
    )]
    Chart {
        #[arg(ignore_case = true)]
        network: Network,

        /// Series name, such as tx_count, hashrate or price_usd
        series: String,

        /// Time bucket for each point
        #[arg(long, value_enum, default_value_t = ChartInterval::Day)]
        interval: ChartInterval,

        /// Start: a block height for --interval block, else unix seconds
        #[arg(long, value_name = "N")]
        from: Option<u64>,

        /// End, in the same units
        #[arg(long, value_name = "N")]
        to: Option<u64>,

        /// Text output shows only the most recent N points (0 for all).
        /// `--json` always returns every point.
        #[arg(long, value_name = "N", default_value_t = 30)]
        last: usize,
    },

    /// Account info: your plan and requests used (needs an API key)
    Account,

    /// Log in with an API key, log out, or show which key is used
    #[command(subcommand)]
    Auth(AuthCommand),

    /// Install the SoChain skill for AI agents (Claude Code, Codex)
    #[command(subcommand)]
    Skill(SkillCommand),

    /// Add the SoChain MCP server to AI agents (Codex, Claude Code)
    #[command(subcommand)]
    Mcp(McpCommand),

    /// Print every command with its API endpoint and output shape, as JSON
    Schema,

    /// Print a usage guide for scripts and AI agents
    Guide,
}

#[derive(Debug, clap::Args)]
pub struct HistoryFilters {
    /// Page to show, starting at 1 (not with filters)
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=999_999), conflicts_with_all = ["from_height", "to_height", "from_time", "to_time", "direction", "unconfirmed", "page_size", "cursor", "csv"])]
    pub page: Option<u32>,

    /// Filter: from this block on (inclusive)
    #[arg(long, value_name = "N")]
    pub from_height: Option<u64>,

    /// Filter: up to this block (inclusive)
    #[arg(long, value_name = "N")]
    pub to_height: Option<u64>,

    /// Filter: from this time on: unix seconds, a UTC date or an RFC 3339 timestamp
    #[arg(long, value_name = "TIME")]
    pub from_time: Option<String>,

    /// Filter: up to this time (a date means its last second)
    #[arg(long, value_name = "TIME")]
    pub to_time: Option<String>,

    /// Filter: in (the balance went up), out (it went down) or both
    #[arg(long, value_enum)]
    pub direction: Option<Direction>,

    /// Filter: pending transactions (default include, or exclude when the
    /// range has an end)
    #[arg(long, value_enum)]
    pub unconfirmed: Option<Unconfirmed>,

    /// Rows per page with filters: 1 to 100 (default 10), or up to 500 with --csv
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=500))]
    pub page_size: Option<u32>,

    /// The next page, as printed after the previous one (carries the filters)
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,

    /// Print CSV (time_utc, block, txid, direction, received, sent, net, price, net value)
    #[arg(long)]
    pub csv: bool,
}

impl HistoryFilters {
    /// Whether the filtered (cursor-paged) form of the endpoint is wanted.
    pub fn filtered(&self) -> bool {
        self.from_height.is_some()
            || self.to_height.is_some()
            || self.from_time.is_some()
            || self.to_time.is_some()
            || self.direction.is_some()
            || self.unconfirmed.is_some()
            || self.page_size.is_some()
            || self.cursor.is_some()
            || self.csv
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Direction {
    In,
    Out,
    Both,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Unconfirmed {
    Include,
    Exclude,
    Only,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HistoryInterval {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ChartInterval {
    Block,
    Minute,
    Hour,
    Day,
}

/// The name a value enum has on the command line, which is also its API value.
pub fn value_name<T: ValueEnum>(v: &T) -> String {
    v.to_possible_value()
        .expect("no skipped values")
        .get_name()
        .to_owned()
}

fn parse_currency(s: &str) -> Result<String, String> {
    if s.len() == 3 && s.bytes().all(|b| b.is_ascii_alphabetic()) {
        Ok(s.to_ascii_uppercase())
    } else {
        Err("expected a three-letter currency code, such as EUR".into())
    }
}

fn parse_page_size(s: &str) -> Result<u32, String> {
    match s.parse() {
        Ok(n @ (25 | 50 | 100)) => Ok(n),
        _ => Err("expected 25, 50 or 100".into()),
    }
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Save an API key (read from a hidden prompt, or from stdin)
    #[command(
        after_help = "Get a key at https://sochain.com/signin; it is shown once, on the dashboard.\n\
            The key is checked with SoChain, then saved in your config folder, readable\n\
            only by you. SOCHAIN_API_KEY, when set, takes precedence over it.\n\n\
            Examples:\n  sochain auth login\n  printenv MY_SOCHAIN_KEY | sochain auth login"
    )]
    Login {
        /// Save the key without checking it with SoChain first
        #[arg(long)]
        no_verify: bool,
    },

    /// Remove the saved API key
    Logout,

    /// Show which API key is used, and its plan
    Status,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Download the skill from sochain.com, verify it and install it
    #[command(
        after_help = "Installs into ~/.claude/skills (Claude Code) and ~/.agents/skills (Codex) for\n\
            each agent found. The skill is fetched from sochain.com and checked against\n\
            its published SHA-256 digest before anything is written.\n\n\
            Examples:\n  sochain skill install\n  sochain skill install --project\n  \
            sochain skill install --dir ~/my-agent/skills\n  sochain skill install --print"
    )]
    Install {
        #[command(flatten)]
        location: SkillLocation,

        /// Print the verified SKILL.md instead of installing it
        #[arg(long)]
        print: bool,

        /// Replace copies that were changed since they were installed
        #[arg(long)]
        force: bool,
    },

    /// Show where the skill is installed and whether it is up to date
    Status {
        #[command(flatten)]
        location: SkillLocation,
    },

    /// Remove the installed skill
    Uninstall {
        #[command(flatten)]
        location: SkillLocation,

        /// Also remove copies that were changed since they were installed
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, clap::Args)]
pub struct SkillLocation {
    /// Use the agents' folders in the current project (.claude/skills and
    /// .agents/skills) instead of your home directory
    #[arg(long, conflicts_with = "dir")]
    pub project: bool,

    /// Use this skills folder instead; the skill goes in a folder inside it
    #[arg(long, value_name = "DIR")]
    pub dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Add https://sochain.com/mcp to Codex and Claude Code
    #[command(
        after_help = "Uses each agent's own command (`codex mcp add`, `claude mcp add`); nothing\n\
            runs locally. Without --use-api-key the server is used without a key (at\n\
            least 2,500 lookups a day per IP address). With it, Codex reads your key from\n\
            SOCHAIN_API_KEY when it starts, and so does Claude Code with --project; for\n\
            Claude Code's user scope the key is saved in Claude Code's own config.\n\
            For Cursor and other apps, --print shows what to add.\n\n\
            Examples:\n  sochain mcp install\n  sochain mcp install --use-api-key\n  sochain mcp install --print"
    )]
    Install {
        /// Have the agent send the key in SOCHAIN_API_KEY (set it where the agent runs)
        #[arg(long)]
        use_api_key: bool,

        /// Add it to the current project only (Claude Code's project scope)
        #[arg(long)]
        project: bool,

        /// Show the setup for every supported app instead of changing anything
        #[arg(long)]
        print: bool,

        /// Replace a server named `sochain` that is set up differently
        #[arg(long)]
        force: bool,
    },

    /// Show which agents have the SoChain MCP server
    Status,

    /// Remove the SoChain MCP server from Codex and Claude Code
    Uninstall {
        /// Remove it from the current project only (Claude Code)
        #[arg(long)]
        project: bool,

        /// Also remove a server named `sochain` that points somewhere else
        #[arg(long)]
        force: bool,
    },
}
