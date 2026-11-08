//! Error único del contrato de bróker (type-erased: permite `Box<dyn Broker>`).

use thiserror::Error;

use crate::transport::error::TransportError;

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("fallo de conexión: {0}")]
    Connection(String),

    #[error("autenticación fallida: {0}")]
    Authentication(String),

    #[error("timeout esperando respuesta del bróker")]
    Timeout,

    #[error("el bróker rechazó la operación: {0}")]
    Rejected(String),

    #[error("error de dominio: {0}")]
    Domain(#[from] crate::domain::error::DomainError),

    #[error("error de transporte: {0}")]
    Transport(#[from] TransportError),

    #[error("error interno: {0}")]
    Internal(String),
}

impl BrokerError {
    /// Identificador de espera agotada en los logs de depuración.
    pub fn is_timeout(&self) -> bool {
        matches!(self, BrokerError::Timeout)
    }
}
