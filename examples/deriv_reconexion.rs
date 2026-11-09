//! Caída forzada de la sesión Deriv y recuperación con OTP nuevo.
//!
//! Suscribe ticks, corta el socket, espera `Disconnected` y `Reconnected`, y
//! comprueba que el stream se reanuda y que el adaptador sigue operativo.
//!
//! `cargo run --example deriv_reconexion`

use std::collections::HashMap;
use std::time::Duration;

use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};
use cripsy_binary_api::domain::Event;
use futures_util::StreamExt;

fn cargar_env() -> HashMap<String, String> {
    let mut mapa = HashMap::new();
    let Ok(texto) = std::fs::read_to_string(".env") else {
        return mapa;
    };
    for linea in texto.lines() {
        let linea = linea.trim().trim_start_matches('\u{feff}');
        if linea.is_empty() || linea.starts_with('#') {
            continue;
        }
        if let Some((clave, valor)) = linea.split_once('=') {
            mapa.insert(
                clave.trim().to_string(),
                valor
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        }
    }
    mapa
}

/// Cuenta ticks durante `segundos`.
async fn contar_ticks(
    eventos: &mut futures_util::stream::BoxStream<'_, Event>,
    segundos: u64,
) -> usize {
    let fin = tokio::time::Instant::now() + Duration::from_secs(segundos);
    let mut total = 0;
    while tokio::time::Instant::now() < fin {
        match tokio::time::timeout(Duration::from_millis(500), eventos.next()).await {
            Ok(Some(Event::PriceTick { .. })) => total += 1,
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }
    total
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = cargar_env();
    let config = DerivConfig {
        token: env.get("DERIV_TOKEN").cloned(),
        app_id: env.get("DERIV_APPID").cloned(),
        account_id: env.get("DERIV_ACCOUNT_ID").cloned(),
        currency: Some("USD".into()),
        ..DerivConfig::default()
    };
    let broker = DerivBroker::connect(BrokerParams::Deriv(config)).await?;
    let mut eventos = broker.events().await?;
    broker.subscribe_ticks("R_100").await?;

    let antes = contar_ticks(&mut eventos, 3).await;
    println!("ticks antes de la caída (3 s): {antes}");

    broker.simular_caida_de_conexion().await;
    println!("caída forzada; esperando recuperación");

    let mut desconectado = false;
    let mut reconexion: Option<(u32, u64)> = None;
    let limite = tokio::time::Instant::now() + Duration::from_secs(45);

    while tokio::time::Instant::now() < limite {
        match tokio::time::timeout(Duration::from_secs(5), eventos.next()).await {
            Ok(Some(Event::Disconnected { motivo })) => {
                desconectado = true;
                println!("Disconnected: {motivo}");
            }
            Ok(Some(Event::Reconnected {
                intentos,
                downtime_ms,
            })) => {
                reconexion = Some((intentos, downtime_ms));
                println!("Reconnected: intentos={intentos} downtime={downtime_ms} ms");
                break;
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }

    let despues = contar_ticks(&mut eventos, 4).await;
    println!("ticks tras reconectar (4 s): {despues}");

    let activo = broker.is_connected().await;
    let cuenta = broker.session().await.ok().and_then(|s| s.account_id);
    println!("conectado={activo} cuenta={cuenta:?}");

    if !desconectado {
        return Err("no se observó Disconnected".into());
    }
    if reconexion.is_none() {
        return Err("no se observó Reconnected en 45 s".into());
    }
    if !activo {
        return Err("la sesión no quedó conectada".into());
    }
    if despues == 0 {
        return Err("el stream de ticks no se reanudó".into());
    }
    println!("reconexión verificada");
    Ok(())
}
