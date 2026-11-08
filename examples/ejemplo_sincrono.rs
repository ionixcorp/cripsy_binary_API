//! El mismo consumo, sin tokio ni `await`: fachada sincrónica (`SyncBroker`).
//!
//! ```powershell
//! $env:IQOPTION_EMAIL="tu-correo"; $env:IQOPTION_PASSWORD="tu-clave"
//! cargo run --example ejemplo_sincrono
//! ```

use std::env;

use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::sync::SyncBroker;
use cripsy_binary_api::domain::{CandleRequest, CandleSubscribe, Event, MarketStatus};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = IqOptionConfig {
        email: env::var("IQOPTION_EMAIL")?,
        password: env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice,
        ..IqOptionConfig::default()
    };

    // Conexión bloqueante: el consumidor no ve tokio.
    let mut broker = SyncBroker::connect(BrokerParams::IqOption(cfg))?;

    let balance = broker.get_balance()?;
    println!("balance: {} {}", balance.amount, balance.currency);

    let activos = broker.list_assets()?;
    let abierto = activos
        .iter()
        .find(|a| a.symbol.ends_with("-OTC") && a.status == MarketStatus::Open)
        .ok_or("no hay activos OTC abiertos ahora mismo")?
        .symbol
        .clone();
    println!(
        "catálogo: {} activos; activo de ejemplo {abierto}",
        activos.len()
    );

    let velas = broker.get_candles(CandleRequest {
        symbol: abierto.clone(),
        interval: 60,
        count: 5,
        end_time: None,
    })?;
    println!("velas históricas: {}", velas.len());

    // Streams como iteradores bloqueantes.
    let mut stream = broker.subscribe_candles(CandleSubscribe {
        symbol: abierto,
        interval: 60,
    })?;
    if let Some(vela) = stream.next() {
        println!("vela en vivo: cierre {}", vela?.close);
    }

    let mut ticks = 0u32;
    for ev in broker.events()?.take(50) {
        if let Event::PriceTick { .. } = ev {
            ticks += 1;
        }
    }
    println!("ticks vistos en 50 eventos: {ticks}");

    broker.disconnect()?;
    println!("listo: consumo sincrónico completo");
    Ok(())
}
