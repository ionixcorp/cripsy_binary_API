//! Datos de plataforma que el bróker publica y que no estaban en el modelo inicial.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::asset::AssetClass;
use super::operation::{Direction, OptionType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopAsset {
    pub symbol: String,
    pub class: AssetClass,
    /// Valor de popularidad del bróker.
    pub popularity: f64,
    /// Payout spot actual (porcentaje).
    pub spot_profit: Option<f64>,
    /// Spread actual.
    pub spread: Option<f64>,
    /// Volatilidad reciente.
    pub volatility: Option<f64>,
    /// Variación del día (%).
    pub diff_day: Option<f64>,
    /// Variación de la hora (%).
    pub diff_hour: Option<f64>,
    /// Precio actual.
    pub cur_price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveDeal {
    pub symbol: String,
    pub option_type: OptionType,
    pub direction: Direction,
    pub amount: f64,
    pub created_at: i64,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub user_id: String,
    pub profit_scaled: i64,
}

/// Información financiera por activo: forma libre del bróker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinancialInfo {
    pub symbol: String,
    pub raw: Value,
}

/// Cambio de comisión de un activo (canal `commission-changed`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommissionChange {
    pub symbol: String,
    /// "binary-option", "turbo-option", "digital-option"...
    pub instrument_type: String,
    /// Comisión en puntos del bróker (el payout = 100 - comisión).
    pub commission_value: Option<f64>,
}

/// Perfil fundamental de un activo (GraphQL `GetAssetProfileInfo`).
///
/// Datos que el bróker publica y que no estaban expuestos hasta ahora: nombre
/// oficial del bróker, ticker y variaciones de precio a varios plazos.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AssetProfile {
    pub symbol: String,
    /// Nombre que el bróker muestra en la sala de trading.
    pub broker_name: String,
    pub ticker: String,
    /// Cambio del día (%).
    pub change_dtd: Option<f64>,
    /// Cambio del mes (%).
    pub change_m1: Option<f64>,
    /// Cambio del año (%).
    pub change_y1: Option<f64>,
    /// Cambio desde inicio de año (%).
    pub change_ytd: Option<f64>,
}

/// Indicadores técnicos enviados por el bróker (`trading-signals`).
///
/// El contenido es forma libre del bróker (`msg.indicators`): la API no calcula
/// indicadores, solo transporta los que el bróker entregue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TechnicalIndicators {
    pub symbol: String,
    pub indicators: Value,
}
