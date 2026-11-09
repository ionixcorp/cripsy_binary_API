//! Configuración de conexión.

use serde::{Deserialize, Serialize};

/// Elección de cuenta al conectar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AccountSelection {
    /// El bróker decide (IQ Option: demo, type 4).
    #[default]
    Default,
    Practice,
    Real,
}

/// Configuración del adaptador IQ Option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IqOptionConfig {
    pub email: String,
    pub password: String,
    /// Host de autenticación (login HTTP).
    pub auth_host: String,
    /// Host de la API.
    pub api_host: String,
    /// Host del WebSocket.
    pub ws_host: String,
    pub timeout_secs: u64,
    pub account: AccountSelection,
    pub transport: TransportConfig,
}

impl Default for IqOptionConfig {
    fn default() -> Self {
        Self {
            email: String::new(),
            password: String::new(),
            auth_host: "auth.iqoption.com".into(),
            api_host: "iqoption.com".into(),
            ws_host: "iqoption.com".into(),
            timeout_secs: 30,
            account: AccountSelection::Default,
            transport: TransportConfig::default(),
        }
    }
}

/// Configuración de la capa de transporte (común a todos los brókers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportConfig {
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
    pub backoff_base_ms: u64,
    pub backoff_max_ms: u64,
    /// Capacidad de la cola de salida del socket.
    ///
    /// Acotada a propósito: si el bróker no consume, el productor espera (o
    /// falla) en vez de acumular órdenes en memoria.
    pub cola_salida: usize,
    /// Verificación TLS real; `false` solo para pruebas documentadas.
    pub verify_tls: bool,
    pub proxy: Option<String>,
    /// User-Agent opcional. Verificado en vivo 2026-10-28: el bróker **no
    /// filtra por User-Agent** (login y WSS funcionan igual con UA propio, de
    /// navegador o ajeno). Se expone solo por compatibilidad; cambiar el UA no
    /// aporta protección alguna contra baneos.
    pub user_agent: Option<String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
            max_retries: 5,
            backoff_base_ms: 250,
            backoff_max_ms: 8_000,
            cola_salida: 256,
            verify_tls: true,
            proxy: None,
            user_agent: None,
        }
    }
}

/// Parámetros de conexión type-erased (requisito de object-safety).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrokerParams {
    IqOption(IqOptionConfig),
    /// Deriv (API moderna). Disponible con el núcleo async.
    #[cfg(feature = "async")]
    Deriv(crate::broker::deriv::DerivConfig),
}
