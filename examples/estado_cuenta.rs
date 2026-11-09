//! Estado de la cuenta: sesión, balance y tipo de cuenta.
//!
//! ```powershell
//! $env:IQOPTION_EMAIL="tu-correo"; $env:IQOPTION_PASSWORD="tu-clave"
//! cargo run --example estado_cuenta
//! ```

use std::env;

use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::iq_option::session::IqOptionBroker;
use cripsy_binary_api::broker::trait_::Broker;
use cripsy_binary_api::domain::{MarketStatus, OptionType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = IqOptionConfig {
        email: env::var("IQOPTION_EMAIL")?,
        password: env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice,
        ..IqOptionConfig::default()
    };

    let mut broker = IqOptionBroker::nuevo(BrokerParams::IqOption(cfg)).await?;

    let sesion = broker.session().await?;
    println!("balance_id : {:?}", sesion.balance_id);
    println!("cuenta     : {:?}", sesion.account_type);

    let balance = broker.get_balance().await?;
    println!("saldo      : {} {}", balance.amount, balance.currency);
    println!("tipo       : {:?}", broker.get_account_type().await?);

    let simbolo = broker
        .list_assets()
        .await?
        .into_iter()
        .find(|a| a.symbol.ends_with("-OTC") && a.status == MarketStatus::Open)
        .map(|a| a.symbol)
        .ok_or("no hay activo OTC abierto")?;
    for (tipo, segundos) in [(OptionType::Digital, 60u64), (OptionType::Binary, 60u64)] {
        let cotizacion = broker.quote_payout(&simbolo, tipo, segundos, 1.0).await?;
        println!(
            "quote      : {} {:?} {}s available={} profit={:?} min={:?} max={:?} captured_at={}",
            cotizacion.symbol,
            cotizacion.option_type,
            cotizacion.expiration_seconds,
            cotizacion.available,
            cotizacion.profit_percent,
            cotizacion.min_amount,
            cotizacion.max_amount,
            cotizacion.captured_at
        );
    }

    broker.disconnect().await?;
    Ok(())
}
