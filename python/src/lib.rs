//! Bindings Python (PyO3) para `cripsy-binary-api`.
//!
//! La superficie es **sincrónica**: cada método bloquea el hilo de Python
//! mientras la operación async corre en un runtime Tokio propio. El GIL se
//! libera durante la espera (`Python::allow_threads`), de modo que otros hilos
//! de Python pueden seguir trabajando.
//!
//! Los resultados se entregan como diccionarios/listas con los mismos nombres
//! que el dominio Rust (`symbol`, `close`, `profit_percent`, ...). No se
//! renombran ni se recalculan valores.

use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

use ::cripsy_binary_api::broker::{
    AccountSelection, Broker, BrokerError, BrokerParams, DerivBroker, DerivConfig, IqOptionBroker,
    IqOptionConfig,
};
use ::cripsy_binary_api::domain::{
    BuyBinaryRequest, BuyBlitzRequest, BuyDigitalRequest, CandleRequest, Direction, Event,
    OptionType,
};
use futures_util::StreamExt;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::Serialize;
use serde_json::Value as Json;
use tokio::runtime::Runtime;

create_exception!(cripsy_binary_api, CripsyError, PyException);

fn broker_error(e: BrokerError) -> PyErr {
    PyErr::new::<CripsyError, _>(e.to_string())
}

#[allow(deprecated)]
fn json_to_py(py: Python<'_>, value: &Json) -> PyResult<PyObject> {
    match value {
        Json::Null => Ok(py.None()),
        Json::Bool(b) => Ok(b.into_py(py)),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(u) = n.as_u64() {
                Ok(u.into_py(py))
            } else {
                Ok(n.as_f64().unwrap_or(0.0).into_py(py))
            }
        }
        Json::String(s) => Ok(s.as_str().into_py(py)),
        Json::Array(items) => {
            let lista = PyList::empty(py);
            for item in items {
                lista.append(json_to_py(py, item)?)?;
            }
            Ok(lista.into_any().unbind())
        }
        Json::Object(map) => {
            let dict = PyDict::new(py);
            for (clave, item) in map {
                dict.set_item(clave, json_to_py(py, item)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn to_py<T: Serialize>(py: Python<'_>, value: &T) -> PyResult<PyObject> {
    let json = serde_json::to_value(value)
        .map_err(|e| PyRuntimeError::new_err(format!("no se pudo serializar: {e}")))?;
    json_to_py(py, &json)
}

fn direccion(valor: &str) -> PyResult<Direction> {
    match valor.trim().to_ascii_lowercase().as_str() {
        "call" | "rise" | "sube" => Ok(Direction::Call),
        "put" | "fall" | "baja" => Ok(Direction::Put),
        otro => Err(PyValueError::new_err(format!(
            "direction debe ser 'call' o 'put' (recibido: {otro})"
        ))),
    }
}

/// Cliente sincrónico de un bróker.
///
/// Un `Client` mantiene una sola sesión. Los métodos bloquean hasta obtener la
/// respuesta; no se debe compartir entre hilos que operen a la vez.
#[pyclass(module = "cripsy_binary_api")]
pub struct Client {
    broker: Arc<Mutex<Box<dyn Broker>>>,
    rt: Arc<Runtime>,
    name: String,
}

impl Client {
    fn construir(rt: Runtime, broker: Box<dyn Broker>) -> Self {
        let name = broker.name().to_string();
        Self {
            broker: Arc::new(Mutex::new(broker)),
            rt: Arc::new(rt),
            name,
        }
    }
}

#[pymethods]
impl Client {
    /// Conecta con Deriv. Sin `token`, abre el modo público (solo mercado).
    #[staticmethod]
    #[pyo3(signature = (token=None, app_id=None, account_id=None, currency=None))]
    fn connect_deriv(
        py: Python<'_>,
        token: Option<String>,
        app_id: Option<String>,
        account_id: Option<String>,
        currency: Option<String>,
    ) -> PyResult<Self> {
        let config = DerivConfig {
            token,
            app_id,
            account_id,
            currency,
            ..DerivConfig::default()
        };
        let rt = Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("no se pudo crear el runtime: {e}")))?;
        let broker = py
            .allow_threads(|| {
                rt.block_on(<DerivBroker as Broker>::connect(BrokerParams::Deriv(
                    config,
                )))
            })
            .map_err(broker_error)?;
        Ok(Self::construir(rt, Box::new(broker)))
    }

    /// Conecta con IQ Option. `practice=True` fuerza cuenta demo.
    #[staticmethod]
    #[pyo3(signature = (email, password, practice=true))]
    fn connect_iqoption(
        py: Python<'_>,
        email: String,
        password: String,
        practice: bool,
    ) -> PyResult<Self> {
        let config = IqOptionConfig {
            email,
            password,
            account: if practice {
                AccountSelection::Practice
            } else {
                AccountSelection::Real
            },
            ..IqOptionConfig::default()
        };
        let rt = Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("no se pudo crear el runtime: {e}")))?;
        let broker = py
            .allow_threads(|| {
                rt.block_on(<IqOptionBroker as Broker>::connect(BrokerParams::IqOption(
                    config,
                )))
            })
            .map_err(broker_error)?;
        Ok(Self::construir(rt, Box::new(broker)))
    }

    /// Nombre estable del adaptador (`iq_option` o `deriv`).
    fn name(&self) -> String {
        self.name.clone()
    }

    fn is_connected(&self, py: Python<'_>) -> PyResult<bool> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        Ok(py.allow_threads(move || {
            rt.block_on(async move {
                let b = broker.lock().await;
                b.is_connected().await
            })
        }))
    }

    /// Sesión activa (cuenta, balance_id, tipo y offset de reloj).
    fn session(&self, py: Python<'_>) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let sesion = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.session().await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &sesion)
    }

    /// Balance de la cuenta activa.
    fn balance(&self, py: Python<'_>) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let balance = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.get_balance().await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &balance)
    }

    /// Catálogo completo de activos.
    fn assets(&self, py: Python<'_>) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let activos = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.list_assets().await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &activos)
    }

    /// Velas históricas. `interval` en segundos y `count` como máximo.
    #[pyo3(signature = (symbol, interval=60, count=100, end_time=None))]
    fn candles(
        &self,
        py: Python<'_>,
        symbol: String,
        interval: u64,
        count: usize,
        end_time: Option<i64>,
    ) -> PyResult<PyObject> {
        let req = CandleRequest {
            symbol,
            interval,
            count,
            end_time,
        };
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let velas = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.get_candles(req).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &velas)
    }

    /// Estado de mercado del activo (`Open`, `Closed`, `Suspended`).
    fn asset_status(&self, py: Python<'_>, symbol: String) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let estado = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.asset_status(&symbol).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &estado)
    }

    /// Payout publicado en el catálogo para el activo.
    #[pyo3(signature = (symbol, seconds=60))]
    fn payout(&self, py: Python<'_>, symbol: String, seconds: u64) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let payout = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.get_payout(&symbol, seconds).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &payout)
    }

    /// Historial de posiciones cerradas.
    #[pyo3(signature = (limit=10, offset=0, symbol=None))]
    fn history(
        &self,
        py: Python<'_>,
        limit: usize,
        offset: usize,
        symbol: Option<String>,
    ) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let historial = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.get_position_history(symbol.as_deref(), limit, offset).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &historial)
    }

    /// Recolecta ticks nativos durante `seconds` y los devuelve como lista.
    ///
    /// No hay polling: se suscribe el stream del bróker y se filtra por símbolo.
    #[pyo3(signature = (symbol, seconds=10.0))]
    fn ticks(&self, py: Python<'_>, symbol: String, seconds: f64) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let ticks = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.subscribe_ticks(&symbol).await?;
                    let mut eventos = b.events().await?;
                    let deadline =
                        tokio::time::Instant::now() + Duration::from_secs_f64(seconds.max(0.1));
                    let mut salida = Vec::new();
                    loop {
                        let restante = deadline.saturating_duration_since(tokio::time::Instant::now());
                        if restante.is_zero() {
                            break;
                        }
                        match tokio::time::timeout(restante, eventos.next()).await {
                            Ok(Some(Event::PriceTick { symbol: s, tick })) if s == symbol => {
                                salida.push(tick);
                            }
                            Ok(Some(_)) => {}
                            Ok(None) | Err(_) => break,
                        }
                    }
                    Ok::<_, BrokerError>(salida)
                })
            })
            .map_err(broker_error)?;
        to_py(py, &ticks)
    }

    /// Compra rise/fall (`direction="call"` o `"put"`). Devuelve el id de orden.
    #[pyo3(signature = (symbol, amount, direction="call", minutes=1))]
    fn buy(
        &self,
        py: Python<'_>,
        symbol: String,
        amount: f64,
        direction: &str,
        minutes: u64,
    ) -> PyResult<String> {
        let req = BuyBinaryRequest {
            symbol,
            amount,
            direction: direccion(direction)?,
            expiration_minutes: minutes,
            option_type: OptionType::Binary,
        };
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        py.allow_threads(move || {
            rt.block_on(async move {
                let b = broker.lock().await;
                b.buy_binary(req).await
            })
        })
        .map_err(broker_error)
    }

    /// Compra digital (solo IQ Option). `strike=0` deja que el bróker elija
    /// el instrumento spot del periodo.
    #[pyo3(signature = (symbol, amount, direction="call", minutes=1, strike=0.0))]
    fn buy_digital(
        &self,
        py: Python<'_>,
        symbol: String,
        amount: f64,
        direction: &str,
        minutes: u64,
        strike: f64,
    ) -> PyResult<String> {
        let req = BuyDigitalRequest {
            symbol,
            amount,
            direction: direccion(direction)?,
            duration_minutes: minutes,
            strike,
        };
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        py.allow_threads(move || {
            rt.block_on(async move {
                let b = broker.lock().await;
                let extras = b.iqoption_extras().ok_or_else(|| {
                    BrokerError::Rejected(format!(
                        "{}: el bróker no ofrece opciones digitales",
                        b.name()
                    ))
                })?;
                extras.buy_digital_spot(req).await
            })
        })
        .map_err(broker_error)
    }

    /// Compra BLITZ (solo IQ Option); `seconds` es la duración admitida por el
    /// activo. `profit_percent=0` usa el payout del catálogo.
    #[pyo3(signature = (symbol, amount, direction="call", seconds=60, profit_percent=0.0))]
    fn buy_blitz(
        &self,
        py: Python<'_>,
        symbol: String,
        amount: f64,
        direction: &str,
        seconds: u64,
        profit_percent: f64,
    ) -> PyResult<String> {
        let req = BuyBlitzRequest {
            symbol,
            amount,
            direction: direccion(direction)?,
            expiration_size_secs: seconds,
            profit_percent,
        };
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        py.allow_threads(move || {
            rt.block_on(async move {
                let b = broker.lock().await;
                let extras = b.iqoption_extras().ok_or_else(|| {
                    BrokerError::Rejected(format!(
                        "{}: el bróker no ofrece opciones blitz",
                        b.name()
                    ))
                })?;
                extras.buy_blitz(req).await
            })
        })
        .map_err(broker_error)
    }

    /// Indicadores técnicos publicados por el bróker (solo IQ Option).
    fn indicators(&self, py: Python<'_>, symbol: String) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let indicadores = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    let extras = b.iqoption_extras().ok_or_else(|| {
                        BrokerError::Rejected(format!(
                            "{}: el bróker no publica indicadores técnicos",
                            b.name()
                        ))
                    })?;
                    extras.get_technical_indicators(&symbol).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &indicadores)
    }

    /// Venta anticipada de una orden abierta.
    fn sell(&self, py: Python<'_>, order_id: String) -> PyResult<()> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        py.allow_threads(move || {
            rt.block_on(async move {
                let b = broker.lock().await;
                b.sell_option(&order_id).await
            })
        })
        .map_err(broker_error)
    }

    /// Resultado en caché (no consulta al bróker): `Win`, `Loss`, `Equal` o `Pending`.
    fn result(&self, py: Python<'_>, order_id: String) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let resultado = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    b.get_operation_result(&order_id).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &resultado)
    }

    /// Espera el cierre por eventos hasta `timeout_secs`; si no llega, devuelve
    /// el estado en caché (`Pending` incluido).
    #[pyo3(signature = (order_id, timeout_secs=120.0))]
    fn wait_result(
        &self,
        py: Python<'_>,
        order_id: String,
        timeout_secs: f64,
    ) -> PyResult<PyObject> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        let resultado = py
            .allow_threads(move || {
                rt.block_on(async move {
                    let b = broker.lock().await;
                    let mut eventos = match b.events().await {
                        Ok(e) => e,
                        Err(_) => return b.get_operation_result(&order_id).await,
                    };
                    let deadline =
                        tokio::time::Instant::now() + Duration::from_secs_f64(timeout_secs.max(0.1));
                    loop {
                        let restante = deadline.saturating_duration_since(tokio::time::Instant::now());
                        if restante.is_zero() {
                            break;
                        }
                        match tokio::time::timeout(restante, eventos.next()).await {
                            Ok(Some(Event::OrderClosed { id, result })) if id == order_id => {
                                return Ok(result);
                            }
                            Ok(Some(_)) => {}
                            Ok(None) | Err(_) => break,
                        }
                    }
                    b.get_operation_result(&order_id).await
                })
            })
            .map_err(broker_error)?;
        to_py(py, &resultado)
    }

    /// Cierra la sesión y libera el WebSocket.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let broker = Arc::clone(&self.broker);
        let rt = Arc::clone(&self.rt);
        py.allow_threads(move || {
            rt.block_on(async move {
                let mut b = broker.lock().await;
                b.disconnect().await
            })
        })
        .map_err(broker_error)
    }

    fn __repr__(&self) -> String {
        format!("<cripsy_binary_api.Client broker={}>", self.name())
    }
}

#[pymodule]
fn cripsy_binary_api(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Client>()?;
    m.add("CripsyError", m.py().get_type::<CripsyError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("BROKERS", vec!["iq_option", "deriv"])?;
    Ok(())
}
