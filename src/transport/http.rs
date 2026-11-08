//! Cliente HTTP asíncrono (`reqwest`).
//!
//! Reglas de la capa: toda petición lleva tiempo límite, la URL
//! completa se loggea sin cuerpos (los cuerpos pueden traer credenciales), y
//! los errores HTTP se convierten en [`TransportError`] antes de subir.

use std::time::Duration;

use serde_json::Value;

use crate::broker::config::TransportConfig;

use super::error::TransportError;
use super::retry::with_retry;

/// Cliente HTTP con base URL, timeouts y reintentos.
pub struct HttpClient {
    inner: reqwest::Client,
    base_url: String,
    cfg: TransportConfig,
}

impl HttpClient {
    /// Crea el cliente. `base_url` sin barra final, p. ej. `https://iqoption.com/api`.
    pub fn new(base_url: &str, cfg: TransportConfig) -> Result<Self, TransportError> {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.request_timeout_secs))
            .connect_timeout(Duration::from_secs(cfg.connect_timeout_secs));
        if let Some(ua) = &cfg.user_agent {
            builder = builder.user_agent(ua.clone());
        }
        if !cfg.verify_tls {
            // Solo para pruebas documentadas: nunca en producción.
            builder = builder.danger_accept_invalid_certs(true);
        }
        if let Some(proxy) = &cfg.proxy {
            builder = builder
                .proxy(reqwest::Proxy::all(proxy).map_err(|e| TransportError::Url(e.to_string()))?);
        }
        let inner = builder.build().map_err(TransportError::from)?;
        Ok(Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
            cfg,
        })
    }

    /// Acceso al cliente `reqwest` subyacente (para cookies y headers).
    pub fn raw(&self) -> &reqwest::Client {
        &self.inner
    }

    /// URL completa de un path relativo.
    pub fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// POST JSON con reintentos. El cuerpo viaja como `serde_json::Value`.
    pub async fn post_json(&self, path: &str, cuerpo: &Value) -> Result<Value, TransportError> {
        let cfg = self.cfg.clone();
        with_retry(&cfg, path, || self.enviar(path, cuerpo)).await
    }

    /// POST JSON devolviendo también las cabeceras de respuesta
    /// (necesario para leer `Set-Cookie: ssid=…` del login).
    pub async fn post_json_completo(
        &self,
        path: &str,
        cuerpo: &Value,
    ) -> Result<(Value, reqwest::header::HeaderMap), TransportError> {
        let url = self.url(path);
        tracing::debug!(url = %url, "POST http (con cabeceras)");
        let resp = self
            .inner
            .post(&url)
            .json(cuerpo)
            .send()
            .await
            .map_err(TransportError::from)?;
        let headers = resp.headers().clone();
        let cuerpo_resp = decodificar(resp).await?;
        Ok((cuerpo_resp, headers))
    }

    /// POST de formulario URL-encoded con reintentos.
    pub async fn post_form(
        &self,
        path: &str,
        campos: &[(String, String)],
    ) -> Result<Value, TransportError> {
        let url = self.url(path);
        let cfg = self.cfg.clone();
        let campos = campos.to_vec();
        with_retry(&cfg, path, || async {
            let resp = self
                .inner
                .post(&url)
                .form(&campos)
                .send()
                .await
                .map_err(TransportError::from)?;
            decodificar(resp).await
        })
        .await
    }

    /// GET con reintentos.
    pub async fn get_json(&self, path: &str) -> Result<Value, TransportError> {
        let url = self.url(path);
        let cfg = self.cfg.clone();
        with_retry(&cfg, path, || async {
            let resp = self
                .inner
                .get(&url)
                .send()
                .await
                .map_err(TransportError::from)?;
            decodificar(resp).await
        })
        .await
    }

    /// Un solo envío (sin reintentos) de POST JSON.
    async fn enviar(&self, path: &str, cuerpo: &Value) -> Result<Value, TransportError> {
        let url = self.url(path);
        tracing::debug!(url = %url, "POST http");
        let resp = self
            .inner
            .post(&url)
            .json(cuerpo)
            .send()
            .await
            .map_err(TransportError::from)?;
        decodificar(resp).await
    }
}

/// Convierte la respuesta en JSON o en error HTTP tipado.
async fn decodificar(resp: reqwest::Response) -> Result<Value, TransportError> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        // El cuerpo de error puede tener detalles: se trunca para no llenar logs.
        let body: String = body.chars().take(500).collect();
        return Err(TransportError::Http {
            status: status.as_u16(),
            body,
        });
    }
    resp.json::<Value>()
        .await
        .map_err(|e| TransportError::Codec(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_une_base_y_path() {
        let http = HttpClient::new("https://ejemplo.com/api", TransportConfig::default()).unwrap();
        assert_eq!(http.url("/v2/login"), "https://ejemplo.com/api/v2/login");
        assert_eq!(http.url("v2/login"), "https://ejemplo.com/api/v2/login");
        let http2 =
            HttpClient::new("https://ejemplo.com/api/", TransportConfig::default()).unwrap();
        assert_eq!(http2.url("x"), "https://ejemplo.com/api/x");
    }

    #[test]
    fn cliente_valido_se_construye() {
        let http = HttpClient::new("https://ejemplo.com", TransportConfig::default()).unwrap();
        assert_eq!(http.url("/x"), "https://ejemplo.com/x");
    }

    #[tokio::test]
    async fn http_status_se_convierte_en_error_tipado() {
        // Servidor efímero: 404 tipado sin entrar a la red real.
        use std::net::SocketAddr;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let _ = sock
                        .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n")
                        .await;
                });
            }
        });
        let cfg = TransportConfig {
            max_retries: 0,
            ..TransportConfig::default()
        };
        let http = HttpClient::new(&format!("http://{addr}"), cfg).unwrap();
        let err = http.get_json("/x").await.expect_err("debe fallar con 404");
        match err {
            TransportError::Http { status, body } => {
                assert_eq!(status, 404);
                assert!(body.is_empty());
            }
            otra => panic!("error inesperado: {otra:?}"),
        }
    }
}
