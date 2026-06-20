use crate::order_book::OrderBook;
use crate::types::{Command, Event};

pub struct Engine {
    order_book: OrderBook,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            order_book: OrderBook::default(),
        }
    }

    pub fn apply(&mut self, cmd: Command, out: &mut Vec<Event>) {
        match cmd {
            Command::Noop => {}
            Command::CancelOrder(id) => {
                let outcome = self.order_book.cancel(id);
                match outcome {
                    Some(o) => out.push(Event::Canceled(o.id)),
                    None => out.push(Event::Rejected(id)),
                }
            }
            Command::NewOrder(order) => {
                out.push(Event::Accepted(order.id));
                let trades = self.order_book.submit(order);
                for trade in trades.iter() {
                    out.push(Event::Fill(*trade));
                }
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
        assert_eq!(out, vec![Event::Rejected(OrderId(999))]);
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
}
