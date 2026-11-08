//! Adaptador IQ Option.
//!
//! - [`session`]: struct de sesión, constructor `nuevo` y tarea del socket.
//! - [`client`]: implementación del trait [`Broker`](crate::broker::trait_::Broker).
//! - [`mapper`]: conversiones dominio ↔ protocolo.
//! - [`endpoints`]: nombres de mensaje y URLs verificados en vivo.

pub mod client;
pub mod endpoints;
pub mod mapper;
pub mod session;

pub use session::IqOptionBroker;
