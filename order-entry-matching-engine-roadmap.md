# Order Entry Path Matching Engine — Rust Build Roadmap

> 目標：做交易所端的 **order-entry hot path**（客戶送單 → 撮合 → 成交）。
> 低延遲、deterministic、single-writer 核心。對標 Binance / Bybit / DEX / prediction market signal。
> 核心哲學：**先把確定性核心做對並 benchmark，再把 network/protocol 包在外面。**

---

## 系統熱路徑（你要建的全部）

```
外部訂單 (TCP → 之後 io_uring)
  │
  ▼
① Ingress        收封包、framing、zero-copy
  │
  ▼
② Protocol       解 binary/FIX，hot path 不 allocate
  │
  ▼
③ Gateway        session、驗證、client order-id namespacing
  │
  ▼
④ Sequencer ★    single-writer ring buffer，賦予全域序號 → 整條路的靈魂
  │
  ▼
⑤ Risk           position limit、max size、self-trade prevention
  │
  ▼
⑥ Matching Core  order book + 撮合演算法
  │
  ▼
輸出：Ack / Fill / Reject（+ optional market data）
```

撮合（⑥）只是最後一站。前面 ①~⑤ 全是「資料從外面進來」的部分，每一站都是低延遲戰場。

---

## Modules（crate / component 切分）

| Module | 職責 | hot path? |
|---|---|---|
| `types` | Order / Side / Price / Qty / OrderId / OrderType / TIF / 事件型別 | — |
| `order_book` | 狀態：bid/ask ladder、每個 price level 的 FIFO、O(1) cancel | ✔ |
| `matching` | 演算法：price-time priority、partial fill、各 order type | ✔ |
| `sequencer` | single-writer ring buffer（Disruptor 風格），定序、驅動引擎 | ✔ |
| `risk` | pre-trade check、STP | ✔ |
| `gateway` | session 狀態、驗證、client↔internal id 映射 | 半 |
| `protocol` | wire codec，zero-copy parse/encode | 半 |
| `ingress` | 網路層（先 TCP，再 io_uring / busy-poll） | 半 |
| `journal` | append-only event log → replay & crash recovery | — |
| `bench` | criterion + HdrHistogram + deterministic replay test | — |

---

## 貫穿全程的硬規則（HFT signal 來源）

- **Price 用 fixed-point**（`i64` ticks），hot path **絕不出現 `f64`**。
- **Hot path 零 heap allocation**：order 用 pre-allocated pool / slab；price level 的 FIFO 用 intrusive doubly-linked list（O(1) cancel）。
- **Matching 核心 single-writer 單執行緒**，cache-line aligned（`#[repr(align(64))]`），pin 到一顆 core（`core_affinity`）。
- **Stage 之間用 lock-free ring buffer**（SPSC/MPSC），不要用 mutex 串 hot path。
- **Deterministic**：同一串 sequenced 輸入，重放得到逐 byte 相同輸出。這是 exchange 跟「玩具撮合」的分水嶺，也是你最強的單點 signal。

---

## 7 週計畫（可壓縮 / 可延伸）

### Week 1 — Domain model + Order Book 資料結構
- `types`：fixed-point `Price(i64)`、`Qty`、`OrderId`、`Side`、`OrderType`、`TimeInForce`。
- `order_book`：bid/ask 各一個 `BTreeMap<Price, Level>`（先求正確，之後再優化近價區）；`Level` 內 order FIFO。
- API：`add`、`cancel`、`best_bid` / `best_ask`、`peek`。
- O(1) cancel：`OrderId → handle`（slab）映射。
- **Done when**：能 add/cancel 任意單、book invariant（bid < ask、qty 守恆）有 unit test。

### Week 2 — Matching 演算法
- incoming limit 撞對側，price-time priority，支援 partial fill，產生 `Trade`/`Fill` 事件。
- order types：Limit、Market、IOC、FOK、Post-only。
- `proptest` property test：任意操作序列後 book invariant 不破。
- **golden-file 測試**：固定輸入序列 → 比對逐筆輸出（為 determinism 鋪路）。
- **Done when**：所有 order type 行為正確、golden test 綠燈。

### Week 3 — Sequencer + single-writer 引擎迴圈（核心成形）
- 先用 `rtrb` / `crossbeam` 的 ring buffer 把引擎跑起來，**之後 Week 7 再換成手刻 SPSC** 展示深度。
- 單一 matching thread 嚴格按序號消費 command → 整個系統變 deterministic。
- benchmark：throughput（ops/sec）+ 隔離 latency histogram（`hdrhistogram`），出 p50/p99/p99.9。
- **目標**：matching core p50 < 500ns、p99 < 2μs；> 1M orders/sec 單核。
- **Done when**：有可重現的 benchmark 數字 + 圖。

### Week 4 — Pre-trade Risk + Gateway/Session
- `risk`：max order size、position/exposure limit、self-trade prevention。
- `gateway`：session map、per-client order-id namespacing、輸入驗證（壞單 → Reject 不進核心）。
- 定稿 command/event：`New`/`Cancel`/`Modify` → `Ack`/`Reject`/`Fill`。
- **Done when**：違規單被正確擋下並回 Reject，合法單照常成交。

### Week 5 — Wire Protocol + Codec
- 自訂 binary protocol（length-prefixed、固定 layout），用 `zerocopy` / `bytes` 做 zero-copy decode。
- encode exec report 回客戶。
- （optional）FIX subset parser。
- `cargo-fuzz` 打 parser。
- **Done when**：bytes ↔ 內部 command 雙向轉換正確、fuzz 無 crash。

### Week 6 — Network Ingress
- 先 `tokio` TCP server：accept、framing、餵進 sequencer。
- 再優化：`tokio-uring` / `glommio`（io_uring）或 busy-poll thread pin 到 core。
- backpressure 處理、多 client 測試。
- **Done when**：多個 client 同時下單，成交順序由 sequencer 決定且可重現。

### Week 7 — Journal/WAL + Replay + Polish
- `journal`：每筆 sequenced command append-only 落盤。
- crash recovery：replay journal → 重建出**完全相同**的 book state。
- **deterministic replay 測試**：journal → 逐 byte 相同輸出（determinism 的鐵證，signal 拉滿）。
- 端到端 latency benchmark（wire → ack），出 HdrHistogram。
- 把 Week 3 的 ring buffer 換成手刻 SPSC。
- README：架構圖 + benchmark 數字 + 一篇技術 blog。
- **Done when**：殺掉 process 重啟能完整回復、replay 證明 determinism、README 有圖有數字。

### Week 8（optional 延伸）
- multi-instrument + per-instrument shard。
- market data 輸出 feed（L2 book snapshot + incremental）。
- 跟 naive 實作對比的 benchmark（凸顯你的優化）。

---

## Crate 選型（先用現成 → 再手刻展示深度）

- bench / 量測：`criterion`、`hdrhistogram`、`core_affinity`
- 資料：`slab`、`smallvec`、`bytes`、`zerocopy`
- ring buffer：先 `rtrb` / `crossbeam` → 後手刻 SPSC
- async / net：先 `tokio` → 後 `tokio-uring` / `glommio`
- 測試：`proptest`、`cargo-fuzz`

---

## Latency 目標（寫進 README）

| 量測點 | p50 | p99 |
|---|---|---|
| Matching core（in-process） | < 500ns | < 2μs |
| 端到端（TCP wire → ack，localhost） | 低個位數 μs | < 50μs |
| Throughput（單核 matching core） | — | > 1M orders/sec |

---

## 參考資料

- **LMAX Disruptor** paper（single-writer ring buffer 的聖經）
- WK Selph, *How to Build a Fast Limit Order Book*（經典 blog）
- Aeron / Chronicle（Java，但架構思路通用）
- NASDAQ ITCH / 各大 crypto exchange engineering blog（wire protocol 參考）

---

## Signal 最大化清單

1. **Deterministic replay** — 整個專案最強的一個 feature，務必做出來並在 README 證明。
2. **HdrHistogram latency 圖** — 放 README，數字會說話。
3. **架構圖** — 一張 hot path 圖勝過千言。
4. **vs naive 的 benchmark** — 證明你的優化是真的快，不是嘴上快。
5. **單寫者 / 零分配 / cache-line** 這些決策在 README 講清楚 why，不只 what。
