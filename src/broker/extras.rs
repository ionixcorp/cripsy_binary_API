//! Capacidades que solo ofrece IQ Option.
//!
//! El contrato común [`Broker`](crate::broker::Broker) describe lo que toda la
//! API garantiza: sesión, catálogo, velas, ticks, rise/fall, venta y eventos.
//! Lo que es propio de un bróker vive aquí, para no imponerlo a los demás
//! adaptadores.

use async_trait::async_trait;

use crate::broker::error::BrokerError;
use crate::broker::trait_::Broker;
use crate::domain::{
    AssetProfile, BuyBlitzRequest, BuyDigitalRequest, OrderId, TechnicalIndicators,
};

/// Capacidades exclusivas de IQ Option.
///
/// Se implementa además como métodos inherentes en [`IqOptionBroker`], de modo
/// que quien use el tipo concreto no necesita importar este trait.
#[async_trait]
pub trait IqOptionExtras: Broker {
    /// Indicadores técnicos que el bróker envía para un activo.
    async fn get_technical_indicators(
        &self,
        symbol: &str,
    ) -> Result<TechnicalIndicators, BrokerError>;

    /// Perfil fundamental del activo (nombre, ticker y variaciones).
    async fn get_asset_profile(&self, symbol: &str) -> Result<AssetProfile, BrokerError>;

    /// Stream de operaciones de terceros (`live-deal`).
    async fn subscribe_live_deals(&self, symbol: &str) -> Result<(), BrokerError>;

    /// Cambios de comisión por tipo de instrumento.
    async fn subscribe_commission_changes(&self) -> Result<(), BrokerError>;

    /// Compra BLITZ (duración en segundos y payout explícito).
    async fn buy_blitz(&self, req: BuyBlitzRequest) -> Result<OrderId, BrokerError>;

    /// Compra digital por strike elegido.
    async fn buy_digital_spot(&self, req: BuyDigitalRequest) -> Result<OrderId, BrokerError>;
}

use crate::broker::iq_option::IqOptionBroker;

#[async_trait]
impl IqOptionExtras for IqOptionBroker {
    async fn get_technical_indicators(
        &self,
        symbol: &str,
    ) -> Result<TechnicalIndicators, BrokerError> {
        self.get_technical_indicators(symbol).await
    }

    async fn get_asset_profile(&self, symbol: &str) -> Result<AssetProfile, BrokerError> {
        self.get_asset_profile(symbol).await
    }

    async fn subscribe_live_deals(&self, symbol: &str) -> Result<(), BrokerError> {
        self.subscribe_live_deals(symbol).await
    }

    async fn subscribe_commission_changes(&self) -> Result<(), BrokerError> {
        self.subscribe_commission_changes().await
    }

    async fn buy_blitz(&self, req: BuyBlitzRequest) -> Result<OrderId, BrokerError> {
        self.buy_blitz(req).await
    }

    async fn buy_digital_spot(&self, req: BuyDigitalRequest) -> Result<OrderId, BrokerError> {
        self.buy_digital_spot(req).await
    }
}
