//! Trait `Broker`: interfaz común async (object-safe vía `async_trait`).

use async_trait::async_trait;
use futures_core::Stream;

use crate::domain::{
    AccountType, Asset, Balance, BuyBinaryRequest, Candle, CandleRequest, CandleSubscribe, Event,
    MarketStatus, OperationResult, OptionType, OrderId, Payout, PositionHistory,
};

use super::config::BrokerParams;
use super::error::BrokerError;
use super::extras::IqOptionExtras;
use super::session::Session;

/// Stream async de velas.
pub type CandleStream = std::pin::Pin<Box<dyn Stream<Item = Result<Candle, BrokerError>> + Send>>;

/// Stream async de eventos del ciclo de vida.
pub type EventStream = std::pin::Pin<Box<dyn Stream<Item = Event> + Send>>;

/// Contrato que todo adaptador de bróker cumple.
///
/// Diseño type-erased a propósito: sin associated types para que
/// `Box<dyn Broker>` y `MultiBroker` compilen (ver `docs/01_introduccion.es.md`).
#[async_trait]
pub trait Broker: Send + Sync {
    /// Nombre estable del adaptador, p. ej. `iq_option`.
    fn name(&self) -> &'static str;

    /// Constructor de la sesión: autentica y deja el bróker operativo.
    async fn connect(params: BrokerParams) -> Result<Self, BrokerError>
    where
        Self: Sized;

    async fn disconnect(&mut self) -> Result<(), BrokerError>;
    async fn is_connected(&self) -> bool;

    /// Metadatos de la sesión activa (copia: la sesión se actualiza en caliente).
    async fn session(&self) -> Result<Session, BrokerError>;

    // -------- Cuenta y balance --------

    async fn get_balance(&self) -> Result<Balance, BrokerError>;
    async fn get_account_type(&self) -> Result<AccountType, BrokerError>;
    async fn switch_account(&mut self, kind: AccountType) -> Result<(), BrokerError>;

    // -------- Activos y mercado --------

    async fn list_assets(&self) -> Result<Vec<Asset>, BrokerError>;
    async fn asset_status(&self, symbol: &str) -> Result<MarketStatus, BrokerError>;

    // -------- Velas --------

    async fn get_candles(&self, req: CandleRequest) -> Result<Vec<Candle>, BrokerError>;
    async fn subscribe_candles(&self, req: CandleSubscribe) -> Result<CandleStream, BrokerError>;

    /// Historial de posiciones cerradas (cartera): para analítica de cuenta.
    /// `limit` ≤ 100 por página, `offset` para paginar.
    async fn get_position_history(
        &self,
        symbol: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PositionHistory, BrokerError>;

    // -------- Payout --------

    /// Payout **publicado en el catálogo** para el activo.
    ///
    /// `expiration_seconds` se devuelve en el resultado pero no altera la
    /// consulta: no se pide una cotización por expiración ni por strike. Los
    /// cambios en vivo llegan por `Event::PayoutChanged`.
    async fn get_payout(
        &self,
        symbol: &str,
        expiration_seconds: u64,
    ) -> Result<Payout, BrokerError>;

    // -------- Indicadores técnicos del bróker --------

    // -------- Órdenes --------

    /// Compra clásica (turbo o binary según la duración).
    ///
    /// BLITZ tiene su propia petición: [`Broker::buy_blitz`].
    async fn buy_binary(&self, req: BuyBinaryRequest) -> Result<OrderId, BrokerError>;

    /// Venta anticipada (buyback) de una orden abierta. El bróker la admite
    /// dentro de la ventana `buyback_deadtime` del activo; si la orden ya no
    /// es vendible responde con su mensaje de rechazo.
    async fn sell_option(&self, order_id: &OrderId) -> Result<(), BrokerError>;

    /// Resultado leído de la caché local (alimentada por `events()`).
    ///
    /// No consulta al bróker: devuelve `OperationResult::Pending` mientras el
    /// cierre no haya llegado por eventos.
    async fn get_operation_result(
        &self,
        order_id: &OrderId,
    ) -> Result<OperationResult, BrokerError>;

    /// Acceso a las capacidades exclusivas de IQ Option, si el adaptador las
    /// ofrece. Los adaptadores que no las implementan devuelven `None`.
    fn iqoption_extras(&self) -> Option<&dyn IqOptionExtras> {
        None
    }

    /// Suscribe el stream **nativo** de ticks de un activo.
    ///
    /// Los ticks llegan por `Event::PriceTick`. Un adaptador sin stream nativo
    /// hereda el rechazo explícito; no se simula con polling.
    async fn subscribe_ticks(&self, symbol: &str) -> Result<(), BrokerError> {
        let _ = symbol;
        Err(capacidad_no_soportada(self.name(), "stream de ticks"))
    }

    // -------- Eventos --------

    /// Stream de eventos (cierres, balances, payouts en vivo, velas suscritas).
    async fn events(&self) -> Result<EventStream, BrokerError>;
}

/// Rechazo uniforme de una capacidad que el bróker no ofrece.
///
/// Las capacidades exclusivas de IQ Option viven en el contrato por
/// compatibilidad; un adaptador que no las ofrece no las implementa y hereda
/// este rechazo explícito.
fn capacidad_no_soportada(broker: &str, capacidad: &str) -> BrokerError {
    BrokerError::Rejected(format!("{broker}: el bróker no ofrece {capacidad}"))
}

/// Marcador de compilación: el trait es `Send + Sync` por contrato.
const fn _assert_broker_bounds<T: Broker + Send + Sync>() {}

/// Tipo de opción por defecto del MVP.
pub const MVP_OPTION_TYPES: &[OptionType] = &[OptionType::Binary, OptionType::Digital];
