//! Descarga paginada de histórico: más allá del tope de 1000 velas por request.
//!
//! LÍMITES VERIFICADOS CONTRA EL BRÓKER (2026-10-30, EURUSD-op)
//!   · `count` máximo: 1000. Pedir 2000 o 5000 devuelve 1000 igual.
//!   · `from`+`to` devuelve 0 velas: el rango no se puede saltar, solo se
//!     pagina hacia atrás con `to`/`to_id`. El canal antiguo `get-candles` v1.0 con
//!     `from`/`to` no devuelve datos ni error.
//!   · Retención por tamaño: 1 s y 5 s ≈ 7 días · 1 m 45–90 días ·
//!     5 m 270–330 días · ≥15 m > 2 años.
//!   · La semana completa de 1 s son ~605 páginas por activo.
//!
//! SEMÁNTICA DE `to` (medida en vivo 2026-10-30): es inclusivo sobre el
//! **inicio** de la vela. Pedir `to=T` devuelve la vela que empieza en `T`
//! (aunque siga en curso) y las anteriores. Por eso [`ultimo_cierre`] resta
//! un paso: entrega el inicio de la última vela cerrada.
//!
//! Enfoque: retroceder `to` un paso por debajo del `from` más viejo de cada
//! página, fusionar y deduplicar por `from`. Se detiene al alcanzar `desde`,
//! al recibir una página vacía (retención del bróker) o al llegar a
//! [`PAGINAS_MAXIMAS`].

use crate::broker::error::BrokerError;
use crate::broker::trait_::Broker;
use crate::domain::{Candle, CandleRequest};

/// Tope real de velas por request en IQ Option (verificado en vivo).
pub const VELAS_POR_PAGINA: usize = 1000;

/// Freno de seguridad para una ventana mal pedida (p. ej. `desde` en el año 1970).
pub const PAGINAS_MAXIMAS: usize = 5_000;

/// Resultado de una descarga de ventana.
#[derive(Debug, Clone, PartialEq)]
pub struct Ventana {
    /// Velas ordenadas de vieja a nueva, sin duplicados por `from`.
    pub velas: Vec<Candle>,
    /// Requests efectivamente enviados.
    pub paginas: usize,
    /// Se alcanzó `desde` (no se agotó la retención antes).
    pub completa: bool,
    /// Se detuvo por [`PAGINAS_MAXIMAS`] en vez de por datos.
    pub truncada: bool,
    /// Huecos según `intervalo` entre la primera y la última vela. Incluye
    /// cierres de mercado (fines de semana): no distingue unos de otros.
    pub faltantes: usize,
}

/// Inicio de la última vela **cerrada** para `intervalo`, que es el `to`
/// seguro: el bróker incluye la vela que empieza exactamente en `to`.
pub fn ultimo_cierre(ahora: i64, intervalo: u64) -> i64 {
    let paso = intervalo.max(1) as i64;
    (ahora / paso) * paso - paso
}

/// Páginas que harían falta para cubrir `desde..hasta` con [`VELAS_POR_PAGINA`].
pub fn paginas_estimadas(intervalo: u64, desde: i64, hasta: i64) -> usize {
    if hasta <= desde {
        return 0;
    }
    let paso = intervalo.max(1) as i64;
    let velas = ((hasta - desde) as f64 / paso as f64).ceil().max(0.0) as usize;
    velas.div_ceil(VELAS_POR_PAGINA)
}

/// Descarga `desde..hasta` paginando hacia atrás.
///
/// `hasta` debe ser el cierre de una vela cerrada; usar [`ultimo_cierre`].
pub async fn descargar(
    broker: &dyn Broker,
    simbolo: &str,
    intervalo: u64,
    desde: i64,
    hasta: i64,
) -> Result<Ventana, BrokerError> {
    if intervalo == 0 {
        return Err(BrokerError::Internal("intervalo de vela 0".into()));
    }
    if hasta <= desde {
        return Ok(Ventana {
            velas: Vec::new(),
            paginas: 0,
            completa: true,
            truncada: false,
            faltantes: 0,
        });
    }

    let mut velas: Vec<Candle> = Vec::new();
    let mut paginas = 0usize;
    let mut to = hasta;
    let mut truncada = false;

    loop {
        if paginas >= PAGINAS_MAXIMAS {
            truncada = true;
            break;
        }
        if to < desde {
            break;
        }
        let request = CandleRequest {
            symbol: simbolo.to_string(),
            interval: intervalo,
            count: VELAS_POR_PAGINA,
            end_time: Some(to),
        };
        let pagina = broker.get_candles(request).await?;
        paginas += 1;
        if pagina.is_empty() {
            // Retención del bróker: no hay nada más viejo.
            break;
        }

        let antes = velas.len();
        velas.extend(pagina.into_iter().filter(|v| v.from >= desde));
        velas.sort_by_key(|v| v.from);
        velas.dedup_by_key(|v| v.from);

        let Some(mas_vieja) = velas.first().map(|v| v.from) else {
            break;
        };
        if mas_vieja <= desde {
            break;
        }
        if velas.len() == antes {
            // La página no aportó nada nuevo: evita el bucle infinito.
            break;
        }
        // `to` es inclusivo sobre el inicio: bajar un paso evita repetir la
        // vela más vieja ya descargada.
        to = mas_vieja - intervalo as i64;
    }

    let completa = velas.first().map(|v| v.from <= desde).unwrap_or(true);
    let faltantes = faltantes(&velas, intervalo);
    Ok(Ventana {
        velas,
        paginas,
        completa,
        truncada,
        faltantes,
    })
}

/// Huecos entre la primera y la última vela, medidos en velas de `intervalo`.
pub fn faltantes(velas: &[Candle], intervalo: u64) -> usize {
    let (Some(primera), Some(ultima)) = (velas.first(), velas.last()) else {
        return 0;
    };
    let paso = intervalo.max(1) as i64;
    let esperadas = ((ultima.to - primera.from) / paso).max(0) as usize;
    esperadas.saturating_sub(velas.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vela(from: i64, precio: f64) -> Candle {
        Candle {
            from,
            to: from + 1,
            open: precio,
            close: precio,
            high: precio,
            low: precio,
            volume: Some(1.0),
        }
    }

    #[test]
    fn ultimo_cierre_excluye_la_vela_en_curso() {
        assert_eq!(ultimo_cierre(1_000, 60), 900);
        assert_eq!(ultimo_cierre(1_000, 1), 999);
        assert_eq!(ultimo_cierre(120, 60), 60);
    }

    #[test]
    fn paginas_estimadas_cubre_la_semana() {
        assert_eq!(paginas_estimadas(1, 0, 7 * 86_400), 605);
        assert_eq!(paginas_estimadas(1, 0, 7_200), 8);
        assert_eq!(paginas_estimadas(60, 0, 3_600), 1);
        assert_eq!(paginas_estimadas(1, 100, 100), 0);
        assert_eq!(paginas_estimadas(1, 200, 100), 0);
    }

    #[test]
    fn faltantes_detecta_un_hueco() {
        let velas = vec![vela(0, 1.0), vela(1, 1.1), vela(4, 1.2)];
        assert_eq!(faltantes(&velas, 1), 2);
        assert_eq!(faltantes(&velas[..2], 1), 0);
        assert_eq!(faltantes(&[], 1), 0);
    }
}
