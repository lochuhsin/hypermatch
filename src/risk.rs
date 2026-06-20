use crate::types::{Order, RejectReason};

#[derive(Debug)]
pub struct Risk {
    max_order_qty: u64,
}

impl Risk {
    pub fn new(max_order_qty: u64) -> Self {
        Self { max_order_qty }
    }

    pub fn check(&self, order: &Order) -> Option<RejectReason> {
        if order.qty.0 > self.max_order_qty {
            return Some(RejectReason::OrderTooLarge);
        }
        None
    }
}

impl Default for Risk {
    fn default() -> Self {
        Self {
            max_order_qty: u64::MAX,
        }
    }
}
