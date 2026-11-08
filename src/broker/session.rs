//! Sesión activa de un bróker.

use serde::{Deserialize, Serialize};

use crate::domain::AccountType;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub broker_name: String,
    /// Epoch segundos de conexión.
    pub connected_at: i64,
    pub account_id: Option<String>,
    /// Balance activo: obligatorio para enviar órdenes (IQ Option lo exige).
    pub balance_id: Option<i64>,
    pub account_type: AccountType,
    /// Offset (ms) entre el reloj local y el del bróker (timeSync).
    pub server_time_offset_ms: i64,
}

impl Session {
    /// Hora actual según el reloj del bróker (epoch ms).
    pub fn broker_time_ms(&self, now_ms: i64) -> i64 {
        now_ms + self.server_time_offset_ms
    }

    /// Hora actual según el reloj del bróker (epoch segundos).
    pub fn broker_time_secs(&self, now_ms: i64) -> i64 {
        self.broker_time_ms(now_ms) / 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_de_tiempo() {
        let s = Session {
            broker_name: "iq_option".into(),
            connected_at: 0,
            account_id: None,
            balance_id: Some(7),
            account_type: AccountType::Practice,
            server_time_offset_ms: 250,
        };
        assert_eq!(s.broker_time_ms(1000), 1250);
        assert_eq!(s.broker_time_secs(1000), 1);
    }
}
