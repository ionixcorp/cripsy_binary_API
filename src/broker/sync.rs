//! Fachada SINCRONA sobre el núcleo async (Fase 6 del roadmap).
//!
//! Misma lógica de protocolo, dos formas de consumo: este envoltorio ejecuta
//! el runtime tokio en su propio hilo y expone métodos bloqueantes. El usuario
//! sincrono no ve `tokio` ni `await`.
//!
//! Reglas de uso (documentadas):
//! - **No usar dentro de un contexto async**: bloquearía el executor del
//!   llamador (los métodos hacen `block_on` sobre el runtime propio).
//! - Los streams se entregan como `Iterator` bloqueantes respaldados por un
//!   canal con buffer acotado.
//! - El bus de eventos tiene buffer acotado: un consumidor lento puede perder
//!   eventos (igual que en async).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use crate::broker::config::BrokerParams;
use crate::broker::error::BrokerError;
use crate::broker::iq_option::session::IqOptionBroker;
use crate::broker::session::Session;
use crate::broker::trait_::Broker;
use crate::domain::*;

/// Tamaño del buffer de los puentes stream→canal.
const BUFFER_PUENTE: usize = 1024;

/// Timeout por defecto de espera en iteradores bloqueantes.
const TIMEOUT_ITER: Duration = Duration::from_secs(30);

/// Adaptador sincrono: envuelve el núcleo async con un runtime propio.
pub struct SyncBroker {
    runtime: tokio::runtime::Runtime,
    nucleo: IqOptionBroker,
}

impl SyncBroker {
    /// Conecta creando el runtime y la sesión async.
    pub fn connect(params: BrokerParams) -> Result<Self, BrokerError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| BrokerError::Internal(format!("no se pudo crear el runtime: {e}")))?;
        let nucleo = runtime.block_on(IqOptionBroker::nuevo(params))?;
        Ok(Self { runtime, nucleo })
    }

    /// Acceso al núcleo async (para quien quiera ambos mundos).
    pub fn async_core(&self) -> &IqOptionBroker {
        &self.nucleo
    }

    // -------- Sesión y cuenta --------

    pub fn session(&self) -> Result<Session, BrokerError> {
        self.runtime.block_on(self.nucleo.session())
    }

    pub fn is_connected(&self) -> bool {
        self.runtime.block_on(self.nucleo.is_connected())
    }

    pub fn disconnect(&mut self) -> Result<(), BrokerError> {
        self.runtime.block_on(self.nucleo.disconnect())
    }

    pub fn get_balance(&self) -> Result<Balance, BrokerError> {
        self.runtime.block_on(self.nucleo.get_balance())
    }

    pub fn get_account_type(&self) -> Result<AccountType, BrokerError> {
        self.runtime.block_on(self.nucleo.get_account_type())
    }

    pub fn switch_account(&mut self, kind: AccountType) -> Result<(), BrokerError> {
        self.runtime.block_on(self.nucleo.switch_account(kind))
    }

    // -------- Activos y mercado --------

    pub fn list_assets(&self) -> Result<Vec<Asset>, BrokerError> {
        self.runtime.block_on(self.nucleo.list_assets())
    }

    pub fn asset_status(&self, symbol: &str) -> Result<MarketStatus, BrokerError> {
        self.runtime.block_on(self.nucleo.asset_status(symbol))
    }

    // -------- Velas --------

    pub fn get_candles(&self, req: CandleRequest) -> Result<Vec<Candle>, BrokerError> {
        self.runtime.block_on(self.nucleo.get_candles(req))
    }

    /// Stream de velas como iterador bloqueante.
    pub fn subscribe_candles(&self, req: CandleSubscribe) -> Result<CandleIter, BrokerError> {
        let stream = self.runtime.block_on(self.nucleo.subscribe_candles(req))?;
        let (tx, rx) = mpsc::sync_channel::<Result<Candle, BrokerError>>(BUFFER_PUENTE);
        self.runtime.spawn(puente_stream(stream, tx));
        Ok(CandleIter { rx })
    }

    // -------- Historial y payout --------

    pub fn get_position_history(
        &self,
        symbol: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PositionHistory, BrokerError> {
        self.runtime
            .block_on(self.nucleo.get_position_history(symbol, limit, offset))
    }

    pub fn get_payout(&self, symbol: &str, expiration_seconds: u64) -> Result<Payout, BrokerError> {
        self.runtime
            .block_on(self.nucleo.get_payout(symbol, expiration_seconds))
    }

    pub fn get_asset_profile(&self, symbol: &str) -> Result<AssetProfile, BrokerError> {
        self.runtime.block_on(self.nucleo.get_asset_profile(symbol))
    }

    pub fn get_technical_indicators(
        &self,
        symbol: &str,
    ) -> Result<TechnicalIndicators, BrokerError> {
        self.runtime
            .block_on(self.nucleo.get_technical_indicators(symbol))
    }

    // -------- Eventos ---------

    pub fn subscribe_live_deals(&self, symbol: &str) -> Result<(), BrokerError> {
        self.runtime
            .block_on(self.nucleo.subscribe_live_deals(symbol))
    }

    pub fn subscribe_commission_changes(&self) -> Result<(), BrokerError> {
        self.runtime
            .block_on(self.nucleo.subscribe_commission_changes())
    }

    /// Iterador bloqueante de eventos (cierre, balance, ticks, deals...).
    pub fn events(&self) -> Result<EventIter, BrokerError> {
        let stream = self.runtime.block_on(self.nucleo.events())?;
        let (tx, rx) = mpsc::sync_channel::<Event>(BUFFER_PUENTE);
        self.runtime.spawn(puente_eventos(stream, tx));
        Ok(EventIter { rx })
    }

    // -------- Órdenes --------

    pub fn buy_binary(&self, req: BuyBinaryRequest) -> Result<OrderId, BrokerError> {
        self.runtime.block_on(self.nucleo.buy_binary(req))
    }

    pub fn buy_digital_spot(&self, req: BuyDigitalRequest) -> Result<OrderId, BrokerError> {
        self.runtime.block_on(self.nucleo.buy_digital_spot(req))
    }

    pub fn sell_option(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        self.runtime.block_on(self.nucleo.sell_option(order_id))
    }

    pub fn get_operation_result(&self, order_id: &OrderId) -> Result<OperationResult, BrokerError> {
        self.runtime
            .block_on(self.nucleo.get_operation_result(order_id))
    }
}

/// Iterador bloqueante de velas.
pub struct CandleIter {
    rx: Receiver<Result<Candle, BrokerError>>,
}

impl Iterator for CandleIter {
    type Item = Result<Candle, BrokerError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rx.recv_timeout(TIMEOUT_ITER) {
            Ok(item) => Some(item),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }
}

/// Iterador bloqueante de eventos.
pub struct EventIter {
    rx: Receiver<Event>,
}

impl Iterator for EventIter {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv_timeout(TIMEOUT_ITER).ok()
    }
}

/// Puente: stream async de velas → canal sincrono.
async fn puente_stream(
    mut stream: crate::broker::trait_::CandleStream,
    tx: mpsc::SyncSender<Result<Candle, BrokerError>>,
) {
    use futures_util::StreamExt;
    while let Some(item) = stream.next().await {
        if tx.send(item).is_err() {
            break;
        }
    }
}

/// Puente: stream async de eventos → canal sincrono.
async fn puente_eventos(
    mut stream: crate::broker::trait_::EventStream,
    tx: mpsc::SyncSender<Event>,
) {
    use futures_util::StreamExt;
    while let Some(ev) = stream.next().await {
        if tx.send(ev).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iteradores_terminan_sin_stream() {
        // Un canal sin emisor: el iterador debe cerrar (no colgar para siempre).
        let (_tx, rx) = mpsc::sync_channel::<Event>(1);
        let it = EventIter { rx };
        assert!(it.into_iter().next().is_none());
    }
}
