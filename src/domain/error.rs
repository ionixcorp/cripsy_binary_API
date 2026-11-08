//! Errores del dominio.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("activo no encontrado: {0}")]
    AssetNotFound(String),

    #[error("mercado cerrado: {0}")]
    MarketClosed(String),

    #[error("saldo insuficiente: se requiere {0}, disponible {1}")]
    InsufficientBalance(f64, f64),

    #[error("monto inválido: {0}")]
    InvalidAmount(String),

    #[error("operación no encontrada: {0}")]
    OperationNotFound(String),

    #[error("strike no disponible para {0}")]
    StrikeUnavailable(String),
}
