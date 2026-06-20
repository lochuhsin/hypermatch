use crate::order_book::OrderBook;
use crate::risk::Risk;
use crate::spsc::SpscQueue;
use crate::types::{Command, Event, RejectReason};
use std::sync::Arc;
use std::thread;

#[derive(Default)]
pub struct Engine {
    order_book: OrderBook,
    risk: Risk,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            order_book: OrderBook::default(),
            risk: Risk::default(),
        }
    }

    pub fn with_risk(risk: Risk) -> Self {
        Self {
            order_book: OrderBook::default(),
            risk,
        }
    }

    pub fn apply(&mut self, cmd: Command, out: &mut Vec<Event>) {
        match cmd {
            Command::Noop => {}
            Command::CancelOrder(id) => {
                let outcome = self.order_book.cancel(id);
                let event = match outcome {
                    Some(o) => Event::Canceled(o.id),
                    None => Event::Rejected(id, RejectReason::OrderNotFound),
                };
                out.push(event);
            }
            Command::NewOrder(order) => {
                if let Some(reason) = self.risk.check(&order) {
                    out.push(Event::Rejected(order.id, reason));
                    return;
                }

                let trades = self.order_book.submit(order);

                if trades.len() + 1 > out.capacity() {
                    out.reserve((trades.len() + 1 - out.capacity()) << 1);
                }

                out.push(Event::Accepted(order.id));
                for trade in trades.iter() {
                    out.push(Event::Fill(*trade));
                }
            }
            Command::ShutDown => {}
        }
    }

    pub fn run(&mut self, cmd_q: Arc<SpscQueue<Command>>, evt_q: Arc<SpscQueue<Event>>) {
        let mut out_buffer = Vec::<Event>::with_capacity(2048);

        loop {
            if let Some(cmd) = cmd_q.try_pop() {
                if cmd == Command::ShutDown {
                    break;
                }

                self.apply(cmd, &mut out_buffer);

                for evt in out_buffer.iter() {
                    loop {
                        if evt_q.try_push(*evt).is_ok() {
                            break;
                        }
                        thread::yield_now();
                    }
                }
                out_buffer.clear();
            } else {
                thread::yield_now();
                continue;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Order, OrderId, Price, Qty, Side, Trade};

    fn order(id: u64, side: Side, price: i64, qty: u64) -> Order {
        Order {
            id: OrderId(id),
            side,
            price: Price(price),
            qty: Qty(qty),
        }
    }

    fn fill(taker: u64, maker: u64, taker_side: Side, price: i64, qty: u64) -> Event {
        Event::Fill(Trade {
            taker: OrderId(taker),
            maker: OrderId(maker),
            taker_side,
            price: Price(price),
            qty: Qty(qty),
        })
    }

    // ---- event mapping(引擎本身的職責)----

    #[test]
    fn resting_order_emits_only_accepted() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::NewOrder(order(1, Side::Buy, 100, 10)), &mut out);
        assert_eq!(out, vec![Event::Accepted(OrderId(1))]);
    }

    #[test]
    fn full_fill_emits_accepted_then_one_fill() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::NewOrder(order(1, Side::Sell, 100, 10)), &mut out);
        out.clear(); // caller 負責清 buffer
        eng.apply(Command::NewOrder(order(2, Side::Buy, 100, 10)), &mut out);
        assert_eq!(
            out,
            vec![Event::Accepted(OrderId(2)), fill(2, 1, Side::Buy, 100, 10)]
        );
    }

    #[test]
    fn partial_fill_emits_accepted_and_one_fill() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::NewOrder(order(1, Side::Sell, 100, 10)), &mut out);
        out.clear();
        // 買 15,只吃得到 maker 的 10
        eng.apply(Command::NewOrder(order(2, Side::Buy, 100, 15)), &mut out);
        assert_eq!(
            out,
            vec![Event::Accepted(OrderId(2)), fill(2, 1, Side::Buy, 100, 10)]
        );
    }

    #[test]
    fn sweeps_two_levels_emits_two_fills() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::NewOrder(order(1, Side::Sell, 100, 5)), &mut out);
        eng.apply(Command::NewOrder(order(2, Side::Sell, 101, 5)), &mut out);
        out.clear();
        eng.apply(Command::NewOrder(order(3, Side::Buy, 101, 10)), &mut out);
        assert_eq!(
            out,
            vec![
                Event::Accepted(OrderId(3)),
                fill(3, 1, Side::Buy, 100, 5), // 先吃便宜的
                fill(3, 2, Side::Buy, 101, 5),
            ]
        );
    }

    #[test]
    fn cancel_existing_emits_canceled() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::NewOrder(order(1, Side::Buy, 100, 10)), &mut out);
        out.clear();
        eng.apply(Command::CancelOrder(OrderId(1)), &mut out);
        assert_eq!(out, vec![Event::Canceled(OrderId(1))]);
    }

    #[test]
    fn cancel_missing_emits_rejected() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::CancelOrder(OrderId(999)), &mut out);
        assert_eq!(
            out,
            vec![Event::Rejected(OrderId(999), RejectReason::OrderNotFound)]
        );
    }

    #[test]
    fn noop_emits_nothing() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::Noop, &mut out);
        assert!(out.is_empty());
    }

    // ---- 整合測試:殘量必須掛回 book(會抓 buy_match 的 bug)----
    // 買 15 部分成交後,殘量 5 應變成 bid maker;再來一個賣 5 應吃到它。
    #[test]
    fn buy_remainder_rests_and_can_be_hit() {
        let mut eng = Engine::new();
        let mut out = Vec::new();
        eng.apply(Command::NewOrder(order(1, Side::Sell, 100, 10)), &mut out);
        eng.apply(Command::NewOrder(order(2, Side::Buy, 100, 15)), &mut out); // 殘量 5 應掛成 bid
        out.clear();
        eng.apply(Command::NewOrder(order(3, Side::Sell, 100, 5)), &mut out);
        assert_eq!(
            out,
            vec![
                Event::Accepted(OrderId(3)),
                fill(3, 2, Side::Sell, 100, 5), // 應吃到 order2 的殘量
            ]
        );
    }

    // ---- 跨執行緒整合測試:driver → SpscQueue<Command> → engine thread → SpscQueue<Event> → drain ----
    // 證明整條管線串起來、命令照序變成正確事件、ShutDown 能讓引擎 thread 收工。
    #[test]
    fn run_pipeline_end_to_end() {
        use std::{sync::Arc, thread};

        // 佇列開大到足以容納本測試所有命令/事件,避免背壓阻塞(這裡不是要測背壓)
        let cmd_q = Arc::new(SpscQueue::<Command>::with_capacity(64).unwrap());
        let evt_q = Arc::new(SpscQueue::<Event>::with_capacity(64).unwrap());

        // 引擎那條 thread:唯一擁有 OrderBook 的 single writer
        let engine_handle = {
            let cmd_q = Arc::clone(&cmd_q);
            let evt_q = Arc::clone(&evt_q);
            thread::spawn(move || {
                let mut engine = Engine::new();
                engine.run(cmd_q, evt_q);
            })
        };

        // driver:依序餵命令,最後一筆是 ShutDown
        let commands = [
            Command::NewOrder(order(1, Side::Sell, 100, 10)), // 掛單
            Command::NewOrder(order(2, Side::Buy, 100, 4)),   // 吃 4,id1 剩 6
            Command::NewOrder(order(3, Side::Buy, 100, 6)),   // 吃 6,id1 吃光
            Command::CancelOrder(OrderId(99)),                // 不存在 → Rejected
            Command::ShutDown,                                // 引擎收工
        ];
        for cmd in commands {
            while cmd_q.try_push(cmd).is_err() {
                thread::yield_now();
            }
        }

        // 引擎處理到 ShutDown 會 break,thread 結束
        engine_handle.join().unwrap();

        // 引擎已結束 → 此刻 evt_q 由本 thread 獨自 drain(仍維持 SPSC:單一 consumer)
        let mut events = Vec::new();
        while let Some(evt) = evt_q.try_pop() {
            events.push(evt);
        }

        assert_eq!(
            events,
            vec![
                Event::Accepted(OrderId(1)),
                Event::Accepted(OrderId(2)),
                fill(2, 1, Side::Buy, 100, 4),
                Event::Accepted(OrderId(3)),
                fill(3, 1, Side::Buy, 100, 6),
                Event::Rejected(OrderId(99), RejectReason::OrderNotFound),
            ]
        );
    }

    #[test]
    fn run_pipeline_rejects_oversized_without_touching_book() {
        use std::{sync::Arc, thread};

        let cmd_q = Arc::new(SpscQueue::<Command>::with_capacity(64).unwrap());
        let evt_q = Arc::new(SpscQueue::<Event>::with_capacity(64).unwrap());

        let engine_handle = {
            let cmd_q = Arc::clone(&cmd_q);
            let evt_q = Arc::clone(&evt_q);
            thread::spawn(move || {
                // ★ 改這裡:注入上限 10 的風控,不要用 Engine::new()
                let mut engine = Engine::with_risk(Risk::new(10));
                engine.run(cmd_q, evt_q);
            })
        };

        let commands = [
            Command::NewOrder(order(1, Side::Sell, 100, 10)), // 合法,掛成 ask
            Command::NewOrder(order(2, Side::Buy, 100, 11)), // ★ 超量 → 被拒,且不該吃掉上面那張 sell
            Command::NewOrder(order(3, Side::Buy, 100, 5)),  // 合法 → 吃到「沒被動過的」sell
            Command::ShutDown,
        ];
        for cmd in commands {
            while cmd_q.try_push(cmd).is_err() {
                thread::yield_now();
            }
        }

        engine_handle.join().unwrap();

        let mut events = Vec::new();
        while let Some(evt) = evt_q.try_pop() {
            events.push(evt);
        }

        assert_eq!(
            events,
            vec![
                Event::Accepted(OrderId(1)),
                Event::Rejected(OrderId(2), RejectReason::OrderTooLarge), // 風控在 pipeline 生效
                Event::Accepted(OrderId(3)),
                fill(3, 1, Side::Buy, 100, 5), // ★ id3 吃到 id1 → 證明被拒的 id2 從沒進 book
            ]
        );
    }
}
