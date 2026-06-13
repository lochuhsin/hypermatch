#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Qty(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub price: Price,
    pub qty: Qty,
}
