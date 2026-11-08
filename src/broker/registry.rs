//! Registro de adaptadores disponibles.

#[cfg(feature = "async")]
use crate::broker::config::BrokerParams;
#[cfg(feature = "async")]
use crate::broker::error::BrokerError;
#[cfg(feature = "async")]
use crate::broker::trait_::Broker;

/// Nombres de los adaptadores registrados.
pub struct BrokerRegistry;

impl BrokerRegistry {
    /// Adaptadores disponibles.
    pub fn list() -> Vec<&'static str> {
        vec!["iq_option"]
    }

    /// Existe el adaptador con este nombre.
    pub fn contains(name: &str) -> bool {
        Self::list().contains(&name)
    }

    /// Crea y conecta un adaptador async.
    #[cfg(feature = "async")]
    pub async fn create_async(params: BrokerParams) -> Result<Box<dyn Broker>, BrokerError> {
        match params {
            BrokerParams::IqOption(_) => {
                let broker = <crate::broker::IqOptionBroker as Broker>::connect(params).await?;
                Ok(Box::new(broker))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registro_incluye_iq_option() {
        assert_eq!(BrokerRegistry::list(), vec!["iq_option"]);
        assert!(BrokerRegistry::contains("iq_option"));
        assert!(!BrokerRegistry::contains("inexistente"));
    }
}
