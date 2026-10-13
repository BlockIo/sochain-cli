# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The public interface covered by semantic versioning is: command and flag
names, environment variables, exit codes, error codes, and the shape of
`--json` output (which is the SoChain API's reply).

## [Unreleased]

## [0.1.2] - 2026-10-13

### Changed

- Transaction kinds use their usual names: CoinJoin, Lightning and MWEB.

## [0.1.1] - 2026-10-12

### Added

- `transaction` shows what kind of transaction it is (such as a payment,
  CoinJoin or inscription) when SoChain has classified it, and says when the
  kind is a guess.
- `account` shows requests this month against the plan's monthly quota, and
  when the count resets.
- `network-info` and `fees` say when SoChain is still reading the mempool in
  and its counts may be low.

## [0.1.0] - 2026-10-10

### Added

- Lookups for Bitcoin, Litecoin, Dogecoin, Zcash and their testnets:
  `balance`, `address-summary`, `transactions` (paged, filtered or CSV),
  `transaction-counts`, `unspent-outputs`, `is-valid-address`, `transaction`,
  `transaction-status`, `block`, `latest-blocks`, `best-block-hash`,
  `network-info`, `fees`, `price`, `rich-list`, `address-rank`,
  `balance-history`, `address-poisoning`, `address-counterparties`, `chart`
  and `account`.
- Plain-English output by default; `--json` for the API's reply.
- `--currency` for prices in 30 currencies.
- `auth login`, `logout` and `status` to save and check an API key.
- `skill` and `mcp` to set up AI agents.
- `schema` and `guide` for scripts and agents.

[Unreleased]: https://github.com/BlockIo/sochain-cli/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/BlockIo/sochain-cli/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/BlockIo/sochain-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/BlockIo/sochain-cli/releases/tag/v0.1.0
