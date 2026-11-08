//! Órdenes, operaciones y resultados.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Call,
    Put,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    Binary,
    Digital,
    /// BLITZ: duraciones en segundos (protocolo v2, `option_type_id: 12`).
    Blitz,
}

/// Compra de binaria/turbo clásica.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuyBinaryRequest {
    pub symbol: String,
    pub amount: f64,
    pub direction: Direction,
    /// Duración en minutos; el adaptador decide turbo (≤5) vs binary (>5).
    pub expiration_minutes: u64,
    pub option_type: OptionType,
}

/// Compra digital por strike explícito (elegido de una `StrikeList`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuyDigitalRequest {
    pub symbol: String,
    pub amount: f64,
    pub direction: Direction,
    pub duration_minutes: u64,
    /// Precio de strike elegido de la `StrikeList`.
    pub strike: f64,
}

/// Compra BLITZ (protocolo v2): duración en segundos y payout explícito.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuyBlitzRequest {
    pub symbol: String,
    pub amount: f64,
    pub direction: Direction,
    /// Duración en segundos (admitidas por el activo: `option.expiration_times`).
    pub expiration_size_secs: u64,
    /// Payout en porcentaje (100 - comisión), del catálogo.
    pub profit_percent: f64,
}

/// Parámetros de rollover automático de una orden BLITZ.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RolloverParams {
    pub deadtime: u32,
    pub limit: u32,
    pub offset: u32,
    pub offset_from_expiration: u32,
    pub expiration_size: u32,
}

/// Identificador de orden opaco (el formato es del bróker).
pub type OrderId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationStatus {
    Open,
    Closed,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OperationResult {
    /// Ganancia neta (ya descontada la inversión).
    Win(f64),
    /// Pérdida igual al monto invertido.
    Loss(f64),
    /// Empate: devolución del monto.
    Equal(f64),
    /// La operación sigue abierta.
    Pending,
}

impl OperationResult {
    /// Resultado neto contable: win/equal → delta positivo o cero; loss → negativo.
    pub fn net(&self) -> Option<f64> {
        match self {
            OperationResult::Win(n) => Some(*n),
            OperationResult::Loss(amount) => Some(-*amount),
            OperationResult::Equal(_) => Some(0.0),
            OperationResult::Pending => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    pub id: OrderId,
    pub symbol: String,
    pub amount: f64,
    pub direction: Direction,
    pub option_type: OptionType,
    pub status: OperationStatus,
    pub result: Option<OperationResult>,
    pub payout_percent: Option<f64>,
    pub opened_at: i64,
    pub closed_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_de_resultados() {
        assert_eq!(OperationResult::Win(8.2).net(), Some(8.2));
        assert_eq!(OperationResult::Loss(10.0).net(), Some(-10.0));
        assert_eq!(OperationResult::Equal(10.0).net(), Some(0.0));
        assert_eq!(OperationResult::Pending.net(), None);
    }

    #[test]
    fn request_binaria_serializa() {
        let r = BuyBinaryRequest {
            symbol: "EURUSD".into(),
            amount: 10.0,
            direction: Direction::Call,
            expiration_minutes: 1,
            option_type: OptionType::Binary,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"direction\":\"Call\""));
        assert!(json.contains("\"expiration_minutes\":1"));
    }
}
