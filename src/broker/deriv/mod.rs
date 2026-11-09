//! Adaptador Deriv (API moderna: REST + OTP + WebSocket JSON).
//!
//! Modo público sin credenciales para mercado; modo autenticado para balance y
//! ejecución. Ver `docs_expantion/deriv/` (interno) para el diseño y las
//! fidelidades pendientes del contrato.

pub mod config;
pub mod protocol;
pub mod session;

pub use config::{DerivConfig, LimitesTasa};
pub use session::DerivBroker;
