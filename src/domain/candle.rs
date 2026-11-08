//! Velas: históricas y streaming.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    pub from: i64,
    pub to: i64,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: Option<f64>,
}

impl Candle {
    /// ¿La vela es consistente (high >= low, precios positivos)?
    pub fn is_consistent(&self) -> bool {
        self.high >= self.low && self.open >= 0.0 && self.close >= 0.0 && self.from <= self.to
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandleRequest {
    pub symbol: String,
    /// Segundos por vela (1, 5, 60, 300, ...).
    pub interval: u64,
    pub count: usize,
    pub end_time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandleSubscribe {
    pub symbol: String,
    pub interval: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vela_consistente() {
        let ok = Candle {
            from: 1,
            to: 60,
            open: 1.0,
            close: 1.2,
            high: 1.3,
            low: 0.9,
            volume: Some(5.0),
        };
        let mala = Candle { low: 2.0, ..ok };
        assert!(ok.is_consistent());
        assert!(!mala.is_consistent());
    }

    #[test]
    fn request_serializa() {
        let r = CandleRequest {
            symbol: "EURUSD".into(),
            interval: 60,
            count: 100,
            end_time: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"symbol\":\"EURUSD\""));
        assert!(json.contains("\"interval\":60"));
    }
}
