//! Verificación de punta a punta de las capacidades documentadas.
//!
//! Recorre la API completa y reporta `[OK]` / `[FALLO]` / `[AVISO]` por
//! capacidad. Sale con código 1 si algo falla.
//!
//! ```powershell
//! .\scripts\verificar_todo.ps1              # incluye órdenes en cuenta demo
//! .\scripts\verificar_todo.ps1 -SoloLectura # sin enviar órdenes
//! ```
//!
//! Las órdenes se envían con 1.0 USD en cuenta **demo** (`Practice`).

use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::error::BrokerError;
use cripsy_binary_api::broker::historico::{descargar, ultimo_cierre};
use cripsy_binary_api::broker::iq_option::session::IqOptionBroker;
use cripsy_binary_api::broker::trait_::Broker;
use cripsy_binary_api::domain::{
    BuyBinaryRequest, BuyDigitalRequest, CandleRequest, CandleSubscribe, Direction, Event,
    MarketStatus, OperationResult, OptionType,
};
use futures_util::StreamExt;

struct Verificador {
    ok: u32,
    fallos: u32,
    avisos: u32,
}

impl Verificador {
    fn nuevo() -> Self {
        Self {
            ok: 0,
            fallos: 0,
            avisos: 0,
        }
    }

    fn ok(&mut self, nombre: &str, detalle: impl AsRef<str>) {
        self.ok += 1;
        println!("  [OK]     {nombre} — {}", detalle.as_ref());
    }

    fn fallo(&mut self, nombre: &str, error: &str) {
        self.fallos += 1;
        println!("  [FALLO]  {nombre} — {error}");
    }

    fn aviso(&mut self, nombre: &str, detalle: &str) {
        self.avisos += 1;
        println!("  [AVISO]  {nombre} — {detalle}");
    }

    fn evaluar<T, E: std::fmt::Display>(
        &mut self,
        nombre: &str,
        res: Result<T, E>,
        detalle: impl Fn(&T) -> String,
    ) -> Option<T> {
        match res {
            Ok(v) => {
                let d = detalle(&v);
                self.ok(nombre, d);
                Some(v)
            }
            Err(e) => {
                self.fallo(nombre, &e.to_string());
                None
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let solo_lectura = env::args().any(|a| a == "--solo-lectura");
    let cfg = IqOptionConfig {
        email: env::var("IQOPTION_EMAIL")?,
        password: env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice,
        ..IqOptionConfig::default()
    };

    let mut v = Verificador::nuevo();
    println!("== Verificación de cripsy_binary_API ==\n");

    let mut broker = match IqOptionBroker::nuevo(BrokerParams::IqOption(cfg)).await {
        Ok(b) => {
            v.ok(
                "conexión (login + WebSocket + perfil)",
                "sesión establecida",
            );
            b
        }
        Err(e) => {
            eprintln!("[FATAL] no se pudo conectar: {e}");
            std::process::exit(1);
        }
    };

    match broker.session().await {
        Ok(s) => v.ok(
            "sesión",
            format!("cuenta {:?}, balance_id {:?}", s.account_type, s.balance_id),
        ),
        Err(e) => v.fallo("sesión", &e.to_string()),
    }

    v.evaluar("balance", broker.get_balance().await, |b| {
        format!("{} {}", b.amount, b.currency)
    });
    v.evaluar("tipo de cuenta", broker.get_account_type().await, |t| {
        format!("{t:?}")
    });

    let activos = v.evaluar("catálogo de activos", broker.list_assets().await, |a| {
        format!("{} activos", a.len())
    });

    let mut simbolo = String::new();
    if let Some(lista) = &activos {
        let mut por_clase: BTreeMap<String, usize> = BTreeMap::new();
        for a in lista {
            *por_clase.entry(format!("{:?}", a.class)).or_default() += 1;
        }
        v.ok(
            "mercados por clase",
            por_clase
                .iter()
                .map(|(k, c)| format!("{k}={c}"))
                .collect::<Vec<_>>()
                .join(", "),
        );

        let mut por_sector: BTreeMap<String, usize> = BTreeMap::new();
        for a in lista {
            *por_sector
                .entry(a.sector.clone().unwrap_or_else(|| "sin_sector".into()))
                .or_default() += 1;
        }
        let mut sectores: Vec<(String, usize)> = por_sector.into_iter().collect();
        sectores.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.ok(
            "mercados por sector",
            sectores
                .iter()
                .take(10)
                .map(|(k, c)| format!("{k}={c}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        if let Some(a) = lista
            .iter()
            .find(|a| a.symbol.ends_with("-OTC") && a.status == MarketStatus::Open)
        {
            simbolo = a.symbol.clone();
        }
        let con_metadatos = lista
            .iter()
            .filter(|a| {
                a.extras.as_ref().is_some_and(|e| {
                    e.expiration_times_secs.is_some()
                        || e.min_amount.is_some()
                        || e.buyback_deadtime.is_some()
                })
            })
            .count();
        v.ok(
            "metadatos operativos por activo",
            format!("{con_metadatos} activos"),
        );
    }

    if simbolo.is_empty() {
        eprintln!("[FATAL] no hay activos OTC abiertos ahora mismo; reintenta más tarde");
        std::process::exit(1);
    }
    println!("\n  activo de prueba: {simbolo}\n");

    v.evaluar(
        "estado de mercado",
        broker.asset_status(&simbolo).await,
        |s| format!("{s:?}"),
    );

    let velas = v.evaluar(
        "velas históricas (1m × 5)",
        broker
            .get_candles(CandleRequest {
                symbol: simbolo.clone(),
                interval: 60,
                count: 5,
                end_time: None,
            })
            .await,
        |v| format!("{} velas", v.len()),
    );
    if let Some(velas) = &velas {
        if velas.iter().all(|c| c.is_consistent()) {
            v.ok("consistencia de velas", "high/low y precios coherentes");
        } else {
            v.fallo("consistencia de velas", "alguna vela es inconsistente");
        }
    }

    let ahora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let hasta = ultimo_cierre(ahora, 60);
    let desde = hasta - 3_600;
    v.evaluar(
        "descarga paginada (1 h de 1m)",
        descargar(&broker, &simbolo, 60, desde, hasta).await,
        |w| {
            format!(
                "{} velas en {} página(s), huecos={}",
                w.velas.len(),
                w.paginas,
                w.faltantes
            )
        },
    );

    v.evaluar("payout", broker.get_payout(&simbolo, 60).await, |p| {
        format!("{:.1}%", p.profit_percent)
    });

    v.evaluar(
        "perfil del activo (GraphQL)",
        broker.get_asset_profile(&simbolo).await,
        |p| format!("{} ({})", p.broker_name, p.ticker),
    );

    v.evaluar(
        "historial de posiciones",
        broker.get_position_history(None, 5, 0).await,
        |h| format!("{} posiciones", h.positions.len()),
    );

    match broker.get_technical_indicators(&simbolo).await {
        Ok(i) => v.ok(
            "indicadores del bróker",
            format!(
                "{} campos recibidos",
                i.indicators.as_object().map_or(0, |o| o.len())
            ),
        ),
        Err(BrokerError::Rejected(m)) => v.aviso(
            "indicadores del bróker",
            &format!("el bróker rechaza el activo (documentado): {m}"),
        ),
        Err(e) => v.fallo("indicadores del bróker", &e.to_string()),
    }

    v.evaluar(
        "suscripción a comisiones",
        broker.subscribe_commission_changes().await,
        |_| "aceptada".to_string(),
    );
    v.evaluar(
        "suscripción a live-deals",
        broker.subscribe_live_deals(&simbolo).await,
        |_| "aceptada".to_string(),
    );

    match broker
        .subscribe_candles(CandleSubscribe {
            symbol: simbolo.clone(),
            interval: 60,
        })
        .await
    {
        Ok(mut stream) => {
            match tokio::time::timeout(Duration::from_secs(90), stream.next()).await {
                Ok(Some(Ok(vela))) => {
                    v.ok("stream de velas en vivo", format!("cierre {}", vela.close))
                }
                Ok(Some(Err(e))) => v.fallo("stream de velas en vivo", &e.to_string()),
                Ok(None) => v.fallo("stream de velas en vivo", "el stream cerró sin datos"),
                Err(_) => v.fallo("stream de velas en vivo", "sin vela en 90 s"),
            }
        }
        Err(e) => v.fallo("stream de velas en vivo", &e.to_string()),
    }

    match broker.events().await {
        Ok(mut eventos) => {
            let limite = tokio::time::Instant::now() + Duration::from_secs(30);
            let mut ticks = 0u32;
            while tokio::time::Instant::now() < limite && ticks < 5 {
                match tokio::time::timeout(Duration::from_secs(5), eventos.next()).await {
                    Ok(Some(Event::PriceTick { symbol, .. })) if symbol == simbolo => ticks += 1,
                    Ok(Some(_)) => continue,
                    Ok(None) => break,
                    Err(_) => continue,
                }
            }
            if ticks >= 5 {
                v.ok("ticks de precio nativos", format!("{ticks} ticks en ≤30 s"));
            } else {
                v.fallo(
                    "ticks de precio nativos",
                    &format!("solo {ticks} ticks en 30 s"),
                );
            }
        }
        Err(e) => v.fallo("ticks de precio nativos", &e.to_string()),
    }

    if solo_lectura {
        println!("\n  (órdenes omitidas por --solo-lectura)");
    } else {
        println!();
        verificar_ordenes(&mut broker, &simbolo, &mut v).await;
    }

    v.evaluar("desconexión limpia", broker.disconnect().await, |_| {
        "cerrada".to_string()
    });

    println!(
        "\n== Resultado: {} OK · {} fallos · {} avisos ==",
        v.ok, v.fallos, v.avisos
    );
    if v.fallos == 0 {
        println!("Todas las capacidades documentadas respondieron.");
        Ok(())
    } else {
        std::process::exit(1);
    }
}

async fn esperar_cierre(
    broker: &IqOptionBroker,
    id: &str,
    segundos: u64,
) -> Option<OperationResult> {
    let mut eventos = broker.events().await.ok()?;
    let limite = tokio::time::Instant::now() + Duration::from_secs(segundos);
    while tokio::time::Instant::now() < limite {
        match tokio::time::timeout(Duration::from_secs(10), eventos.next()).await {
            Ok(Some(Event::OrderClosed {
                id: cerrada,
                result,
            })) if cerrada == id => {
                return Some(result);
            }
            Ok(Some(_)) => continue,
            Ok(None) => return None,
            Err(_) => continue,
        }
    }
    None
}

async fn verificar_ordenes(broker: &mut IqOptionBroker, simbolo: &str, v: &mut Verificador) {
    // 1) Orden clásica (1 minuto => turbo) y cierre por evento.
    let turbo = broker
        .buy_binary(BuyBinaryRequest {
            symbol: simbolo.to_string(),
            amount: 1.0,
            direction: Direction::Call,
            expiration_minutes: 1,
            option_type: OptionType::Binary,
        })
        .await;
    match turbo {
        Ok(id) => {
            v.ok("orden clásica (turbo 1m)", format!("aceptada: {id}"));
            match esperar_cierre(broker, &id, 180).await {
                Some(result) => {
                    v.ok("cierre por evento", format!("{result:?}"));
                    match broker.get_operation_result(&id).await {
                        Ok(cache) if cache == result => {
                            v.ok("caché de resultado", "coincide con el evento")
                        }
                        Ok(cache) => v.fallo(
                            "caché de resultado",
                            &format!("evento {result:?} vs caché {cache:?}"),
                        ),
                        Err(e) => v.fallo("caché de resultado", &e.to_string()),
                    }
                }
                None => v.fallo("cierre por evento", "sin cierre en 180 s"),
            }
        }
        Err(e) => v.fallo("orden clásica (turbo 1m)", &e.to_string()),
    }

    // 2) Digital spot (1 minuto).
    let digital = broker
        .buy_digital_spot(BuyDigitalRequest {
            symbol: simbolo.to_string(),
            amount: 1.0,
            direction: Direction::Put,
            duration_minutes: 1,
            strike: 0.0,
        })
        .await;
    match digital {
        Ok(id) => {
            v.ok("orden digital (spot 1m)", format!("aceptada: {id}"));
            match esperar_cierre(broker, &id, 180).await {
                Some(result) => v.ok("cierre digital por evento", format!("{result:?}")),
                None => v.fallo("cierre digital por evento", "sin cierre en 180 s"),
            }
        }
        Err(e) => v.fallo("orden digital (spot 1m)", &e.to_string()),
    }

    // 3) Blitz (30 segundos): el bróker puede no admitirlo según el activo.
    let blitz = broker
        .buy_binary(BuyBinaryRequest {
            symbol: simbolo.to_string(),
            amount: 1.0,
            direction: Direction::Call,
            expiration_minutes: 30, // en blitz el campo transporta segundos
            option_type: OptionType::Blitz,
        })
        .await;
    match blitz {
        Ok(id) => {
            v.ok("orden blitz (30 s)", format!("aceptada: {id}"));
            match esperar_cierre(broker, &id, 120).await {
                Some(result) => v.ok("cierre blitz por evento", format!("{result:?}")),
                None => v.fallo("cierre blitz por evento", "sin cierre en 120 s"),
            }
        }
        Err(e) => v.aviso(
            "orden blitz (30 s)",
            &format!("el bróker no la aceptó ahora (estado del activo): {e}"),
        ),
    }

    // 4) Venta anticipada sobre una turbo de 5 minutos.
    let larga = broker
        .buy_binary(BuyBinaryRequest {
            symbol: simbolo.to_string(),
            amount: 1.0,
            direction: Direction::Call,
            expiration_minutes: 5,
            option_type: OptionType::Binary,
        })
        .await;
    match larga {
        Ok(id) => {
            v.ok("orden turbo 5m", format!("aceptada: {id}"));
            tokio::time::sleep(Duration::from_secs(15)).await;
            match broker.sell_option(&id).await {
                Ok(()) => v.ok("venta anticipada (buyback)", "aceptada por el bróker"),
                Err(e) => v.aviso(
                    "venta anticipada (buyback)",
                    &format!("no admitida en este momento: {e}"),
                ),
            }
        }
        Err(e) => v.fallo("orden turbo 5m", &e.to_string()),
    }
}
