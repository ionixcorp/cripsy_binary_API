//! cripsy_binary_api — API moderna multibroker para opciones binarias.
//!
//! Diseño (ver `docs/`):
//! - [`domain`]: modelo de dominio puro, sin conocimiento de brókers.
//! - [`broker`]: contrato común async + configuración + registro de adaptadores.
//! - [`transport`]: capa de red moderna (HTTP/WebSocket), sin lógica de negocio.
//!
//! El núcleo es async; la fachada sincrona (`broker::sync::SyncBroker`, Fase 6)
//! envuelve este mismo núcleo para consumidores bloqueantes.

#[cfg(not(any(feature = "async", feature = "blocking")))]
compile_error!(
    "cripsy-binary-api requiere una feature de consumo: `async` (por defecto) o `blocking`; \
     sin ellas no hay transporte y el crate no compila."
);

pub mod broker;
pub mod compliance;
pub mod domain;
pub mod transport;

pub use compliance::{
    mensaje_consumidor, revisar_config, revisar_transporte, ComplianceReport, API_VERSION,
    DOCS_INDEX, GARANTIA,
};
