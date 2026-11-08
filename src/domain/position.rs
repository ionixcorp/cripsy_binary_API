//! Posiciones (cfd/forex/crypto/digital) e histórico.

use serde::{Deserialize, Serialize};

use crate::domain::operation::OrderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub id: OrderId,
    /// Tipo de instrumento del bróker, p. ej. `digital-option`, `forex`, `crypto`.
    pub instrument_type: String,
    pub symbol: String,
    pub side: Side,
    pub amount: f64,
    pub open_price: Option<f64>,
    pub pnl: Option<f64>,
    pub invested: Option<f64>,
    pub open_at: i64,
    pub close_at: Option<i64>,
    pub leverage: Option<u32>,
}

impl Position {
    pub fn is_closed(&self) -> bool {
        self.close_at.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionHistory {
    pub positions: Vec<Position>,
    pub offset: usize,
    pub total: Option<usize>,
}
