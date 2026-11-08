//! Errores de la capa de transporte.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("error de red: {0}")]
    Io(String),

    #[error("url inválida: {0}")]
    Url(String),

    #[error("timeout en fase '{phase}' tras {ms} ms")]
    Timeout { phase: String, ms: u64 },

    #[error("error TLS: {0}")]
    Tls(String),

    #[error("websocket cerrado por el par")]
    Closed,

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("no se pudo codificar/decodificar el mensaje: {0}")]
    Codec(String),
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            TransportError::Timeout {
                phase: "http".into(),
                ms: 0,
            }
        } else {
            TransportError::Io(e.to_string())
        }
    }
}
