//! Sondeo de la API de Deriv (interno, no documentado).
//!
//! Modo público (sin credenciales): `cargo run --example deriv_probe -- public`
//! Modo autenticado (PAT + OTP):   `cargo run --example deriv_probe`
//!
//! Nunca imprime el token ni la URL del OTP.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const REST: &str = "https://api.derivws.com";
const WS_PUBLICO: &str = "wss://api.derivws.com/trading/v1/options/ws/public";

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
            let valor = valor.trim().trim_matches('"').trim_matches('\'');
            mapa.insert(clave.trim().to_string(), valor.to_string());
        }
    }
    mapa
}

fn recorte(valor: &Value, max: usize) -> String {
    let texto = valor.to_string();
    if texto.chars().count() <= max {
        texto
    } else {
        let corto: String = texto.chars().take(max).collect();
        format!("{corto}… ({} bytes)", texto.len())
    }
}

/// Envía las peticiones y muestra las respuestas relevantes.
async fn conversar(
    etiqueta: &str,
    url: &str,
    peticiones: &[(&str, Value)],
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== WebSocket: {etiqueta} ===");
    let (mut ws, _) = connect_async(url).await?;
    println!("conectado");

    for (_, msg) in peticiones {
        ws.send(Message::Text(msg.to_string())).await?;
    }

    let limite = tokio::time::Instant::now() + Duration::from_secs(25);
    let mut vistos = 0;
    while vistos < 14 && tokio::time::Instant::now() < limite {
        let Ok(Some(Ok(Message::Text(texto)))) =
            tokio::time::timeout(Duration::from_secs(6), ws.next()).await
        else {
            continue;
        };
        let valor: Value = match serde_json::from_str(&texto) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tipo = valor
            .get("msg_type")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        println!(
            "\n--- {tipo} (req_id={}) ---",
            valor.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0)
        );
        if let Some(e) = valor.get("error") {
            println!("error: {}", recorte(e, 300));
        }
        println!("{}", recorte(&valor, 1200));
        vistos += 1;
    }
    let _ = ws.close(None).await;
    println!("\nfin ({vistos} mensajes)");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let modo_publico = std::env::args().any(|a| a == "public");

    let peticiones_publicas = vec![
        ("time", json!({"time": 1, "req_id": 1})),
        (
            "active_symbols",
            json!({"active_symbols": "brief", "req_id": 2}),
        ),
        (
            "ticks_history",
            json!({"ticks_history": "R_100", "count": 3, "end": "latest",
                   "style": "candles", "granularity": 60, "req_id": 3}),
        ),
        (
            "proposal",
            json!({"proposal": 1, "amount": 1, "basis": "stake", "contract_type": "CALL",
                   "currency": "USD", "underlying_symbol": "R_100", "duration": 1,
                   "duration_unit": "m", "req_id": 4}),
        ),
        (
            "ticks",
            json!({"ticks": "R_100", "subscribe": 1, "req_id": 5}),
        ),
    ];

    if modo_publico {
        println!("modo público: sin credenciales");
        return conversar("público", WS_PUBLICO, &peticiones_publicas).await;
    }

    let env = cargar_env();
    let token = env.get("DERIV_TOKEN").cloned().unwrap_or_default();
    let appid = env.get("DERIV_APPID").cloned().unwrap_or_default();
    println!(
        "token presente: {} ({} chars)",
        !token.is_empty(),
        token.len()
    );
    println!(
        "appid presente: {} ({} chars)",
        !appid.is_empty(),
        appid.len()
    );

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;

    println!("\n=== GET /trading/v1/options/accounts ===");
    let resp = http
        .get(format!("{REST}/trading/v1/options/accounts"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Deriv-App-ID", &appid)
        .send()
        .await?;
    let estado = resp.status();
    let cuerpo = resp.text().await?;
    println!("http={estado}");
    println!("{}", recorte(&Value::String(cuerpo.clone()), 800));

    let cuentas: Value = match serde_json::from_str(&cuerpo) {
        Ok(v) if estado.is_success() => v,
        _ => {
            println!("\nREST no usable con este token; se prueba solo el gateway público.");
            return conversar("público (respaldo)", WS_PUBLICO, &peticiones_publicas).await;
        }
    };
    let demo = cuentas
        .get("data")
        .and_then(|d| {
            if d.is_array() {
                Some(d)
            } else {
                d.get("accounts")
            }
        })
        .and_then(|a| a.as_array())
        .and_then(|lista| {
            lista.iter().find(|c| {
                c.get("account_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.eq_ignore_ascii_case("demo"))
                    .unwrap_or(false)
                    || c.get("is_virtual")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    || c.get("account_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.starts_with("VRTC") || s.starts_with("DOT"))
                        .unwrap_or(false)
            })
        })
        .cloned();

    let Some(cuenta_demo) = demo else {
        println!("\nsin cuenta demo; fin");
        return Ok(());
    };
    let account_id = cuenta_demo
        .get("account_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    println!("\ncuenta demo: {account_id}");

    println!("\n=== POST /otp ===");
    let resp = http
        .post(format!(
            "{REST}/trading/v1/options/accounts/{account_id}/otp"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Deriv-App-ID", &appid)
        .send()
        .await?;
    println!("http={}", resp.status());
    let otp: Value = resp.json().await?;
    let Some(url) = otp
        .get("data")
        .and_then(|d| d.get("url"))
        .and_then(|v| v.as_str())
    else {
        println!("sin URL de OTP: {}", recorte(&otp, 800));
        return Ok(());
    };
    println!("otp url host: {}", url.split('?').next().unwrap_or("?"));
    let url = url.to_string();

    let autenticadas = vec![
        ("time", json!({"time": 1, "req_id": 1})),
        ("balance", json!({"balance": 1, "req_id": 2})),
        (
            "ticks_history",
            json!({"ticks_history": "R_100", "count": 3, "end": "latest",
                   "style": "candles", "granularity": 60, "req_id": 3}),
        ),
        (
            "proposal",
            json!({"proposal": 1, "amount": 1, "basis": "stake", "contract_type": "CALL",
                   "currency": "USD", "underlying_symbol": "R_100", "duration": 1,
                   "duration_unit": "m", "req_id": 4}),
        ),
        ("portfolio", json!({"portfolio": 1, "req_id": 5})),
        (
            "profit_table",
            json!({"profit_table": 1, "description": 1, "limit": 3, "offset": 0, "req_id": 6}),
        ),
    ];

    conversar("autenticado (OTP)", &url, &autenticadas).await
}
