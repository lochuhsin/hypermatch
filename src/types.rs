#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(pub i64);

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Qty(pub u64);

impl Qty {
    pub fn checked_sub(self, rhs: Qty) -> Option<Qty> {
        self.0.checked_sub(rhs.0).map(Qty)
    }
}
// memorize this
impl TryFrom<i64> for Qty {
    type Error = ();
    fn try_from(v: i64) -> Result<Self, Self::Error> {
        u64::try_from(v).map(Qty).map_err(|_| ())
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trade {
    pub taker: OrderId,
    pub maker: OrderId,
    pub taker_side: Side,
    pub price: Price,
    pub qty: Qty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Command {
    NewOrder(Order),
    CancelOrder(OrderId),
    ShutDown,

    #[default]
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Event {
    Accepted(OrderId),
    Fill(Trade),
    Rejected(OrderId, RejectReason),
    Canceled(OrderId),

    #[default]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    OrderTooLarge,
    PositionLimit,
    SelfTrade,
    OrderNotFound,
}
