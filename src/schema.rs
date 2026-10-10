//! `sochain schema`: a machine-readable description of every command, for
//! tool-calling agents. Keep it in step with cli.rs and AGENTS.md.

use serde_json::{Value, json};

pub fn schema() -> Value {
    json!({
        "name": "sochain",
        "version": env!("CARGO_PKG_VERSION"),
        "api": "https://sochain.com/api/v3",
        "openapi": "https://sochain.com/api/v3/openapi.json",
        "terms_of_service": "https://sochain.com/terms",
        "networks": ["BTC", "LTC", "DOGE", "ZEC", "BTCTEST", "LTCTEST", "DOGETEST", "ZECTEST"],
        "output": {
            "text": "plain English on stdout (default)",
            "json": "--json or SOCHAIN_JSON=1: the API's reply, unchanged, on stdout",
        },
        "envelope": {
            "success": { "status": "success", "data": "<per command>" },
            "failure": {
                "status": "fail",
                "data": {
                    "error_message": "string",
                    "code": "BAD_INPUT | NOT_FOUND | UNAVAILABLE | RATE_LIMITED | UNAUTHORIZED | OTHER",
                    "retry_after_secs": "integer, RATE_LIMITED only"
                }
            },
            "failure_stream": "stderr"
        },
        "exit_codes": {
            "0": "ok",
            "1": "other error (OTHER)",
            "2": "bad input: invalid arguments, or rejected by the API (BAD_INPUT)",
            "3": "not found (NOT_FOUND)",
            "4": "SoChain unreachable or unavailable; retry later (UNAVAILABLE)",
            "5": "rate limited after waiting and retrying (RATE_LIMITED)",
            "6": "API key missing, invalid, or out of requests (UNAUTHORIZED)"
        },
        "conventions": {
            "network": "first argument of every lookup, any case",
            "amounts": "decimal strings in the network's coin with 8 decimals, e.g. \"0.00112405\"; never floats",
            "times": "unix seconds",
            "time_inputs": "unix seconds, a UTC date (YYYY-MM-DD) or an RFC 3339 timestamp",
            "prices": "objects {value, currency}; USD unless --currency; always 0 on testnets"
        },
        "global_flags": {
            "--json": "JSON output (env SOCHAIN_JSON)",
            "--timeout <SECS>": "per-request timeout, default 30 (env SOCHAIN_TIMEOUT)",
            "--currency <CODE>": "prices in this currency for price, transaction, transactions, block, latest-blocks and the price_usd chart (env SOCHAIN_CURRENCY)"
        },
        "auth": {
            "key": "optional for every lookup; read from SOCHAIN_API_KEY, else from the file saved by `sochain auth login`",
            "header": "API-KEY, sent only to sochain.com"
        },
        "commands": commands(),
    })
}

fn commands() -> Value {
    json!({
        "balance <network> <address> [--height N | --time T]": {
            "api": "GET /balance/{network}/{address}",
            "data": "confirmed, unconfirmed; with --height or --time: address, height, time, block_hash, confirmed, tip_height"
        },
        "address-summary <network> <address>": {
            "aliases": ["address"],
            "api": "GET /address_summary/{network}/{address}",
            "data": "txs_sent, txs_received, txs_total, confirmed_balance, unconfirmed_balance, confirmed_received, first_active{height,time}, last_active{height,time}"
        },
        "transactions <network> <address> [--page N]": {
            "aliases": ["txs"],
            "api": "GET /transactions/{network}/{address}/{page}",
            "data": "transactions[{hash, value_sent, value_received, balance_change, time, block|null, price}]",
            "notes": "10 per page; unconfirmed first, then newest first"
        },
        "transactions <network> <address> [filters]": {
            "api": "GET /transactions/{network}/{address}?from_height&to_height&from_time&to_time&direction&unconfirmed&page_size&cursor",
            "filters": "--from-height, --to-height, --from-time, --to-time, --direction in|out|both, --unconfirmed include|exclude|only, --page-size 1-100, --cursor, --csv (page size up to 500)",
            "data": "transactions[...], next_cursor|null, page_size, from_height, to_height, direction, unconfirmed, tip_height, reorged?, fork_height?",
            "notes": "send next_cursor back with --cursor until it is null; a short or empty page with a cursor is not the end; after a reorg rows may repeat, so deduplicate by hash. --csv prints CSV on stdout and the next cursor on stderr"
        },
        "transaction-counts <network> <address>": {
            "aliases": ["counts"],
            "api": "GET /transaction_counts/{network}/{address}",
            "data": "sent, received, total"
        },
        "unspent-outputs <network> <address> [--page N]": {
            "aliases": ["utxos", "unspent"],
            "api": "GET /unspent_outputs/{network}/{address}/{page}",
            "data": "outputs[{hash, index, script, address, value, block|null, tx_hex}]",
            "notes": "10 per page, oldest first"
        },
        "is-valid-address <network> <address>": {
            "aliases": ["validate"],
            "api": "GET /is_valid_address/{network}/{address}",
            "data": "is_valid (boolean; an invalid address is not an error)"
        },
        "transaction <network> <txid>": {
            "aliases": ["tx"],
            "api": "GET /transaction/{network}/{hash}",
            "data": "hash, confirmations, size, vsize, weight, version, locktime, block_hash, block_height|null, time, inputs_n, outputs_n, inputs_value, outputs_value, fee, price, inputs[{index, value, address, previous_output{hash,index}, ...}], outputs[{index, value, address, type, script, spent, ...}], shielded? (Zcash), mweb labels (Litecoin)"
        },
        "transaction-status <network> <txid>": {
            "aliases": ["status"],
            "api": "GET /transaction_status/{network}/{hash}",
            "data": "network, hash, status (pending | mined | replaced | dropped | expired | unknown), block_height, confirmations, replaced_by, replaced_by_status, replaced_by_height, replaced_ancestor, removed_at"
        },
        "block <network> <height|hash|latest>": {
            "api": "GET /block/{network}/{hash_or_height} (latest: GET /best_block_hash/{network} first)",
            "data": "height, hash, time, confirmations, num_txs, size, weight, version, nonce, bits, difficulty, merkleroot, previous_block_hash, next_block_hash|null, is_orphan, reward, fees, value_out, price, txs[txid], mweb? (Litecoin)"
        },
        "latest-blocks <network>": {
            "aliases": ["blocks"],
            "api": "GET /latest_blocks_summary/{network}",
            "data": "blocks[{height, time, num_txs, size, weight, version, difficulty, price, miner, reward_and_fees}], newest first"
        },
        "best-block-hash <network>": {
            "api": "GET /best_block_hash/{network}",
            "data": "hash"
        },
        "network-info <network>": {
            "aliases": ["network"],
            "api": "GET /network_info/{network}",
            "data": "block_count, best_block_hash, hashrate, mempool{mempool_txs, mempool_size, updated_at, blocks[{block_num, num_txs, size, weight, min_fee_rate, median_fee_rate, max_fee_rate, *_precise}]}"
        },
        "fees <network>": {
            "api": "GET /network_info/{network}",
            "data": "network, unit (sat/vB, lit/vB, koinu/vB or zat/action), suggested_fee_rate (the next block's median, a decimal string), mempool_txs, mempool_size, blocks (as in network-info)",
            "notes": "unlike other commands, --json reshapes the API reply to the fee fields"
        },
        "price <network> [--time T]": {
            "api": "GET /price/{network} or GET /price/{network}/{unix_timestamp}",
            "data": "value, currency, as_of?, sources?; with --currency other than USD: method (native | usd_fx), fx_rate?, fx_date?",
            "notes": "--time takes unix seconds or a date and gives that UTC day's opening price"
        },
        "rich-list <network> [--page N] [--page-size 25|50|100]": {
            "aliases": ["richlist", "top"],
            "api": "GET /rich_list/{network}?page&page_size",
            "data": "height, time, supply, page, page_size, total_rows, rows[{rank, address, balance, share_of_supply, tx_count, received, sent, first_seen, last_seen, delta_24h, delta_7d}]",
            "notes": "top 1,000 holders; Zcash counts the transparent pool only"
        },
        "address-rank <network> <address>": {
            "aliases": ["rank"],
            "api": "GET /address_rank/{network}/{address}",
            "data": "address, balance, rank|null, rank_limit, share_of_supply"
        },
        "balance-history <network> <address> [--interval day|week|month] [--from T] [--to T] [--last N]": {
            "api": "GET /balance_history/{network}/{address}?interval&from&to",
            "data": "address, interval, points[{t, height, v, received, sent, tx_in, tx_out}]",
            "notes": "--last limits text output only"
        },
        "address-poisoning <network> <address> [--limit N]": {
            "aliases": ["poisoning"],
            "api": "GET /address_poisoning/{network}/{address}?limit",
            "data": "address, building, findings[{sender (the look-alike), imitates, dust_txid, amount, real_txid, real_height, real_amount, receiver_paid, shared_prefix, shared_suffix, sender_kind, sender_txs, height, confirmed}]"
        },
        "address-counterparties <network> <address> [--limit N]": {
            "aliases": ["counterparties"],
            "api": "GET /address_counterparties/{network}/{address}?limit",
            "data": "address, window_blocks, from_height, building, counterparties[{address, direction (sent | received), amount, txid, height}]"
        },
        "chart <network> <series> [--interval block|minute|hour|day] [--from N] [--to N] [--last N]": {
            "api": "GET /charts/{network}/{series}/{interval}?from&to",
            "data": "series, interval, points[{t, v, height?}], oldest first; v is a number, an amount string or an object",
            "notes": "series and intervals: https://sochain.com/docs/charts/series; --last limits text output only"
        },
        "account": {
            "api": "GET /account_info (needs a key)",
            "data": "the account object, without the envelope: plan, num_requests_used, key{name, access}, ..."
        },
        "auth login [--no-verify]": {
            "does": "read a key from a hidden prompt or stdin, check it with GET /account_info, and save it readable only by you",
            "data": "saved_to, verified, plan, key_name"
        },
        "auth logout": { "data": "removed, path, env_key_set" },
        "auth status": { "data": "key: null, or {source: {kind: env | file, ...}} with account" },
        "skill install [--project | --dir DIR] [--print] [--force]": {
            "does": "download the SoChain agent skill from sochain.com, verify its published sha256 digest, and install it for Claude Code (~/.claude/skills) and Codex (~/.agents/skills)",
            "source": "https://sochain.com/.well-known/agent-skills/index.json"
        },
        "skill status [--project | --dir DIR]": {},
        "skill uninstall [--project | --dir DIR] [--force]": {},
        "mcp install [--use-api-key] [--project] [--print] [--force]": {
            "does": "add https://sochain.com/mcp to Codex and Claude Code with their own `mcp add` commands; --print shows the setup for other apps"
        },
        "mcp status": {},
        "mcp uninstall [--project] [--force]": {},
        "schema": { "does": "print this description" },
        "guide": { "does": "print the usage guide for scripts and AI agents" }
    })
}
