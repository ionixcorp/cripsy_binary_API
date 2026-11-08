//! Contrato común de bróker, configuración y registro de adaptadores.

pub mod config;
pub mod error;
pub mod historico;
pub mod registry;
pub mod session;
pub mod trait_;

#[cfg(feature = "async")]
pub mod sync;

#[cfg(feature = "async")]
pub mod iq_option;

pub use config::{AccountSelection, BrokerParams, IqOptionConfig, TransportConfig};
pub use error::BrokerError;
pub use historico::{descargar, paginas_estimadas, ultimo_cierre, Ventana, VELAS_POR_PAGINA};
#[cfg(feature = "async")]
pub use iq_option::IqOptionBroker;
pub use registry::BrokerRegistry;
pub use session::Session;
pub use trait_::{Broker, CandleStream, EventStream};
