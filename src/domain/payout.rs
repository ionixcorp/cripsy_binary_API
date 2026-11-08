//! Payout, comisiones y strike list digital.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payout {
    pub symbol: String,
    pub expiration_seconds: u64,
    /// Porcentaje de ganancia 0-100 (p. ej. 82.0 = 82%).
    pub profit_percent: f64,
    /// true = digital spot; false = binaria/turbo clásica.
    pub is_spot: bool,
    pub strike: Option<f64>,
    /// El bróker reporta el activo cerrado (ask nulo ⇒ sin payout).
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrikeQuote {
    pub strike: f64,
    /// `instrument_id` call, p. ej. `doEURUSD201907191250PT1MC11350481SPT`.
    pub call_instrument_id: Option<String>,
    pub put_instrument_id: Option<String>,
    pub call_profit_percent: Option<f64>,
    pub put_profit_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrikeList {
    pub symbol: String,
    pub duration_minutes: u64,
    pub quotes: Vec<StrikeQuote>,
    /// Epoch segundos de la captura (hora del servidor).
    pub captured_at: i64,
}

impl StrikeList {
    /// Quote más cercano a `price`; None si la lista está vacía.
    pub fn nearest(&self, price: f64) -> Option<&StrikeQuote> {
        self.quotes.iter().min_by(|a, b| {
            (a.strike - price)
                .abs()
                .partial_cmp(&(b.strike - price).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(strike: f64) -> StrikeQuote {
        StrikeQuote {
            strike,
            call_instrument_id: None,
            put_instrument_id: None,
            call_profit_percent: None,
            put_profit_percent: None,
        }
    }

    #[test]
    fn nearest_encuentra_el_mas_cercano() {
        let list = StrikeList {
            symbol: "EURUSD".into(),
            duration_minutes: 1,
            quotes: vec![quote(1.10), quote(1.25), quote(1.30)],
            captured_at: 0,
        };
        let q = list.nearest(1.24).unwrap();
        assert_eq!(q.strike, 1.25);
    }

    #[test]
    fn nearest_vacio_es_none() {
        let list = StrikeList {
            symbol: "EURUSD".into(),
            duration_minutes: 1,
            quotes: vec![],
            captured_at: 0,
        };
        assert!(list.nearest(1.0).is_none());
    }
}
