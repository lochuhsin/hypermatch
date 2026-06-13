use crate::types::{Order, OrderId, Price, Side};
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Default)]
struct Level {
    pub orders: VecDeque<Order>,
}

#[derive(Default)]
pub struct OrderBook {
    bids: BTreeMap<Price, Level>,
    asks: BTreeMap<Price, Level>,

    locations: HashMap<OrderId, (Price, Side)>, // low performance, at least O(n)
}

impl OrderBook {
    pub fn new() -> Self {
        OrderBook {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            locations: HashMap::new(),
        }
    }

    pub fn add(&mut self, order: Order) {
        match order.side {
            Side::Buy => {
                self.bids
                    .entry(order.price)
                    .or_default()
                    .orders
                    .push_back(order);

                self.locations.insert(order.id, (order.price, Side::Buy));
            }

            Side::Sell => {
                self.asks
                    .entry(order.price)
                    .or_default()
                    .orders
                    .push_back(order);

                self.locations.insert(order.id, (order.price, Side::Sell));
            }
        }
    }

    pub fn best_bid(&self) -> Option<Price> {
        self.bids.last_key_value().map(|(price, _level)| *price)
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.asks.first_key_value().map(|(price, _level)| *price)
    }

    // low performance, at least O(n)
    pub fn cancel(&mut self, order_id: OrderId) -> Option<Order> {
        let (price, side) = self.locations.remove(&order_id)?;

        let book = match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        let level = book.get_mut(&price).expect("logic error: level not found");
        let pos = level.orders.iter().position(|o| o.id == order_id)?;
        self.locations.remove(&order_id);
        let order = level.orders.remove(pos);

        if level.orders.is_empty() {
            book.remove(&price);
        };

        order
    }
}

#[cfg(test)]
mod tests {
    use super::*; // 把外層模組(OrderBook 等)引進來
    use crate::types::Qty; // ⚠️ Qty 在 order_book.rs 還沒 import,測試要造 Order 會用到

    // 小幫手:一行造一張單,省得每次寫一長串
    fn order(id: u64, side: Side, price: i64, qty: u64) -> Order {
        Order {
            id: OrderId(id),
            side,
            price: Price(price),
            qty: Qty(qty),
        }
    }

    #[test] // ← 每個測試函式都要這個標記
    fn empty_book_has_no_best() {
        let book = OrderBook::new();
        assert_eq!(book.best_bid(), None); // assert_eq! 失敗會印出兩邊的值
        assert_eq!(book.best_ask(), None);
    }

    // 👇 其餘 case 換你寫

    #[test]
    fn add_bid_sets_best_bid() {
        let mut book = OrderBook::new();
        let bid1 = order(1, Side::Buy, 100, 10);

        book.add(bid1);

        assert_eq!(book.best_bid(), Some(Price(100)));
    }

    #[test]
    fn add_ask_sets_best_ask() {
        let mut book = OrderBook::new();
        let ask1 = order(1, Side::Sell, 100, 10);

        book.add(ask1);

        assert_eq!(book.best_ask(), Some(Price(100)));
    }

    #[test]
    fn two_bids_pick_higher() {
        let mut book = OrderBook::new();

        book.add(order(1, Side::Buy, 100, 10));
        book.add(order(2, Side::Buy, 101, 20));

        assert_eq!(book.best_bid(), Some(Price(101)));
    }

    #[test]
    fn two_asks_pick_lower() {
        let mut book = OrderBook::new();

        book.add(order(1, Side::Sell, 100, 10));
        book.add(order(2, Side::Sell, 99, 20));

        assert_eq!(book.best_ask(), Some(Price(99)));
    }

    #[test]
    fn cancel_removes_order_and_updates_best_if_needed() {
        let mut book = OrderBook::new();

        book.add(order(1, Side::Buy, 100, 10));
        book.add(order(2, Side::Buy, 101, 20));

        assert!(book.cancel(OrderId(1)).is_some());

        assert_eq!(book.best_bid(), Some(Price(101)));

        assert!(!book.locations.contains_key(&OrderId(1)));

        assert!(book.cancel(OrderId(999)).is_none());
    }
}
