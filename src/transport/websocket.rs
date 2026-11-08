//! Cliente WebSocket asíncrono (`tokio-tungstenite`).
//!
//! Envelope tipado `{name, msg, request_id}` del protocolo. Un único lector
//! consume el socket: el llamador recibe mensajes por [`Self::next_frame`].
//! La reconexión con backoff la provee [`connect_con_reintentos`].

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::broker::config::TransportConfig;

use super::error::TransportError;
use super::retry::with_retry;

/// Stream de WebSocket (alias del tipo con TLS automático).
pub type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Mensaje recibido, ya con el envelope parseado.
#[derive(Debug, Clone, PartialEq)]
pub struct WsFrame {
    pub name: String,
    pub msg: Value,
    pub request_id: Option<String>,
}

/// Cliente WebSocket con header de conexión (p. ej. cookie de sesión).
pub struct WsClient {
    inner: WsStream,
    url: String,
}

impl WsClient {
    /// Conecta a `url` con los headers dados (nombre, valor).
    pub async fn connect(
        url: &str,
        headers: &[(&str, &str)],
        cfg: &TransportConfig,
    ) -> Result<Self, TransportError> {
        let mut request = url
            .into_client_request()
            .map_err(|e| TransportError::Url(e.to_string()))?;
        if let Some(ua) = &cfg.user_agent {
            request.headers_mut().insert(
                "User-Agent",
                HeaderValue::from_str(ua).map_err(|e| TransportError::Url(e.to_string()))?,
            );
        }
        for (nombre, valor) in headers {
            let clave = tokio_tungstenite::tungstenite::http::HeaderName::try_from(*nombre)
                .map_err(|e| TransportError::Url(e.to_string()))?;
            let valor_hdr =
                HeaderValue::from_str(valor).map_err(|e| TransportError::Url(e.to_string()))?;
            request.headers_mut().insert(clave, valor_hdr);
        }
        let conectar = tokio::time::timeout(
            Duration::from_secs(cfg.connect_timeout_secs),
            tokio_tungstenite::connect_async(request),
        );
        let (inner, _) = conectar
            .await
            .map_err(|_| TransportError::Timeout {
                phase: "ws-connect".into(),
                ms: cfg.connect_timeout_secs * 1000,
            })?
            .map_err(|e| TransportError::Io(e.to_string()))?;
        tracing::debug!(url, "websocket conectado");
        Ok(Self {
            inner,
            url: url.to_string(),
        })
    }

    /// Conecta con reintentos (backoff de la configuración).
    pub async fn connect_con_reintentos(
        url: &str,
        headers: Vec<(String, String)>,
        cfg: TransportConfig,
    ) -> Result<Self, TransportError> {
        with_retry(&cfg, "ws-connect", || {
            let cfg_local = cfg.clone();
            let pares: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            async move { Self::connect(url, &pares, &cfg_local).await }
        })
        .await
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Envía un mensaje con el envelope `{name, msg, request_id}`.
    pub async fn enviar(
        &mut self,
        name: &str,
        msg: &Value,
        request_id: &str,
    ) -> Result<(), TransportError> {
        let frame = json!({ "name": name, "msg": msg, "request_id": request_id });
        tracing::debug!(name, request_id, "ws enviar");
        self.inner
            .send(Message::Text(frame.to_string()))
            .await
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    /// Envía mensaje sin `request_id` (respuestas del servidor no lo llevan).
    pub async fn enviar_sin_id(&mut self, name: &str, msg: &Value) -> Result<(), TransportError> {
        let frame = json!({ "name": name, "msg": msg });
        self.inner
            .send(Message::Text(frame.to_string()))
            .await
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    /// Siguiente mensaje ya parseado. `None` cuando el socket cierra.
    ///
    /// Ping/Pong se manejan aquí: el socket nunca ve frames de control.
    pub async fn siguiente(&mut self) -> Option<Result<WsFrame, TransportError>> {
        loop {
            let mensaje = match self.inner.next().await {
                Some(Ok(m)) => m,
                Some(Err(e)) => return Some(Err(TransportError::Io(e.to_string()))),
                None => return None,
            };
            let texto = match mensaje {
                Message::Text(t) => t,
                Message::Ping(datos) => {
                    let _ = self.inner.send(Message::Pong(datos)).await;
                    continue;
                }
                Message::Pong(_) => continue,
                Message::Close(_) => return None,
                Message::Binary(datos) => match String::from_utf8(datos) {
                    Ok(t) => t,
                    Err(_) => return Some(Err(TransportError::Codec("binario no utf-8".into()))),
                },
                Message::Frame(_) => continue,
            };
            return match parsear(&texto) {
                Ok(frame) => Some(Ok(frame)),
                Err(e) => Some(Err(e)),
            };
        }
    }

    /// Cierra el socket limpiamente.
    pub async fn cerrar(&mut self) -> Result<(), TransportError> {
        self.inner
            .close(None)
            .await
            .map_err(|e| TransportError::Io(e.to_string()))
    }
}

/// Parsea el envelope `{name, msg, request_id}` a [`WsFrame`].
pub fn parsear(texto: &str) -> Result<WsFrame, TransportError> {
    let valor: Value = serde_json::from_str(texto)
        .map_err(|e| TransportError::Codec(format!("json inválido: {e}")))?;
    let name = valor
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| TransportError::Codec("falta 'name' en el mensaje".into()))?
        .to_string();
    let msg = valor.get("msg").cloned().unwrap_or(Value::Null);
    let request_id = valor
        .get("request_id")
        .and_then(|r| r.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(WsFrame {
        name,
        msg,
        request_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parsea_envelope_completo() {
        let frame =
            parsear(r#"{"name":"profile","msg":{"balance":1},"request_id":"abc-1"}"#).unwrap();
        assert_eq!(frame.name, "profile");
        assert_eq!(frame.request_id.as_deref(), Some("abc-1"));
        assert_eq!(frame.msg, json!({"balance": 1}));
    }

    #[test]
    fn parsea_sin_request_id() {
        let frame = parsear(r#"{"name":"timeSync","msg":1790000000000}"#).unwrap();
        assert_eq!(frame.name, "timeSync");
        assert!(frame.request_id.is_none());
    }

    #[test]
    fn request_id_vacio_se_trata_como_ausente() {
        let frame = parsear(r#"{"name":"x","msg":null,"request_id":""}"#).unwrap();
        assert!(frame.request_id.is_none());
    }

    #[test]
    fn sin_name_es_error_de_codec() {
        let err = parsear(r#"{"msg":null}"#).expect_err("falta name");
        assert!(matches!(err, TransportError::Codec(_)));
    }

    #[test]
    fn json_invalido_es_error_de_codec() {
        assert!(matches!(parsear("no-json"), Err(TransportError::Codec(_))));
    }
}
