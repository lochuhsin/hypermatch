use crate::types::{Order, OrderId, Price, Side};
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Default)]
struct Level {
    pub orders: VecDeque<Order>,
}

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
