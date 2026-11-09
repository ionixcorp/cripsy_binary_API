//! Payout, comisiones y strike list digital.

use serde::{Deserialize, Serialize};

use super::operation::OptionType;

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

/// Cotización de payout para una combinación concreta.
///
/// A diferencia de [`Payout`], distingue “sin cotización” (`available: false`)
/// de un payout real. `captured_at == 0` indica que no se capturó una
/// cotización del bróker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayoutQuote {
    pub symbol: String,
    pub option_type: OptionType,
    pub expiration_seconds: u64,
    /// Porcentaje de ganancia 0-100; `None` si no hay cotización.
    pub profit_percent: Option<f64>,
    pub min_amount: Option<f64>,
    pub max_amount: Option<f64>,
    /// `false` si el bróker no ofrece la combinación o no hay dato.
    pub available: bool,
    /// Epoch segundos de la captura; `0` si no hubo captura.
    pub captured_at: i64,
}

impl PayoutQuote {
    /// Cotización ausente: no inventa `0.0` ni un payout.
    pub fn unavailable(
        symbol: impl Into<String>,
        option_type: OptionType,
        expiration_seconds: u64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            option_type,
            expiration_seconds,
            profit_percent: None,
            min_amount: None,
            max_amount: None,
            available: false,
            captured_at: 0,
        }
    }
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

    #[test]
    fn payout_no_disponible_no_fabrica_porcentaje() {
        let quote = PayoutQuote::unavailable("EURUSD", OptionType::Binary, 60);
        assert!(!quote.available);
        assert_eq!(quote.profit_percent, None);
        assert_eq!(quote.min_amount, None);
        assert_eq!(quote.max_amount, None);
        assert_eq!(quote.captured_at, 0);

        let json = serde_json::to_string(&quote).unwrap();
        assert!(json.contains("\"available\":false"));
        assert!(json.contains("\"profit_percent\":null"));
    }
}
