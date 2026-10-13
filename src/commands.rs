//! Runs each command: one API call (two for `block latest`), then either the
//! API's JSON reply unchanged (`--json`) or plain-English text.
//!
//! House rules for text output: addresses, transaction ids and hashes are
//! always printed in full, never shortened; amounts are in the network's own
//! coin.

use std::io::Write;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::amount::{Amount, group_thousands};
use crate::api::Api;
use crate::auth;
use crate::cli::{
    AuthCommand, Cli, Command, HistoryFilters, McpCommand, SkillCommand, SkillLocation, value_name,
};
use crate::error::{CliError, Result};
use crate::mcp;
use crate::model::*;
use crate::network::Network;
use crate::secret;
use crate::skill::{self, Scope};
use crate::time::{ago, now, parse_unix_or_date, utc, utc_date, utc_minute};

const PAGE_SIZE: usize = 10;

pub fn run(cli: Cli, out: &mut dyn Write) -> Result<()> {
    // Nothing that looks like a private key or recovery phrase is ever sent.
    for input in lookup_inputs(&cli.command) {
        secret::refuse_secrets(input)?;
    }
    let json = cli.json;
    let timeout = Duration::from_secs(cli.timeout);
    let make_api = |key: Option<String>| Api::new(timeout, key, !json);
    // Lookups use the key when there is one; they work without it too.
    let api = || make_api(auth::resolve()?.map(|(key, _)| key));
    let currency = cli.currency.as_deref();
    let cur = || -> Vec<(&str, String)> {
        currency
            .map(|c| vec![("currency", c.to_owned())])
            .unwrap_or_default()
    };
    let lookup = |segments: &[&str], query: &[(&str, String)]| -> Result<Value> {
        api()?.get(segments, query)
    };

    match cli.command {
        Command::Balance {
            network,
            address,
            height,
            time,
        } => {
            let historic = height.is_some() || time.is_some();
            let mut query = vec![];
            if let Some(h) = height {
                query.push(("height", h.to_string()));
            }
            if let Some(t) = time {
                query.push(("time", t));
            }
            let env = lookup(&["balance", network.code(), &address], &query)?;
            emit(out, json, &env, |out, d| {
                show_balance(out, network, &address, historic, d)
            })
        }
        Command::AddressSummary { network, address } => {
            let env = lookup(&["address_summary", network.code(), &address], &[])?;
            emit(out, json, &env, |out, d| {
                show_summary(out, network, &address, d)
            })
        }
        Command::Transactions {
            network,
            address,
            filters,
        } => transactions(&api()?, network, &address, &filters, currency, json, out),
        Command::TransactionCounts { network, address } => {
            let env = lookup(&["transaction_counts", network.code(), &address], &[])?;
            emit(out, json, &env, |out, c: TransactionCounts| {
                writeln!(
                    out,
                    "{}: {} received, {} sent",
                    plural(c.total, "transaction"),
                    group_thousands(c.received.into()),
                    group_thousands(c.sent.into())
                )?;
                Ok(())
            })
        }
        Command::UnspentOutputs {
            network,
            address,
            page,
        } => {
            let env = lookup(
                &[
                    "unspent_outputs",
                    network.code(),
                    &address,
                    &page.to_string(),
                ],
                &[],
            )?;
            emit(out, json, &env, |out, d| {
                show_unspent(out, network, &address, page, d)
            })
        }
        Command::IsValidAddress { network, address } => {
            let env = lookup(&["is_valid_address", network.code(), &address], &[])?;
            emit(out, json, &env, |out, v: IsValid| {
                if v.is_valid {
                    writeln!(out, "Valid {} address.", network.name())?;
                } else {
                    writeln!(out, "Not a valid {} address.", network.name())?;
                }
                Ok(())
            })
        }
        Command::Transaction { network, hash } => {
            let hash = lower_hex(hash);
            let env = lookup(&["transaction", network.code(), &hash], &cur())?;
            emit(out, json, &env, |out, d| show_transaction(out, network, d))
        }
        Command::TransactionStatus { network, hash } => {
            let hash = lower_hex(hash);
            let env = lookup(&["transaction_status", network.code(), &hash], &[])?;
            emit(out, json, &env, |out, d| show_status(out, &hash, d))
        }
        Command::Block { network, id } => {
            let api = api()?;
            let id = if id.eq_ignore_ascii_case("latest") {
                let env = api.get(&["best_block_hash", network.code()], &[])?;
                data::<BestBlockHash>(&env)?.hash
            } else {
                lower_hex(id)
            };
            let env = api.get(&["block", network.code(), &id], &cur())?;
            emit(out, json, &env, |out, d| show_block(out, network, d))
        }
        Command::LatestBlocks { network } => {
            let env = lookup(&["latest_blocks_summary", network.code()], &cur())?;
            emit(out, json, &env, |out, d| {
                show_latest_blocks(out, network, d)
            })
        }
        Command::BestBlockHash { network } => {
            let env = lookup(&["best_block_hash", network.code()], &[])?;
            emit(out, json, &env, |out, b: BestBlockHash| {
                writeln!(out, "{}", b.hash)?;
                Ok(())
            })
        }
        Command::NetworkInfo { network } => {
            let env = lookup(&["network_info", network.code()], &[])?;
            emit(out, json, &env, |out, d| show_network_info(out, network, d))
        }
        Command::Fees { network } => {
            let env = lookup(&["network_info", network.code()], &[])?;
            if json {
                return print_json(
                    out,
                    &json!({ "status": "success", "data": fees_json(network, &env["data"]) }),
                );
            }
            show_fees(out, network, &data(&env)?)
        }
        Command::Price { network, time } => {
            let mut segments = vec!["price".to_owned(), network.code().to_owned()];
            if let Some(t) = &time {
                // SoChain reads only 7 to 10 digits as a time, and answers
                // anything else with the latest price.
                let digits = t.bytes().all(|b| b.is_ascii_digit());
                let ts = parse_unix_or_date(t)
                    .filter(|_| !digits || (7..=10).contains(&t.len()))
                    .ok_or_else(|| {
                        CliError::BadInput(format!(
                            "invalid --time {t:?}: expected unix seconds (7 to 10 digits) or a date (YYYY-MM-DD)"
                        ))
                    })?;
                segments.push(ts.to_string());
            }
            let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
            let env = lookup(&segments, &cur())?;
            emit(out, json, &env, |out, d| {
                show_price(out, network, time.is_some(), d)
            })
        }
        Command::RichList {
            network,
            page,
            page_size,
        } => {
            let pages = 1000 / page_size;
            if page > pages {
                return Err(CliError::BadInput(format!(
                    "--page is at most {pages} with {page_size} holders per page"
                )));
            }
            let env = lookup(
                &["rich_list", network.code()],
                &[
                    ("page", page.to_string()),
                    ("page_size", page_size.to_string()),
                ],
            )?;
            emit(out, json, &env, |out, d| show_rich_list(out, network, d))
        }
        Command::AddressRank { network, address } => {
            let env = lookup(&["address_rank", network.code(), &address], &[])?;
            emit(out, json, &env, |out, d| show_rank(out, network, d))
        }
        Command::BalanceHistory {
            network,
            address,
            interval,
            from,
            to,
            last,
        } => {
            let mut query = vec![("interval", value_name(&interval))];
            if let Some(f) = from {
                query.push(("from", f));
            }
            if let Some(t) = to {
                query.push(("to", t));
            }
            let env = lookup(&["balance_history", network.code(), &address], &query)?;
            emit(out, json, &env, |out, d| {
                show_balance_history(out, network, &address, d, last)
            })
        }
        Command::AddressPoisoning {
            network,
            address,
            limit,
        } => {
            let env = lookup(
                &["address_poisoning", network.code(), &address],
                &[("limit", limit.to_string())],
            )?;
            emit(out, json, &env, |out, d| show_poisoning(out, network, d))
        }
        Command::AddressCounterparties {
            network,
            address,
            limit,
        } => {
            let env = lookup(
                &["address_counterparties", network.code(), &address],
                &[("limit", limit.to_string())],
            )?;
            emit(out, json, &env, |out, d| {
                show_counterparties(out, network, d)
            })
        }
        Command::Chart {
            network,
            series,
            interval,
            from,
            to,
            last,
        } => {
            // Only price_usd takes a currency; other series refuse it.
            let mut query = if series == "price_usd" { cur() } else { vec![] };
            if let Some(f) = from {
                query.push(("from", f.to_string()));
            }
            if let Some(t) = to {
                query.push(("to", t.to_string()));
            }
            let interval = value_name(&interval);
            let env = lookup(&["charts", network.code(), &series, &interval], &query)?;
            emit(out, json, &env, |out, d| show_chart(out, network, d, last))
        }
        Command::Account => {
            let Some((key, _)) = auth::resolve()? else {
                return Err(CliError::Unauthorized(
                    "`sochain account` needs an API key: run `sochain auth login` or set SOCHAIN_API_KEY"
                        .into(),
                ));
            };
            let account = make_api(Some(key))?.get_plain(&["account_info"])?;
            if json {
                return print_json(out, &account);
            }
            show_account(out, &account)
        }
        Command::Auth(cmd) => match cmd {
            AuthCommand::Login { no_verify } => auth::login(make_api, !no_verify, json, out),
            AuthCommand::Logout => auth::logout(json, out),
            AuthCommand::Status => auth::status(auth::resolve()?, make_api, json, out),
        },
        Command::Skill(cmd) => {
            // The skill comes from the public site; no key is sent for it.
            let site = || make_api(None);
            match cmd {
                SkillCommand::Install { print: true, .. } => {
                    let s = skill::fetch(&site()?)?;
                    if json {
                        let text = String::from_utf8_lossy(&s.bytes);
                        print_json(
                            out,
                            &json!({ "status": "success", "data": { "name": s.name, "digest": s.digest, "content": text } }),
                        )
                    } else {
                        Ok(out.write_all(&s.bytes)?)
                    }
                }
                SkillCommand::Install {
                    location, force, ..
                } => skill::install(&site()?, &scope(location), force, json, out),
                SkillCommand::Status { location } => {
                    skill::status(&site()?, &scope(location), json, out)
                }
                SkillCommand::Uninstall { location, force } => {
                    skill::uninstall(&scope(location), force, json, out)
                }
            }
        }
        Command::Mcp(cmd) => match cmd {
            McpCommand::Install {
                project,
                use_api_key,
                print: true,
                ..
            } => mcp::print(project, use_api_key, json, out),
            McpCommand::Install {
                project,
                use_api_key,
                force,
                ..
            } => {
                let auth = if use_api_key {
                    let (key, _) = auth::resolve()?.ok_or_else(|| {
                        CliError::Unauthorized(
                            "--use-api-key needs a key: run `sochain auth login` or set SOCHAIN_API_KEY"
                                .into(),
                        )
                    })?;
                    mcp::Auth::Key(key)
                } else {
                    mcp::Auth::None
                };
                mcp::install(project, auth, force, json, out)
            }
            McpCommand::Status => mcp::status(json, out),
            McpCommand::Uninstall { project, force } => mcp::uninstall(project, force, json, out),
        },
        Command::Schema => print_json(out, &crate::schema::schema()),
        Command::Guide => Ok(out.write_all(crate::GUIDE.as_bytes())?),
    }
}

/// The user-supplied identifiers a command sends to the API.
fn lookup_inputs(command: &Command) -> Vec<&str> {
    match command {
        Command::Balance { address, .. }
        | Command::AddressSummary { address, .. }
        | Command::Transactions { address, .. }
        | Command::TransactionCounts { address, .. }
        | Command::UnspentOutputs { address, .. }
        | Command::IsValidAddress { address, .. }
        | Command::AddressRank { address, .. }
        | Command::BalanceHistory { address, .. }
        | Command::AddressPoisoning { address, .. }
        | Command::AddressCounterparties { address, .. } => vec![address],
        Command::Transaction { hash, .. } | Command::TransactionStatus { hash, .. } => vec![hash],
        Command::Block { id, .. } => vec![id],
        Command::Chart { series, .. } => vec![series],
        _ => vec![],
    }
}

fn scope(location: SkillLocation) -> Scope {
    match (location.project, location.dir) {
        (_, Some(dir)) => Scope::Dir(dir),
        (true, None) => Scope::Project,
        (false, None) => Scope::User,
    }
}

fn data<T: DeserializeOwned>(envelope: &Value) -> Result<T> {
    serde_json::from_value(envelope["data"].clone())
        .map_err(|e| CliError::Other(format!("unexpected response from SoChain: {e}")))
}

/// Prints the envelope unchanged with --json, else reads its data and shows it.
fn emit<T: DeserializeOwned>(
    out: &mut dyn Write,
    json: bool,
    envelope: &Value,
    show: impl FnOnce(&mut dyn Write, T) -> Result<()>,
) -> Result<()> {
    if json {
        return print_json(out, envelope);
    }
    show(out, data(envelope)?)
}

fn print_json(out: &mut dyn Write, v: &Value) -> Result<()> {
    serde_json::to_writer_pretty(&mut *out, v)
        .map_err(|e| CliError::Other(format!("could not write output: {e}")))?;
    writeln!(out)?;
    Ok(())
}

fn show_balance(
    out: &mut dyn Write,
    n: Network,
    address: &str,
    historic: bool,
    b: Balance,
) -> Result<()> {
    if historic {
        match b.height {
            Some(height) => {
                let when = b.time.map(|t| format!(", {}", utc(t))).unwrap_or_default();
                writeln!(
                    out,
                    "{address} held {} {n} at block {}{when}",
                    b.confirmed,
                    group_thousands(height.into())
                )?;
            }
            None => writeln!(
                out,
                "{address} held nothing then: that is before {}'s first block",
                n.name()
            )?,
        }
        return Ok(());
    }
    writeln!(out, "Balance {} {n}", b.confirmed)?;
    if let Some(u) = b.unconfirmed.filter(|u| !u.is_zero()) {
        writeln!(out, "Pending {u} {n}, not yet confirmed")?;
    }
    Ok(())
}

fn show_summary(out: &mut dyn Write, n: Network, address: &str, s: AddressSummary) -> Result<()> {
    writeln!(out, "{address} on {}", n.name())?;
    writeln!(out, "Balance {} {n}", s.confirmed_balance)?;
    if !s.unconfirmed_balance.is_zero() {
        writeln!(
            out,
            "Pending {} {n}, not yet confirmed",
            s.unconfirmed_balance
        )?;
    }
    writeln!(
        out,
        "Received {} {n} in total · {} ({} received, {} sent)",
        s.confirmed_received,
        plural(s.txs_total, "transaction"),
        group_thousands(s.txs_received.into()),
        group_thousands(s.txs_sent.into())
    )?;
    if let (Some(first), Some(last)) = (&s.first_active, &s.last_active) {
        writeln!(
            out,
            "First active {} (block {}) · last active {} (block {})",
            utc_date(first.time),
            group_thousands(first.height.into()),
            utc_date(last.time),
            group_thousands(last.height.into())
        )?;
    }
    if s.txs_total > 0 {
        writeln!(out, "History: sochain transactions {n} {address}")?;
    }
    Ok(())
}

fn transactions(
    api: &Api,
    n: Network,
    address: &str,
    f: &HistoryFilters,
    currency: Option<&str>,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let mut query: Vec<(&str, String)> = currency
        .map(|c| vec![("currency", c.to_owned())])
        .unwrap_or_default();
    // Flags the next page needs again: the cursor carries the filters, not
    // the page size or currency.
    let mut again = String::new();
    if let Some(size) = f.page_size {
        again += &format!(" --page-size {size}");
    }
    if let Some(c) = currency {
        again += &format!(" --currency {c}");
    }
    if !f.filtered() {
        let page = f.page.unwrap_or(1);
        let env = api.get(
            &["transactions", n.code(), address, &page.to_string()],
            &query,
        )?;
        return emit(out, json, &env, |out, d: AddressTransactions| {
            show_history(out, n, &d)?;
            if d.transactions.is_empty() {
                writeln!(out, "No transactions on page {page}.")?;
            } else if d.transactions.len() >= PAGE_SIZE {
                writeln!(
                    out,
                    "More: sochain transactions {n} {address} --page {}{again}",
                    page + 1
                )?;
            }
            Ok(())
        });
    }
    let optional = [
        ("from_height", f.from_height.map(|v| v.to_string())),
        ("to_height", f.to_height.map(|v| v.to_string())),
        ("from_time", f.from_time.clone()),
        ("to_time", f.to_time.clone()),
        ("direction", f.direction.as_ref().map(value_name)),
        ("unconfirmed", f.unconfirmed.as_ref().map(value_name)),
        ("page_size", f.page_size.map(|v| v.to_string())),
        ("cursor", f.cursor.clone()),
    ];
    query.extend(optional.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))));
    let segments = ["transactions", n.code(), address];
    if f.csv {
        query.push(("format", "csv".into()));
        let raw = api.get_csv(&segments, &query)?;
        out.write_all(&raw.body)?;
        let next = raw
            .headers
            .get("x-next-cursor")
            .and_then(|v| v.to_str().ok())
            .filter(|c| !c.is_empty());
        if let Some(cursor) = next {
            // stderr, so the CSV on stdout stays clean.
            eprintln!(
                "sochain: more rows: sochain transactions {n} {address} --csv --cursor {}{again}",
                crate::sanitize::clean(cursor)
            );
        }
        return Ok(());
    }
    if f.page_size.is_some_and(|s| s > 100) {
        return Err(CliError::BadInput(
            "--page-size is at most 100, or 500 with --csv".into(),
        ));
    }
    let env = api.get(&segments, &query)?;
    emit(out, json, &env, |out, d: AddressTransactions| {
        if d.reorged == Some(true) {
            writeln!(
                out,
                "Note: the chain reorganized under this cursor; rows may repeat."
            )?;
        }
        show_history(out, n, &d)?;
        if d.transactions.is_empty() && d.next_cursor.is_none() {
            writeln!(out, "No matching transactions.")?;
        }
        if let Some(cursor) = &d.next_cursor {
            writeln!(
                out,
                "More: sochain transactions {n} {address} --cursor {cursor}{again}"
            )?;
        }
        Ok(())
    })
}

fn show_history(out: &mut dyn Write, n: Network, d: &AddressTransactions) -> Result<()> {
    let t = now();
    for h in &d.transactions {
        let when = match h.block {
            None => "in the mempool".to_owned(),
            Some(_) => ago(h.time, t),
        };
        let sign = if h.balance_change.is_negative() {
            "-"
        } else {
            "+"
        };
        let worth = h
            .price
            .as_ref()
            .and_then(|p| worth(h.balance_change.abs(), p))
            .map(|w| format!(" ({w})"))
            .unwrap_or_default();
        writeln!(
            out,
            "  {sign}{} {n}{worth}  {when}  {}",
            h.balance_change.abs(),
            h.hash
        )?;
    }
    Ok(())
}

fn show_unspent(
    out: &mut dyn Write,
    n: Network,
    address: &str,
    page: u32,
    u: UnspentOutputs,
) -> Result<()> {
    if u.outputs.is_empty() {
        writeln!(out, "No unspent outputs on page {page}.")?;
        return Ok(());
    }
    for o in &u.outputs {
        let block = o
            .block
            .map(|b| format!("block {}", group_thousands(b.into())))
            .unwrap_or_else(|| "unconfirmed".into());
        writeln!(out, "  {} {n}  {}:{}  {block}", o.value, o.hash, o.index)?;
    }
    if u.outputs.len() >= PAGE_SIZE {
        writeln!(
            out,
            "More: sochain unspent-outputs {n} {address} --page {}",
            page + 1
        )?;
    }
    Ok(())
}

fn show_transaction(out: &mut dyn Write, n: Network, tx: Transaction) -> Result<()> {
    writeln!(out, "Transaction {} on {}", tx.hash, n.name())?;
    let status = match tx.block_height {
        Some(h) if tx.confirmations > 0 => format!(
            "{} in block {}",
            plural(tx.confirmations, "confirmation"),
            group_thousands(h.into())
        ),
        _ => "waiting in the mempool (unconfirmed)".into(),
    };
    writeln!(out, "Status {status} · {}", utc(tx.time))?;
    if let Some(kind) = &tx.kind {
        let guess = if tx.heuristic == Some(true) {
            " (likely)"
        } else {
            ""
        };
        writeln!(out, "Kind {}{guess}", kind_name(kind))?;
    }
    writeln!(out, "From:")?;
    if tx.inputs.is_empty() {
        writeln!(out, "  (no transparent inputs)")?;
    }
    for i in &tx.inputs {
        let who = match (i.address.as_deref(), i.mweb.as_deref(), &i.previous_output) {
            (Some(a), _, _) => a,
            (None, Some("pegin"), _) => "MWEB peg-in",
            (None, Some(_), _) => "MWEB pool",
            (None, None, None) => "newly mined coins",
            // Pay-to-public-key, bare multisig and other scripts without one.
            (None, None, Some(_)) => "(no address)",
        };
        match i.value {
            Some(v) => writeln!(out, "  {who}  {v} {n}")?,
            None => writeln!(out, "  {who}")?,
        }
    }
    writeln!(out, "To:")?;
    if tx.outputs.is_empty() {
        writeln!(out, "  (no transparent outputs)")?;
    }
    for o in &tx.outputs {
        let who = match (o.address.as_deref(), o.kind.as_deref(), o.mweb.as_deref()) {
            (_, Some("nulldata"), _) | (Some("op_return"), _, _) => "data (OP_RETURN)",
            (Some(a), _, _) => a,
            (None, _, Some("pegin")) => "into MWEB (peg-in)",
            (None, _, Some(_)) => "MWEB pool",
            (None, _, None) => "(no address)",
        };
        writeln!(out, "  {who}  {} {n}", o.value)?;
    }
    if let Some(pools) = &tx.shielded {
        let moved: Vec<String> = pools
            .iter()
            .filter(|(k, _)| k.as_str() != "net")
            .filter_map(|(k, v)| {
                let a = v.as_str().and_then(Amount::parse)?;
                (!a.is_zero()).then(|| format!("{k} {}{a}", if a.is_negative() { "" } else { "+" }))
            })
            .collect();
        if !moved.is_empty() {
            writeln!(
                out,
                "Shielded pools into the transparent side: {} {n}",
                moved.join(", ")
            )?;
        }
    }
    let fee_worth = tx
        .price
        .as_ref()
        .filter(|_| !tx.fee.is_zero())
        .and_then(|p| worth(tx.fee, p))
        .map(|w| format!(" ({w})"))
        .unwrap_or_default();
    let vsize = tx
        .vsize
        .filter(|&v| v != tx.size)
        .map(|v| format!(", {} vbytes", group_thousands(v.into())))
        .unwrap_or_default();
    writeln!(
        out,
        "Fee {} {n}{fee_worth} · {} bytes{vsize}",
        tx.fee,
        group_thousands(tx.size.into())
    )?;
    if let Some(p) = tx.price.as_ref().filter(|p| !p.value.is_zero()) {
        writeln!(out, "Price then 1 {n} = {}", format_price(p))?;
    }
    Ok(())
}

fn show_status(out: &mut dyn Write, hash: &str, s: TransactionStatus) -> Result<()> {
    let line = match s.status.as_str() {
        "pending" => "Pending: waiting in the mempool to be mined.".to_owned(),
        "mined" => match s.block_height {
            Some(h) => format!(
                "Mined in block {} · {}.",
                group_thousands(h.into()),
                plural(s.confirmations.unwrap_or(0), "confirmation")
            ),
            None => "Mined.".to_owned(),
        },
        "replaced" => {
            let mut line = match &s.replaced_by {
                Some(by) => {
                    let state = match (s.replaced_by_status.as_deref(), s.replaced_by_height) {
                        (Some("mined"), Some(h)) => {
                            format!(", mined in block {}", group_thousands(h.into()))
                        }
                        (Some("mined"), None) => ", mined".into(),
                        (Some("pending"), _) => ", waiting in the mempool".into(),
                        (Some("unknown"), _) => ", which also left the mempool unconfirmed".into(),
                        _ => String::new(),
                    };
                    format!(
                        "Replaced: another transaction spends one of its inputs (a fee bump or double spend): {by}{state}."
                    )
                }
                None => "Replaced: another transaction spends one of its inputs.".to_owned(),
            };
            if let Some(ancestor) = &s.replaced_ancestor {
                line += &format!(" It was replaced because a transaction it spends from was: {ancestor}.");
            }
            line
        }
        "dropped" => "Dropped: it left the mempool without being mined.".to_owned(),
        "expired" => "Expired: it passed its expiry height without being mined.".to_owned(),
        "unknown" => "Unknown: SoChain never saw this transaction, or it left the mempool more than about a day ago.".to_owned(),
        other => format!("Status: {other}."),
    };
    writeln!(out, "{line}")?;
    if let Some(at) = s.removed_at {
        writeln!(out, "Left the mempool {}.", utc(at))?;
    }
    writeln!(out, "Transaction id {hash}")?;
    Ok(())
}

fn show_block(out: &mut dyn Write, n: Network, b: Block) -> Result<()> {
    let t = now();
    writeln!(
        out,
        "{} block {} · {} · {} bytes",
        n.name(),
        group_thousands(b.height.into()),
        plural(b.num_txs, "transaction"),
        group_thousands(b.size.into())
    )?;
    let orphaned = b.is_orphan || b.confirmations < 0;
    if orphaned {
        writeln!(out, "Orphaned: this block is not part of the main chain.")?;
    }
    writeln!(out, "Mined {} ({})", utc(b.time), ago(b.time, t))?;
    // On Litecoin value_out includes the MWEB pool carried over from the
    // previous block, which did not move.
    let moved = match &b.mweb {
        Some(m) => Amount(b.value_out.0 - m.pool.0),
        None => b.value_out,
    };
    writeln!(
        out,
        "Reward {} {n} plus {} {n} in fees · {} {n} moved",
        b.reward,
        b.fees,
        moved.display(2)
    )?;
    if let Some(m) = &b.mweb {
        writeln!(out, "MWEB pool {} {n}", m.pool.display(2))?;
    }
    if let Some(p) = b.price.as_ref().filter(|p| !p.value.is_zero()) {
        writeln!(out, "Price then 1 {n} = {}", format_price(p))?;
    }
    if !orphaned {
        writeln!(
            out,
            "Confirmations {}",
            group_thousands(b.confirmations.into())
        )?;
    }
    writeln!(out, "Difficulty {}", format_difficulty(&b.difficulty))?;
    writeln!(out, "Hash {}", b.hash)?;
    if let Some(prev) = &b.previous_block_hash {
        writeln!(out, "Previous block {prev}")?;
    }
    match &b.next_block_hash {
        Some(next) => writeln!(out, "Next block {next}")?,
        None => writeln!(out, "Next block not mined yet")?,
    }
    Ok(())
}

fn show_latest_blocks(out: &mut dyn Write, n: Network, r: LatestBlocks) -> Result<()> {
    let t = now();
    for b in &r.blocks {
        let miner = b
            .miner
            .as_deref()
            .map(|m| format!(" · paid to {m}"))
            .unwrap_or_default();
        writeln!(
            out,
            "Block {} · {} · {} · {} {n} reward and fees{miner}",
            group_thousands(b.height.into()),
            ago(b.time, t),
            plural(b.num_txs, "transaction"),
            b.reward_and_fees.display(4)
        )?;
    }
    Ok(())
}

fn show_network_info(out: &mut dyn Write, n: Network, i: NetworkInfo) -> Result<()> {
    writeln!(
        out,
        "{} · latest block {}",
        n.name(),
        group_thousands(i.block_count.into())
    )?;
    writeln!(out, "Hash {}", i.best_block_hash)?;
    writeln!(
        out,
        "Mempool {} waiting, {} bytes",
        plural(i.mempool.mempool_txs, "transaction"),
        group_thousands(i.mempool.mempool_size.into())
    )?;
    if i.mempool.loading {
        writeln!(
            out,
            "Note: SoChain is still reading the mempool in, so these counts may be low for now."
        )?;
    }
    if let Some(next) = i.mempool.blocks.first() {
        writeln!(
            out,
            "Next block about {}, median fee rate {} {}",
            plural(next.num_txs, "transaction"),
            rate(
                next.median_fee_rate_precise.as_deref(),
                next.median_fee_rate
            ),
            n.fee_rate_unit()
        )?;
    }
    writeln!(out, "Hashrate {}", format_hashrate(&i.hashrate))?;
    Ok(())
}

/// `fees --json`: the fee part of network_info, in the same envelope.
fn fees_json(n: Network, d: &Value) -> Value {
    let blocks = d["mempool"]["blocks"].clone();
    // The next block's median rounded up to a whole unit, as SoChain's docs
    // recommend: the precise median can be below what nodes relay.
    let suggested = blocks[0]["median_fee_rate"].clone();
    json!({
        "network": n.code(),
        "unit": n.fee_rate_unit(),
        "suggested_fee_rate": suggested,
        "mempool_txs": d["mempool"]["mempool_txs"],
        "mempool_size": d["mempool"]["mempool_size"],
        "blocks": blocks,
    })
}

fn show_fees(out: &mut dyn Write, n: Network, i: &NetworkInfo) -> Result<()> {
    let unit = n.fee_rate_unit();
    let blocks = &i.mempool.blocks;
    if let Some(next) = blocks.first() {
        if let (Some(fee), Some(grace)) = (next.marginal_fee, next.grace_actions) {
            writeln!(
                out,
                "Zcash fees (ZIP 317): {} zatoshis per logical action, with {grace} actions minimum.",
                group_thousands(fee.into())
            )?;
        } else {
            writeln!(
                out,
                "Suggested fee rate {} {unit} (the next block's median, rounded up).",
                format_number(next.median_fee_rate)
            )?;
        }
    }
    writeln!(
        out,
        "{} waiting in the mempool ({} bytes).",
        plural(i.mempool.mempool_txs, "transaction"),
        group_thousands(i.mempool.mempool_size.into())
    )?;
    if i.mempool.loading {
        writeln!(
            out,
            "Note: SoChain is still reading the mempool in, so these counts may be low for now."
        )?;
    }
    if blocks.is_empty() {
        return Ok(());
    }
    writeln!(out, "Projected blocks, fee rates in {unit}:")?;
    for b in blocks.iter().filter(|b| b.num_txs > 0) {
        let label = if b.block_num == 1 {
            "Next block".to_owned()
        } else {
            format!("Block {}", b.block_num)
        };
        writeln!(
            out,
            "  {label:<10}  min {:>8}  median {:>8}  max {:>8}  {}",
            rate(b.min_fee_rate_precise.as_deref(), b.min_fee_rate),
            rate(b.median_fee_rate_precise.as_deref(), b.median_fee_rate),
            rate(b.max_fee_rate_precise.as_deref(), b.max_fee_rate),
            plural(b.num_txs, "transaction")
        )?;
    }
    Ok(())
}

/// A fee rate: the API's precise decimal string when it sends one.
fn rate(precise: Option<&str>, rounded: f64) -> String {
    precise
        .map(str::to_owned)
        .unwrap_or_else(|| format_number(rounded))
}

fn show_price(out: &mut dyn Write, n: Network, historic: bool, p: PriceData) -> Result<()> {
    if p.value.is_zero() && n.is_testnet() {
        writeln!(out, "Testnet coins have no price.")?;
        return Ok(());
    }
    let price = format_price(&Price {
        value: p.value,
        currency: p.currency.clone(),
    });
    let when = match (historic, p.as_of) {
        (true, _) => " (that UTC day's opening price)".to_owned(),
        (false, Some(t)) => format!(" as of {}", utc(t)),
        _ => String::new(),
    };
    writeln!(out, "1 {n} = {price}{when}")?;
    Ok(())
}

fn show_rich_list(out: &mut dyn Write, n: Network, r: RichList) -> Result<()> {
    let pages = r.total_rows.div_ceil(r.page_size.max(1).into()).max(1);
    writeln!(
        out,
        "Largest {} holders at block {}, page {} of {pages} · supply {} {n}",
        n.name(),
        group_thousands(r.height.into()),
        r.page,
        r.supply.display(0)
    )?;
    if n.is_zcash() {
        writeln!(out, "(transparent addresses only)")?;
    }
    for row in &r.rows {
        let who = row.address.as_deref().unwrap_or("(non-standard script)");
        let share = row
            .share_of_supply
            .map(|s| format!("  {:.4}%", s * 100.0))
            .unwrap_or_default();
        writeln!(
            out,
            "{:>5}  {who}  {} {n}{share}",
            row.rank,
            row.balance.display(2)
        )?;
    }
    Ok(())
}

fn show_rank(out: &mut dyn Write, n: Network, r: AddressRank) -> Result<()> {
    writeln!(out, "{} holds {} {n}", r.address, r.balance)?;
    match (r.rank, r.rank_limit) {
        (Some(rank), _) => writeln!(
            out,
            "Rank {} among {} holders",
            group_thousands(rank.into()),
            n.name()
        )?,
        (None, Some(limit)) => writeln!(
            out,
            "Not in the top {} holders",
            group_thousands(limit.into())
        )?,
        (None, None) => writeln!(out, "Not ranked")?,
    }
    if let Some(share) = r.share_of_supply {
        writeln!(out, "Share of supply {}", format_share(share))?;
    }
    Ok(())
}

fn show_balance_history(
    out: &mut dyn Write,
    n: Network,
    address: &str,
    h: BalanceHistory,
    last: usize,
) -> Result<()> {
    let total = h.points.len();
    let shown = if last == 0 { total } else { last.min(total) };
    let points = &h.points[total - shown..];
    let range = if shown < total {
        format!(
            ", last {} of {} points (--last 0 for all)",
            group_thousands(shown as i128),
            group_thousands(total as i128)
        )
    } else {
        String::new()
    };
    writeln!(
        out,
        "Balance of {address} at the end of each {}{range}",
        h.interval
    )?;
    if points.is_empty() {
        writeln!(out, "No activity in this range.")?;
    }
    for p in points {
        let txs = |c: Option<u64>| {
            c.map(|c| format!(" ({})", plural(c, "transaction")))
                .unwrap_or_default()
        };
        let mut moves = vec![];
        if !p.received.is_zero() {
            moves.push(format!("received {}{}", p.received, txs(p.tx_in)));
        }
        if !p.sent.is_zero() {
            moves.push(format!("sent {}{}", p.sent, txs(p.tx_out)));
        }
        let moves = if moves.is_empty() {
            String::new()
        } else {
            format!("  {}", moves.join(", "))
        };
        writeln!(
            out,
            "  {}  {} {n}{moves}",
            period_label(p.t, &h.interval),
            p.v
        )?;
    }
    Ok(())
}

fn show_poisoning(out: &mut dyn Write, n: Network, p: AddressPoisoning) -> Result<()> {
    if p.building {
        writeln!(
            out,
            "Note: this check is still catching up on {}'s history; results may be incomplete.",
            n.name()
        )?;
    }
    if p.findings.is_empty() {
        writeln!(out, "No address poisoning found for {}.", p.address)?;
        return Ok(());
    }
    writeln!(
        out,
        "WARNING: {} sent {} dust imitating addresses it really dealt with. \
         Never copy an address from transaction history.",
        plural(p.findings.len() as u64, "look-alike address"),
        p.address
    )?;
    for f in &p.findings {
        let amount = f
            .amount
            .map(|a| format!(" sent {a} {n}"))
            .unwrap_or_default();
        let txid = f
            .dust_txid
            .as_deref()
            .map(|t| format!(" · {t}"))
            .unwrap_or_default();
        let paid = if f.receiver_paid == Some(true) {
            "  (it was paid: check that payment)"
        } else {
            ""
        };
        writeln!(
            out,
            "  {}{amount}, imitating {}{txid}{paid}",
            f.sender, f.imitates
        )?;
    }
    Ok(())
}

fn show_counterparties(out: &mut dyn Write, n: Network, c: Counterparties) -> Result<()> {
    if c.building {
        writeln!(
            out,
            "Note: still catching up on {}'s history; results may be incomplete.",
            n.name()
        )?;
    }
    let window = c
        .window_blocks
        .map(|b| format!(" in the last {}", plural(b, "block")))
        .unwrap_or_default();
    if c.counterparties.is_empty() {
        writeln!(
            out,
            "No significant counterparties for {}{window}.",
            c.address
        )?;
        return Ok(());
    }
    writeln!(
        out,
        "Counterparties of {}{window}, newest first:",
        c.address
    )?;
    for p in &c.counterparties {
        let (verb, sign) = match p.direction.as_str() {
            "received" => ("from", "+"),
            "sent" => ("to", "-"),
            _ => ("with", ""),
        };
        let at = p
            .height
            .map(|h| format!("  block {}", group_thousands(h.into())))
            .unwrap_or_default();
        writeln!(
            out,
            "  {sign}{} {n} {verb} {}{at}  {}",
            p.amount, p.address, p.txid
        )?;
    }
    Ok(())
}

fn show_chart(out: &mut dyn Write, n: Network, r: Chart, last: usize) -> Result<()> {
    let total = r.points.len();
    let shown = if last == 0 { total } else { last.min(total) };
    let points = &r.points[total - shown..];
    if shown < total {
        writeln!(
            out,
            "{} {} by {}, last {} of {} points (--last 0 for all)",
            n,
            r.series,
            r.interval,
            group_thousands(shown as i128),
            group_thousands(total as i128)
        )?;
    } else {
        writeln!(out, "{} {} by {}", n, r.series, r.interval)?;
    }
    let rows: Vec<(String, String)> = points
        .iter()
        .map(|p| {
            (
                point_label(p, &r.interval),
                format_chart_value(&r.series, &p.v),
            )
        })
        .collect();
    let width = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    for (label, v) in rows {
        writeln!(out, "  {label:<width$}  {v}")?;
    }
    Ok(())
}

/// A transaction kind as words, written as sochain.com writes it:
/// `coinjoin` is `CoinJoin`, `lightning_close` is `Lightning close`.
fn kind_name(kind: &str) -> String {
    match kind {
        "mweb_peg_in" => return "MWEB peg-in".into(),
        "fee_bumped" => return "fee-bumped".into(),
        _ => {}
    }
    kind.split('_')
        .map(|w| match w {
            "coinjoin" => "CoinJoin",
            "lightning" => "Lightning",
            "mweb" => "MWEB",
            w => w,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `3 days 4 h`, `5 h 20 min`, `12 min`.
fn duration(s: u64) -> String {
    let (d, h, m) = (s / 86_400, s % 86_400 / 3_600, s % 3_600 / 60);
    match (d, h) {
        (0, 0) => format!("{} min", m.max(1)),
        (0, _) => format!("{h} h {m} min"),
        (1, _) => format!("1 day {h} h"),
        _ => format!("{d} days {h} h"),
    }
}

/// A balance-history period from its start: `2026-08-03`, `week of
/// 2026-08-03`, `2026-08`. The last point is the balance now, at a time
/// that starts no period, so it is shown as its date.
fn period_label(t: i64, interval: &str) -> String {
    let date = utc_date(t);
    let day = t.div_euclid(86_400);
    let midnight = t.rem_euclid(86_400) == 0;
    match interval {
        // 1970-01-01 was a Thursday, so Mondays are 4 days on.
        "week" if midnight && (day - 4).rem_euclid(7) == 0 => format!("week of {date}"),
        "month" if midnight && date.ends_with("-01") => date[..7].to_owned(),
        _ => date,
    }
}

/// A transaction id or block hash in the lower case the API expects.
fn lower_hex(s: String) -> String {
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        s.to_ascii_lowercase()
    } else {
        s
    }
}

fn point_label(p: &ChartPoint, interval: &str) -> String {
    match (interval, p.height) {
        ("block", Some(h)) => format!("block {}", group_thousands(h.into())),
        ("minute" | "hour", _) => utc_minute(p.t),
        _ => utc_date(p.t),
    }
}

fn show_account(out: &mut dyn Write, a: &Value) -> Result<()> {
    if let Some(plan) = a["plan"]["name"].as_str() {
        writeln!(out, "Plan {plan}")?;
    }
    let used = &a["num_requests_used"];
    if let (Some(today), Some(yesterday)) = (used["today"].as_u64(), used["yesterday"].as_u64()) {
        writeln!(
            out,
            "Requests today {} · yesterday {}",
            group_thousands(today.into()),
            group_thousands(yesterday.into())
        )?;
    }
    if let Some(month) = used["this_month"].as_u64() {
        let quota = a["plan"]["max_monthly_requests"]
            .as_u64()
            .map(|q| format!(" of {}", group_thousands(q.into())))
            .unwrap_or_default();
        let resets = a["quota_resets_in_seconds"]
            .as_u64()
            .map(|s| format!(" · resets in {}", duration(s)))
            .unwrap_or_default();
        writeln!(
            out,
            "Requests this month {}{quota}{resets}",
            group_thousands(month.into())
        )?;
    }
    if let Some(ends) = a["plan"]["current_period_ends_at"].as_str() {
        let renews = if a["plan"]["will_renew_at_period_end"] == true {
            "renews"
        } else {
            "ends"
        };
        writeln!(out, "Current period {renews} {ends}")?;
    }
    if let Some(name) = a["key"]["name"].as_str() {
        let access = match a["key"]["access"].as_str() {
            Some("read_only") => " (read-only)",
            Some("full") => " (full access)",
            _ => "",
        };
        writeln!(out, "Key \"{name}\"{access}")?;
    }
    Ok(())
}

/// Chart values are numbers for most series and objects for breakdowns
/// (pool_share, holder_share, transfers, mweb_flows, zcash_pools). Amounts
/// arrive as decimal strings; holder_share is fractions of the supply.
fn format_chart_value(series: &str, v: &Value) -> String {
    match v {
        Value::Null => "no data".into(),
        Value::Number(n) => n
            .as_f64()
            .map(format_number)
            .unwrap_or_else(|| n.to_string()),
        Value::String(s) => Amount::parse(s)
            .map(|a| a.display(8))
            .unwrap_or_else(|| s.clone()),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| match (series, v) {
                ("holder_share", Value::Number(n)) => {
                    format!("{k} {:.2}%", n.as_f64().unwrap_or(0.0) * 100.0)
                }
                (_, Value::Object(o)) if o.contains_key("count") => {
                    let count = o["count"].as_f64().map(format_number).unwrap_or_default();
                    match o
                        .get("sent")
                        .and_then(Value::as_str)
                        .and_then(Amount::parse)
                    {
                        Some(sent) => format!("{k} {count} ({})", sent.display(0)),
                        None => format!("{k} {count}"),
                    }
                }
                _ => format!("{k} {}", format_chart_value(series, v)),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

/// What `amount` coins were worth at `price`, if the price is known.
fn worth(amount: Amount, price: &Price) -> Option<String> {
    if price.value.is_zero() {
        return None;
    }
    Some(format_money(
        amount.to_f64() * price.value.to_f64(),
        &price.currency,
    ))
}

/// A price per coin: `$84,455.38`, `€0.0939` style precision for small prices.
fn format_price(p: &Price) -> String {
    let v = p.value.to_f64();
    let text = if v >= 1.0 {
        format_money_digits(v, 2)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    with_currency(&text, &p.currency)
}

/// A money amount: `$0.52`, `$1,234.56`, `$33,100`; `<$0.01` for dust.
fn format_money(v: f64, currency: &str) -> String {
    if v > 0.0 && v < 0.01 {
        return format!("<{}", with_currency("0.01", currency));
    }
    let text = if v < 10_000.0 {
        format_money_digits(v, 2)
    } else {
        group_thousands(v.round() as i128)
    };
    with_currency(&text, currency)
}

fn format_money_digits(v: f64, digits: u32) -> String {
    let scale = 10f64.powi(digits as i32);
    let cents = (v * scale).round() as i128;
    let unit = scale as i128;
    format!(
        "{}.{:0width$}",
        group_thousands(cents / unit),
        (cents % unit).abs(),
        width = digits as usize
    )
}

fn with_currency(text: &str, currency: &str) -> String {
    match currency {
        "USD" => format!("${text}"),
        other => format!("{text} {other}"),
    }
}

/// A fraction of the supply as a percentage, with enough digits to show
/// small holdings: `1.2371%`, `0.00002%`.
fn format_share(f: f64) -> String {
    let pct = f * 100.0;
    if pct == 0.0 || pct >= 0.01 {
        return format!("{pct:.4}%");
    }
    let decimals = (2 - pct.log10().floor() as i32).clamp(4, 12) as usize;
    let s = format!("{pct:.decimals$}");
    format!("{}%", s.trim_end_matches('0').trim_end_matches('.'))
}

/// Hashes per second with an SI prefix: `925.64 EH/s`. Falls back to the raw
/// text if it is not a number.
fn format_hashrate(raw: &str) -> String {
    let Ok(h) = raw.trim().parse::<f64>() else {
        return raw.to_owned();
    };
    const UNITS: [&str; 8] = [
        "H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s", "ZH/s",
    ];
    let mut value = h;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}

/// `47,952,738.33`. Falls back to the raw text if it is not a number.
fn format_difficulty(raw: &str) -> String {
    match raw.trim().parse::<f64>() {
        Ok(d) if d.is_finite() => format_money_digits(d, 2),
        _ => raw.to_owned(),
    }
}

fn plural(n: u64, noun: &str) -> String {
    format!(
        "{} {noun}{}",
        group_thousands(n.into()),
        match (n, noun.ends_with('s')) {
            (1, _) => "",
            (_, true) => "es",
            (_, false) => "s",
        }
    )
}

fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e18 {
        group_thousands(v as i128)
    } else if v.abs() >= 1000.0 {
        group_thousands(v.round() as i128)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(v: &str, c: &str) -> Price {
        Price {
            value: Amount::parse(v).unwrap(),
            currency: c.into(),
        }
    }

    #[test]
    fn formats_money() {
        assert_eq!(format_money(33_100.4, "USD"), "$33,100");
        assert_eq!(format_money(1234.567, "USD"), "$1,234.57");
        assert_eq!(format_money(0.52, "EUR"), "0.52 EUR");
        assert_eq!(format_money(0.001, "USD"), "<$0.01");
        assert_eq!(format_money(0.0, "USD"), "$0.00");
        assert_eq!(format_price(&price("84455.38", "USD")), "$84,455.38");
        assert_eq!(format_price(&price("0.0938771", "USD")), "$0.093877");
        assert_eq!(
            format_price(&price("34028.14955012", "EUR")),
            "34,028.15 EUR"
        );
    }

    #[test]
    fn values_amounts_at_a_price() {
        let p = price("80000", "USD");
        assert_eq!(
            worth(Amount::parse("0.5").unwrap(), &p).as_deref(),
            Some("$40,000")
        );
        assert_eq!(worth(Amount::parse("1").unwrap(), &price("0", "USD")), None);
    }

    #[test]
    fn formats_shares() {
        assert_eq!(format_share(0.012370660198574827), "1.2371%");
        assert_eq!(format_share(1.9518949928633102e-7), "0.0000195%");
        assert_eq!(format_share(0.0), "0.0000%");
    }

    #[test]
    fn formats_hashrate_and_difficulty() {
        assert_eq!(format_hashrate("925644484990023500000"), "925.64 EH/s");
        assert_eq!(format_hashrate("999"), "999.00 H/s");
        assert_eq!(format_hashrate("n/a"), "n/a");
        assert_eq!(
            format_difficulty("132716002350731.3"),
            "132,716,002,350,731.30"
        );
        assert_eq!(format_difficulty("0.5"), "0.50");
    }

    #[test]
    fn formats_chart_values() {
        assert_eq!(format_chart_value("tx_count", &json!(620424)), "620,424");
        assert_eq!(
            format_chart_value("price_usd", &json!("82581.40000000")),
            "82,581.4"
        );
        assert_eq!(
            format_chart_value("holder_share", &json!({"top10": 0.0601, "top100": 0.5})),
            "top10 6.01%, top100 50.00%"
        );
        assert_eq!(
            format_chart_value("pool_share", &json!({"Foundry": 446, "unknown": 243})),
            "Foundry 446, unknown 243"
        );
        assert_eq!(
            format_chart_value(
                "transfers",
                &json!({"lt_1": {"count": 11407, "sent": "227.928"}})
            ),
            "lt_1 11,407 (228)"
        );
        assert_eq!(format_chart_value("tx_count", &Value::Null), "no data");
    }

    #[test]
    fn names_transaction_kinds() {
        assert_eq!(kind_name("coinjoin"), "CoinJoin");
        assert_eq!(kind_name("lightning_close"), "Lightning close");
        assert_eq!(kind_name("mweb_peg_in"), "MWEB peg-in");
        assert_eq!(kind_name("fee_bumped"), "fee-bumped");
        assert_eq!(kind_name("batch_payout"), "batch payout");
        assert_eq!(kind_name("payment"), "payment");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(30), "1 min");
        assert_eq!(duration(720), "12 min");
        assert_eq!(duration(19_200), "5 h 20 min");
        assert_eq!(duration(100_800), "1 day 4 h");
        assert_eq!(duration(273_600), "3 days 4 h");
    }

    #[test]
    fn labels_balance_periods() {
        assert_eq!(period_label(1_780_272_000, "month"), "2026-06");
        assert_eq!(period_label(1_780_272_000 + 3_600, "month"), "2026-06-01");
        assert_eq!(period_label(1_785_715_200, "week"), "week of 2026-08-03");
        assert_eq!(period_label(1_785_801_600, "week"), "2026-08-04");
        assert_eq!(period_label(1_785_801_600, "day"), "2026-08-04");
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(format_number(1234.0), "1,234");
        assert_eq!(format_number(1234.6), "1,235");
        assert_eq!(format_number(0.12345), "0.1235");
        assert_eq!(plural(1, "block"), "1 block");
        assert_eq!(plural(2, "address"), "2 addresses");
    }
}
