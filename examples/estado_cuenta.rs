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

    broker.disconnect().await?;
    Ok(())
}
