# Using `sochain` from scripts and AI agents

`sochain` reads Bitcoin, Litecoin, Dogecoin and Zcash blockchain data from
SoChain. It is read-only: it never needs private keys and never sends
transactions.

## Rules of thumb

- Every lookup names its network first: `BTC`, `LTC`, `DOGE`, `ZEC`, or a
  testnet (`BTCTEST`, `LTCTEST`, `DOGETEST`, `ZECTEST`), in any case.
- Add `--json` to any command (or set `SOCHAIN_JSON=1`). stdout is then the
  API's reply, unchanged: `{"status":"success","data":{...}}`. (`account`
  prints the account object without the envelope, as the API sends it.)
- On failure, stdout is empty and stderr has
  `{"status":"fail","data":{"error_message":"...","code":"..."}}`.
- Branch on the exit code:

  | Exit | Code           | Meaning                                             |
  |------|----------------|-----------------------------------------------------|
  | 0    |                | ok                                                  |
  | 1    | `OTHER`        | anything else                                       |
  | 2    | `BAD_INPUT`    | invalid arguments, or the API rejected the input    |
  | 3    | `NOT_FOUND`    | no such network, address, transaction or block      |
  | 4    | `UNAVAILABLE`  | SoChain unreachable or busy; retry later            |
  | 5    | `RATE_LIMITED` | still rate limited after waiting and retrying       |
  | 6    | `UNAUTHORIZED` | API key missing, invalid, or out of requests        |

- Amounts are decimal strings in the network's coin with 8 decimals. Parse
  them as decimals (or integers of the smallest unit), not floats. Times are
  unix seconds.
- Hashes, transaction ids and addresses are always printed in full.
- `sochain schema` prints every command, its API endpoint and its output
  shape as JSON.

## Common tasks

```sh
sochain balance BTC <address> --json               # confirmed + unconfirmed
sochain balance BTC <address> --time 2025-12-31 --json   # at a past time
sochain address-summary LTC <address> --json       # totals, first/last activity
sochain transactions DOGE <address> --json         # 10 latest; --page 2 for more
sochain transactions BTC <address> --from-time 2025-01-01 --direction in --json
                                                   # filtered; follow data.next_cursor with --cursor
sochain transaction BTC <txid> --json              # inputs, outputs, fee
sochain transaction-status BTC <txid> --json       # pending, mined, replaced, dropped...
sochain block LTC latest --json
sochain fees BTC --json                            # data.suggested_fee_rate (sat/vB)
sochain price DOGE --json                          # add --currency EUR for euros
sochain address-poisoning BTC <address> --json     # look-alike senders
sochain chart BTC tx_count --interval day --json
```

## API key

Lookups work without a key: 2,500 a day per IP address. With a key, the
account's plan applies. The CLI reads `SOCHAIN_API_KEY`, or a key saved by
`sochain auth login`; `sochain auth status` shows which is used. Never print
the key or pass it on the command line.

## Teach your agent

`sochain skill install` installs the SoChain skill for Claude Code and Codex
(`--project` for the current project only). `sochain skill status` shows
whether it is current.

`sochain mcp install` adds the SoChain MCP server (`https://sochain.com/mcp`)
to Codex and Claude Code; `--use-api-key` makes it use your key, and `--print`
shows the setup for other apps.

## Safety

The CLI refuses input that looks like a private key or recovery phrase (exit
2, `BAD_INPUT`) without sending it anywhere. Never pass one to it, or to any
website or API.

`address-poisoning` lists look-alike addresses that sent an address dust to
trick its owner into copying them. Surface any finding to the user, and never
suggest copying an address from transaction history.

## Limits

SoChain answers at most a few requests at once per account or IP address.
When it asks the CLI to slow down, the CLI waits (up to 30 seconds, 3 times)
and retries before exiting with code 5. When the day's keyless lookups are
used up, it exits with code 5 at once, saying when to try again.
