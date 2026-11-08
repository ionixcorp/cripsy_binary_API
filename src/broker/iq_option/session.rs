//! Sesión del adaptador IQ Option (núcleo async).
//!
//! Arquitectura: una única tarea posee el WebSocket y atiende un `select!`
//! entre mensajes entrantes y una cola de salida. Las llamadas del trait
//! generan un `request_id`, lo registran en el router, envían por la cola y
//! esperan la respuesta con timeout. Cero polling, cero estado global.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::broker::config::{BrokerParams, IqOptionConfig};
use crate::broker::error::BrokerError;
use crate::broker::iq_option::endpoints::{self, msg};
use crate::broker::iq_option::mapper;
use crate::broker::session::Session;
use crate::domain::*;
use crate::transport::http::HttpClient;
use crate::transport::router::RequestRouter;
use crate::transport::websocket::WsClient;

/// Mensaje a enviar por la cola de salida.
pub(crate) struct Saliente {
    name: String,
    msg: Value,
    request_id: String,
}

/// Estado mutable de la sesión (solo lo toca la tarea lectora).
#[derive(Default)]
pub(crate) struct Estado {
    pub(crate) session: Option<Session>,
    pub(crate) balances: Vec<AccountBalance>,
    pub(crate) conectado: bool,
}

/// Suscripción activa de velas.
pub(crate) struct SubVelas {
    pub(crate) symbol: String,
    pub(crate) interval: u64,
    pub(crate) tx: mpsc::UnboundedSender<Candle>,
}

/// Adaptador IQ Option.
pub struct IqOptionBroker {
    config: IqOptionConfig,
    salida: Option<mpsc::UnboundedSender<Saliente>>,
    pub(crate) router: Arc<RequestRouter>,
    pub(crate) estado: Arc<RwLock<Estado>>,
    pub(crate) catalogo: Arc<RwLock<Vec<Asset>>>,
    pub(crate) sub_velas: Arc<RwLock<Vec<SubVelas>>>,
    pub(crate) eventos: broadcast::Sender<Event>,
    /// Payloads crudos de `price-splitter.client-price-generated`.
    pub(crate) precios: broadcast::Sender<Value>,
    /// Resultados cerrados por id de orden (los alimentan los eventos).
    pub(crate) resultados: Arc<RwLock<HashMap<String, OperationResult>>>,
    /// Órdenes digitales abiertas por `instrument_id` (para `position-changed`).
    pub(crate) digitales: Arc<RwLock<HashMap<String, OrderId>>>,
}

impl IqOptionBroker {
    /// Constructor de la sesión: login, WSS, ssid, perfil y catálogo.
    pub async fn nuevo(params: BrokerParams) -> Result<Self, BrokerError> {
        let (config, cfg_trafico) = match params {
            BrokerParams::IqOption(c) => (c.clone(), c.transport.clone()),
        };

        // ---------- login HTTP ----------
        let http = HttpClient::new(
            &endpoints::api_base_url(&config.auth_host),
            cfg_trafico.clone(),
        )?;
        let (cuerpo, headers) = http
            .post_json_completo(
                endpoints::LOGIN_PATH.trim_start_matches('/'),
                &json!({ "identifier": config.email, "password": config.password }),
            )
            .await?;
        let ssid = extraer_ssid(&headers, &cuerpo).ok_or_else(|| {
            BrokerError::Authentication("el login no devolvió cookie ssid".into())
        })?;

        // ---------- WSS ----------
        // Reintentos con backoff en el handshake. La reconexión posterior a
        // una caída no es automática: ver «Limitaciones conocidas» en docs/.
        let ws = WsClient::connect_con_reintentos(
            &endpoints::ws_url(&config.ws_host),
            vec![("Cookie".to_string(), format!("ssid={ssid}"))],
            cfg_trafico.clone(),
        )
        .await?;

        let (tx_salida, rx_salida) = mpsc::unbounded_channel::<Saliente>();
        let (tx_eventos, _rx_eventos) = broadcast::channel::<Event>(1024);
        let (tx_precios, _rx_precios) = broadcast::channel::<Value>(512);
        let router = Arc::new(RequestRouter::new());
        let estado = Arc::new(RwLock::new(Estado::default()));
        let catalogo = Arc::new(RwLock::new(Vec::new()));
        let sub_velas: Arc<RwLock<Vec<SubVelas>>> = Arc::new(RwLock::new(Vec::new()));
        let resultados: Arc<RwLock<HashMap<String, OperationResult>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let digitales: Arc<RwLock<HashMap<String, OrderId>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let broker = Self {
            config,
            salida: Some(tx_salida),
            router: router.clone(),
            estado: estado.clone(),
            catalogo: catalogo.clone(),
            sub_velas: sub_velas.clone(),
            eventos: tx_eventos.clone(),
            precios: tx_precios.clone(),
            resultados: resultados.clone(),
            digitales: digitales.clone(),
        };

        // ---------- tarea lectora/escritora ----------
        tokio::spawn(tarea_socket(
            ws,
            rx_salida,
            Ctx {
                router: router.clone(),
                estado: estado.clone(),
                catalogo: catalogo.clone(),
                sub_velas: sub_velas.clone(),
                eventos: tx_eventos,
                precios: tx_precios,
                resultados: resultados.clone(),
                digitales: digitales.clone(),
            },
        ));

        // ---------- ssid → profile ----------
        let perfil = broker
            .solicitar(msg::SSID, &json!(ssid), msg::PROFILE)
            .await?;
        let (session, balances) = mapper::sesion_desde_perfil(&perfil, &broker.config.account)
            .ok_or_else(|| BrokerError::Rejected("profile sin balances utilizables".into()))?;
        {
            let mut e = estado.write().await;
            e.session = Some(session);
            e.balances = balances;
            e.conectado = true;
        }

        // ---------- catálogo moderno (grupos en /msg) ----------
        let init = broker
            .solicitar(
                msg::SEND_MESSAGE,
                &json!({ "name": msg::GET_INITIALIZATION_DATA, "version": "3.0", "body": {} }),
                msg::INITIALIZATION_DATA,
            )
            .await?;
        let activos = mapper::catalogo_desde_init(&init);
        {
            let mut c = catalogo.write().await;
            *c = activos;
        }

        // ---------- suscripción a cierres de cartera (digital y demás) ----------
        for instrumento in ["digital-option", "binary-option", "trading"] {
            broker
                .enviar_sin_espera(
                    msg::SUBSCRIBE_MESSAGE,
                    &json!({
                        "name": msg::POSITION_CHANGED,
                        "version": "2.0",
                        "params": { "routingFilters": { "instrument_type": instrumento } }
                    }),
                )
                .await?;
        }
        Ok(broker)
    }

    /// Envía un mensaje y espera la respuesta correlacionada por `request_id`.
    pub(crate) async fn solicitar(
        &self,
        name: &str,
        msg: &Value,
        _respuesta: &str,
    ) -> Result<Value, BrokerError> {
        let (rid, rx) = self.router.register("iq");
        self.salida
            .as_ref()
            .ok_or_else(|| BrokerError::Internal("canal de salida cerrado".into()))?
            .send(Saliente {
                name: name.into(),
                msg: msg.clone(),
                request_id: rid.clone(),
            })
            .map_err(|_| BrokerError::Internal("canal de salida cerrado".into()))?;
        let plazo = std::time::Duration::from_secs(self.config.timeout_secs);
        match tokio::time::timeout(plazo, rx).await {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(_)) => Err(BrokerError::Internal("respuesta descartada".into())),
            Err(_) => {
                // Sin respuesta no hay quién recoja el id: si no se retira, la
                // entrada queda en la tabla hasta que llegue un mensaje tardío.
                self.router.discard(&rid);
                Err(BrokerError::Timeout)
            }
        }
    }

    /// Envía sin esperar respuesta (suscripciones).
    pub(crate) async fn enviar_sin_espera(
        &self,
        name: &str,
        msg: &Value,
    ) -> Result<(), BrokerError> {
        // Sin receptor: registrar uno que se descarta al instante dejaría una
        // entrada pendiente por cada suscripción.
        let rid = self.router.next_id("iq");
        self.salida
            .as_ref()
            .ok_or_else(|| BrokerError::Internal("canal de salida cerrado".into()))?
            .send(Saliente {
                name: name.into(),
                msg: msg.clone(),
                request_id: rid,
            })
            .map_err(|_| BrokerError::Internal("canal de salida cerrado".into()))?;
        Ok(())
    }

    pub(crate) async fn estado_lectura(&self) -> tokio::sync::RwLockReadGuard<'_, Estado> {
        self.estado.read().await
    }

    /// Hora actual según el reloj del servidor (epoch segundos).
    pub(crate) async fn hora_servidor_segs(&self) -> i64 {
        let offset = self
            .estado_lectura()
            .await
            .session
            .as_ref()
            .map(|s| s.server_time_offset_ms)
            .unwrap_or(0);
        let ahora_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        (ahora_ms + offset) / 1000
    }

    /// Cierra la cola de salida: la tarea del socket cierra el WSS.
    pub(crate) fn cerrar_salida(&mut self) {
        self.salida = None;
    }
}

/// Extrae el `ssid` de la cookie o del cuerpo del login.
fn extraer_ssid(headers: &reqwest::header::HeaderMap, cuerpo: &Value) -> Option<String> {
    for header in headers.get_all("set-cookie") {
        if let Ok(raw) = header.to_str() {
            if let Some(rest) = raw.strip_prefix("ssid=") {
                return Some(rest.split(';').next().unwrap_or(rest).trim().to_string());
            }
        }
    }
    cuerpo
        .get("ssid")
        .and_then(|s| s.as_str())
        .map(str::to_string)
}

/// Contexto compartido de la tarea de socket (evita listas de 9 argumentos).
pub(crate) struct Ctx {
    pub(crate) router: Arc<RequestRouter>,
    pub(crate) estado: Arc<RwLock<Estado>>,
    pub(crate) catalogo: Arc<RwLock<Vec<Asset>>>,
    pub(crate) sub_velas: Arc<RwLock<Vec<SubVelas>>>,
    pub(crate) eventos: broadcast::Sender<Event>,
    pub(crate) precios: broadcast::Sender<Value>,
    pub(crate) resultados: Arc<RwLock<HashMap<String, OperationResult>>>,
    pub(crate) digitales: Arc<RwLock<HashMap<String, OrderId>>>,
}

/// Tarea única que posee el socket: lee, escribe y enruta.
async fn tarea_socket(
    mut ws: WsClient,
    mut rx_salida: mpsc::UnboundedReceiver<Saliente>,
    ctx: Ctx,
) {
    let motivo = loop {
        tokio::select! {
            frame = ws.siguiente() => {
                match frame {
                    Some(Ok(f)) => {
                        if let Some(rid) = f.request_id.as_deref() {
                            if !ctx.router.deliver(Some(rid), f.msg.clone()) {
                                tracing::debug!(rid, "request_id tardío o desconocido");
                            }
                        }
                        enrutar_evento(&f, &ctx).await;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(error = %e, "error de lectura del websocket");
                        break "error de lectura";
                    }
                    None => {
                        break "el bróker cerró el socket";
                    }
                }
            }
            saliente = rx_salida.recv() => {
                match saliente {
                    Some(s) => {
                        if let Err(e) = ws.enviar(&s.name, &s.msg, &s.request_id).await {
                            tracing::warn!(error = %e, "error de escritura del websocket");
                            break "error de escritura";
                        }
                    }
                    None => {
                        let _ = ws.cerrar().await;
                        break "cierre solicitado";
                    }
                }
            }
        }
    };

    // Cierre ordenado: sin esto, las peticiones en vuelo esperarían hasta su
    // timeout y las suscripciones quedarían como receptores sin emisor.
    ctx.router.fail_all();
    ctx.sub_velas.write().await.clear();
    ctx.estado.write().await.conectado = false;
    tracing::warn!(motivo, "sesión sin conexión; no hay reconexión automática");
}

/// Enruta un frame al estado, a las suscripciones o al bus de eventos.
async fn enrutar_evento(frame: &crate::transport::websocket::WsFrame, ctx: &Ctx) {
    let Ctx {
        router: _,
        estado,
        catalogo,
        sub_velas,
        eventos,
        precios,
        resultados,
        digitales,
    } = ctx;
    match frame.name.as_str() {
        msg::TIME_SYNC => {
            if let Some(ms) = frame.msg.as_i64() {
                let ahora = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(ms);
                let mut e = estado.write().await;
                if let Some(s) = e.session.as_mut() {
                    s.server_time_offset_ms = ms - ahora;
                }
            }
        }
        msg::PROFILE | msg::BALANCES => {
            let balances = if frame.msg.is_array() {
                mapper::balances_desde_lista(&frame.msg)
            } else {
                mapper::balances_desde_perfil(&frame.msg)
            };
            let mut e = estado.write().await;
            if !balances.is_empty() {
                e.balances = balances;
            }
        }
        msg::BALANCE_CHANGED => {
            if let Some(bal) = mapper::balance_desde_cambio(&frame.msg) {
                let mut e = estado.write().await;
                e.balances.retain(|b| b.id != bal.id);
                e.balances.push(bal.clone());
                let _ = eventos.send(Event::BalanceChanged(bal));
            }
        }
        msg::CANDLE_GENERATED => {
            let active_id = frame
                .msg
                .get("active_id")
                .and_then(|a| a.as_u64())
                .unwrap_or(0) as u32;
            let catalogo = catalogo.read().await;
            if let Some(symbol) = catalogo
                .iter()
                .find(|a| a.opcode == active_id)
                .map(|a| a.symbol.clone())
            {
                // Tick de precio nativo (densidad intrasegundo, sin polling).
                if let Some(tick) = mapper::tick_desde_frame(&frame.msg) {
                    let _ = eventos.send(Event::PriceTick {
                        symbol: symbol.clone(),
                        tick,
                    });
                }
                if let Some((_, candle)) = mapper::vela_viva_unica(&frame.msg) {
                    // Poda: las suscripciones cuyo stream soltó el consumidor se
                    // retiran aquí (no hay API de baja explícita).
                    let mut subs = sub_velas.write().await;
                    subs.retain(|s| !s.tx.is_closed());
                    for sub in subs.iter() {
                        if sub.symbol == symbol
                            && (sub.interval == 0 || sub.interval as i64 == candle.to - candle.from)
                        {
                            let _ = sub.tx.send(candle);
                        }
                    }
                }
            }
        }
        msg::CANDLES_GENERATED => {
            let active_id = frame
                .msg
                .get("active_id")
                .and_then(|a| a.as_u64())
                .unwrap_or(0) as u32;
            if active_id > 0 {
                let catalogo = catalogo.read().await;
                if let Some(symbol) = catalogo
                    .iter()
                    .find(|a| a.opcode == active_id)
                    .map(|a| a.symbol.clone())
                {
                    // Tick de precio nativo (densidad intrasegundo, sin polling).
                    if let Some(tick) = mapper::tick_desde_frame(&frame.msg) {
                        let _ = eventos.send(Event::PriceTick {
                            symbol: symbol.clone(),
                            tick,
                        });
                    }
                    let mut subs = sub_velas.write().await;
                    subs.retain(|s| !s.tx.is_closed());
                    for (size, candle) in mapper::velas_vivas_multi(&frame.msg) {
                        for sub in subs.iter() {
                            if sub.symbol == symbol && (sub.interval == 0 || sub.interval == size) {
                                let _ = sub.tx.send(candle);
                            }
                        }
                    }
                }
            }
        }
        msg::CLIENT_PRICE_GENERATED => {
            // Payout digital en vivo: se republica crudo y como evento.
            let _ = precios.send(frame.msg.clone());
            if let Some((active_id, payout)) = mapper::payout_price_splitter_con_id(&frame.msg) {
                let catalogo = catalogo.read().await;
                if let Some(asset) = catalogo.iter().find(|a| a.opcode == active_id) {
                    let payout = Payout {
                        symbol: asset.symbol.clone(),
                        ..payout
                    };
                    let _ = eventos.send(Event::PayoutChanged(payout));
                }
            }
        }
        msg::OPTION_OPENED => {
            if let Some(operacion) = mapper::operacion_desde_apertura(&frame.msg) {
                let _ = eventos.send(Event::OrderOpened(operacion));
            }
        }
        msg::OPTION_CLOSED | msg::SOCKET_OPTION_CLOSED => {
            let (id, result) = if frame.name == msg::OPTION_CLOSED {
                (
                    frame.msg.get("option_id").map(|i| i.to_string()),
                    mapper::resultado_desde_cierre(&frame.msg),
                )
            } else {
                (
                    frame.msg.get("id").map(|i| i.to_string()),
                    mapper::resultado_desde_socket_cerrado(&frame.msg),
                )
            };
            if let (Some(id), Some(result)) = (id, result) {
                // Corrección 2026-10-31 (informe del consumidor): `insert`
                // SIEMPRE escribe, así que el segundo canal pisaba el cache.
                // Ahora el primer cierre con datos fija cache y evento; los
                // siguientes solo confirman. (Los mappers además descartan
                // mensajes de cierre incompletos: `win` sin crédito.)
                let mut cache = resultados.write().await;
                if !cache.contains_key(&id) {
                    cache.insert(id.clone(), result);
                    drop(cache);
                    let _ = eventos.send(Event::OrderClosed { id, result });
                }
            }
        }
        msg::POSITION_CHANGED => {
            if let Some((instrument_id, result)) = mapper::cierre_digital_desde_position(&frame.msg)
            {
                let order_id = digitales.write().await.remove(&instrument_id);
                if let Some(id) = order_id {
                    let mut cache = resultados.write().await;
                    if !cache.contains_key(&id) {
                        cache.insert(id.clone(), result);
                        drop(cache);
                        let _ = eventos.send(Event::OrderClosed { id, result });
                    }
                }
            }
        }
        msg::LIVE_DEAL | msg::LIVE_DEAL_BINARY => {
            let active_id = frame
                .msg
                .get("active_id")
                .or_else(|| frame.msg.get("instrument_active_id"))
                .and_then(|a| a.as_i64())
                .unwrap_or(0) as u32;
            let catalogo = catalogo.read().await;
            if let Some(symbol) = catalogo
                .iter()
                .find(|a| a.opcode == active_id)
                .map(|a| a.symbol.clone())
            {
                if let Some(deal) = mapper::live_deal_desde_frame(&symbol, &frame.msg) {
                    let _ = eventos.send(Event::LiveDeal(deal));
                }
            }
        }
        msg::COMMISSION_CHANGED => {
            let active_id = frame
                .msg
                .get("active_id")
                .and_then(|a| a.as_i64())
                .unwrap_or(0) as u32;
            let catalogo = catalogo.read().await;
            if let Some(symbol) = catalogo
                .iter()
                .find(|a| a.opcode == active_id)
                .map(|a| a.symbol.clone())
            {
                if let Some(cambio) = mapper::comision_desde_frame(&symbol, &frame.msg) {
                    let _ = eventos.send(Event::CommissionChanged(cambio));
                }
            }
        }
        _ => {
            if let Some(evento) = mapper::evento_desde_frame(frame) {
                let _ = eventos.send(evento);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::websocket::WsFrame;

    fn frame(name: &str, msg: Value) -> WsFrame {
        WsFrame {
            name: name.into(),
            msg,
            request_id: None,
        }
    }

    struct CtxPrueba {
        ctx: Ctx,
        rx: broadcast::Receiver<Event>,
        resultados: Arc<RwLock<HashMap<String, OperationResult>>>,
    }

    fn ctx() -> CtxPrueba {
        let (tx, rx) = broadcast::channel(64);
        let resultados = Arc::new(RwLock::new(HashMap::new()));
        CtxPrueba {
            ctx: Ctx {
                router: Arc::new(RequestRouter::new()),
                estado: Arc::new(RwLock::new(Estado::default())),
                catalogo: Arc::new(RwLock::new(Vec::new())),
                sub_velas: Arc::new(RwLock::new(Vec::new())),
                eventos: tx,
                precios: broadcast::channel(64).0,
                resultados: resultados.clone(),
                digitales: Arc::new(RwLock::new(HashMap::new())),
            },
            rx,
            resultados,
        }
    }

    /// Informe 2026-10-31: el cierre llega por dos canales; el cache debe
    /// quedar con el primer resultado válido y emitirse UN solo evento.
    #[tokio::test]
    async fn cache_no_se_pisa_con_el_segundo_canal() {
        let prueba = ctx();
        let ctx = prueba.ctx;
        let mut rx = prueba.rx;
        let resultados = prueba.resultados;

        // 1) option-closed con datos completos → Win(0.83)
        enrutar_evento(
            &frame(
                "option-closed",
                json!({"option_id": 14405260361i64, "result": "win", "amount": 1, "win_enrolled_amount": 1.83}),
            ),
            &ctx,
        )
        .await;

        // 2) socket-option-closed posterior e incompleto (win sin win_amount)
        enrutar_evento(
            &frame(
                "socket-option-closed",
                json!({"id": 14405260361i64, "win": "win", "amount": 1000000}),
            ),
            &ctx,
        )
        .await;

        // Cache == evento
        let cache = resultados.read().await;
        let guardado = cache.get("14405260361").copied();
        assert_eq!(guardado, Some(OperationResult::Win(0.83)));
        drop(cache);

        // Un solo evento, con el resultado bueno
        let ev = rx.try_recv().expect("debe haberse emitido un evento");
        match ev {
            Event::OrderClosed { id, result } => {
                assert_eq!(id, "14405260361");
                assert_eq!(result, OperationResult::Win(0.83));
            }
            otra => panic!("evento inesperado: {otra:?}"),
        }
        assert!(
            rx.try_recv().is_err(),
            "no debe emitirse un segundo OrderClosed"
        );
    }
}
