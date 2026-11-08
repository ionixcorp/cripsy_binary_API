//! Ejemplo de solo lectura: sesión, cuenta, catálogo, histórico y stream.
//!
//! ```powershell
//! $env:IQOPTION_EMAIL="tu-correo"; $env:IQOPTION_PASSWORD="tu-clave"
//! cargo run --example ejemplo_lectura
//! ```
//!
//! No envía órdenes: no mueve saldo. Usa cuenta demo por defecto.

use std::env;

use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::iq_option::session::IqOptionBroker;
use cripsy_binary_api::broker::trait_::Broker;
use cripsy_binary_api::domain::{CandleRequest, CandleSubscribe, MarketStatus};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = IqOptionConfig {
        email: env::var("IQOPTION_EMAIL")?,
        password: env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice, // demo explícita
        ..IqOptionConfig::default()
    };

    // 1) Conexión: login HTTP + WebSocket + perfil.
    let mut broker = IqOptionBroker::nuevo(BrokerParams::IqOption(cfg)).await?;
    let sesion = broker.session().await?;
    println!(
        "sesión: cuenta {:?}, balance_id {:?}",
        sesion.account_type, sesion.balance_id
    );

    // 2) Balance de la cuenta activa.
    let balance = broker.get_balance().await?;
    println!("balance: {} {}", balance.amount, balance.currency);

    // 3) Catálogo: se elige un activo OTC abierto.
    let activos = broker.list_assets().await?;
    println!("catálogo: {} activos", activos.len());
    let abierto = activos
        .iter()
        .find(|a| a.symbol.ends_with("-OTC") && a.status == MarketStatus::Open)
        .ok_or("no hay activos OTC abiertos ahora mismo")?
        .symbol
        .clone();
    println!("activo de ejemplo: {abierto}");

    // 4) Velas históricas de 1 minuto.
    let velas = broker
        .get_candles(CandleRequest {
            symbol: abierto.clone(),
            interval: 60,
            count: 5,
            end_time: None,
        })
        .await?;
    for vela in &velas {
        println!("vela: cierre {} (desde {})", vela.close, vela.from);
    }

    // 5) Stream de velas en vivo (la primera llega al cerrar la vela).
    let mut stream = broker
        .subscribe_candles(CandleSubscribe {
            symbol: abierto,
            interval: 60,
        })
        .await?;
    if let Some(vela) = stream.next().await {
        println!("vela en vivo: cierre {}", vela?.close);
    }

    broker.disconnect().await?;
    println!("listo: sesión, catálogo, histórico y stream");
    Ok(())
}
