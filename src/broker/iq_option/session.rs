//! Sesión del adaptador IQ Option (núcleo async).
//!
//! Arquitectura: una única tarea posee el WebSocket y atiende un `select!`
//! entre mensajes entrantes y una cola de salida. Las llamadas del trait
//! generan un `request_id`, lo registran en el router, envían por la cola y
//! esperan la respuesta con timeout. Cero polling, cero estado global.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, Notify, RwLock};

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

/// Suscripción registrada: se reproduce íntegra tras una reconexión.
#[derive(Debug, Clone)]
pub(crate) struct Suscripcion {
    pub(crate) clave: String,
    pub(crate) name: String,
    pub(crate) msg: Value,
}

/// Motivo con el que el bucle del socket termina por petición del consumidor.
const MOTIVO_CIERRE: &str = "cierre solicitado";

/// Registra una suscripción si su clave no estaba presente.
///
/// Devuelve `true` cuando es nueva (y hay que enviarla), `false` si ya estaba
/// registrada: el bróker mantiene la suscripción mientras la sesión siga viva.
fn registrar_suscripcion(
    lista: &mut Vec<Suscripcion>,
    clave: &str,
    name: &str,
    msg: &Value,
) -> bool {
    if lista.iter().any(|s| s.clave == clave) {
        return false;
    }
    lista.push(Suscripcion {
        clave: clave.to_string(),
        name: name.to_string(),
        msg: msg.clone(),
    });
    true
}

/// Siguiente espera del backoff: duplica y satura en `maximo`.
fn siguiente_espera(actual: u64, maximo: u64) -> u64 {
    let techo = maximo.max(1);
    actual.saturating_mul(2).clamp(1, techo)
}

/// Un rechazo de credenciales no se reintenta: insistir bloquea la cuenta.
fn es_fatal(e: &BrokerError) -> bool {
    matches!(e, BrokerError::Authentication(_))
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
    salida: Option<mpsc::Sender<Saliente>>,
    /// Suscripciones activas, para reproducirlas tras una reconexión.
    pub(crate) suscripciones: Arc<RwLock<Vec<Suscripcion>>>,
    /// `true` mientras la tarea del socket tiene un WebSocket operativo.
    pub(crate) vivo: Arc<AtomicBool>,
    /// Señal de caída simulada, solo para pruebas de resiliencia.
    pub(crate) cortar: Arc<Notify>,
    /// Órdenes abiertas sin cierre observado (base de la reconciliación).
    pub(crate) abiertas: Arc<RwLock<HashSet<OrderId>>>,
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

        // Cola acotada: con el socket caído el productor espera en vez de
        // acumular órdenes en memoria; una caída sostenida devuelve error.
        let (tx_salida, rx_salida) = mpsc::channel::<Saliente>(config.transport.cola_salida.max(1));
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
        let suscripciones: Arc<RwLock<Vec<Suscripcion>>> = Arc::new(RwLock::new(Vec::new()));
        let vivo = Arc::new(AtomicBool::new(true));
        let cortar = Arc::new(Notify::new());
        let abiertas: Arc<RwLock<HashSet<OrderId>>> = Arc::new(RwLock::new(HashSet::new()));

        let broker = Self {
            config,
            salida: Some(tx_salida),
            suscripciones: suscripciones.clone(),
            vivo: vivo.clone(),
            cortar: cortar.clone(),
            abiertas: abiertas.clone(),
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
                config: broker.config.clone(),
                suscripciones: suscripciones.clone(),
                vivo: vivo.clone(),
                cortar: cortar.clone(),
                abiertas: abiertas.clone(),
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
                .suscribir(
                    &format!("posicion:{instrumento}"),
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
        // Sin socket no se encola nada: una orden guardada para más tarde se
        // ejecutaría contra un mercado distinto al que se decidió.
        if !self.vivo.load(Ordering::Acquire) {
            return Err(BrokerError::NotConnected);
        }
        let (rid, rx) = self.router.register("iq");
        self.salida
            .as_ref()
            .ok_or_else(|| BrokerError::Internal("canal de salida cerrado".into()))?
            .send(Saliente {
                name: name.into(),
                msg: msg.clone(),
                request_id: rid.clone(),
            })
            .await
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
        if !self.vivo.load(Ordering::Acquire) {
            return Err(BrokerError::NotConnected);
        }
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
            .await
            .map_err(|_| BrokerError::Internal("canal de salida cerrado".into()))?;
        Ok(())
    }

    /// Registra una suscripción y la envía.
    ///
    /// El registro permite reproducirla tras una reconexión; repetir la misma
    /// clave no genera tráfico duplicado.
    pub(crate) async fn suscribir(
        &self,
        clave: &str,
        name: &str,
        msg: &Value,
    ) -> Result<(), BrokerError> {
        let nueva = {
            let reg = self.suscripciones.read().await;
            !reg.iter().any(|s| s.clave == clave)
        };
        if !nueva {
            return Ok(());
        }
        // Primero se envía: si no hay socket, la clave no queda registrada y el
        // replay no reproduciría una suscripción que nunca llegó al bróker.
        self.enviar_sin_espera(name, msg).await?;
        let mut reg = self.suscripciones.write().await;
        registrar_suscripcion(&mut reg, clave, name, msg);
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

    /// Fuerza la caída del WebSocket para ejercitar el camino de reconexión.
    ///
    /// Herramienta de pruebas de resiliencia: cierra el socket como si el
    /// bróker lo hubiera cortado. La sesión debe reconectar, restaurar el
    /// estado, reproducir suscripciones y emitir `Event::Reconnected`.
    /// No debe usarse en operación normal.
    #[doc(hidden)]
    pub async fn simular_caida_de_conexion(&self) {
        self.cortar.notify_one();
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
    pub(crate) config: IqOptionConfig,
    pub(crate) suscripciones: Arc<RwLock<Vec<Suscripcion>>>,
    pub(crate) vivo: Arc<AtomicBool>,
    pub(crate) cortar: Arc<Notify>,
    pub(crate) abiertas: Arc<RwLock<HashSet<OrderId>>>,
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
///
/// Si la conexión cae, reautentica, reconstruye el WebSocket, restaura el
/// estado y reproduce las suscripciones antes de volver a despachar. El
/// consumidor recibe `Event::Disconnected` y `Event::Reconnected`, y sus
/// streams siguen vivos.
async fn tarea_socket(mut ws: WsClient, mut rx_salida: mpsc::Receiver<Saliente>, ctx: Ctx) {
    loop {
        let motivo = bucle_socket(&mut ws, &mut rx_salida, &ctx).await;

        // Cierre ordenado: sin esto las peticiones en vuelo esperarían hasta su
        // timeout y quedarían registradas hasta que llegara un mensaje tardío.
        ctx.router.fail_all();
        ctx.vivo.store(false, Ordering::Release);
        ctx.estado.write().await.conectado = false;
        let _ = ctx.eventos.send(Event::Disconnected {
            motivo: motivo.to_string(),
        });

        if motivo == MOTIVO_CIERRE {
            break;
        }

        tracing::warn!(motivo, "conexión perdida; reintentando");
        let inicio = tokio::time::Instant::now();
        let mut intentos = 0u32;
        match reconectar(&ctx, &mut intentos).await {
            Ok(nuevo) => {
                ws = nuevo;
                ctx.vivo.store(true, Ordering::Release);
                ctx.estado.write().await.conectado = true;
                let _ = ctx.eventos.send(Event::Reconnected {
                    intentos,
                    downtime_ms: inicio.elapsed().as_millis() as u64,
                });
                tracing::info!(intentos, "sesión restablecida");
            }
            Err(e) => {
                tracing::error!(error = %e, "reconexión abandonada");
                break;
            }
        }
    }

    // Solo al terminar definitivamente: los streams de velas dejan de recibir.
    ctx.sub_velas.write().await.clear();
    ctx.suscripciones.write().await.clear();
    tracing::warn!("sesión cerrada; no habrá más reconexiones");
}

/// Bucle de despacho mientras el socket esté vivo. Devuelve el motivo de salida.
async fn bucle_socket(
    ws: &mut WsClient,
    rx_salida: &mut mpsc::Receiver<Saliente>,
    ctx: &Ctx,
) -> &'static str {
    loop {
        tokio::select! {
            frame = ws.siguiente() => {
                match frame {
                    Some(Ok(f)) => {
                        if let Some(rid) = f.request_id.as_deref() {
                            if !ctx.router.deliver(Some(rid), f.msg.clone()) {
                                tracing::debug!(rid, "request_id tardío o desconocido");
                            }
                        }
                        enrutar_evento(&f, ctx).await;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(error = %e, "error de lectura del websocket");
                        return "error de lectura";
                    }
                    None => return "el bróker cerró el socket",
                }
            }

            saliente = rx_salida.recv() => {
                match saliente {
                    Some(s) => {
                        if let Err(e) = ws.enviar(&s.name, &s.msg, &s.request_id).await {
                            tracing::warn!(error = %e, "error de escritura del websocket");
                            return "error de escritura";
                        }
                    }
                    None => {
                        let _ = ws.cerrar().await;
                        return MOTIVO_CIERRE;
                    }
                }
            }

            _ = ctx.cortar.notified() => {
                // Solo pruebas de resiliencia: se comporta como un corte externo.
                tracing::warn!("caída simulada: cerrando el socket");
                let _ = ws.cerrar().await;
                return "caída simulada";
            }
        }
    }
}

/// Inicia sesión y devuelve el `ssid` vigente.
async fn iniciar_sesion(ctx: &Ctx) -> Result<String, BrokerError> {
    let http = HttpClient::new(
        &endpoints::api_base_url(&ctx.config.auth_host),
        ctx.config.transport.clone(),
    )?;
    let (cuerpo, headers) = http
        .post_json_completo(
            endpoints::LOGIN_PATH.trim_start_matches('/'),
            &json!({ "identifier": ctx.config.email, "password": ctx.config.password }),
        )
        .await?;
    extraer_ssid(&headers, &cuerpo)
        .ok_or_else(|| BrokerError::Authentication("el login no devolvió cookie ssid".into()))
}

/// Reautentica y restaura estado y suscripciones, con backoff exponencial.
async fn reconectar(ctx: &Ctx, intentos: &mut u32) -> Result<WsClient, BrokerError> {
    let cfg = &ctx.config.transport;
    let mut espera = siguiente_espera(0, cfg.backoff_base_ms.max(1));
    let mut ssid: Option<String> = None;

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(espera)).await;
        *intentos += 1;

        let credencial = match ssid.clone() {
            Some(s) => Ok(s),
            None => iniciar_sesion(ctx).await,
        };
        match credencial {
            Ok(sesion) => match abrir_y_restaurar(ctx, &sesion).await {
                Ok(ws) => return Ok(ws),
                Err(e) if es_fatal(&e) => return Err(e),
                Err(e) => {
                    // La sesión anterior pudo caducar: el próximo intento loguea.
                    tracing::warn!(error = %e, "no se pudo restaurar la sesión");
                    ssid = None;
                }
            },
            Err(e) if es_fatal(&e) => return Err(e),
            Err(e) => tracing::warn!(error = %e, "login fallido durante la reconexión"),
        }
        espera = siguiente_espera(espera, cfg.backoff_max_ms);
    }
}

/// Abre el WebSocket con el `ssid` dado y restaura perfil, catálogo y suscripciones.
async fn abrir_y_restaurar(ctx: &Ctx, ssid: &str) -> Result<WsClient, BrokerError> {
    let mut ws = WsClient::connect_con_reintentos(
        &endpoints::ws_url(&ctx.config.ws_host),
        vec![("Cookie".to_string(), format!("ssid={ssid}"))],
        ctx.config.transport.clone(),
    )
    .await?;

    let plazo = std::time::Duration::from_secs(ctx.config.timeout_secs);
    let perfil = solicitar_directo(&mut ws, ctx, msg::SSID, &json!(ssid), plazo).await?;
    let (session, balances) = mapper::sesion_desde_perfil(&perfil, &ctx.config.account)
        .ok_or_else(|| BrokerError::Rejected("profile sin balances utilizables".into()))?;
    {
        let mut e = ctx.estado.write().await;
        e.session = Some(session);
        e.balances = balances;
    }

    let init = solicitar_directo(
        &mut ws,
        ctx,
        msg::SEND_MESSAGE,
        &json!({ "name": msg::GET_INITIALIZATION_DATA, "version": "3.0", "body": {} }),
        plazo,
    )
    .await?;
    {
        let mut c = ctx.catalogo.write().await;
        *c = mapper::catalogo_desde_init(&init);
    }

    // Reproducción íntegra: sin esto el bróker dejaría de emitir velas, precios
    // y cierres de posiciones para esta sesión.
    let pendientes = ctx.suscripciones.read().await.clone();
    for sub in pendientes {
        let rid = ctx.router.next_id("iq");
        ws.enviar(&sub.name, &sub.msg, &rid).await?;
    }

    // Reconciliación: un cierre ocurrido sin conexión no llegó por evento. El
    // historial de posiciones es la única fuente disponible para recuperarlo.
    if let Ok(historial) = pedir_historial(&mut ws, ctx, plazo).await {
        let pendientes = ctx.abiertas.read().await.clone();
        for (id, result) in reconciliar(&pendientes, &historial) {
            let nuevo = {
                let mut cache = ctx.resultados.write().await;
                if cache.contains_key(&id) {
                    false
                } else {
                    cache.insert(id.clone(), result);
                    true
                }
            };
            ctx.abiertas.write().await.remove(&id);
            if nuevo {
                let _ = ctx.eventos.send(Event::OrderClosed { id, result });
            }
        }
    } else {
        tracing::warn!("sin historial de posiciones: no se pudo reconciliar");
    }
    Ok(ws)
}

/// Historial de posiciones cerradas del balance activo.
async fn pedir_historial(
    ws: &mut WsClient,
    ctx: &Ctx,
    plazo: std::time::Duration,
) -> Result<PositionHistory, BrokerError> {
    let balance_id = ctx
        .estado
        .read()
        .await
        .session
        .as_ref()
        .and_then(|s| s.balance_id);
    let respuesta = solicitar_directo(
        ws,
        ctx,
        msg::SEND_MESSAGE,
        &json!({
            "name": msg::PORTFOLIO_GET_HISTORY,
            "body": {
                "instrument_types": ["turbo-option", "binary-option", "digital-option"],
                "limit": 100,
                "offset": 0,
                "start": 0,
                "end": 0,
                "user_balance_id": balance_id
            }
        }),
        plazo,
    )
    .await?;
    mapper::historial_desde_positions(&respuesta, 0)
        .ok_or_else(|| BrokerError::Rejected("historial sin positions".into()))
}

/// Traduce una posición cerrada al resultado contable del dominio.
fn resultado_desde_posicion(p: &Position) -> Option<OperationResult> {
    let pnl = p.pnl?;
    Some(if pnl > 0.0 {
        OperationResult::Win(pnl)
    } else if pnl < 0.0 {
        OperationResult::Loss(pnl.abs())
    } else {
        OperationResult::Equal(p.amount)
    })
}

/// Cierres que el historial conoce y la sesión aún tiene como abiertos.
///
/// Solo se aceptan identificadores pendientes: un cierre ajeno al consumidor
/// no debe aparecer en su caché de resultados.
fn reconciliar(
    pendientes: &HashSet<OrderId>,
    historial: &PositionHistory,
) -> Vec<(OrderId, OperationResult)> {
    let mut vistos = HashSet::new();
    historial
        .positions
        .iter()
        .filter(|p| pendientes.contains(&p.id) && vistos.insert(p.id.clone()))
        .filter_map(|p| resultado_desde_posicion(p).map(|r| (p.id.clone(), r)))
        .collect()
}

/// Petición correlacionada sobre un socket que aún no tiene tarea de despacho.
///
/// Mientras espera la respuesta sigue enrutando lo que llegue (time-sync,
/// eventos), para no perder mensajes durante la reautenticación.
async fn solicitar_directo(
    ws: &mut WsClient,
    ctx: &Ctx,
    name: &str,
    msg: &Value,
    plazo: std::time::Duration,
) -> Result<Value, BrokerError> {
    let (rid, mut rx) = ctx.router.register("iq");
    ws.enviar(name, msg, &rid).await?;
    let limite = tokio::time::Instant::now() + plazo;
    loop {
        tokio::select! {
            respuesta = &mut rx => {
                return respuesta.map_err(|_| BrokerError::Internal("respuesta descartada".into()));
            }
            frame = ws.siguiente() => {
                match frame {
                    Some(Ok(f)) => {
                        if f.request_id.as_deref() == Some(rid.as_str()) {
                            return Ok(f.msg);
                        }
                        if let Some(otro) = f.request_id.as_deref() {
                            let _ = ctx.router.deliver(Some(otro), f.msg.clone());
                        }
                        enrutar_evento(&f, ctx).await;
                    }
                    Some(Err(e)) => return Err(BrokerError::Transport(e)),
                    None => {
                        return Err(BrokerError::Connection(
                            "el socket se cerró durante la reautenticación".into(),
                        ))
                    }
                }
            }
            _ = tokio::time::sleep_until(limite) => {
                ctx.router.discard(&rid);
                return Err(BrokerError::Timeout);
            }
        }
    }
}

/// Enruta un frame al estado, a las suscripciones o al bus de eventos.
async fn enrutar_evento(frame: &crate::transport::websocket::WsFrame, ctx: &Ctx) {
    let Ctx {
        config: _,
        suscripciones: _,
        vivo: _,
        cortar: _,
        abiertas,
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
                    abiertas.write().await.remove(&id);
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
                        abiertas.write().await.remove(&id);
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
                config: IqOptionConfig::default(),
                suscripciones: Arc::new(RwLock::new(Vec::new())),
                vivo: Arc::new(AtomicBool::new(true)),
                cortar: Arc::new(Notify::new()),
                abiertas: Arc::new(RwLock::new(HashSet::new())),
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

    /// El registro de suscripciones alimenta el replay: no admite duplicados.
    #[test]
    fn registro_de_suscripciones_no_duplica_claves() {
        let mut lista = Vec::new();
        assert!(registrar_suscripcion(
            &mut lista,
            "velas:EURUSD",
            "subscribeMessage",
            &json!({"name": "candles-generated"})
        ));
        assert!(!registrar_suscripcion(
            &mut lista,
            "velas:EURUSD",
            "subscribeMessage",
            &json!({"name": "candles-generated"})
        ));
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].clave, "velas:EURUSD");
    }

    /// El backoff arranca en 1 ms, duplica y satura en el techo configurado.
    #[test]
    fn backoff_duplica_y_satura() {
        assert_eq!(siguiente_espera(0, 8_000), 1);
        assert_eq!(siguiente_espera(1, 8_000), 2);
        assert_eq!(siguiente_espera(4_000, 8_000), 8_000);
        assert_eq!(siguiente_espera(8_000, 8_000), 8_000);
        assert_eq!(siguiente_espera(8_000, 0), 1);
    }

    /// La reconciliación solo completa órdenes propias sin cierre observado.
    #[test]
    fn reconciliacion_solo_rellena_ordenes_pendientes() {
        let posicion = |id: &str, pnl: Option<f64>| Position {
            id: id.into(),
            instrument_type: "turbo-option".into(),
            symbol: "EURUSD".into(),
            side: Side::Buy,
            amount: 1.0,
            open_price: Some(1.1),
            pnl,
            invested: Some(1.0),
            open_at: 1,
            close_at: Some(2),
            leverage: None,
        };
        let historial = PositionHistory {
            positions: vec![
                posicion("111", Some(0.83)),
                posicion("222", Some(-1.0)),
                posicion("333", Some(0.0)),
                posicion("999", Some(5.0)),
                posicion("111", Some(0.83)),
            ],
            offset: 0,
            total: Some(5),
        };
        let pendientes: HashSet<OrderId> = ["111", "222", "333"]
            .into_iter()
            .map(str::to_string)
            .collect();

        let mut filas = reconciliar(&pendientes, &historial);
        filas.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            filas,
            vec![
                ("111".to_string(), OperationResult::Win(0.83)),
                ("222".to_string(), OperationResult::Loss(1.0)),
                ("333".to_string(), OperationResult::Equal(1.0)),
            ]
        );
    }

    /// Un fallo de credenciales es terminal; uno de red se reintenta.
    #[test]
    fn solo_la_autenticacion_es_fatal() {
        assert!(es_fatal(&BrokerError::Authentication("rechazado".into())));
        assert!(!es_fatal(&BrokerError::Connection("cortado".into())));
        assert!(!es_fatal(&BrokerError::Timeout));
    }
}
