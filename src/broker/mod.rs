//! Contrato común de bróker, configuración y registro de adaptadores.

pub mod config;
pub mod error;
#[cfg(feature = "async")]
pub mod extras;
pub mod historico;
pub mod registry;
pub mod session;
pub mod trait_;

#[cfg(feature = "async")]
pub mod sync;

#[cfg(feature = "async")]
pub mod deriv;

#[cfg(feature = "async")]
pub mod iq_option;

pub use config::{AccountSelection, BrokerParams, IqOptionConfig, TransportConfig};
#[cfg(feature = "async")]
pub use deriv::{DerivBroker, DerivConfig};
pub use error::BrokerError;
#[cfg(feature = "async")]
pub use extras::IqOptionExtras;
pub use historico::{descargar, paginas_estimadas, ultimo_cierre, Ventana, VELAS_POR_PAGINA};
pub use iq_option::IqOptionBroker;
pub use registry::BrokerRegistry;
pub use session::Session;
pub use trait_::{Broker, CandleStream, EventStream};
