//! Capa de transporte: HTTP y WebSocket modernos.
//!
//! - [`http`]: cliente HTTP asíncrono (`reqwest`) con timeouts, cookies y
//!   reintentos.
//! - [`websocket`]: cliente WebSocket asíncrono (`tokio-tungstenite`) con
//!   reconexión y envelope tipado `{name, msg, request_id}`.
//! - [`router`]: correlación de respuestas por `request_id` (sin estado
//!   compartido: cada registro muere con su receptor).
//! - [`retry`]: reintentos con backoff exponencial y jitter.
//!
//! No conoce el dominio de opciones binarias: solo mueve mensajes.

pub mod error;
pub mod router;

#[cfg(feature = "async")]
pub mod retry;

#[cfg(feature = "async")]
pub mod http;
#[cfg(feature = "async")]
pub mod websocket;

pub use error::TransportError;
#[cfg(feature = "async")]
pub use retry::with_retry;
pub use router::RequestRouter;
