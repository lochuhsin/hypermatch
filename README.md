# Order Entry Matching Engine

A low-latency, deterministic **order-entry hot path** for an exchange, written in Rust.

This is the core engine that takes client orders from the wire, sequences them,
runs pre-trade risk, matches them against a limit order book, and emits
acknowledgements and fills — all on a single-writer matching core designed for
deterministic, sub-microsecond processing.

---

## What this is

A single **core matching server**: one process that client sessions connect to,
where all orders converge to be matched. The matching core runs on a single
writer thread (LMAX Disruptor style); I/O and journaling run on separate threads,
connected by lock-free ring buffers.

```
client orders (TCP -> io_uring)
        │
        ▼
  [Ingress thread]      receive, framing, zero-copy decode
        │  (ring buffer)
        ▼
  [Matching thread] ★   single-writer core:
        │                 sequencer -> risk -> match -> events
        │  (ring buffer)
        ▼
  [Egress thread]       encode + send Ack / Fill / Reject back to client
  [Journal thread]      append-only event log for replay & recovery
```

The full hot path covered, in order:

```
Ingress -> Protocol decode -> Gateway/Session -> Sequencer
        -> Pre-trade Risk -> Matching (order book) -> Output (Ack/Fill/Reject)
```

---

## Scope

This project implements the exchange-side path from **wire-in to wire-out**:

- **Network ingress** — receive order events, framing, backpressure
- **Wire protocol** — zero-copy decode/encode (custom binary; optional FIX subset)
- **Gateway / session** — session state, validation, per-client order-id mapping
- **Sequencer** — single-writer ring buffer assigning a global sequence number
  (the source of determinism)
- **Pre-trade risk** — order-size and position/exposure limits, self-trade prevention
- **Matching core** — price-time priority limit order book; Limit / Market / IOC /
  FOK / Post-only; partial fills
- **Output events** — Ack / Fill / Reject encoded and returned to the client
- **Journal & replay** — append-only journal enabling crash recovery and
  byte-for-byte deterministic replay

In one line: **from "order event arrives on the wire" to "Ack/Fill/Reject sent
back to the client."**

---

## Non-Goals (out of scope)

To keep the boundary explicit — this is the core engine, not a whole exchange:

- **Market data dissemination** — broadcasting trades / L2 book updates to all
  subscribers is a separate egress path (tracked as an optional later phase, not
  part of the core).
- **Clearing & settlement** — no post-trade clearing, settlement, ledger, wallet,
  or custody.
- **Account / user management** — no KYC, balances, deposits, or withdrawals.
- **Market data ingestion** — this does *not* consume an external exchange's feed
  to rebuild someone else's book. That is the trading-firm-side feed handler, a
  different project.
- **High availability** — real exchanges run a primary + hot standby with state
  machine replication over the journal. This project builds the single core
  engine; HA/replication is intentionally out of scope.
- **Multi-instrument sharding** — the engine starts single-instrument (one logical
  matching instance). Per-instrument sharding is a later extension, not a core goal.

---

## Design principles

- **Single-writer matching core** — one thread owns the book; no locks on the hot
  path; deterministic by construction.
- **Determinism** — identical sequenced input always produces byte-for-byte
  identical output; proven via journal replay.
- **Fixed-point prices** — `i64` ticks throughout; no `f64` on the hot path.
- **Zero allocation on the hot path** — pre-allocated order pool; intrusive
  doubly-linked lists per price level for O(1) cancel.
- **Mechanical sympathy** — cache-line alignment to avoid false sharing, core
  pinning, lock-free ring buffers between stages.

---

## Performance targets

| Stage | p50 | p99 |
|---|---|---|
| Matching core (in-process) | < 500 ns | < 2 µs |
| End-to-end (TCP wire → ack, localhost) | low single-digit µs | < 50 µs |
| Throughput (single-core matching) | — | > 1M orders/sec |

Latency is measured with HdrHistogram; benchmarks and histograms are published
in the repo.

---

## Status

Work in progress. See the build roadmap for the staged plan and milestones.

## Build & run

```bash
cargo build --release
cargo test            # includes property tests + deterministic golden/replay tests
cargo bench           # criterion + HdrHistogram latency reports
```
