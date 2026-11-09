//! Verificación viva de ejecución en cuenta **demo** de Deriv.
//!
//! Compra 1 USD a 1 minuto en R_100, intenta venta anticipada a los 8 s y
//! espera el cierre. Se niega a ejecutar si la cuenta no es de práctica.
//!
//! `cargo run --example deriv_orden_demo`

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

    let mut broker = DerivBroker::connect(BrokerParams::Deriv(config)).await?;
    let inicio = broker.get_balance().await?;
    if inicio.account_type != AccountType::Practice {
        return Err("abortado: la cuenta no es demo; este ejemplo no opera en real".into());
    }
    println!(
        "cuenta demo {} | balance inicial {:.2} {}",
        broker.session().await?.account_id.unwrap_or_default(),
        inicio.amount,
        inicio.currency
    );

    let mut eventos = broker.events().await?;
    let id = broker
        .buy_binary(BuyBinaryRequest {
            symbol: "R_100".into(),
            amount: 1.0,
            direction: Direction::Call,
            expiration_minutes: 1,
            option_type: OptionType::Binary,
        })
        .await?;
    println!("orden abierta: {id}");

    // Espera el evento de apertura (hasta 10 s).
    let apertura = tokio::time::timeout(Duration::from_secs(10), eventos.next()).await;
    if let Ok(Some(Event::OrderOpened(op))) = apertura {
        println!(
            "OrderOpened: {} {} {:?} {:.2} payout={:?}",
            op.id, op.symbol, op.direction, op.amount, op.payout_percent
        );
    }

    // Venta anticipada a los 8 s (si el bróker la permite en ese momento).
    tokio::time::sleep(Duration::from_secs(8)).await;
    match broker.sell_option(&id).await {
        Ok(()) => println!("venta anticipada aceptada"),
        Err(e) => println!("venta anticipada rechazada: {e}"),
    }

    // Espera el cierre (máximo 120 s desde la compra).
    let limite = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut cierre = None;
    while tokio::time::Instant::now() < limite {
        match tokio::time::timeout(Duration::from_secs(10), eventos.next()).await {
            Ok(Some(Event::OrderClosed {
                id: cerrada,
                result,
            })) if cerrada == id => {
                cierre = Some(result);
                break;
            }
            Ok(Some(otro)) => println!("evento: {otro:?}"),
            Ok(None) => break,
            Err(_) => {}
        }
    }

    let final_ = broker.get_operation_result(&id).await?;
    println!("resultado en caché: {final_:?} | evento: {cierre:?}");

    let fin = broker.get_balance().await?;
    println!(
        "balance final {:.2} {} | delta {:+.2}",
        fin.amount,
        fin.currency,
        fin.amount - inicio.amount
    );

    match broker.get_position_history(None, 5, 0).await {
        Ok(h) => {
            println!(
                "historial: {} posiciones (offset={})",
                h.positions.len(),
                h.offset
            );
            for p in h.positions.iter().take(3) {
                println!(
                    "  {} {} {:?} pnl={:?} abierta={} cerrada={}",
                    p.id,
                    p.symbol,
                    p.instrument_type,
                    p.pnl,
                    !p.is_closed(),
                    p.is_closed()
                );
            }
        }
        Err(e) => println!("historial no disponible: {e}"),
    }

    broker.disconnect().await?;
    Ok(())
}
