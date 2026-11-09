//! Densidad de ticks por clase de activo (Deriv, gateway público).
//!
//! Elige índices sintéticos, forex, cripto y materias primas del catálogo real,
//! se suscribe a los ticks nativos de cada uno durante 8 s y publica la
//! densidad observada con el hueco entre ticks.
//!
//! `cargo run --example deriv_densidad`

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};
use cripsy_binary_api::domain::{AssetClass, Event};
use futures_util::StreamExt;

const VENTANA: Duration = Duration::from_secs(8);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let broker = DerivBroker::connect(BrokerParams::Deriv(DerivConfig::publico())).await?;

    let activos = broker.list_assets().await?;
    let mut elegidos: Vec<(String, AssetClass)> = vec![
        ("R_100".into(), AssetClass::Index),
        ("1HZ100V".into(), AssetClass::Index),
    ];

    for clase in [
        AssetClass::Forex,
        AssetClass::Crypto,
        AssetClass::Commodity,
        AssetClass::Index,
    ] {
        if let Some(a) = activos
            .iter()
            .find(|a| a.class == clase && !elegidos.iter().any(|(s, _)| s == &a.symbol))
        {
            elegidos.push((a.symbol.clone(), a.class));
        }
    }

    for (simbolo, _) in &elegidos {
        broker.subscribe_ticks(simbolo).await?;
    }
    println!("activos suscritos: {elegidos:?}\n");

    let mut eventos = broker.events().await?;
    let mut llegadas: HashMap<String, Vec<Instant>> = HashMap::new();
    let fin = Instant::now() + VENTANA;

    while Instant::now() < fin {
        match tokio::time::timeout(Duration::from_millis(300), eventos.next()).await {
            Ok(Some(Event::PriceTick { symbol, .. })) => {
                llegadas.entry(symbol).or_default().push(Instant::now());
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }

    println!(
        "{:<14} {:<10} {:>6} {:>10} {:>12} {:>12}",
        "activo", "clase", "ticks", "ticks/s", "hueco medio", "hueco máx"
    );
    for (simbolo, clase) in &elegidos {
        let Some(marcas) = llegadas.get(simbolo) else {
            println!("{simbolo:<14} {:?} {:>6}", clase, 0);
            continue;
        };
        let total = marcas.len();
        let (medio, maximo) = if total >= 2 {
            let huecos: Vec<f64> = marcas
                .windows(2)
                .map(|p| p[1].duration_since(p[0]).as_secs_f64() * 1000.0)
                .collect();
            (
                huecos.iter().sum::<f64>() / huecos.len() as f64,
                huecos.iter().copied().fold(0.0_f64, f64::max),
            )
        } else {
            (0.0, 0.0)
        };
        println!(
            "{simbolo:<14} {clase:?} {total:>6} {:>10.2} {:>10.0} ms {:>9.0} ms",
            total as f64 / VENTANA.as_secs_f64(),
            medio,
            maximo
        );
    }
    Ok(())
}
