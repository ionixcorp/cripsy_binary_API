//! Compra una orden clásica (turbo, 1 minuto) en cuenta DEMO y espera su cierre
//! por evento.
//!
//! ```powershell
//! $env:IQOPTION_EMAIL="tu-correo"; $env:IQOPTION_PASSWORD="tu-clave"
//! cargo run --example ejemplo_orden_demo
//! ```
//!
//! Mueve saldo de práctica, nunca dinero real: la cuenta se fuerza a `Practice`.
//! Ejecútalo solo si eres el dueño de la cuenta o tienes autorización explícita.

use std::env;
use std::time::Duration;

use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::iq_option::session::IqOptionBroker;
use cripsy_binary_api::broker::trait_::Broker;
use cripsy_binary_api::domain::{BuyBinaryRequest, Direction, Event, MarketStatus, OptionType};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = IqOptionConfig {
        email: env::var("IQOPTION_EMAIL")?,
        password: env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice, // demo explícita
        ..IqOptionConfig::default()
    };

    let mut broker = IqOptionBroker::nuevo(BrokerParams::IqOption(cfg)).await?;

    let activos = broker.list_assets().await?;
    let simbolo = activos
        .iter()
        .find(|a| a.symbol.ends_with("-OTC") && a.status == MarketStatus::Open)
        .ok_or("no hay activos OTC abiertos ahora mismo")?
        .symbol
        .clone();

    let antes = broker.get_balance().await?;
    println!("{simbolo}: balance inicial {}", antes.amount);

    // 1) Enviar la orden (1 minuto => turbo).
    let id = broker
        .buy_binary(BuyBinaryRequest {
            symbol: simbolo,
            amount: 1.0,
            direction: Direction::Call,
            expiration_minutes: 1,
            option_type: OptionType::Binary,
        })
        .await?;
    println!("orden {id} enviada; esperando el cierre por evento…");

    // 2) El resultado llega como evento, sin polling.
    let mut eventos = broker.events().await?;
    let mut resultado = None;
    let limite = tokio::time::Instant::now() + Duration::from_secs(180);
    while tokio::time::Instant::now() < limite {
        match tokio::time::timeout(Duration::from_secs(10), eventos.next()).await {
            Ok(Some(Event::OrderClosed {
                id: cerrada,
                result,
            })) if cerrada == id => {
                resultado = Some(result);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    println!("resultado: {resultado:?}");

    // 3) El caché del resultado se consulta sin latencia de red.
    println!("caché: {:?}", broker.get_operation_result(&id).await?);

    // 4) El bróker acredita el resultado un instante después del cierre: se
    //    espera el evento de balance. Leerlo al instante mostraría solo el
    //    stake descontado y daría un delta engañoso.
    let mut acreditado = false;
    let limite = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < limite {
        match tokio::time::timeout(Duration::from_secs(5), eventos.next()).await {
            Ok(Some(Event::BalanceChanged(_))) => {
                acreditado = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    let despues = broker.get_balance().await?;
    println!(
        "balance final {} (delta {}){}",
        despues.amount,
        despues.amount - antes.amount,
        if acreditado {
            ""
        } else {
            " — el bróker aún no había notificado el crédito"
        }
    );

    broker.disconnect().await?;
    Ok(())
}
