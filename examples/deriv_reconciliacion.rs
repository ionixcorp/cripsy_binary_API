//! Reconciliación de un cierre ocurrido durante una caída (Deriv, demo).
//!
//! Compra 1 minuto, corta el socket y mantiene la reconexión pausada 70 s para
//! que el contrato expire sin conexión. Al volver, la API debe recuperar el
//! cierre del historial y publicar **un solo** `OrderClosed`.
//!
//! `cargo run --example deriv_reconciliacion`

use std::collections::HashMap;
use std::time::Duration;

use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};
use cripsy_binary_api::domain::{AccountType, BuyBinaryRequest, Direction, Event, OptionType};
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
    if broker.get_balance().await?.account_type != AccountType::Practice {
        return Err("abortado: la cuenta no es demo".into());
    }
    let mut eventos = broker.events().await?;
    let inicio = broker.get_balance().await?;

    let id = broker
        .buy_binary(BuyBinaryRequest {
            symbol: "R_100".into(),
            amount: 1.0,
            direction: Direction::Call,
            expiration_minutes: 1,
            option_type: OptionType::Binary,
        })
        .await?;
    println!("orden {id} abierta; se corta la conexión 70 s para que expire sin red");

    broker.simular_caida_prolongada(70).await;

    let mut cierres = 0usize;
    let mut resultado = None;
    let mut reconexion = None;
    let mut desconectado = false;
    let mut primera_cierre: Option<tokio::time::Instant> = None;
    let limite = tokio::time::Instant::now() + Duration::from_secs(170);

    while tokio::time::Instant::now() < limite {
        if let Some(t) = primera_cierre {
            if t.elapsed() > Duration::from_secs(3) {
                break;
            }
        }
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
                println!("Reconnected tras {downtime_ms} ms ({intentos} intento/s)");
            }
            Ok(Some(Event::OrderClosed {
                id: cerrada,
                result,
            })) if cerrada == id => {
                cierres += 1;
                if resultado.is_none() {
                    resultado = Some(result);
                    primera_cierre = Some(tokio::time::Instant::now());
                }
                println!("OrderClosed #{cierres}: {result:?}");
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }

    let cache = broker.get_operation_result(&id).await?;
    let fin = broker.get_balance().await?;
    println!(
        "cierres={cierres} resultado={resultado:?} caché={cache:?} delta={:+.2}",
        fin.amount - inicio.amount
    );

    if !desconectado {
        return Err("no se observó Disconnected".into());
    }
    if reconexion.is_none() {
        return Err("no se observó Reconnected".into());
    }
    if cierres != 1 {
        return Err(format!("se esperaba un único OrderClosed, llegaron {cierres}").into());
    }
    let Some(resultado) = resultado else {
        return Err("no llegó resultado de cierre".into());
    };
    let Some(neto) = resultado.net() else {
        return Err("el resultado no es terminal".into());
    };
    if (fin.amount - inicio.amount - neto).abs() > 0.011 {
        return Err(format!(
            "el balance no coincide con el resultado: delta {} vs neto {neto}",
            fin.amount - inicio.amount
        )
        .into());
    }
    println!("reconciliación verificada: cierre único y balance coherente");
    Ok(())
}
