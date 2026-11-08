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
        Ok(self
            .estado_lectura()
            .await
            .session
            .as_ref()
            .map(|s| s.account_type)
            .unwrap_or(AccountType::Practice))
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
        let respuesta = self
            .solicitar(msg::GET_BALANCES, &json!({}), "balances")
            .await;
        if let Ok(lista) = respuesta {
            let balances = mapper::balances_desde_lista(&lista);
            if !balances.is_empty() {
                let mut estado = self.estado.write().await;
                estado.balances = balances;
                if let Some(s) = estado.session.as_mut() {
                    s.balance_id = Some(balance_id);
                    s.account_type = kind;
                }
            }
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
        self.enviar_sin_espera(
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::CANDLES_GENERATED,
                "params": { "routingFilters": { "active_id": active_id.to_string() } }
            }),
        )
        .await?;

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
        // El payout digital en vivo (price-splitter) se añade en la Fase 5.
        Ok(Payout {
            symbol: symbol.to_string(),
            expiration_seconds,
            profit_percent,
            is_spot: asset.class == AssetClass::Digital,
            strike: None,
            closed: asset.status == MarketStatus::Closed,
        })
    }

    async fn sell_option(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        // Mensaje `sell-options` v2.0 con la lista de ids (verificado contra
        // el canal de la referencia; el bróker confirma con `sold-options`).
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
    async fn get_technical_indicators(
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

    // -------- Órdenes --------

    async fn get_asset_profile(&self, symbol: &str) -> Result<AssetProfile, BrokerError> {
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
    async fn subscribe_live_deals(&self, symbol: &str) -> Result<(), BrokerError> {
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
        self.enviar_sin_espera(
            msg::SUBSCRIBE_MESSAGE,
            &json!({
                "name": msg::LIVE_DEAL,
                "version": "1.0",
                "params": { "routingFilters": { "instrument_active_id": active_id } }
            }),
        )
        .await?;
        self.enviar_sin_espera(
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

    async fn subscribe_commission_changes(&self) -> Result<(), BrokerError> {
        // Los tres tipos de opción: el bróker emite por instrumento.
        for instrumento in ["binary-option", "turbo-option", "digital-option"] {
            self.enviar_sin_espera(
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
    async fn buy_binary(&self, req: BuyBinaryRequest) -> Result<OrderId, BrokerError> {
        // Cada grupo del catálogo (binary/turbo/blitz) tiene SUS PROPIOS
        // metadatos: el blitz debe resolverse contra el grupo blitz
        // (expiraciones en segundos distintas a las de binary/turbo).
        let catalogo = self.catalogo.read().await;
        let asset = if req.option_type == OptionType::Blitz {
            catalogo
                .iter()
                .find(|a| a.symbol == req.symbol && a.class == AssetClass::Blitz)
                .cloned()
        } else {
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
        drop(catalogo);

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

        let option_type_id = match req.option_type {
            OptionType::Blitz => {
                let extras = asset
                    .extras
                    .as_ref()
                    .ok_or_else(|| BrokerError::Rejected("activo sin metadata de blitz".into()))?;
                // Para blitz, `expiration_minutes` transporta el tamaño en SEGUNDOS
                // (las duraciones del catálogo blitz están en segundos).
                let secs = req.expiration_minutes;
                // Duraciones admitidas: `option.expiration_times` ∪ `rollovers[].expiration_size`.
                let mut permitidos = extras
                    .expiration_times_secs
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                for r in extras.rollovers.as_ref().into_iter().flatten() {
                    let s = r.expiration_size as u64;
                    if !permitidos.contains(&s) {
                        permitidos.push(s);
                    }
                }
                permitidos.sort_unstable();
                if !permitidos.is_empty() && !permitidos.contains(&secs) {
                    return Err(BrokerError::Rejected(format!(
                        "duración {secs}s no admitida; admite {permitidos:?}"
                    )));
                }
                // `expired` debe ir alineado al multiplo de la duracion
                // (el broker rechaza valores desalineados)
                let expired =
                    mapper::expiracion_blitz_alineada(self.hora_servidor_segs().await, secs);
                body["expired"] = json!(expired);
                body["expiration_size"] = json!(secs);
                // El bróker exige entero (rechaza "86.0").
                let profit_percent = asset.payout_percent.unwrap_or(0.0).round() as i64;
                body["profit_percent"] = json!(profit_percent);
                option_type_id::BLITZ
            }
            OptionType::Binary | OptionType::Digital => {
                // Clásica v1.0: expiración por tabla de minutos/cuartos de hora.
                let (expired, idx) = mapper::tiempo_de_expiracion(
                    self.hora_servidor_segs().await,
                    req.expiration_minutes,
                );
                body["expired"] = json!(expired);
                if idx < 5 {
                    option_type_id::TURBO
                } else {
                    option_type_id::BINARY
                }
            }
        };
        body["option_type_id"] = json!(option_type_id);

        // Blitz usa v2.0 (respuesta `option`); clásica usa v1.0 (`buyComplete`).
        let (version, esperada) = if req.option_type == OptionType::Blitz {
            ("2.0", msg::OPTION)
        } else {
            ("1.0", msg::BUY_COMPLETE)
        };
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
                Ok(id.to_string())
            }
            Err(e) => Err(e),
        }
    }

    async fn buy_digital_spot(&self, req: BuyDigitalRequest) -> Result<OrderId, BrokerError> {
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
        self.enviar_sin_espera(
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
                Ok(id.to_string())
            }
            Err(e) => Err(e),
        }
    }

    async fn get_operation_result(
        &self,
        order_id: &OrderId,
    ) -> Result<OperationResult, BrokerError> {
        // Cache alimentada por eventos de cierre: sin polling.
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
                Err(_) => None,
            }
        });
        Ok(Box::pin(stream))
    }
}
