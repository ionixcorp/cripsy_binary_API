//! Configuración del adaptador Deriv.
//!
//! El token **nunca** se serializa ni se imprime: `Debug` lo redacta y
//! `serde` lo omite. Un `BrokerParams` serializado no transporta credenciales.

use serde::{Deserialize, Serialize};

use crate::broker::config::{AccountSelection, TransportConfig};
use crate::broker::error::BrokerError;

/// Cupos de peticiones por clase de presupuesto.
///
/// Deriv comparte el cupo entre llamadas del mismo grupo (proposal, buy y sell
/// comparten 360/min). Por defecto se usa el 80 % de lo documentado para dejar
/// margen a reconexiones.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LimitesTasa {
    /// Fracción del cupo documentado que se permite usar (0.1 - 1.0).
    pub porcentaje: f64,
    pub trading_por_min: u32,
    pub trading_por_hora: u32,
    pub balance_por_min: u32,
    pub balance_por_hora: u32,
    pub cartera_por_min: u32,
    pub cartera_por_hora: u32,
    pub resto_por_min: u32,
    pub resto_por_hora: u32,
}

impl Default for LimitesTasa {
    fn default() -> Self {
        let p = 0.8;
        Self {
            porcentaje: p,
            trading_por_min: (360.0 * p) as u32,
            trading_por_hora: (14_400.0 * p) as u32,
            balance_por_min: (100.0 * p) as u32,
            balance_por_hora: (2_000.0 * p) as u32,
            cartera_por_min: (30.0 * p) as u32,
            cartera_por_hora: (1_500.0 * p) as u32,
            resto_por_min: (220.0 * p) as u32,
            resto_por_hora: (14_400.0 * p) as u32,
        }
    }
}

/// Configuración del adaptador Deriv.
///
/// Dos modos:
/// - **Público** (sin `token`): solo mercado, sin cuenta ni órdenes.
/// - **Autenticado** (`token` + `account_id`): OTP, balance y ejecución.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivConfig {
    /// PAT o access token OAuth. Nunca se serializa ni se imprime.
    #[serde(default, skip_serializing)]
    pub token: Option<String>,
    /// App ID de la aplicación (obligatorio con PAT).
    pub app_id: Option<String>,
    /// Cuenta sobre la que se opera (`CR…`, `VRTC…`). Si falta, se elige la
    /// primera del tipo pedido en `account`.
    pub account_id: Option<String>,
    pub base_url: String,
    pub public_ws_url: String,
    pub account: AccountSelection,
    /// Divisa para propuestas; si falta se usa la del balance.
    pub currency: Option<String>,
    /// Importe de referencia para cotizar payout (Deriv cotiza por importe).
    pub stake_referencia: f64,
    pub transport: TransportConfig,
    pub limites: LimitesTasa,
}

impl Default for DerivConfig {
    fn default() -> Self {
        Self {
            token: None,
            app_id: None,
            account_id: None,
            base_url: "https://api.derivws.com".into(),
            public_ws_url: "wss://api.derivws.com/trading/v1/options/ws/public".into(),
            account: AccountSelection::Practice,
            currency: None,
            stake_referencia: 1.0,
            transport: TransportConfig::default(),
            limites: LimitesTasa::default(),
        }
    }
}

impl DerivConfig {
    /// Configuración de solo mercado público (sin credenciales).
    pub fn publico() -> Self {
        Self::default()
    }

    /// ¿Opera sin credenciales?
    pub fn es_publico(&self) -> bool {
        self.token.is_none()
    }

    /// Valida la configuración antes de abrir red.
    pub fn validar(&self) -> Result<(), BrokerError> {
        if self.base_url.trim().is_empty() {
            return Err(BrokerError::Internal("deriv: base_url vacía".into()));
        }
        if !self.public_ws_url.starts_with("wss://") {
            return Err(BrokerError::Internal(
                "deriv: public_ws_url debe ser wss://".into(),
            ));
        }
        if let Some(token) = &self.token {
            if token.trim().is_empty() {
                return Err(BrokerError::Authentication(
                    "deriv: token vacío; usa None para el modo público".into(),
                ));
            }
        }
        if self.stake_referencia <= 0.0 {
            return Err(BrokerError::Internal(
                "deriv: stake_referencia debe ser positivo".into(),
            ));
        }
        if !(0.05..=1.0).contains(&self.limites.porcentaje) {
            return Err(BrokerError::Internal(
                "deriv: limites.porcentaje fuera de 0.05..=1.0".into(),
            ));
        }
        Ok(())
    }
}

impl std::fmt::Debug for DerivConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let token = match &self.token {
            Some(t) if !t.is_empty() => format!("<redactado:{}chars>", t.len()),
            Some(_) => "<vacío>".to_string(),
            None => "<ninguno>".to_string(),
        };
        f.debug_struct("DerivConfig")
            .field("token", &token)
            .field("app_id", &self.app_id.as_deref().map(|_| "<presente>"))
            .field("account_id", &self.account_id)
            .field("base_url", &self.base_url)
            .field("public_ws_url", &self.public_ws_url)
            .field("account", &self.account)
            .field("currency", &self.currency)
            .field("stake_referencia", &self.stake_referencia)
            .field("transport", &self.transport)
            .field("limites", &self.limites)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_no_filtra_el_token() {
        let cfg = DerivConfig {
            token: Some("pat_secreto_123".into()),
            app_id: Some("12345".into()),
            ..DerivConfig::default()
        };
        let texto = format!("{cfg:?}");
        assert!(!texto.contains("pat_secreto_123"));
        assert!(texto.contains("redactado"));
    }

    #[test]
    fn serde_omite_el_token() {
        let cfg = DerivConfig {
            token: Some("pat_secreto_123".into()),
            ..DerivConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("pat_secreto_123"));
        assert!(!json.contains("token"));
    }

    #[test]
    fn publico_no_requiere_token() {
        assert!(DerivConfig::publico().es_publico());
        assert!(DerivConfig::publico().validar().is_ok());
    }

    #[test]
    fn token_vacio_es_error() {
        let cfg = DerivConfig {
            token: Some("   ".into()),
            ..DerivConfig::default()
        };
        assert!(matches!(cfg.validar(), Err(BrokerError::Authentication(_))));
    }

    #[test]
    fn cupos_por_defecto_bajo_el_limite() {
        let l = LimitesTasa::default();
        assert!(l.trading_por_min < 360);
        assert!(l.cartera_por_min < 30);
    }
}
