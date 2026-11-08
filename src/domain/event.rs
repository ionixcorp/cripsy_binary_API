//! Eventos del ciclo de vida: el reemplazo tipado del polling.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::account::AccountBalance;
use super::candle::Candle;
use super::operation::{Operation, OperationResult, OrderId};
use super::payout::Payout;
use super::platform::{CommissionChange, LiveDeal, TopAsset};

/// Tick de precio (stream nativo): densidad intrasegundo sin polling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PriceTick {
    pub price: f64,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    /// Epoch milisegundos del bróker.
    pub at_ms: i64,
}

/// Evento emitido por la sesión de un bróker.
///
/// Cada solicitud de datos **push** (resultado de orden, cambio de balance,
/// payout en vivo) llega aquí; la consulta por polling (`get_operation_result`)
/// queda solo como respaldo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    OrderOpened(Operation),
    OrderClosed {
        id: OrderId,
        result: OperationResult,
    },
    PositionChanged(super::position::Position),
    BalanceChanged(AccountBalance),
    Candle(Candle),
    /// Tick de precio en vivo (stream `candles-generated`, campo `value`).
    PriceTick {
        symbol: String,
        tick: PriceTick,
    },
    PayoutChanged(Payout),
    /// Humor del mercado: porcentaje de call (0-100).
    TradersMoodChanged {
        symbol: String,
        call_percent: f64,
    },
    /// Popularidad del activo en la plataforma.
    TopAssetUpdated(TopAsset),
    /// Indicadores técnicos del bróker (forma libre, no recalculados).
    TechnicalIndicators {
        symbol: String,
        indicators: Value,
    },
    /// Operación de un tercero capturada en vivo.
    LiveDeal(LiveDeal),
    /// Comisión de un activo actualizada por el bróker.
    CommissionChanged(CommissionChange),
    /// Pérdida de continuidad: el canal descartó `count` eventos.
    ///
    /// El stream continúa entregando los eventos posteriores. Se emite para
    /// distinguir «me quedé atrás» de «la sesión terminó».
    EventsLost {
        count: u64,
    },
}
