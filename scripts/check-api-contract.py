#!/usr/bin/env python3
"""Checks the SoChain OpenAPI description against what the CLI relies on:

1. every endpoint the CLI calls is still a GET in the spec, and
2. every field the text output requires (the non-Option fields of
   src/model.rs) is still present and never null.

Keep the lists below in step with src/commands.rs and src/model.rs.
Usage: check-api-contract.py [spec.json]  (default: the live spec)
"""

import json
import sys
import urllib.request

SPEC_URL = "https://sochain.com/api/v3/openapi.json"

ENDPOINTS = [
    "/balance/{network}/{address}",
    "/address_summary/{network}/{address}",
    "/transactions/{network}/{address}/{page}",
    "/transactions/{network}/{address}",
    "/transaction_counts/{network}/{address}",
    "/unspent_outputs/{network}/{address}/{page}",
    "/is_valid_address/{network}/{address}",
    "/transaction/{network}/{hash}",
    "/transaction_status/{network}/{hash}",
    "/block/{network}/{hash_or_height}",
    "/latest_blocks_summary/{network}",
    "/best_block_hash/{network}",
    "/network_info/{network}",
    "/price/{network}",
    "/price/{network}/{unix_timestamp}",
    "/rich_list/{network}",
    "/address_rank/{network}/{address}",
    "/balance_history/{network}/{address}",
    "/address_poisoning/{network}/{address}",
    "/address_counterparties/{network}/{address}",
    "/charts/{network}/{series}/{interval}",
    "/account_info",
]

# (component schema, path to an object inside it, fields the CLI requires)
REQUIRED = [
    ("Balance", [], ["confirmed"]),
    ("AddressSummary", [], ["txs_sent", "txs_received", "txs_total", "confirmed_balance",
                            "unconfirmed_balance", "confirmed_received"]),
    ("AddressTransactions", [], ["transactions"]),
    ("AddressTransaction", [], ["hash", "balance_change", "time"]),
    ("TransactionCounts", [], ["sent", "received", "total"]),
    ("UnspentOutputs", [], ["outputs"]),
    ("UnspentOutput", [], ["hash", "index", "value"]),
    ("IsValidAddress", [], ["is_valid"]),
    ("Transaction", [], ["hash", "confirmations", "size", "time", "fee", "inputs", "outputs"]),
    ("TransactionOutput", [], ["value"]),
    ("TransactionStatus", [], ["status"]),
    ("Block", [], ["height", "hash", "time", "confirmations", "num_txs", "size",
                   "difficulty", "reward", "fees", "value_out"]),
    ("LatestBlocks", [], ["blocks"]),
    ("LatestBlock", [], ["height", "time", "num_txs", "reward_and_fees"]),
    ("BestBlockHash", [], ["hash"]),
    ("NetworkInfo", [], ["block_count", "best_block_hash", "hashrate", "mempool"]),
    ("NetworkInfo", ["mempool"], ["mempool_txs", "mempool_size"]),
    ("ProjectedBlock", [], ["block_num", "num_txs", "min_fee_rate", "median_fee_rate",
                            "max_fee_rate"]),
    ("PriceData", [], ["value", "currency"]),
    ("RichList", [], ["height", "supply", "page", "page_size", "total_rows", "rows"]),
    ("RichList", ["rows", "[]"], ["rank", "balance"]),
    ("AddressRank", [], ["address", "balance"]),
    ("BalanceHistory", [], ["interval", "points"]),
    ("BalanceHistory", ["points", "[]"], ["t", "v", "received", "sent"]),
    ("AddressPoisoning", [], ["address", "findings"]),
    ("AddressPoisoning", ["findings", "[]"], ["sender", "imitates"]),
    ("Counterparties", [], ["address", "counterparties"]),
    ("Counterparties", ["counterparties", "[]"], ["address", "direction", "amount", "txid"]),
    ("Chart", [], ["series", "interval", "points"]),
    ("ChartPoint", [], ["t"]),
]


def main():
    if len(sys.argv) > 1:
        with open(sys.argv[1]) as f:
            spec = json.load(f)
    else:
        request = urllib.request.Request(
            SPEC_URL, headers={"User-Agent": "sochain-cli-contract-check"}
        )
        with urllib.request.urlopen(request, timeout=30) as r:
            spec = json.load(r)

    def res(s):
        while isinstance(s, dict) and "$ref" in s:
            node = spec
            for key in s["$ref"].lstrip("#/").split("/"):
                node = node[key]
            s = node
        return s

    def merge(s):
        s = res(s)
        if "allOf" in s:
            m = {"properties": {}, "required": []}
            for part in s["allOf"]:
                part = merge(part)
                m["properties"].update(part.get("properties", {}))
                m["required"] += part.get("required", [])
            return m
        return s

    def nullable(s):
        s = res(s)
        t = s.get("type")
        if t == "null" or (isinstance(t, list) and "null" in t):
            return True
        if None in s.get("enum", []):
            return True
        return any(nullable(x) for x in s.get("anyOf", []) + s.get("oneOf", []))

    problems = []
    paths = spec.get("paths", {})
    for endpoint in ENDPOINTS:
        if "get" not in paths.get(endpoint, {}):
            problems.append(f"GET {endpoint} is not in the spec")

    schemas = spec.get("components", {}).get("schemas", {})
    for name, inner, fields in REQUIRED:
        if name not in schemas:
            problems.append(f"schema {name} is missing")
            continue
        node = merge(schemas[name])
        where = name
        for step in inner:
            node = merge(node["items"] if step == "[]" else node.get("properties", {}).get(step, {}))
            where += f".{step}"
        props = node.get("properties", {})
        declared = node.get("required")
        for field in fields:
            if field not in props:
                problems.append(f"{where}.{field} is gone")
            elif declared is not None and field not in declared:
                problems.append(f"{where}.{field} is no longer required")
            elif nullable(props[field]):
                problems.append(f"{where}.{field} can now be null")

    if problems:
        print("The SoChain API no longer guarantees what the CLI relies on:")
        for p in problems:
            print(f"  - {p}")
        sys.exit(1)
    print(f"ok: {len(ENDPOINTS)} endpoints and {sum(len(f) for _, _, f in REQUIRED)} fields")


if __name__ == "__main__":
    main()
