//! Implementación del contrato `Broker` para IQ Option.

use async_trait::async_trait;
use futures_util::stream::unfold;
use serde_json::json;
use tokio::sync::mpsc;

use crate::broker::config::BrokerParams;
use crate::broker::error::BrokerError;
use crate::broker::iq_option::endpoints::{msg, option_type_id};
use crate::broker::iq_option::mapper;
use crate::broker::iq_option::session::IqOptionBroker;
use crate::broker::session::Session;
use crate::broker::trait_::{Broker, CandleStream, EventStream};
use crate::domain::*;

#[async_trait]
impl Broker for IqOptionBroker {
    fn name(&self) -> &'static str {
        "iq_option"
    }

    async fn connect(params: BrokerParams) -> Result<Self, BrokerError>
    where
        Self: Sized,
    {
        Self::nuevo(params).await
    }

    async fn disconnect(&mut self) -> Result<(), BrokerError> {
        self.cerrar_salida();
        let mut estado = self.estado.write().await;
        estado.conectado = false;
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.estado.read().await.conectado
    }

    async fn session(&self) -> Result<Session, BrokerError> {
        self.estado_lectura()
            .await
            .session
            .clone()
            .ok_or_else(|| BrokerError::Internal("sesión no inicializada".into()))
    }

    // -------- Cuenta y balance --------

    fn iqoption_extras(&self) -> Option<&dyn crate::broker::extras::IqOptionExtras> {
        Some(self)
    }

    async fn get_balance(&self) -> Result<Balance, BrokerError> {
        let estado = self.estado_lectura().await;
        let balance_id = estado
            .session
            .as_ref()
            .and_then(|s| s.balance_id)
            .ok_or_else(|| BrokerError::Rejected("sesión sin balance activo".into()))?;
        let bal = estado
            .balances
            .iter()
            .find(|b| b.id == balance_id)
            .cloned()
            .ok_or_else(|| BrokerError::Rejected("balance activo no encontrado".into()))?;
        Ok(Balance {
            amount: bal.amount,
            currency: bal.currency,
            account_type: bal.account_type,
            balance_id: bal.id,
        })
    }

    async fn get_account_type(&self) -> Result<AccountType, BrokerError> {
        self.estado_lectura()
            .await
            .session
            .as_ref()
            .map(|s| s.account_type)
            .ok_or_else(|| BrokerError::Internal("sesión no inicializada".into()))
    }

    async fn switch_account(&mut self, kind: AccountType) -> Result<(), BrokerError> {
        // `api_profile_changebalance` (WS) + refresco de balances (`get-balances`).
        let balance_id = {
            let estado = self.estado_lectura().await;
            estado
                .balances
                .iter()
                .find(|b| b.account_type == kind)
                .map(|b| b.id)
                .ok_or_else(|| BrokerError::Rejected(format!("no hay cuenta {kind:?}")))?
        };
        self.enviar_sin_espera(
            "api_profile_changebalance",
            &json!({ "balance_id": balance_id }),
        )
        .await?;

        // Si el refresco falla, el cambio NO se da por hecho: el error se
        // propaga y la sesión conserva la cuenta anterior.
        let lista = self
            .solicitar(msg::GET_BALANCES, &json!({}), "balances")
            .await?;
        let balances = mapper::balances_desde_lista(&lista);
        if balances.is_empty() {
            return Err(BrokerError::Rejected(
                "el bróker no devolvió balances tras el cambio de cuenta".into(),
            ));
        }
        if !balances.iter().any(|b| b.id == balance_id) {
            return Err(BrokerError::Rejected(format!(
                "el bróker no confirmó el balance {balance_id}"
            )));
        }
        let mut estado = self.estado.write().await;
        estado.balances = balances;
        if let Some(s) = estado.session.as_mut() {
            s.balance_id = Some(balance_id);
            s.account_type = kind;
        }
        Ok(())
    }

    // -------- Activos y mercado --------

    async fn list_assets(&self) -> Result<Vec<Asset>, BrokerError> {
        Ok(self.catalogo.read().await.clone())
    }

    async fn asset_status(&self, symbol: &str) -> Result<MarketStatus, BrokerError> {
        let catalogo = self.catalogo.read().await;
        catalogo
            .iter()
            .find(|a| a.symbol == symbol)
            .map(|a| a.status)
            .ok_or_else(|| {
                BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                    symbol.into(),
                ))
            })
    }

    // -------- Velas --------

    async fn get_candles(&self, req: CandleRequest) -> Result<Vec<Candle>, BrokerError> {
        let catalogo = self.catalogo.read().await;
        let active_id = catalogo
            .iter()
            .find(|a| a.symbol == req.symbol)
            .map(|a| a.opcode)
            .ok_or_else(|| {
                BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                    req.symbol.clone(),
                ))
            })?;
        drop(catalogo);

        let ahora = self.hora_servidor_segs().await;
        // El bróker solo devuelve velas CERRADAS: `to` = cierre de vela cerrada.
        let to = req
            .end_time
            .unwrap_or_else(|| mapper::cierre_de_vela_cerrada(ahora, req.interval as i64, 90));
        let respuesta = self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({
                    "name": msg::QUOTES_HISTORY_GET_CANDLES,
                    "version": "2.0",
                    "body": {
                        "active_id": active_id,
                        "size": req.interval,
                        "count": req.count,
                        "split_normalization": true,
                        "to": to
                    }
                }),
                msg::CANDLES,
            )
            .await?;
        let velas = respuesta
            .get("candles")
            .and_then(|c| c.as_array())
            .map(|lista| lista.iter().filter_map(mapper::vela_desde_json).collect())
            .unwrap_or_default();
        Ok(velas)
    }

    async fn subscribe_candles(&self, req: CandleSubscribe) -> Result<CandleStream, BrokerError> {
        let catalogo = self.catalogo.read().await;
        let active_id = catalogo
            .iter()
            .find(|a| a.symbol == req.symbol)
            .map(|a| a.opcode)
            .ok_or_else(|| {
                BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                    req.symbol.clone(),
                ))
            })?;
        drop(catalogo);

        // Stream multi-tamaño: un único canal por símbolo con el tamaño pedido
        // (0 = todos los tamaños).
        self.suscribir(
            &format!("velas:{}", req.symbol),
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::CANDLES_GENERATED,
                "params": { "routingFilters": { "active_id": active_id.to_string() } }
            }),
        )
        .await?;

        // Canal por suscripción. Al soltar el `CandleStream` el receptor queda
        // cerrado y la suscripción se retira en el siguiente despacho de velas
        // (`sub_velas.retain`): no hay API de baja explícita. El canal es
        // ilimitado, así que el consumidor debe drenarlo.
        let (tx, rx) = mpsc::unbounded_channel::<Candle>();
        self.sub_velas
            .write()
            .await
            .push(crate::broker::iq_option::session::SubVelas {
                symbol: req.symbol.clone(),
                interval: req.interval,
                tx,
            });
        let stream = unfold(rx, |mut rx| async move {
            rx.recv().await.map(|c| (Ok::<Candle, BrokerError>(c), rx))
        });
        Ok(Box::pin(stream))
    }

    // -------- Payout --------

    async fn get_payout(
        &self,
        symbol: &str,
        expiration_seconds: u64,
    ) -> Result<Payout, BrokerError> {
        let catalogo = self.catalogo.read().await;
        let asset = catalogo
            .iter()
            .find(|a| a.symbol == symbol)
            .ok_or_else(|| {
                BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                    symbol.into(),
                ))
            })?;
        let profit_percent = asset.payout_percent.unwrap_or(0.0);
        // El payout digital en vivo se consulta por `quote_payout`; este método
        // conserva el valor del catálogo por compatibilidad.
        Ok(Payout {
            symbol: symbol.to_string(),
            expiration_seconds,
            profit_percent,
            is_spot: asset.class == AssetClass::Digital,
            strike: None,
            closed: asset.status == MarketStatus::Closed,
        })
    }

    /// Cotización viva por expiración y tipo de opción.
    ///
    /// IQ Option publica el payout digital en
    /// `price-splitter.client-price-generated`: la entrada `SPT` es el spot y
    /// `call.bid` el precio con el que se calcula el payout de CALL. El canal
    /// se verificó en vivo con la cuenta; no hubo frames equivalentes para
    /// `instrument-quotes-generated` (binary/turbo/blitz). Por eso esos tipos
    /// devuelven `available: false` en vez de reutilizar el payout del
    /// catálogo, que no es una cotización por expiración.
    async fn quote_payout(
        &self,
        symbol: &str,
        option_type: OptionType,
        expiration_seconds: u64,
        amount: f64,
    ) -> Result<PayoutQuote, BrokerError> {
        if expiration_seconds == 0 {
            return Err(BrokerError::Rejected(
                "iq_option: expiration_seconds debe ser positivo".into(),
            ));
        }

        let asset = {
            let catalogo = self.catalogo.read().await;
            catalogo
                .iter()
                .find(|a| a.symbol == symbol)
                .cloned()
                .ok_or_else(|| {
                    BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                        symbol.into(),
                    ))
                })?
        };

        // `amount` no altera el precio digital de IQ; se conserva por contrato.
        let _ = amount;

        if option_type != OptionType::Digital || asset.status == MarketStatus::Closed {
            return Ok(PayoutQuote::unavailable(
                symbol,
                option_type,
                expiration_seconds,
            ));
        }

        self.suscribir(
            &format!("splitter:{}", asset.opcode),
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::PRICE_SPLITTER,
                "version": "1.0",
                "params": { "routingFilters": { "instrument_type": "digital-option", "asset_id": asset.opcode } }
            }),
        )
        .await?;

        let mut precios = self.precios.subscribe();
        // Mismo plazo que la compra digital: si el bróker no publica el periodo,
        // la respuesta correcta es “sin cotización”, no un payout inventado.
        let plazo = std::time::Duration::from_secs(15);
        let limite = tokio::time::Instant::now() + plazo;
        loop {
            let restante = limite.saturating_duration_since(tokio::time::Instant::now());
            if restante.is_zero() {
                return Ok(PayoutQuote::unavailable(
                    symbol,
                    option_type,
                    expiration_seconds,
                ));
            }
            let mensaje = match tokio::time::timeout(restante, precios.recv()).await {
                Err(_) => {
                    return Ok(PayoutQuote::unavailable(
                        symbol,
                        option_type,
                        expiration_seconds,
                    ))
                }
                Ok(Ok(mensaje)) => mensaje,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    return Err(BrokerError::Internal(
                        "iq_option: canal de precios cerrado".into(),
                    ))
                }
            };

            if mensaje.get("asset_id").and_then(|v| v.as_u64()) != Some(u64::from(asset.opcode))
                || mensaje.get("period").and_then(|v| v.as_u64()) != Some(expiration_seconds)
            {
                continue;
            }

            let Some(payout) = mapper::payout_price_splitter(&mensaje, Direction::Call) else {
                continue;
            };
            let extras = asset.extras.as_ref();
            return Ok(PayoutQuote {
                symbol: symbol.to_string(),
                option_type,
                expiration_seconds,
                profit_percent: Some(payout.profit_percent),
                min_amount: extras.and_then(|e| e.min_amount),
                max_amount: extras.and_then(|e| e.max_amount),
                available: true,
                captured_at: self.hora_servidor_segs().await,
            });
        }
    }

    async fn sell_option(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        // Mensaje `sell-options` v2.0 con la lista de ids (verificado contra
        // el canal del bróker; el bróker confirma con `sold-options`).
        let id_num = order_id
            .parse::<i64>()
            .map_err(|_| BrokerError::Rejected(format!("id de orden no numerico: {order_id}")))?;
        let respuesta = self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({
                    "name": msg::SELL_OPTIONS,
                    "version": "2.0",
                    "body": { "options_ids": [id_num] }
                }),
                msg::SOLD_OPTIONS,
            )
            .await?;
        // El broker responde con el detalle de la venta o con error.
        if respuesta.get("message").is_some() {
            return Err(BrokerError::Rejected(
                respuesta
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("venta rechazada")
                    .to_string(),
            ));
        }
        Ok(())
    }

    async fn get_position_history(
        &self,
        symbol: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PositionHistory, BrokerError> {
        let balance_id = self
            .estado_lectura()
            .await
            .session
            .as_ref()
            .and_then(|s| s.balance_id);
        let limit = limit.min(100) as u64;
        let respuesta = self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({
                    "name": msg::PORTFOLIO_GET_HISTORY,
                    "body": {
                        "instrument_types": ["turbo-option", "binary-option", "digital-option"],
                        "limit": limit,
                        "offset": offset as u64,
                        "start": 0,
                        "end": 0,
                        "user_balance_id": balance_id
                    }
                }),
                msg::HISTORY_POSITIONS,
            )
            .await?;
        let historial = mapper::historial_desde_positions(&respuesta, offset)
            .ok_or_else(|| BrokerError::Rejected("respuesta sin positions".into()))?;
        if let Some(simbolo) = symbol {
            if historial.positions.iter().all(|p| p.symbol != simbolo) {
                return Err(BrokerError::Domain(
                    crate::domain::error::DomainError::OperationNotFound(format!(
                        "sin historial para {simbolo}"
                    )),
                ));
            }
        }
        Ok(historial)
    }

    // -------- Órdenes --------

    async fn buy_binary(&self, req: BuyBinaryRequest) -> Result<OrderId, BrokerError> {
        // BLITZ tiene su propia petición (duración en segundos y payout): se
        // delega en `buy_blitz` para no transportar segundos en
        // `expiration_minutes`.
        if req.option_type == OptionType::Blitz {
            let payout = {
                let catalogo = self.catalogo.read().await;
                catalogo
                    .iter()
                    .find(|a| a.symbol == req.symbol && a.class == AssetClass::Blitz)
                    .and_then(|a| a.payout_percent)
                    .unwrap_or(0.0)
            };
            return self
                .buy_blitz(BuyBlitzRequest {
                    symbol: req.symbol,
                    amount: req.amount,
                    direction: req.direction,
                    expiration_size_secs: req.expiration_minutes,
                    profit_percent: payout,
                })
                .await;
        }

        let asset = {
            let catalogo = self.catalogo.read().await;
            catalogo
                .iter()
                .find(|a| {
                    a.symbol == req.symbol
                        && (a.class == AssetClass::Binary || a.class == AssetClass::Turbo)
                })
                .cloned()
        }
        .ok_or_else(|| {
            BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                req.symbol.clone(),
            ))
        })?;

        if asset.status == MarketStatus::Closed {
            return Err(BrokerError::Domain(
                crate::domain::error::DomainError::MarketClosed(req.symbol),
            ));
        }
        if let Some(extras) = &asset.extras {
            if let Some(min) = extras.min_amount {
                if req.amount < min {
                    return Err(BrokerError::Domain(
                        crate::domain::error::DomainError::InvalidAmount(format!("mínimo {min}")),
                    ));
                }
            }
        }

        let mut body = json!({
            "active_id": asset.opcode,
            "user_balance_id": self.estado_lectura().await.session.as_ref()
                .and_then(|s| s.balance_id),
            "price": req.amount,
            "direction": match req.direction { Direction::Call => "call", Direction::Put => "put" }
        });

        // Clásica v1.0: expiración por tabla de minutos/cuartos de hora.
        let (expired, idx) =
            mapper::tiempo_de_expiracion(self.hora_servidor_segs().await, req.expiration_minutes);
        body["expired"] = json!(expired);
        body["option_type_id"] = json!(if idx < 5 {
            option_type_id::TURBO
        } else {
            option_type_id::BINARY
        });

        let (version, esperada) = ("1.0", msg::BUY_COMPLETE);
        match self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({ "name": msg::OPEN_OPTION, "version": version, "body": body }),
                esperada,
            )
            .await
        {
            Ok(respuesta) => {
                let id = respuesta
                    .get("id")
                    .and_then(|i| i.as_i64())
                    .or_else(|| respuesta.pointer("/result/id").and_then(|i| i.as_i64()))
                    .ok_or_else(|| {
                        BrokerError::Rejected(
                            respuesta
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("respuesta sin id de orden")
                                .to_string(),
                        )
                    })?;
                // Base de la reconciliación: hasta que llegue un cierre.
                self.abiertas.write().await.insert(id.to_string());
                Ok(id.to_string())
            }
            Err(e) => Err(e),
        }
    }

    /// Lee el resultado de la caché local, alimentada por los eventos de
    /// cierre (`option-closed`, `socket-option-closed`, `position-changed`).
    ///
    /// **No hace ninguna petición al bróker**: si el cierre aún no ha llegado
    /// por eventos, devuelve `OperationResult::Pending`.
    async fn get_operation_result(
        &self,
        order_id: &OrderId,
    ) -> Result<OperationResult, BrokerError> {
        let cache = self.resultados.read().await;
        Ok(cache
            .get(order_id)
            .copied()
            .unwrap_or(OperationResult::Pending))
    }

    // -------- Eventos --------

    async fn events(&self) -> Result<EventStream, BrokerError> {
        let rx = self.eventos.subscribe();
        let stream = unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Ok(ev) => Some((ev, rx)),
                // Pérdida de continuidad: se informa y el stream continúa, en
                // vez de cerrarse como si la sesión hubiera terminado.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    Some((Event::EventsLost { count: n }, rx))
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
            }
        });
        Ok(Box::pin(stream))
    }
}

impl IqOptionBroker {
    pub async fn get_technical_indicators(
        &self,
        symbol: &str,
    ) -> Result<TechnicalIndicators, BrokerError> {
        let asset_id = {
            let catalogo = self.catalogo.read().await;
            catalogo
                .iter()
                .find(|a| a.symbol == symbol)
                .map(|a| a.opcode)
                .ok_or_else(|| {
                    BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                        symbol.into(),
                    ))
                })?
        };
        // Mensaje: `trading-signals.get-technical-indicators` v1.0, body {id}.
        // El `id` DEBE ser entero (string → INVALID_SCHEMA). Respuesta:
        // `technical-indicators` con `msg.indicators`, o `msg.message` de error.
        let respuesta = self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({
                    "name": msg::TECHNICAL_INDICATORS,
                    "version": "1.0",
                    "body": { "id": asset_id }
                }),
                msg::TECHNICAL_INDICATORS_RESPONSE,
            )
            .await?;
        mapper::indicadores_desde_respuesta(&respuesta, symbol).map_err(BrokerError::Rejected)
    }

    pub async fn get_asset_profile(&self, symbol: &str) -> Result<AssetProfile, BrokerError> {
        let asset_id = {
            let catalogo = self.catalogo.read().await;
            catalogo
                .iter()
                .find(|a| a.symbol == symbol)
                .map(|a| a.opcode)
                .ok_or_else(|| {
                    BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                        symbol.into(),
                    ))
                })?
        };
        // GraphQL: el broker responde `financial-information` con data.active.
        let respuesta = self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({
                    "name": msg::GET_FINANCIAL_INFORMATION,
                    "version": "1.0",
                    "body": {
                        "query": mapper::QUERY_ASSET_PROFILE,
                        "operationName": "GetAssetProfileInfo",
                        "variables": { "activeId": asset_id }
                    }
                }),
                msg::FINANCIAL_INFORMATION,
            )
            .await?;
        mapper::perfil_desde_financial(symbol, &respuesta).map_err(BrokerError::Rejected)
    }

    pub async fn subscribe_live_deals(&self, symbol: &str) -> Result<(), BrokerError> {
        let active_id = {
            let catalogo = self.catalogo.read().await;
            catalogo
                .iter()
                .find(|a| a.symbol == symbol)
                .map(|a| a.opcode)
                .ok_or_else(|| {
                    BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                        symbol.into(),
                    ))
                })?
        };
        // Ambos canales: el agregado y el específico de binarias.
        self.suscribir(
            &format!("live-deals:{symbol}"),
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::LIVE_DEAL,
                "version": "1.0",
                "params": { "routingFilters": { "instrument_active_id": active_id } }
            }),
        )
        .await?;
        self.suscribir(
            &format!("live-deals-bin:{symbol}"),
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::LIVE_DEAL_BINARY,
                "version": "1.0",
                "params": { "routingFilters": { "active_id": active_id, "option_type": "binary-option" } }
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn subscribe_commission_changes(&self) -> Result<(), BrokerError> {
        // Los tres tipos de opción: el bróker emite por instrumento.
        for instrumento in ["binary-option", "turbo-option", "digital-option"] {
            self.suscribir(
                &format!("comision:{instrumento}"),
                msg::SUBSCRIBE_MESSAGE,
                &json!({
                    "name": msg::COMMISSION_CHANGED,
                    "version": "1.0",
                    "params": { "routingFilters": { "instrument_type": instrumento } }
                }),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn buy_blitz(&self, req: BuyBlitzRequest) -> Result<OrderId, BrokerError> {
        // El blitz se resuelve contra el grupo blitz del catálogo, cuyos
        // metadatos (duraciones en segundos, rollovers) son propios.
        let asset = {
            let catalogo = self.catalogo.read().await;
            catalogo
                .iter()
                .find(|a| a.symbol == req.symbol && a.class == AssetClass::Blitz)
                .cloned()
        }
        .ok_or_else(|| {
            BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                req.symbol.clone(),
            ))
        })?;

        if asset.status == MarketStatus::Closed {
            return Err(BrokerError::Domain(
                crate::domain::error::DomainError::MarketClosed(req.symbol),
            ));
        }
        let extras = asset
            .extras
            .as_ref()
            .ok_or_else(|| BrokerError::Rejected("activo sin metadata de blitz".into()))?;
        if let Some(min) = extras.min_amount {
            if req.amount < min {
                return Err(BrokerError::Domain(
                    crate::domain::error::DomainError::InvalidAmount(format!("mínimo {min}")),
                ));
            }
        }

        // Duraciones admitidas: `option.expiration_times` ∪ `rollovers[].expiration_size`.
        let mut permitidos = extras.expiration_times_secs.clone().unwrap_or_default();
        for r in extras.rollovers.as_ref().into_iter().flatten() {
            let s = r.expiration_size as u64;
            if !permitidos.contains(&s) {
                permitidos.push(s);
            }
        }
        permitidos.sort_unstable();
        let secs = req.expiration_size_secs;
        if !permitidos.is_empty() && !permitidos.contains(&secs) {
            return Err(BrokerError::Rejected(format!(
                "duración {secs}s no admitida; admite {permitidos:?}"
            )));
        }

        // Payout: el de la petición si viene informado; si no, el del catálogo.
        let payout = if req.profit_percent > 0.0 {
            req.profit_percent
        } else {
            asset.payout_percent.unwrap_or(0.0)
        };

        let body = json!({
            "active_id": asset.opcode,
            "user_balance_id": self.estado_lectura().await.session.as_ref()
                .and_then(|s| s.balance_id),
            "price": req.amount,
            "direction": match req.direction { Direction::Call => "call", Direction::Put => "put" },
            // `expired` debe quedar alineado al múltiplo de la duración.
            "expired": mapper::expiracion_blitz_alineada(self.hora_servidor_segs().await, secs),
            "expiration_size": secs,
            // El bróker exige entero (rechaza "86.0").
            "profit_percent": payout.round() as i64,
            "option_type_id": option_type_id::BLITZ,
        });

        let respuesta = self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({ "name": msg::OPEN_OPTION, "version": "2.0", "body": body }),
                msg::OPTION,
            )
            .await?;
        let id = respuesta
            .get("id")
            .and_then(|i| i.as_i64())
            .or_else(|| respuesta.pointer("/result/id").and_then(|i| i.as_i64()))
            .ok_or_else(|| {
                BrokerError::Rejected(
                    respuesta
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("respuesta sin id de orden blitz")
                        .to_string(),
                )
            })?;
        self.abiertas.write().await.insert(id.to_string());
        Ok(id.to_string())
    }

    pub async fn buy_digital_spot(&self, req: BuyDigitalRequest) -> Result<OrderId, BrokerError> {
        let (asset_id, periodo) = {
            let catalogo = self.catalogo.read().await;
            let asset = catalogo
                .iter()
                .find(|a| a.symbol == req.symbol)
                .ok_or_else(|| {
                    BrokerError::Domain(crate::domain::error::DomainError::AssetNotFound(
                        req.symbol.clone(),
                    ))
                })?;
            if asset.status == MarketStatus::Closed {
                return Err(BrokerError::Domain(
                    crate::domain::error::DomainError::MarketClosed(req.symbol),
                ));
            }
            (asset.opcode, req.duration_minutes * 60)
        };

        // Suscripción price-splitter (payout + instrument_id en vivo).
        self.suscribir(
            &format!("splitter:{asset_id}"),
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::PRICE_SPLITTER,
                "version": "1.0",
                "params": { "routingFilters": { "instrument_type": "digital-option", "asset_id": asset_id } }
            }),
        )
        .await?;
        let mut rx = self.precios.subscribe();

        // Espera el mensaje del periodo pedido: instrumento SPOT (strike "SPT")
        // e instrument_index del MISMO mensaje (evita carreras de rollover).
        let (instrument_id, asset_id_json, instrument_index) = loop {
            let msg = tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv())
                .await
                .map_err(|_| BrokerError::Timeout)?
                .map_err(|e| BrokerError::Internal(format!("canal de precios cerrado: {e}")))?;
            if msg.get("period").and_then(|p| p.as_u64()) != Some(periodo) {
                continue;
            }
            if let Some(prices) = msg.pointer("/prices").and_then(|p| p.as_array()) {
                let spot = prices
                    .iter()
                    .find(|p| p.get("strike").and_then(|s| s.as_str()) == Some("SPT"));
                if let Some(spot) = spot {
                    let campo = match req.direction {
                        Direction::Call => "/call/symbol",
                        Direction::Put => "/put/symbol",
                    };
                    if let Some(simbolo) = spot.pointer(campo).and_then(|s| s.as_str()) {
                        let idx = msg
                            .get("instrument_index")
                            .and_then(|i| i.as_i64())
                            .unwrap_or(0);
                        let aid = msg.get("asset_id").cloned().unwrap_or(json!(asset_id));
                        break (simbolo.to_string(), aid, idx);
                    }
                }
            }
        };
        let balance_id = self
            .estado_lectura()
            .await
            .session
            .as_ref()
            .and_then(|s| s.balance_id);
        match self
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({
                    "name": msg::PLACE_DIGITAL_OPTION,
                    // v2.0 verificada en vivo: amount como STRING, con asset_id e
                    // instrument_index del mismo mensaje de price-splitter.
                    "version": "2.0",
                    "body": {
                        "user_balance_id": balance_id,
                        "instrument_id": instrument_id,
                        "asset_id": asset_id_json,
                        "instrument_index": instrument_index,
                        "amount": format!("{:.2}", req.amount)
                    }
                }),
                msg::DIGITAL_OPTION_PLACED,
            )
            .await
        {
            Ok(respuesta) => {
                let id = respuesta
                    .get("id")
                    .and_then(|i| i.as_i64())
                    .ok_or_else(|| {
                        BrokerError::Rejected(
                            respuesta
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("respuesta sin id de orden digital")
                                .to_string(),
                        )
                    })?;
                // Correlación para `position-changed`: instrumento → id de orden.
                self.digitales
                    .write()
                    .await
                    .insert(instrument_id.clone(), id.to_string());
                self.abiertas.write().await.insert(id.to_string());
                Ok(id.to_string())
            }
            Err(e) => Err(e),
        }
    }
}
