use crate::types::{Command, Event, Order, OrderId, Price, Qty, RejectReason, Side, Trade};
use std::vec::Vec;

pub enum DecodeError {
    InvalidFormat,
}

pub fn encode_command(cmd: &Command, bytes: &mut Vec<u8>) {
    match cmd {
        Command::NewOrder(order) => {
            bytes.push(b'O');
            bytes.extend_from_slice(&order.id.0.to_le_bytes());
            bytes.push(match order.side {
                Side::Buy => b'B',
                Side::Sell => b'S',
            });
            bytes.extend_from_slice(&order.price.0.to_le_bytes());
            bytes.extend_from_slice(&order.qty.0.to_le_bytes());
        }
        Command::CancelOrder(id) => {
            bytes.push(b'X');
            bytes.extend_from_slice(&id.0.to_le_bytes());
        }
        Command::ShutDown | Command::Noop => {}
    }
}

pub fn decode_command(bytes: &[u8]) -> Result<Command, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::InvalidFormat);
    }

    let order_type = bytes[0];

    match order_type {
        b'O' => {
            if bytes.len() < 26 {
                return Err(DecodeError::InvalidFormat);
            }

            let order_id = OrderId(u64::from_le_bytes(bytes[1..9].try_into().unwrap()));
            let side = match bytes[9] {
                b'B' => Side::Buy,
                b'S' => Side::Sell,
                _ => return Err(DecodeError::InvalidFormat),
            };
            let price = Price(i64::from_le_bytes(bytes[10..18].try_into().unwrap()));
            let qty = Qty(u64::from_le_bytes(bytes[18..26].try_into().unwrap()));

            Ok(Command::NewOrder(Order {
                id: order_id,
                side,
                price,
                qty,
            }))
        }
        b'X' => {
            if bytes.len() != 9 {
                Err(DecodeError::InvalidFormat)
            } else {
                let order_id = OrderId(u64::from_le_bytes(bytes[1..9].try_into().unwrap()));
                Ok(Command::CancelOrder(order_id))
            }
        }
        _ => Err(DecodeError::InvalidFormat),
    }
}

pub fn encode_event(evt: &Event, bytes: &mut Vec<u8>) {
    match evt {
        Event::Accepted(id) => {
            bytes.push(b'A');
            bytes.extend_from_slice(&id.0.to_le_bytes());
        }
        Event::Canceled(id) => {
            bytes.push(b'C');
            bytes.extend_from_slice(&id.0.to_le_bytes());
        }
        Event::Rejected(id, reason) => {
            bytes.push(b'J');
            bytes.extend_from_slice(&id.0.to_le_bytes());
            bytes.push(match reason {
                RejectReason::OrderTooLarge => 1,
                RejectReason::PositionLimit => 2,
                RejectReason::SelfTrade => 3,
                RejectReason::OrderNotFound => 4,
            });
        }
        Event::Fill(trade) => {
            bytes.push(b'E');
            bytes.extend_from_slice(&trade.taker.0.to_le_bytes());
            bytes.extend_from_slice(&trade.maker.0.to_le_bytes());
            bytes.push(match trade.taker_side {
                Side::Buy => b'B',
                Side::Sell => b'S',
            });
            bytes.extend_from_slice(&trade.price.0.to_le_bytes());
            bytes.extend_from_slice(&trade.qty.0.to_le_bytes());
        }
        Event::None => {}
    }
}

pub fn decode_event(bytes: &[u8]) -> Result<Event, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::InvalidFormat);
    }

    match bytes[0] {
        b'A' => {
            if bytes.len() < 9 {
                return Err(DecodeError::InvalidFormat);
            }
            let id = OrderId(u64::from_le_bytes(bytes[1..9].try_into().unwrap()));
            Ok(Event::Accepted(id))
        }
        b'C' => {
            if bytes.len() < 9 {
                return Err(DecodeError::InvalidFormat);
            }
            let id = OrderId(u64::from_le_bytes(bytes[1..9].try_into().unwrap()));
            Ok(Event::Canceled(id))
        }
        b'J' => {
            if bytes.len() < 10 {
                return Err(DecodeError::InvalidFormat);
            }
            let id = OrderId(u64::from_le_bytes(bytes[1..9].try_into().unwrap()));
            let reason = match bytes[9] {
                1 => RejectReason::OrderTooLarge,
                2 => RejectReason::PositionLimit,
                3 => RejectReason::SelfTrade,
                4 => RejectReason::OrderNotFound,
                _ => return Err(DecodeError::InvalidFormat),
            };
            Ok(Event::Rejected(id, reason))
        }
        b'E' => {
            if bytes.len() < 34 {
                return Err(DecodeError::InvalidFormat);
            }
            let taker = OrderId(u64::from_le_bytes(bytes[1..9].try_into().unwrap()));
            let maker = OrderId(u64::from_le_bytes(bytes[9..17].try_into().unwrap()));
            let taker_side = match bytes[17] {
                b'B' => Side::Buy,
                b'S' => Side::Sell,
                _ => return Err(DecodeError::InvalidFormat),
            };
            let price = Price(i64::from_le_bytes(bytes[18..26].try_into().unwrap()));
            let qty = Qty(u64::from_le_bytes(bytes[26..34].try_into().unwrap()));
            Ok(Event::Fill(Trade {
                taker,
                maker,
                taker_side,
                price,
                qty,
            }))
        }
        _ => Err(DecodeError::InvalidFormat),
    }
}
