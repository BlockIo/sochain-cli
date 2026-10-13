# sochain

Bitcoin, Litecoin, Dogecoin and Zcash blockchain data from your terminal, via
[SoChain](https://sochain.com).

```console
$ sochain balance BTC bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh
Balance 3.92247777 BTC

$ sochain fees BTC
Suggested fee rate 1 sat/vB (the next block's median, rounded up).
...

$ sochain transaction LTC <txid>
$ sochain block DOGE latest
$ sochain transactions BTC <address> --from-time 2025-01-01 --direction in
```

Output is plain English by default. Add `--json` to any command for the SoChain
API's reply, unchanged, for scripts and AI agents.

`sochain` is read-only: it never needs private keys and never sends
transactions. Lookups work without an account; an API key raises the limits.

## Install

**Homebrew (macOS and Linux)**

```sh
brew install blockio/tap/sochain
```

**npm (any platform with Node.js)**

```sh
npm install -g sochain-cli    # or run once with: npx sochain-cli
```

**macOS and Linux, without Homebrew**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/BlockIo/sochain-cli/releases/latest/download/sochain-cli-installer.sh | sh
```

**Windows (PowerShell)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/BlockIo/sochain-cli/releases/latest/download/sochain-cli-installer.ps1 | iex"
```

**From source** (Rust 1.88 or newer)

```sh
cargo install --git https://github.com/BlockIo/sochain-cli --locked
```

Prebuilt binaries for each platform are attached to every
[release](https://github.com/BlockIo/sochain-cli/releases).

## Networks

Every lookup names its network first, in any case:

| Code | Network |
|---|---|
| `BTC` | Bitcoin |
| `LTC` | Litecoin |
| `DOGE` | Dogecoin |
| `ZEC` | Zcash (transparent addresses and transactions) |
| `BTCTEST`, `LTCTEST`, `DOGETEST`, `ZECTEST` | Their testnets |

## Commands

| Command | Shows |
|---|---|
| `balance <net> <address> [--height N \| --time T]` | Confirmed and unconfirmed balance, or the balance at a past block or time |
| `address-summary <net> <address>` | Balance, totals received, transaction counts, first and last activity (alias `address`) |
| `transactions <net> <address> [--page N]` | Transactions, 10 per page, newest first (alias `txs`) |
| `transactions <net> <address> [filters] [--csv]` | Filtered by `--from-time`, `--to-time`, `--from-height`, `--to-height`, `--direction`, `--unconfirmed`; paged by `--cursor`; CSV export |
| `transaction-counts <net> <address>` | Sent, received and total (alias `counts`) |
| `unspent-outputs <net> <address> [--page N]` | Spendable outputs (aliases `utxos`, `unspent`) |
| `is-valid-address <net> <address>` | Whether an address is valid on a network (alias `validate`) |
| `transaction <net> <txid>` | Confirmations, inputs, outputs, fee and kind, such as a payment or CoinJoin (alias `tx`) |
| `transaction-status <net> <txid>` | Pending, mined, replaced, dropped, expired or unknown (alias `status`) |
| `block <net> <height\|hash\|latest>` | A block |
| `latest-blocks <net>` | The 10 newest blocks (alias `blocks`) |
| `best-block-hash <net>` | The newest block's hash |
| `network-info <net>` | Block count, hashrate, the mempool (alias `network`) |
| `fees <net>` | Fee rates for the next blocks, and a suggested rate |
| `price <net> [--time T]` | The latest price, or a day's opening price |
| `rich-list <net> [--page N] [--page-size N]` | The top 1,000 holders (aliases `richlist`, `top`) |
| `address-rank <net> <address>` | Rank among the largest holders, and share of supply (alias `rank`) |
| `balance-history <net> <address> [--interval day\|week\|month]` | Balance over time, with amounts in and out |
| `address-poisoning <net> <address>` | Look-alike addresses that sent dust to trick the owner (alias `poisoning`) |
| `address-counterparties <net> <address>` | Who it paid and was paid by lately (alias `counterparties`) |
| `chart <net> <series> [--interval I]` | A chart series as a table, such as `tx_count`, `hashrate` or `price_usd` |
| `account` | Your plan and requests used (needs an API key) |
| `auth login` / `logout` / `status` | Save, remove or check your API key |
| `skill install` / `status` / `uninstall` | The SoChain skill for AI agents (Claude Code, Codex) |
| `mcp install` / `status` / `uninstall` | The SoChain MCP server for AI agents (Codex, Claude Code; `--print` for others) |
| `schema` | Every command's API endpoint and output shape, as JSON |
| `guide` | A usage guide for scripts and AI agents |

`--currency EUR` (or any of 30 currencies) shows prices in that currency
instead of US dollars. Run `sochain <command> --help` for details and examples.

## API key

Lookups work without a key: at least 2,500 a day per IP address, and up to
25,000 when there's room. For more, get a
key at [sochain.com/signin](https://sochain.com/signin) (plans and limits:
[sochain.com/pricing](https://sochain.com/pricing)), then either:

```sh
sochain auth login              # paste the key at the hidden prompt
export SOCHAIN_API_KEY=...      # or set it in the environment
```

`auth login` checks the key with SoChain and saves it in your config folder
(`~/.config/sochain` on macOS and Linux, `%APPDATA%\sochain` on Windows),
readable only by you. `SOCHAIN_API_KEY` takes precedence when set. The key is
sent only to sochain.com, and never printed.

## Scripts and AI agents

- `--json` (or `SOCHAIN_JSON=1`) prints the API's reply on stdout:
  `{"status":"success","data":{...}}`. Errors are then JSON on stderr:
  `{"status":"fail","data":{"error_message":"...","code":"..."}}`.
- Exit codes: 0 ok, 1 other error, 2 bad input, 3 not found, 4 SoChain
  unavailable, 5 rate limited, 6 API key missing, invalid or out of requests.
- Amounts are decimal strings with 8 decimals; parse them as decimals, not
  floats. Times are unix seconds.
- `sochain guide` prints a guide for agents ([AGENTS.md](AGENTS.md));
  `sochain schema` describes every command as JSON.
- `sochain skill install` adds the SoChain skill to Claude Code and Codex.
- `sochain mcp install` adds the hosted MCP server, `https://sochain.com/mcp`,
  to Codex and Claude Code. With `--use-api-key` it uses your key: Codex reads
  it from `SOCHAIN_API_KEY`, and Claude Code saves it in its own config (or, with
  `--project`, reads `SOCHAIN_API_KEY`). By hand:

  ```sh
  codex mcp add sochain --url https://sochain.com/mcp --bearer-token-env-var SOCHAIN_API_KEY
  claude mcp add --transport http sochain https://sochain.com/mcp --header "API-KEY: $SOCHAIN_API_KEY"
  ```

  For Cursor and other clients, use the URL with an `API-KEY` header;
  `sochain mcp install --print` shows each app's setup.

## Safety

The CLI refuses input that looks like a private key or recovery phrase,
without sending it anywhere. Text from the API is stripped of terminal control
characters before it is printed.

## Settings

| Variable | Flag | Meaning |
|---|---|---|
| `SOCHAIN_API_KEY` | | API key (else the one saved by `auth login`) |
| `SOCHAIN_JSON` | `--json` | JSON output |
| `SOCHAIN_TIMEOUT` | `--timeout` | Seconds to wait for a reply (default 30) |
| `SOCHAIN_CURRENCY` | `--currency` | Currency for prices (default USD) |
| `SOCHAIN_CONFIG_DIR` | | Where `auth login` saves the key |

## Links

- API documentation: https://sochain.com/docs
- OpenAPI description: https://sochain.com/api/v3/openapi.json
- Status: https://status.sochain.com
- Terms of Service: https://sochain.com/terms

## License

MIT. Data comes from SoChain and is subject to its
[Terms of Service](https://sochain.com/terms).
