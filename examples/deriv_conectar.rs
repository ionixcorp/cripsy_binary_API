//! Ejemplo del adaptador Deriv.
//!
//! Sin credenciales (modo público): mercado y precios.
//! Con `DERIV_TOKEN` en `.env`: además balance y sesión.
//!
//! `cargo run --example deriv_conectar`
//! `cargo run --example deriv_conectar -- public`

use std::collections::HashMap;

use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};
use cripsy_binary_api::domain::{CandleRequest, MarketStatus, OptionType};

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
    let forzar_publico = std::env::args().any(|a| a == "public");
    let token = env.get("DERIV_TOKEN").cloned().filter(|t| !t.is_empty());
    let publico = forzar_publico || token.is_none();

    let config = if publico {
        DerivConfig::publico()
    } else {
        DerivConfig {
            token,
            app_id: env.get("DERIV_APPID").cloned(),
            account_id: env.get("DERIV_ACCOUNT_ID").cloned(),
            currency: Some("USD".into()),
            ..DerivConfig::default()
        }
    };
    println!(
        "modo: {} | broker: deriv | config: {:?}",
        if publico { "público" } else { "autenticado" },
        config
    );

    let broker = DerivBroker::connect(BrokerParams::Deriv(config)).await?;
    println!("conectado: {}", broker.is_connected().await);

    let activos = broker.list_assets().await?;
    println!("activos: {}", activos.len());
    for a in activos.iter().take(3) {
        println!("  {} — {} ({:?})", a.symbol, a.name, a.class);
    }
    if let Some(r100) = activos.iter().find(|a| a.symbol == "R_100") {
        println!(
            "R_100 estado: {:?}",
            broker.asset_status(&r100.symbol).await?
        );
        assert_eq!(broker.asset_status("R_100").await?, r100.status);
    } else {
        println!("aviso: R_100 no está en la lista");
    }

    let velas = broker
        .get_candles(CandleRequest {
            symbol: "R_100".into(),
            interval: 60,
            count: 5,
            end_time: None,
        })
        .await?;
    println!("velas: {}", velas.len());
    if let Some(ultima) = velas.last() {
        println!(
            "  última: from={} o={} h={} l={} c={} volumen={:?} coherente={}",
            ultima.from,
            ultima.open,
            ultima.high,
            ultima.low,
            ultima.close,
            ultima.volume,
            ultima.is_consistent()
        );
    }

    let payout = broker.get_payout("R_100", 60).await?;
    println!(
        "payout(60s): {:.2}% cerrado={}",
        payout.profit_percent, payout.closed
    );

    for (etiqueta, opcion, segundos) in [
        ("rise/fall 60s", OptionType::Binary, 60),
        ("rise/fall 300s", OptionType::Binary, 300),
        ("digital 60s", OptionType::Digital, 60),
    ] {
        match broker.quote_payout("R_100", opcion, segundos, 1.0).await {
            Ok(q) => println!(
                "quote {etiqueta}: available={} profit={:?} min={:?} max={:?} captured_at={}",
                q.available, q.profit_percent, q.min_amount, q.max_amount, q.captured_at
            ),
            Err(e) => println!("quote {etiqueta}: error {e}"),
        }
    }

    match broker.get_balance().await {
        Ok(b) => {
            let s = broker.session().await?;
            println!(
                "balance: {:.2} {} | cuenta={:?} tipo={:?} offset={}ms",
                b.amount, b.currency, s.account_id, b.account_type, s.server_time_offset_ms
            );
        }
        Err(e) => println!("balance no disponible: {e}"),
    }

    match broker.get_position_history(None, 5, 0).await {
        Ok(h) => {
            println!(
                "historial: {} posiciones (total={:?})",
                h.positions.len(),
                h.total
            );
            for p in h.positions.iter().take(3) {
                println!(
                    "  {} {} {:?} pnl={:?} cerrada={}",
                    p.id,
                    p.symbol,
                    p.instrument_type,
                    p.pnl,
                    p.is_closed()
                );
            }
        }
        Err(e) => println!("historial no disponible: {e}"),
    }

    let estado = broker.asset_status("R_100").await?;
    assert!(matches!(
        estado,
        MarketStatus::Open | MarketStatus::Closed | MarketStatus::Suspended
    ));
    println!("fin");
    Ok(())
}
