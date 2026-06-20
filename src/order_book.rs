use crate::types::{Order, OrderId, Price, Qty, Side, Trade};
use std::{
    cmp::min,
    collections::{BTreeMap, HashMap, VecDeque},
};

#[derive(Default)]
struct Level {
    pub orders: VecDeque<Order>,
}

#[derive(Default)]
pub struct OrderBook {
    bids: BTreeMap<Price, Level>, // buyer
    asks: BTreeMap<Price, Level>, // seller

    locations: HashMap<OrderId, (Price, Side)>, // low performance, at least O(n)
}

// needs to optimize this orderbook
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

    // Price belongs to Maker
    pub fn submit(&mut self, taker: Order) -> Vec<Trade> {
        /*
        Try match first then add rest to book.
         */

        match taker.side {
            Side::Buy => self.buy_match(taker),
            Side::Sell => self.sell_match(taker),
        }
    }

    pub fn buy_match(&mut self, incoming: Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let mut taker = incoming;

        while taker.qty > Qty::default() {
            match self.best_ask() {
                Some(price) => {
                    if taker.price < price {
                        break;
                    }

                    let level = self
                        .asks
                        .get_mut(&price)
                        .expect("shouldn't happen logic error");
                    let mut maker = level
                        .orders
                        .pop_front()
                        .expect("shouldn't happen, logic error");

                    trades.push(Trade {
                        taker: taker.id,
                        maker: maker.id,
                        taker_side: taker.side,
                        price: maker.price,
                        qty: min(taker.qty, maker.qty),
                    });

                    let qty = maker.qty;

                    maker.qty = maker.qty.checked_sub(taker.qty).unwrap_or_default();
                    taker.qty = taker.qty.checked_sub(qty).unwrap_or_default();

                    if maker.qty > Qty::default() {
                        level.orders.push_front(maker);
                    } else {
                        self.locations.remove(&maker.id);
                    }

                    if level.orders.is_empty() {
                        self.asks.remove(&price);
                    }
                }
                None => break,
            }
        }

        if taker.qty > Qty::default() {
            self.add(taker);
        }

        trades
    }

    pub fn sell_match(&mut self, incoming: Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let mut taker = incoming;

        while taker.qty > Qty(0) {
            match self.best_bid() {
                Some(price) => {
                    if taker.price > price {
                        break;
                    }

                    let level = self
                        .bids
                        .get_mut(&price)
                        .expect("logic error, shouldn't happen");

                    let mut maker = level
                        .orders
                        .pop_front()
                        .expect("logic error, shouldn't happen");

                    trades.push(Trade {
                        taker: taker.id,
                        maker: maker.id,
                        taker_side: taker.side,
                        price,
                        qty: min(maker.qty, taker.qty),
                    });

                    let t_qty = taker.qty;
                    taker.qty = taker.qty.checked_sub(maker.qty).unwrap_or_default();
                    maker.qty = maker.qty.checked_sub(t_qty).unwrap_or_default();

                    if maker.qty > Qty(0) {
                        level.orders.push_front(maker);
                    } else {
                        self.locations.remove(&maker.id);
                    };

                    if level.orders.is_empty() {
                        self.bids.remove(&price);
                    };
                }
                None => break,
            }
        }

        if taker.qty > Qty(0) {
            self.add(taker);
        }

        trades
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Qty;

    fn order(id: u64, side: Side, price: i64, qty: u64) -> Order {
        Order {
            id: OrderId(id),
            side,
            price: Price(price),
            qty: Qty(qty),
        }
    }

    #[test]
    fn empty_book_has_no_best() {
        let book = OrderBook::new();
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
    }

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

        assert!(book.cancel(OrderId(2)).is_some());

        assert_eq!(book.best_bid(), Some(Price(100)));

        assert!(!book.locations.contains_key(&OrderId(2)));

        assert!(book.cancel(OrderId(999)).is_none());
    }

    #[test]
    fn cancel_a_sell() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 10));
        assert!(book.cancel(OrderId(1)).is_some());
        assert_eq!(book.best_ask(), None);
    }

    // ---------- matching: buy side ----------
    #[test]
    fn submit_buy_no_cross_rests() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 101, 10));
        let trades = book.submit(order(2, Side::Buy, 100, 5));
        assert!(trades.is_empty());
        assert_eq!(book.best_bid(), Some(Price(100))); // taker 掛成 bid
        assert_eq!(book.best_ask(), Some(Price(101))); // ask 沒被動
    }

    #[test]
    fn submit_buy_full_fill() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 10));
        let trades = book.submit(order(2, Side::Buy, 100, 10));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, Price(100));
        assert_eq!(trades[0].qty, Qty(10));
        assert_eq!(book.best_ask(), None);
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn submit_buy_maker_bigger_partial() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 6)); // maker 有 6
        let trades = book.submit(order(2, Side::Buy, 100, 4)); // taker 要 4
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, Qty(4)); // fill = 4
        assert_eq!(trades[0].price, Price(100));
        assert_eq!(book.best_ask(), Some(Price(100))); // maker 剩 2,還在
        assert_eq!(book.best_bid(), None); // taker 填滿
    }

    #[test]
    fn submit_buy_taker_bigger_partial() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 10));
        let trades = book.submit(order(2, Side::Buy, 100, 15));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, Qty(10));
        assert_eq!(book.best_ask(), None); // maker 被吃光
        assert_eq!(book.best_bid(), Some(Price(100))); // 殘量 5 掛成 bid
    }

    #[test]
    fn submit_buy_sweeps_levels() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 5));
        book.add(order(2, Side::Sell, 101, 5));
        let trades = book.submit(order(3, Side::Buy, 101, 10));
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].price, Price(100)); // 先吃便宜的
        assert_eq!(trades[1].price, Price(101));
        assert_eq!(book.best_ask(), None);
    }

    #[test]
    fn submit_buy_time_priority() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 5)); // 較舊
        book.add(order(2, Side::Sell, 100, 5)); // 較新,同價
        let trades = book.submit(order(3, Side::Buy, 100, 5));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].maker, OrderId(1)); // 先吃最舊
        assert_eq!(book.best_ask(), Some(Price(100))); // id2 還在
    }

    #[test]
    fn submit_buy_exec_at_maker_price() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 10));
        let trades = book.submit(order(2, Side::Buy, 105, 10)); // 願付到 105
        assert_eq!(trades[0].price, Price(100)); // 成交在 maker 的 100
    }

    // ---------- matching: sell side ----------
    #[test]
    fn submit_sell_full_fill() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Buy, 100, 10));
        let trades = book.submit(order(2, Side::Sell, 100, 10));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, Price(100));
        assert_eq!(trades[0].qty, Qty(10));
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn submit_sell_time_priority_after_partial() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Buy, 100, 10)); // 較舊
        book.add(order(2, Side::Buy, 100, 10)); // 較新
        book.submit(order(3, Side::Sell, 100, 4)); // 部分吃 id1,id1 剩 6 應留隊首
        let trades = book.submit(order(4, Side::Sell, 100, 6)); // 應繼續吃 id1
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].maker, OrderId(1));
    }

    #[test]
    fn submit_locations_cleaned_after_fill() {
        let mut book = OrderBook::new();
        book.add(order(1, Side::Sell, 100, 10));
        book.submit(order(2, Side::Buy, 100, 10)); // 完全吃掉 maker 1
        assert!(!book.locations.contains_key(&OrderId(1)));
    }
}
