//! Activos, clases y estado de mercado.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    Forex,
    Crypto,
    Cfd,
    Digital,
    Turbo,
    Binary,
    /// Grupo blitz del inicializador v3 (duraciones en segundos).
    Blitz,
    Commodity,
    Etf,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScheduleWindow {
    /// Epoch segundos: apertura del mercado.
    pub open: i64,
    /// Epoch segundos: cierre del mercado.
    pub close: i64,
}

impl ScheduleWindow {
    /// ¿Está `now` dentro de la ventana?
    pub fn contains(&self, now: i64) -> bool {
        self.open <= now && now < self.close
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// Símbolo legible, p. ej. `EURUSD` o `EURUSD-OTC`.
    pub symbol: String,
    /// Nombre legible del bróker.
    pub name: String,
    pub class: AssetClass,
    /// ID numérico del bróker (obligatorio en IQ Option).
    pub opcode: u32,
    /// Estado de mercado informado por el bróker.
    pub status: MarketStatus,
    /// Payout en porcentaje (100 - comisión), si el bróker lo informa.
    pub payout_percent: Option<f64>,
    /// Sector declarado por el bróker (p. ej. `forex`, `energy`).
    pub sector: Option<String>,
    /// Horario declarado por el bróker, si lo expone.
    pub schedule: Option<Vec<ScheduleWindow>>,
    /// Metadatos específicos del tipo de opción (blitz: rollovers, minmax...).
    pub extras: Option<AssetExtras>,
}

/// Metadatos adicionales por tipo de opción (verificados en vivo). Expone lo
/// que el bróker publica en el inicializador y que ninguna API capturaba.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetExtras {
    /// Duraciones admitidas en segundos (blitz).
    pub expiration_times_secs: Option<Vec<u64>>,
    pub rollovers: Option<Vec<super::operation::RolloverParams>>,
    pub min_amount: Option<f64>,
    pub max_amount: Option<f64>,
    /// Proveedor del precio: `OTC` o mercado real.
    pub provider: Option<String>,
    /// Feed de precio que alimenta el activo (p. ej. `front.exchange_eurusdotc`).
    pub exchange: Option<String>,
    /// Antigüedad del activo (epoch segundos).
    pub start_time: Option<i64>,
    /// Ventana (s) en la que el bróker admite venta anticipada (buyback).
    pub buyback_deadtime: Option<u32>,
    /// Margen (s) antes del cierre en que la orden deja de aceptarse.
    pub deadtime: Option<u32>,
    /// ¿El activo admite venta anticipada?
    pub is_buyback: Option<bool>,
    /// ¿Participa en el ranking social de la plataforma?
    pub top_traders_enabled: Option<bool>,
    /// Decimales de precio del activo.
    pub precision: Option<u32>,
    /// Monto mínimo/máximo de operación declarados por el bróker.
    pub minimal_bet: Option<f64>,
    pub maximal_bet: Option<f64>,
}

impl Asset {
    /// ¿El mercado de este activo está abierto según la ventana horaria?
    pub fn is_open_by_schedule(&self, now: i64) -> bool {
        self.schedule
            .as_ref()
            .map(|windows| windows.iter().any(|w| w.contains(now)))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketStatus {
    Open,
    Closed,
    Suspended,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_window_contiene_now() {
        let w = ScheduleWindow {
            open: 100,
            close: 200,
        };
        assert!(w.contains(100));
        assert!(w.contains(150));
        assert!(!w.contains(200));
        assert!(!w.contains(99));
    }

    #[test]
    fn asset_is_open_by_schedule() {
        let asset = Asset {
            symbol: "EURUSD".into(),
            name: "EUR/USD".into(),
            class: AssetClass::Forex,
            opcode: 1,
            status: MarketStatus::Open,
            payout_percent: Some(80.0),
            sector: Some("forex".into()),
            schedule: Some(vec![ScheduleWindow { open: 0, close: 10 }]),
            extras: None,
        };
        assert!(asset.is_open_by_schedule(5));
        assert!(!asset.is_open_by_schedule(50));
        assert!(!Asset {
            schedule: None,
            ..asset
        }
        .is_open_by_schedule(5));
    }
}
