//! Sesión del adaptador Deriv: WebSocket, OTP, router y supervisor.
//!
//! Arquitectura: una tarea posee el socket y atiende un `select!` entre la cola
//! de salida y los mensajes entrantes. Las llamadas registran un `req_id`
//! numérico y esperan su respuesta con plazo. La reconexión pide un **OTP
//! nuevo** (el anterior es de un solo uso) y reproduce las suscripciones.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Notify, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::broker::config::{AccountSelection, BrokerParams};
use crate::broker::deriv::config::{DerivConfig, LimitesTasa};
use crate::broker::deriv::protocol;
use crate::broker::error::BrokerError;
use crate::broker::session::Session;
use crate::broker::trait_::{Broker, CandleStream, EventStream};
use crate::domain::*;

// ---------------------------------------------------------------------------
// Tipos internos
// ---------------------------------------------------------------------------

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type Escritor = SplitSink<Ws, Message>;
type Lector = SplitStream<Ws>;

/// Plazo por defecto de una petición.
const PLAZO: Duration = Duration::from_secs(15);

/// Granularidades de velas admitidas por Deriv.
const GRANULARIDADES: [u64; 12] = [
    60, 120, 180, 300, 600, 900, 1800, 3600, 7200, 14400, 28800, 86400,
];

/// Clase de presupuesto de tasa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Clase {
    Trading,
    Balance,
    Cartera,
    Resto,
}

/// Limitador por ventana deslizante (minuto y hora).
struct Limitador {
    cfg: LimitesTasa,
    historial: Mutex<HashMap<Clase, VecDeque<Instant>>>,
}

impl Limitador {
    fn new(cfg: LimitesTasa) -> Self {
        Self {
            cfg,
            historial: Mutex::new(HashMap::new()),
        }
    }

    fn cupo(&self, clase: Clase) -> (usize, usize) {
        match clase {
            Clase::Trading => (
                self.cfg.trading_por_min as usize,
                self.cfg.trading_por_hora as usize,
            ),
            Clase::Balance => (
                self.cfg.balance_por_min as usize,
                self.cfg.balance_por_hora as usize,
            ),
            Clase::Cartera => (
                self.cfg.cartera_por_min as usize,
                self.cfg.cartera_por_hora as usize,
            ),
            Clase::Resto => (
                self.cfg.resto_por_min as usize,
                self.cfg.resto_por_hora as usize,
            ),
        }
    }

    /// Espera hasta que haya hueco. Nunca falla: el objetivo es no recibir
    /// `RateLimit` del bróker, no descartar trabajo.
    async fn esperar(&self, clase: Clase) {
        let (min, hora) = self.cupo(clase);
        loop {
            let espera = {
                let ahora = Instant::now();
                let mut guard = self.historial.lock().expect("limitador no envenenado");
                let cola = guard.entry(clase).or_default();
                while let Some(t) = cola.front() {
                    if ahora.duration_since(*t) >= Duration::from_secs(3600) {
                        cola.pop_front();
                    } else {
                        break;
                    }
                }
                let en_minuto = cola
                    .iter()
                    .filter(|t| ahora.duration_since(**t) < Duration::from_secs(60))
                    .count();
                if en_minuto < min && cola.len() < hora {
                    cola.push_back(ahora);
                    None
                } else if en_minuto >= min {
                    let mas_viejo = cola
                        .iter()
                        .find(|t| ahora.duration_since(**t) < Duration::from_secs(60))
                        .copied();
                    mas_viejo.map(|t| {
                        Duration::from_secs(60)
                            .saturating_sub(ahora.duration_since(t))
                            .max(Duration::from_millis(50))
                    })
                } else {
                    Some(Duration::from_secs(5))
                }
            };
            match espera {
                None => return,
                Some(d) => tokio::time::sleep(d).await,
            }
        }
    }
}

/// Estado compartido entre el adaptador y la tarea del socket.
struct Interna {
    router: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    contador: AtomicU64,
    salida: mpsc::Sender<Value>,
    vivo: AtomicBool,
    activo: AtomicBool,
    cortar: Notify,
    eventos: broadcast::Sender<Event>,
    estado: RwLock<Option<Session>>,
    balance: RwLock<Option<Balance>>,
    catalogo: RwLock<Vec<Asset>>,
    resultados: RwLock<HashMap<OrderId, OperationResult>>,
    abiertas: RwLock<HashSet<OrderId>>,
    subs: RwLock<Vec<Value>>,
    sub_velas: RwLock<HashMap<String, mpsc::Sender<Candle>>>,
    limite: Limitador,
    listo: watch::Sender<u64>,
    /// Tipo de la cuenta resuelta por REST (Deriv usa prefijos nuevos).
    tipo_cuenta: RwLock<AccountType>,
    pausa_reconexion_segs: AtomicU64,
    ultimo_error: Mutex<Option<String>>,
}

impl Interna {
    fn siguiente_id(&self) -> u64 {
        self.contador.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Envía y espera la respuesta correlacionada.
    async fn solicitar(&self, msg: Value, plazo: Duration) -> Result<Value, BrokerError> {
        if !self.vivo.load(Ordering::SeqCst) {
            return Err(BrokerError::NotConnected);
        }
        let id = self.siguiente_id();
        let mut msg = msg;
        msg["req_id"] = json!(id);
        let (tx, rx) = oneshot::channel();
        self.router
            .lock()
            .expect("router no envenenado")
            .insert(id, tx);
        if self.salida.send(msg).await.is_err() {
            self.router
                .lock()
                .expect("router no envenenado")
                .remove(&id);
            return Err(BrokerError::NotConnected);
        }
        match tokio::time::timeout(plazo, rx).await {
            Ok(Ok(valor)) => interpretar(valor),
            Ok(Err(_)) => Err(BrokerError::NotConnected),
            Err(_) => {
                self.router
                    .lock()
                    .expect("router no envenenado")
                    .remove(&id);
                Err(BrokerError::Timeout)
            }
        }
    }

    /// Registra una suscripción para reproducirla tras reconectar.
    async fn registrar_sub(&self, msg: Value) {
        let mut lista = self.subs.write().await;
        if !lista.contains(&msg) {
            lista.push(msg);
        }
    }
}

/// Error del bróker a partir del objeto `error` de un mensaje.
fn error_de(e: &Value) -> BrokerError {
    let code = e.get("code").and_then(Value::as_str).unwrap_or("Error");
    let msg = e
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("error del bróker");
    match code {
        "InvalidToken" | "Unauthorized" | "AuthorizationRequired" => {
            BrokerError::Authentication(format!("deriv {code}: {msg}"))
        }
        _ => BrokerError::Rejected(format!("deriv {code}: {msg}")),
    }
}

fn interpretar(valor: Value) -> Result<Value, BrokerError> {
    match valor.get("error") {
        Some(e) => Err(error_de(e)),
        None => Ok(valor),
    }
}

fn epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn tipo_cuenta(id: &str, sel: AccountSelection) -> AccountType {
    if id.starts_with("VRTC") || id.starts_with("VRT") || id.starts_with("DOT") {
        AccountType::Practice
    } else if id.starts_with("CR") || id.starts_with("ROT") {
        AccountType::Real
    } else {
        match sel {
            AccountSelection::Practice => AccountType::Practice,
            AccountSelection::Real => AccountType::Real,
            AccountSelection::Default => AccountType::Unknown,
        }
    }
}

/// Duración en la unidad más grande que divida exactamente.
fn duracion(segundos: u64) -> (u64, &'static str) {
    if segundos >= 86_400 && segundos % 86_400 == 0 {
        (segundos / 86_400, "d")
    } else if segundos >= 3_600 && segundos % 3_600 == 0 {
        (segundos / 3_600, "h")
    } else if segundos >= 60 && segundos % 60 == 0 {
        (segundos / 60, "m")
    } else {
        (segundos.max(1), "s")
    }
}

// ---------------------------------------------------------------------------
// Adaptador
// ---------------------------------------------------------------------------

/// Adaptador Deriv.
pub struct DerivBroker {
    config: DerivConfig,
    interna: Arc<Interna>,
    tarea: Option<JoinHandle<()>>,
}

impl DerivBroker {
    /// Construye el adaptador y espera la primera conexión.
    pub async fn nuevo(config: DerivConfig) -> Result<Self, BrokerError> {
        config.validar()?;
        let capacidad = config.transport.cola_salida.clamp(16, 4096);
        let (salida, salida_rx) = mpsc::channel::<Value>(capacidad);
        let (listo, mut rx_listo) = watch::channel(0u64);
        let (eventos, _) = broadcast::channel(512);

        let interna = Arc::new(Interna {
            router: Mutex::new(HashMap::new()),
            contador: AtomicU64::new(0),
            salida,
            vivo: AtomicBool::new(false),
            activo: AtomicBool::new(true),
            cortar: Notify::new(),
            eventos,
            estado: RwLock::new(None),
            balance: RwLock::new(None),
            catalogo: RwLock::new(Vec::new()),
            resultados: RwLock::new(HashMap::new()),
            abiertas: RwLock::new(HashSet::new()),
            subs: RwLock::new(Vec::new()),
            sub_velas: RwLock::new(HashMap::new()),
            limite: Limitador::new(config.limites),
            listo,
            tipo_cuenta: RwLock::new(AccountType::Unknown),
            pausa_reconexion_segs: AtomicU64::new(0),
            ultimo_error: Mutex::new(None),
        });

        let cfg = config.clone();
        let interna_tarea = Arc::clone(&interna);
        let tarea = tokio::spawn(async move {
            bucle(interna_tarea, cfg, salida_rx).await;
        });

        let espera = tokio::time::timeout(Duration::from_secs(30), async {
            while *rx_listo.borrow() == 0 {
                if rx_listo.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;

        if espera.is_err() || *rx_listo.borrow() == 0 {
            tarea.abort();
            let motivo = interna
                .ultimo_error
                .lock()
                .expect("error no envenenado")
                .clone()
                .unwrap_or_else(|| "sin conexión con Deriv".to_string());
            return Err(BrokerError::Connection(motivo));
        }

        Ok(Self {
            config,
            interna,
            tarea: Some(tarea),
        })
    }

    /// Configuración efectiva.
    pub fn config(&self) -> &DerivConfig {
        &self.config
    }

    /// ¿Es una sesión sin credenciales?
    pub fn es_publico(&self) -> bool {
        self.config.es_publico()
    }

    /// Fuerza la caída del socket (solo pruebas).
    ///
    /// El supervisor pide un OTP nuevo, reconecta y reproduce las
    /// suscripciones; no cierra la sesión lógica.
    #[doc(hidden)]
    pub async fn simular_caida_de_conexion(&self) {
        self.interna.cortar.notify_waiters();
    }

    /// Corta el socket y mantiene la reconexión pausada (`segundos`).
    #[doc(hidden)]
    pub async fn simular_caida_prolongada(&self, segundos: u64) {
        self.interna
            .pausa_reconexion_segs
            .store(segundos, Ordering::SeqCst);
        self.interna.cortar.notify_waiters();
    }

    /// Divisa de trabajo: la configurada o la del balance.
    async fn divisa(&self) -> Result<String, BrokerError> {
        if let Some(c) = &self.config.currency {
            return Ok(c.clone());
        }
        // Sin credenciales no hay balance: Deriv cotiza en la divisa indicada.
        if self.config.es_publico() {
            return Ok("USD".to_string());
        }
        if let Some(b) = self.interna.balance.read().await.as_ref() {
            return Ok(b.currency.clone());
        }
        let v = self.interna.solicitar(json!({"balance": 1}), PLAZO).await?;
        let (_, currency, _) = protocol::balance(&v)
            .ok_or_else(|| BrokerError::Internal("deriv: balance sin divisa".into()))?;
        Ok(currency)
    }

    async fn activos_actualizados(&self) -> Result<Vec<Asset>, BrokerError> {
        self.interna.limite.esperar(Clase::Resto).await;
        let v = self
            .interna
            .solicitar(json!({"active_symbols": "full"}), PLAZO)
            .await?;
        let lista = protocol::activos(&v);
        if lista.is_empty() {
            return Err(BrokerError::Internal(
                "deriv: active_symbols devolvió una lista vacía".into(),
            ));
        }
        *self.interna.catalogo.write().await = lista.clone();
        Ok(lista)
    }
}

#[async_trait::async_trait]
impl Broker for DerivBroker {
    fn name(&self) -> &'static str {
        "deriv"
    }

    async fn connect(params: BrokerParams) -> Result<Self, BrokerError> {
        match params {
            BrokerParams::Deriv(cfg) => Self::nuevo(cfg).await,
            _ => Err(BrokerError::Internal(
                "deriv: se esperaba BrokerParams::Deriv".into(),
            )),
        }
    }

    async fn disconnect(&mut self) -> Result<(), BrokerError> {
        self.interna.activo.store(false, Ordering::SeqCst);
        self.interna.cortar.notify_waiters();
        if let Some(tarea) = self.tarea.take() {
            tarea.abort();
        }
        self.interna.vivo.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.interna.vivo.load(Ordering::SeqCst)
    }

    async fn session(&self) -> Result<Session, BrokerError> {
        if let Some(s) = self.interna.estado.read().await.as_ref() {
            return Ok(s.clone());
        }
        if !self.interna.vivo.load(Ordering::SeqCst) {
            return Err(BrokerError::NotConnected);
        }
        let cuenta = self
            .config
            .account_id
            .clone()
            .unwrap_or_else(|| "public".to_string());
        Ok(Session {
            broker_name: "deriv".into(),
            connected_at: epoch_secs(),
            account_id: Some(cuenta.clone()),
            balance_id: Some(protocol::id_surrogado(&cuenta)),
            account_type: tipo_cuenta(&cuenta, self.config.account),
            server_time_offset_ms: 0,
        })
    }

    async fn get_balance(&self) -> Result<Balance, BrokerError> {
        if self.config.es_publico() {
            return Err(BrokerError::Authentication(
                "deriv: el modo público no tiene cuenta; configura token y account_id".into(),
            ));
        }
        self.interna.limite.esperar(Clase::Balance).await;
        let v = self.interna.solicitar(json!({"balance": 1}), PLAZO).await?;
        let (amount, currency, loginid) = protocol::balance(&v)
            .ok_or_else(|| BrokerError::Internal("deriv: respuesta de balance sin datos".into()))?;
        let id = if loginid.is_empty() {
            self.config.account_id.clone().unwrap_or_default()
        } else {
            loginid
        };
        let tipo = match *self.interna.tipo_cuenta.read().await {
            AccountType::Unknown => tipo_cuenta(&id, self.config.account),
            conocido => conocido,
        };
        let balance = Balance {
            amount,
            currency,
            account_type: tipo,
            balance_id: protocol::id_surrogado(&id),
        };
        *self.interna.balance.write().await = Some(balance.clone());
        Ok(balance)
    }

    async fn get_account_type(&self) -> Result<AccountType, BrokerError> {
        Ok(self.session().await?.account_type)
    }

    async fn switch_account(&mut self, _kind: AccountType) -> Result<(), BrokerError> {
        Err(BrokerError::Rejected(
            "deriv: cambia account_id/account en DerivConfig y reconecta; \
             el OTP está ligado a una cuenta concreta"
                .into(),
        ))
    }

    async fn list_assets(&self) -> Result<Vec<Asset>, BrokerError> {
        let cache = self.interna.catalogo.read().await.clone();
        if !cache.is_empty() {
            return Ok(cache);
        }
        self.activos_actualizados().await
    }

    async fn asset_status(&self, symbol: &str) -> Result<MarketStatus, BrokerError> {
        if let Some(a) = self
            .interna
            .catalogo
            .read()
            .await
            .iter()
            .find(|a| a.symbol == symbol)
        {
            return Ok(a.status);
        }
        let lista = self.activos_actualizados().await?;
        lista
            .iter()
            .find(|a| a.symbol == symbol)
            .map(|a| a.status)
            .ok_or_else(|| BrokerError::Rejected(format!("deriv: símbolo desconocido {symbol}")))
    }

    async fn get_candles(&self, req: CandleRequest) -> Result<Vec<Candle>, BrokerError> {
        if !GRANULARIDADES.contains(&req.interval) {
            return Err(BrokerError::Rejected(format!(
                "deriv: intervalo {} no admitido; válidos: {GRANULARIDADES:?}",
                req.interval
            )));
        }
        self.interna.limite.esperar(Clase::Resto).await;
        let end = req
            .end_time
            .map(|t| t.to_string())
            .unwrap_or_else(|| "latest".to_string());
        let v = self
            .interna
            .solicitar(
                json!({
                    "ticks_history": req.symbol,
                    "count": req.count.min(5000),
                    "end": end,
                    "style": "candles",
                    "granularity": req.interval,
                }),
                PLAZO,
            )
            .await?;
        Ok(protocol::velas(&v, req.interval))
    }

    async fn subscribe_candles(&self, req: CandleSubscribe) -> Result<CandleStream, BrokerError> {
        if !GRANULARIDADES.contains(&req.interval) {
            return Err(BrokerError::Rejected(format!(
                "deriv: intervalo {} no admitido; válidos: {GRANULARIDADES:?}",
                req.interval
            )));
        }
        let clave = format!("{}|{}", req.symbol, req.interval);
        let (tx, rx) = mpsc::channel::<Candle>(128);
        self.interna.sub_velas.write().await.insert(clave, tx);
        let msg = json!({
            "ticks_history": req.symbol,
            "count": 1,
            "end": "latest",
            "style": "candles",
            "granularity": req.interval,
            "subscribe": 1,
        });
        self.interna.registrar_sub(msg.clone()).await;
        self.interna.limite.esperar(Clase::Resto).await;
        self.interna.solicitar(msg, PLAZO).await?;
        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|c| (Ok(c), rx))
        });
        Ok(Box::pin(stream))
    }

    async fn subscribe_ticks(&self, symbol: &str) -> Result<(), BrokerError> {
        let msg = json!({"ticks": symbol, "subscribe": 1});
        self.interna.registrar_sub(msg.clone()).await;
        self.interna.limite.esperar(Clase::Resto).await;
        self.interna.solicitar(msg, PLAZO).await?;
        Ok(())
    }

    async fn get_position_history(
        &self,
        symbol: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PositionHistory, BrokerError> {
        if self.config.es_publico() {
            return Err(BrokerError::Authentication(
                "deriv: el historial requiere credenciales".into(),
            ));
        }
        self.interna.limite.esperar(Clase::Cartera).await;
        let v = self
            .interna
            .solicitar(
                json!({
                    "profit_table": 1,
                    "description": 1,
                    "limit": limit.clamp(1, 100),
                    "offset": offset,
                    "sort": "DESC",
                }),
                PLAZO,
            )
            .await?;
        let mut historial = protocol::posiciones(&v, offset);
        if let Some(simbolo) = symbol {
            historial.positions.retain(|p| p.symbol == simbolo);
        }
        Ok(historial)
    }

    async fn get_payout(
        &self,
        symbol: &str,
        expiration_seconds: u64,
    ) -> Result<Payout, BrokerError> {
        let currency = self.divisa().await?;
        let (duracion, unidad) = duracion(expiration_seconds.max(60));
        self.interna.limite.esperar(Clase::Trading).await;
        let v = self
            .interna
            .solicitar(
                json!({
                    "proposal": 1,
                    "amount": self.config.stake_referencia,
                    "basis": "stake",
                    "contract_type": "CALL",
                    "currency": currency,
                    "underlying_symbol": symbol,
                    "duration": duracion,
                    "duration_unit": unidad,
                }),
                PLAZO,
            )
            .await;
        match v {
            Ok(valor) => {
                let pct = protocol::payout_porcentaje(&valor, self.config.stake_referencia)
                    .unwrap_or_default();
                Ok(Payout {
                    symbol: symbol.to_string(),
                    expiration_seconds,
                    profit_percent: pct,
                    is_spot: false,
                    strike: None,
                    closed: false,
                })
            }
            Err(BrokerError::Rejected(mensaje)) => Ok(Payout {
                symbol: symbol.to_string(),
                expiration_seconds,
                profit_percent: 0.0,
                is_spot: false,
                strike: None,
                closed: true,
            })
            .inspect(|_| {
                tracing::warn!(
                    mensaje,
                    "deriv: propuesta rechazada, payout marcado cerrado"
                );
            }),
            Err(e) => Err(e),
        }
    }

    async fn buy_binary(&self, req: BuyBinaryRequest) -> Result<OrderId, BrokerError> {
        if self.config.es_publico() {
            return Err(BrokerError::Authentication(
                "deriv: operar requiere token con scope trade".into(),
            ));
        }
        let currency = self.divisa().await?;
        let tipo = match req.direction {
            Direction::Call => "CALL",
            Direction::Put => "PUT",
        };
        self.interna.limite.esperar(Clase::Trading).await;
        let propuesta = self
            .interna
            .solicitar(
                json!({
                    "proposal": 1,
                    "amount": req.amount,
                    "basis": "stake",
                    "contract_type": tipo,
                    "currency": currency,
                    "underlying_symbol": req.symbol,
                    "duration": req.expiration_minutes,
                    "duration_unit": "m",
                }),
                PLAZO,
            )
            .await?;
        let id_propuesta = propuesta
            .get("proposal")
            .and_then(|p| p.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| BrokerError::Internal("deriv: propuesta sin id".into()))?
            .to_string();
        let precio = propuesta
            .get("proposal")
            .and_then(|p| p.get("ask_price"))
            .and_then(Value::as_f64)
            .unwrap_or(req.amount);

        let compra = self
            .interna
            .solicitar(json!({"buy": id_propuesta, "price": precio}), PLAZO)
            .await?;
        let contrato = compra
            .get("buy")
            .and_then(|b| b.get("contract_id"))
            .and_then(Value::as_u64)
            .ok_or_else(|| BrokerError::Internal("deriv: compra sin contract_id".into()))?;
        let id = contrato.to_string();
        self.interna.abiertas.write().await.insert(id.clone());
        self.interna
            .registrar_sub(json!({
                "proposal_open_contract": 1,
                "contract_id": contrato,
                "subscribe": 1,
            }))
            .await;
        let _ = self
            .interna
            .solicitar(
                json!({"proposal_open_contract": 1, "contract_id": contrato, "subscribe": 1}),
                PLAZO,
            )
            .await;
        let _ = self.interna.eventos.send(Event::OrderOpened(Operation {
            id: id.clone(),
            symbol: req.symbol.clone(),
            amount: req.amount,
            direction: req.direction,
            option_type: req.option_type,
            status: OperationStatus::Open,
            result: None,
            payout_percent: protocol::payout_porcentaje(&propuesta, req.amount),
            opened_at: epoch_secs(),
            closed_at: None,
        }));
        Ok(id)
    }

    async fn sell_option(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        if self.config.es_publico() {
            return Err(BrokerError::Authentication(
                "deriv: vender requiere credenciales".into(),
            ));
        }
        let contrato: u64 = order_id.parse().map_err(|_| {
            BrokerError::Rejected(format!("deriv: contract_id inválido {order_id}"))
        })?;
        self.interna.limite.esperar(Clase::Trading).await;
        self.interna
            .solicitar(json!({"sell": contrato, "price": 0}), PLAZO)
            .await?;
        Ok(())
    }

    async fn get_operation_result(
        &self,
        order_id: &OrderId,
    ) -> Result<OperationResult, BrokerError> {
        Ok(self
            .interna
            .resultados
            .read()
            .await
            .get(order_id)
            .copied()
            .unwrap_or(OperationResult::Pending))
    }

    async fn events(&self) -> Result<EventStream, BrokerError> {
        let rx = self.interna.eventos.subscribe();
        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Ok(evento) => Some((evento, rx)),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    Some((Event::EventsLost { count: n }, rx))
                }
                Err(broadcast::error::RecvError::Closed) => None,
            }
        });
        Ok(Box::pin(stream))
    }
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

/// URL de conexión: pública o un OTP nuevo.
async fn url_de_conexion(
    config: &DerivConfig,
    interna: &Arc<Interna>,
) -> Result<String, BrokerError> {
    let Some(token) = config.token.as_ref() else {
        return Ok(config.public_ws_url.clone());
    };

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.transport.request_timeout_secs.max(5),
        ))
        .build()
        .map_err(|e| BrokerError::Internal(format!("deriv: cliente http: {e}")))?;

    let (cuenta, tipo) = resolver_cuenta(&http, config, token).await?;
    *interna.tipo_cuenta.write().await = tipo;

    let url = format!(
        "{}/trading/v1/options/accounts/{}/otp",
        config.base_url.trim_end_matches('/'),
        cuenta
    );
    let mut peticion = http
        .post(url)
        .header("Authorization", format!("Bearer {token}"));
    if let Some(app) = &config.app_id {
        peticion = peticion.header("Deriv-App-ID", app);
    }
    let resp = peticion
        .send()
        .await
        .map_err(|e| BrokerError::Connection(format!("deriv otp: {e}")))?;
    let estado = resp.status();
    let texto = resp.text().await.unwrap_or_default();
    if !estado.is_success() {
        let detalle: String = texto.chars().take(300).collect();
        return Err(match estado.as_u16() {
            401 | 403 => BrokerError::Authentication(format!(
                "deriv otp {estado}: {} (el token necesita scope trade)",
                detalle.trim()
            )),
            _ => BrokerError::Connection(format!("deriv otp {estado}: {}", detalle.trim())),
        });
    }
    let valor: Value = serde_json::from_str(&texto)
        .map_err(|e| BrokerError::Internal(format!("deriv otp: respuesta no JSON: {e}")))?;
    valor
        .get("data")
        .and_then(|d| d.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| BrokerError::Connection("deriv otp: respuesta sin url".into()))
}

/// Resuelve la cuenta del token y su tipo (`demo`/`real`).
async fn resolver_cuenta(
    http: &reqwest::Client,
    config: &DerivConfig,
    token: &str,
) -> Result<(String, AccountType), BrokerError> {
    let url = format!(
        "{}/trading/v1/options/accounts",
        config.base_url.trim_end_matches('/')
    );
    let mut peticion = http
        .get(url)
        .header("Authorization", format!("Bearer {token}"));
    if let Some(app) = &config.app_id {
        peticion = peticion.header("Deriv-App-ID", app);
    }
    let resp = peticion
        .send()
        .await
        .map_err(|e| BrokerError::Connection(format!("deriv cuentas: {e}")))?;
    let estado = resp.status();
    let texto = resp.text().await.unwrap_or_default();
    if !estado.is_success() {
        let detalle: String = texto.chars().take(300).collect();
        return Err(match estado.as_u16() {
            401 | 403 => {
                BrokerError::Authentication(format!("deriv cuentas {estado}: {}", detalle.trim()))
            }
            _ => BrokerError::Connection(format!("deriv cuentas {estado}: {}", detalle.trim())),
        });
    }
    let valor: Value = serde_json::from_str(&texto)
        .map_err(|e| BrokerError::Internal(format!("deriv cuentas: respuesta no JSON: {e}")))?;
    // La API devuelve `data` como lista; se acepta también `data.accounts`
    // por compatibilidad con la forma documentada en algunos ejemplos.
    let lista = valor
        .get("data")
        .and_then(|d| {
            if d.is_array() {
                Some(d)
            } else {
                d.get("accounts")
            }
        })
        .and_then(Value::as_array)
        .ok_or_else(|| BrokerError::Connection("deriv cuentas: respuesta sin lista".into()))?;

    let quiere_demo = !matches!(config.account, AccountSelection::Real);
    let mut elegida: Option<(String, AccountType)> = None;

    for c in lista {
        let id = match c.get("account_id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };
        let tipo = match c.get("account_type").and_then(Value::as_str) {
            Some("demo") | Some("virtual") => AccountType::Practice,
            Some("real") => AccountType::Real,
            _ => {
                let virtual_ = c
                    .get("is_virtual")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if virtual_ {
                    AccountType::Practice
                } else {
                    tipo_cuenta(id, config.account)
                }
            }
        };
        if config.account_id.as_deref() == Some(id) {
            return Ok((id.to_string(), tipo));
        }
        let es_demo = matches!(tipo, AccountType::Practice);
        if elegida.is_none() && es_demo == quiere_demo {
            elegida = Some((id.to_string(), tipo));
        }
    }

    if let Some(configurada) = &config.account_id {
        return Ok((configurada.clone(), AccountType::Unknown));
    }
    elegida.ok_or_else(|| {
        BrokerError::Authentication(format!(
            "deriv: el token no tiene cuenta {}",
            if quiere_demo { "demo" } else { "real" }
        ))
    })
}

async fn enviar_json(sink: &mut Escritor, msg: Value) -> Result<(), BrokerError> {
    sink.send(Message::Text(msg.to_string()))
        .await
        .map_err(|e| BrokerError::Connection(e.to_string()))
}

/// Bucle del supervisor: conecta, inicializa, atiende y reconecta.
async fn bucle(interna: Arc<Interna>, config: DerivConfig, mut salida: mpsc::Receiver<Value>) {
    let base = config.transport.backoff_base_ms.max(200);
    let maximo = config.transport.backoff_max_ms.max(base);
    let mut backoff = base;
    let mut reconexiones = 0u32;
    let mut caido_en: Option<Instant> = None;

    while interna.activo.load(Ordering::SeqCst) {
        let url = match url_de_conexion(&config, &interna).await {
            Ok(u) => u,
            Err(e) => {
                *interna.ultimo_error.lock().expect("error no envenenado") = Some(e.to_string());
                if matches!(e, BrokerError::Authentication(_)) {
                    tracing::error!(error = %e, "deriv: credenciales rechazadas, no se reintenta");
                    break;
                }
                tracing::warn!(error = %e, "deriv: sin URL de conexión; reintento");
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff = backoff.saturating_mul(2).min(maximo);
                continue;
            }
        };

        let ws = match connect_async(&url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                tracing::warn!(error = %e, "deriv: conexión ws fallida");
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff = backoff.saturating_mul(2).min(maximo);
                continue;
            }
        };

        let (mut sink, mut stream) = ws.split();
        interna.vivo.store(true, Ordering::SeqCst);

        match inicializar(&interna, &mut sink, &mut stream).await {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!(error = %e, "deriv: inicialización fallida");
                interna.vivo.store(false, Ordering::SeqCst);
                let _ = sink.close().await;
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff = backoff.saturating_mul(2).min(maximo);
                continue;
            }
        }

        let generacion = (*interna.listo.borrow()).wrapping_add(1);
        let _ = interna.listo.send(generacion);
        backoff = base;

        if reconexiones > 0 {
            let _ = interna.eventos.send(Event::Reconnected {
                intentos: reconexiones,
                downtime_ms: caido_en
                    .map(|t| t.elapsed().as_millis() as u64)
                    .unwrap_or(0),
            });
        }

        for sub in interna.subs.read().await.iter().cloned() {
            let mut sub = sub;
            sub["req_id"] = json!(interna.siguiente_id());
            if let Err(e) = enviar_json(&mut sink, sub).await {
                tracing::warn!(error = %e, "deriv: replay de suscripción fallido");
            }
        }

        {
            let rec = Arc::clone(&interna);
            tokio::spawn(async move {
                reconciliar(&rec).await;
            });
        }
        let motivo = bucle_socket(&interna, &mut sink, &mut stream, &mut salida).await;
        caido_en = Some(Instant::now());
        interna.vivo.store(false, Ordering::SeqCst);
        interna.router.lock().expect("router no envenenado").clear();
        let _ = interna.eventos.send(Event::Disconnected {
            motivo: motivo.clone(),
        });
        tracing::warn!(motivo, "deriv: conexión perdida");

        if !interna.activo.load(Ordering::SeqCst) {
            break;
        }
        reconexiones += 1;
        let pausa = interna.pausa_reconexion_segs.swap(0, Ordering::SeqCst);
        if pausa > 0 {
            tracing::warn!(pausa, "deriv: reconexión pausada para prueba");
            tokio::time::sleep(Duration::from_secs(pausa)).await;
        }
        tokio::time::sleep(Duration::from_millis(backoff)).await;
        backoff = backoff.saturating_mul(2).min(maximo);
    }
}

/// Primera petición de la conexión: hora del servidor y balance.
async fn inicializar(
    interna: &Arc<Interna>,
    sink: &mut Escritor,
    stream: &mut Lector,
) -> Result<(), BrokerError> {
    let id = interna.siguiente_id();
    enviar_json(sink, json!({"time": 1, "req_id": id})).await?;
    let tiempo = leer_hasta(stream, id, Duration::from_secs(10)).await?;
    let servidor = tiempo
        .get("time")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let offset_ms = (servidor - epoch_secs()) * 1000;

    let mut cuenta = String::from("public");
    let mut balance = None;

    if interna.activo.load(Ordering::SeqCst) {
        let id = interna.siguiente_id();
        enviar_json(sink, json!({"balance": 1, "subscribe": 1, "req_id": id})).await?;
        if let Ok(valor) = leer_hasta(stream, id, Duration::from_secs(10)).await {
            if let Some((amount, currency, loginid)) = protocol::balance(&valor) {
                if !loginid.is_empty() {
                    cuenta = loginid;
                }
                balance = Some(Balance {
                    amount,
                    currency,
                    account_type: AccountType::Unknown,
                    balance_id: protocol::id_surrogado(&cuenta),
                });
            }
        }
    }

    if let Some(b) = balance {
        *interna.balance.write().await = Some(b);
    }

    let sesion = Session {
        broker_name: "deriv".into(),
        connected_at: epoch_secs(),
        account_id: Some(cuenta.clone()),
        balance_id: Some(protocol::id_surrogado(&cuenta)),
        account_type: *interna.tipo_cuenta.read().await,
        server_time_offset_ms: offset_ms,
    };
    *interna.estado.write().await = Some(sesion);
    Ok(())
}

/// Lee del socket hasta la respuesta con `id`.
async fn leer_hasta(stream: &mut Lector, id: u64, plazo: Duration) -> Result<Value, BrokerError> {
    let fin = tokio::time::Instant::now() + plazo;
    loop {
        let restante = fin.saturating_duration_since(tokio::time::Instant::now());
        if restante.is_zero() {
            return Err(BrokerError::Timeout);
        }
        match tokio::time::timeout(restante, stream.next()).await {
            Err(_) => return Err(BrokerError::Timeout),
            Ok(None) => return Err(BrokerError::NotConnected),
            Ok(Some(Err(e))) => return Err(BrokerError::Connection(e.to_string())),
            Ok(Some(Ok(Message::Text(texto)))) => {
                if let Ok(valor) = serde_json::from_str::<Value>(&texto) {
                    if valor.get("req_id").and_then(Value::as_u64) == Some(id) {
                        return interpretar(valor);
                    }
                }
            }
            Ok(Some(Ok(Message::Close(_)))) => return Err(BrokerError::NotConnected),
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// Atiende el socket hasta que se cierra. Devuelve el motivo.
async fn bucle_socket(
    interna: &Arc<Interna>,
    sink: &mut Escritor,
    stream: &mut Lector,
    salida: &mut mpsc::Receiver<Value>,
) -> String {
    loop {
        tokio::select! {
            Some(msg) = salida.recv() => {
                if let Err(e) = enviar_json(sink, msg).await {
                    return e.to_string();
                }
            }
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Text(texto))) => procesar(interna, &texto).await,
                    Some(Ok(Message::Ping(datos))) => {
                        let _ = sink.send(Message::Pong(datos)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => return "socket cerrado".into(),
                    Some(Err(e)) => return e.to_string(),
                    _ => {}
                }
            }
            _ = interna.cortar.notified() => {
                let _ = sink.close().await;
                return "cierre solicitado".into();
            }
        }
    }
}

/// Entrega respuestas y trata mensajes de suscripción.
async fn procesar(interna: &Arc<Interna>, texto: &str) {
    let Ok(valor) = serde_json::from_str::<Value>(texto) else {
        return;
    };

    if let Some(id) = valor.get("req_id").and_then(Value::as_u64) {
        let espera = interna
            .router
            .lock()
            .expect("router no envenenado")
            .remove(&id);
        if let Some(tx) = espera {
            let _ = tx.send(valor.clone());
        }
    }

    match valor.get("msg_type").and_then(Value::as_str).unwrap_or("") {
        "ohlc" => {
            let eco = valor.get("echo_req").cloned().unwrap_or(Value::Null);
            let symbol = eco
                .get("ticks_history")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let intervalo = eco.get("granularity").and_then(Value::as_u64).unwrap_or(60);
            if let Some(vela) = protocol::vela_ohlc(&valor, intervalo) {
                let clave = format!("{symbol}|{intervalo}");
                let emisor = interna.sub_velas.read().await.get(&clave).cloned();
                if let Some(tx) = emisor {
                    let _ = tx.send(vela).await;
                }
            }
        }
        "proposal_open_contract" => {
            if let Some((id, resultado)) = protocol::resultado_contrato(&valor) {
                registrar_cierre(interna, &id, resultado).await;
            }
        }
        "tick" => {
            if let Some(t) = valor.get("tick") {
                let symbol = t
                    .get("symbol")
                    .and_then(Value::as_str)
                    .or_else(|| valor.pointer("/echo_req/ticks").and_then(Value::as_str))
                    .unwrap_or_default()
                    .to_string();
                let price = t.get("quote").and_then(Value::as_f64).unwrap_or_default();
                let epoch = t.get("epoch").and_then(Value::as_i64).unwrap_or_default();
                let _ = interna.eventos.send(Event::PriceTick {
                    symbol,
                    tick: PriceTick {
                        price,
                        bid: t.get("bid").and_then(Value::as_f64),
                        ask: t.get("ask").and_then(Value::as_f64),
                        at_ms: epoch * 1000,
                    },
                });
            }
        }
        "balance" => {
            if let Some((amount, currency, loginid)) = protocol::balance(&valor) {
                let balance = Balance {
                    amount,
                    currency,
                    account_type: tipo_cuenta(&loginid, AccountSelection::Default),
                    balance_id: protocol::id_surrogado(&loginid),
                };
                *interna.balance.write().await = Some(balance.clone());
                let _ = interna.eventos.send(Event::BalanceChanged(AccountBalance {
                    id: balance.balance_id,
                    currency: balance.currency.clone(),
                    amount: balance.amount,
                    account_type: balance.account_type,
                    hold_amount: None,
                    bonus_amount: None,
                    enrolled_amount: None,
                    has_deposits: None,
                }));
            }
        }
        _ => {}
    }
}

/// Registra un cierre una sola vez (los eventos y la reconciliación pueden
/// observar el mismo contrato).
async fn registrar_cierre(interna: &Arc<Interna>, id: &str, resultado: OperationResult) {
    if interna.resultados.read().await.contains_key(id) {
        return;
    }
    interna
        .resultados
        .write()
        .await
        .insert(id.to_string(), resultado);
    interna.abiertas.write().await.remove(id);
    let _ = interna.eventos.send(Event::OrderClosed {
        id: id.to_string(),
        result: resultado,
    });
}

/// Recupera cierres ocurridos sin conexión: contrato puntual y, si ya no está
/// en el POC activo, historial paginado. No reenvía órdenes ni duplica eventos.
async fn reconciliar(interna: &Arc<Interna>) {
    let abiertas: Vec<OrderId> = interna.abiertas.read().await.iter().cloned().collect();
    if abiertas.is_empty() {
        return;
    }
    tracing::info!(pendientes = abiertas.len(), "deriv: reconciliando cierres");

    for id in &abiertas {
        let Ok(contrato) = id.parse::<u64>() else {
            continue;
        };
        let Ok(valor) = interna
            .solicitar(
                json!({"proposal_open_contract": 1, "contract_id": contrato}),
                PLAZO,
            )
            .await
        else {
            continue;
        };
        if let Some((cid, resultado)) = protocol::resultado_contrato(&valor) {
            registrar_cierre(interna, &cid, resultado).await;
        }
    }

    let mut offset = 0usize;
    for _ in 0..10 {
        if interna.abiertas.read().await.is_empty() {
            break;
        }
        let Ok(valor) = interna
            .solicitar(
                json!({"profit_table": 1, "description": 1, "limit": 100, "offset": offset}),
                PLAZO,
            )
            .await
        else {
            break;
        };
        let pagina = protocol::posiciones(&valor, offset);
        if pagina.positions.is_empty() {
            break;
        }
        for posicion in &pagina.positions {
            if !interna.abiertas.read().await.contains(&posicion.id) {
                continue;
            }
            let resultado = match posicion.pnl {
                Some(x) if x > 0.0 => OperationResult::Win(x),
                Some(x) if x < 0.0 => OperationResult::Loss(x.abs()),
                Some(_) => OperationResult::Equal(0.0),
                None => continue,
            };
            registrar_cierre(interna, &posicion.id, resultado).await;
        }
        offset += 100;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duracion_usa_la_unidad_mayor_exacta() {
        assert_eq!(duracion(60), (1, "m"));
        assert_eq!(duracion(300), (5, "m"));
        assert_eq!(duracion(3600), (1, "h"));
        assert_eq!(duracion(86400), (1, "d"));
        assert_eq!(duracion(45), (45, "s"));
    }

    #[test]
    fn tipo_cuenta_por_prefijo() {
        assert_eq!(
            tipo_cuenta("VRTC123", AccountSelection::Real),
            AccountType::Practice
        );
        assert_eq!(
            tipo_cuenta("CR456", AccountSelection::Practice),
            AccountType::Real
        );
        assert_eq!(
            tipo_cuenta("X", AccountSelection::Default),
            AccountType::Unknown
        );
    }

    #[test]
    fn error_invalid_token_es_autenticacion() {
        let e = error_de(&json!({"code": "InvalidToken", "message": "bad"}));
        assert!(matches!(e, BrokerError::Authentication(_)));
    }

    #[test]
    fn error_de_validacion_es_rechazo() {
        let e = error_de(&json!({"code": "InputValidationFailed", "message": "falta"}));
        assert!(matches!(e, BrokerError::Rejected(_)));
    }
}
