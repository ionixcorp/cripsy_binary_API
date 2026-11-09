//! Reintentos con backoff exponencial y jitter.

use std::future::Future;
use std::time::Duration;

use super::error::TransportError;
use crate::broker::config::TransportConfig;

/// Ejecuta `op` con reintentos según la configuración.
///
/// El jitter se deriva del reloj del sistema para evitar dependencias extra.
/// Ante error de transporte reintenta; el último error se devuelve tal cual.
pub async fn with_retry<F, Fut, T>(
    cfg: &TransportConfig,
    nombre: &str,
    mut op: F,
) -> Result<T, TransportError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, TransportError>>,
{
    let mut intento: u32 = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                if intento >= cfg.max_retries {
                    return Err(e);
                }
                let espera = backoff(cfg, intento);
                tracing::warn!(
                    error = %e,
                    intento = intento + 1,
                    max = cfg.max_retries,
                    espera_ms = espera.as_millis() as u64,
                    "reintentando {nombre}"
                );
                tokio::time::sleep(espera).await;
                intento += 1;
            }
        }
    }
}

/// Backoff exponencial con tope y jitter.
fn backoff(cfg: &TransportConfig, intento: u32) -> Duration {
    let base = cfg
        .backoff_base_ms
        .saturating_mul(2u64.saturating_pow(intento));
    let tope = cfg.backoff_max_ms;
    let jitter = nanos_jitter();
    Duration::from_millis(base.min(tope) + jitter % (tope / 4 + 1))
}

/// Jitter simple derivado del reloj (0..tope/4 ms).
fn nanos_jitter() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()))
        .unwrap_or(0)
        % 64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn cfg(max: u32) -> TransportConfig {
        TransportConfig {
            connect_timeout_secs: 2,
            request_timeout_secs: 2,
            max_retries: max,
            backoff_base_ms: 1,
            backoff_max_ms: 4,
            cola_salida: 64,
            verify_tls: true,
            proxy: None,
            user_agent: None,
        }
    }

    #[tokio::test]
    async fn reintenta_hasta_lograr() {
        let n = AtomicU32::new(0);
        let out = with_retry(&cfg(3), "prueba", || {
            let n = n.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if n < 3 {
                    Err(TransportError::Closed)
                } else {
                    Ok(n)
                }
            }
        })
        .await
        .expect("debe lograrse al tercer intento");
        assert_eq!(out, 3);
        assert_eq!(n.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn agota_reintentos_y_devuelve_error() {
        let n = AtomicU32::new(0);
        let err = with_retry(&cfg(2), "prueba", || {
            n.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(TransportError::Closed) }
        })
        .await
        .expect_err("debe fallar");
        assert!(matches!(err, TransportError::Closed));
        assert_eq!(n.load(Ordering::SeqCst), 3); // 1 inicial + 2 reintentos
    }

    #[tokio::test]
    async fn sin_error_no_reintenta() {
        let n = AtomicU32::new(0);
        let out = with_retry(&cfg(5), "prueba", || {
            n.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, TransportError>(7u32) }
        })
        .await
        .unwrap();
        assert_eq!(out, 7);
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn backoff_crece_hasta_el_tope() {
        let c = cfg(9);
        let b0 = backoff(&c, 0);
        let b5 = backoff(&c, 5);
        assert!(b0 <= Duration::from_millis(c.backoff_base_ms + 64));
        assert!(b5 <= Duration::from_millis(c.backoff_max_ms + 64));
    }
}
