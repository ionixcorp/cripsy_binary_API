//! Densidad real del stream de ticks de Deriv (sin credenciales).
//!
//! Se suscriben dos activos sintéticos durante 10 s y se mide cuántos ticks
//! entrega el bróker por segundo, con el hueco máximo entre ticks.
//!
//! `cargo run --example deriv_ticks`

use std::collections::HashMap;
use std::time::Duration;

use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};
use cripsy_binary_api::domain::Event;
use futures_util::StreamExt;

const VENTANA: Duration = Duration::from_secs(10);
const SIMBOLOS: [&str; 2] = ["R_100", "1HZ100V"];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let broker = DerivBroker::connect(BrokerParams::Deriv(DerivConfig::publico())).await?;
    let mut eventos = broker.events().await?;

    for simbolo in SIMBOLOS {
        broker.subscribe_ticks(simbolo).await?;
    }

    let mut por_simbolo: HashMap<String, Vec<i64>> = HashMap::new();
    let fin = tokio::time::Instant::now() + VENTANA;
    let mut otros = 0usize;

    while tokio::time::Instant::now() < fin {
        match tokio::time::timeout(Duration::from_millis(500), eventos.next()).await {
            Ok(Some(Event::PriceTick { symbol, tick })) => {
                por_simbolo.entry(symbol).or_default().push(tick.at_ms);
            }
            Ok(Some(_)) => otros += 1,
            Ok(None) => break,
            Err(_) => {}
        }
    }

    println!(
        "ventana: {:.0} s | otros eventos: {otros}",
        VENTANA.as_secs_f64()
    );
    for simbolo in SIMBOLOS {
        let Some(marcas) = por_simbolo.get(simbolo) else {
            println!("{simbolo}: sin ticks");
            continue;
        };
        let total = marcas.len();
        if total < 2 {
            println!("{simbolo}: {total} tick(s)");
            continue;
        }
        let huecos: Vec<i64> = marcas.windows(2).map(|p| p[1] - p[0]).collect();
        let maximo = huecos.iter().copied().max().unwrap_or_default();
        let medio = huecos.iter().sum::<i64>() as f64 / huecos.len() as f64;
        println!(
            "{simbolo}: {total} ticks en {:.0} s → {:.2} ticks/s | hueco medio {:.0} ms | hueco máx {} ms",
            VENTANA.as_secs_f64(),
            total as f64 / VENTANA.as_secs_f64(),
            medio,
            maximo
        );
    }
    Ok(())
}
