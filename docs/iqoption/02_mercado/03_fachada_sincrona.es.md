# Fachada sincrónica e histórico

La API ofrece el mismo núcleo de dos formas. Si tu proyecto no es async, usa la
fachada bloqueante: **no necesitas tokio ni `await`**.

## Fachada sincrona

```rust
use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::sync::SyncBroker;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = IqOptionConfig {
        email: std::env::var("IQOPTION_EMAIL")?,
        password: std::env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice,
        ..IqOptionConfig::default()
    };

    let mut broker = SyncBroker::connect(BrokerParams::IqOption(cfg))?;

    // Todo bloqueante, misma semántica que la versión async.
    let balance = broker.get_balance()?;
    println!("balance: {} {}", balance.amount, balance.currency);

    let activos = broker.list_assets()?;
    let abierto = activos
        .iter()
        .find(|a| a.symbol.ends_with("-OTC") && a.status == cripsy_binary_api::domain::MarketStatus::Open)
        .expect("OTC abierto");

    // Streams como iteradores bloqueantes.
    let mut velas = broker.subscribe_candles(cripsy_binary_api::domain::CandleSubscribe {
        symbol: abierto.symbol.clone(),
        interval: 60,
    })?;
    if let Some(v) = velas.next() {
        println!("vela en vivo: {:?}", v?);
    }

    let mut eventos = broker.events()?;
    for ev in eventos.take(5) {
        println!("evento: {ev:?}");
    }

    broker.disconnect()?;
    Ok(())
}
```

Reglas:

- **No uses la fachada dentro de un contexto async**: bloquearía el executor
  del llamador. En async, usa el trait `Broker` directamente.
- Los iteradores tienen timeout de 30 s: si no llega nada en ese plazo, el
  iterador se cierra (vuelve a suscribirte si lo necesitas).
- El bus de eventos tiene buffer acotado: un consumidor lento puede perder
  eventos, igual que en async.

Métodos disponibles (todos bloqueantes): `session`, `is_connected`,
`disconnect`, `get_balance`, `get_account_type`, `switch_account`,
`list_assets`, `asset_status`, `get_candles`, `subscribe_candles`,
`get_position_history`, `get_payout`, `quote_payout`, `get_asset_profile`,
`get_technical_indicators`, `subscribe_live_deals`,
`subscribe_commission_changes`, `events`, `buy_binary`,
`buy_digital_spot`, `sell_option`, `get_operation_result`.

## Ayudante de histórico

`cripsy_binary_api::broker::historico` facilita descargar el histórico granular
(límites verificados: 1 s × 7 días, 1000 velas por página):

| Elemento | Para qué sirve |
|---|---|
| `VELAS_POR_PAGINA` | Tamaño de página recomendado (1000) |
| `PAGINAS_MAXIMAS` | Techo de seguridad para no bucles infinitos (5000) |
| `Ventana` | Rango con `count` máximo ya contemplado |
| `ultimo_cierre(ahora, intervalo)` | Último cierre de vela cerrada (multiplo del intervalo) |
| `paginas_estimadas(intervalo, desde, hasta)` | Cuántas páginas pedirá la descarga |
| `faltantes(velas, intervalo)` | Huecos detectados en una página |
| `descargar(...)` | Descarga completa con paginación |

Ejemplo:

```rust
use cripsy_binary_api::broker::historico::{descargar, paginas_estimadas};

// Antes de descargar, sabe cuánto costará:
let paginas = paginas_estimadas(1, desde_epoch, hasta_epoch);

// Descarga completa con paginación (devuelve una Ventana con las velas).
let ventana = descargar(
    &broker,               // &dyn Broker (async)
    "USDZAR-OTC",
    1,                     // intervalo: 1 segundo
    desde_epoch,           // epoch segundos
    hasta_epoch,           // epoch segundos
).await?;

println!("{} velas en {} páginas", ventana.velas.len(), ventana.paginas);
```

Con size=1 y el límite de 7 días del bróker, una semana completa de un activo
son ~605 páginas de 1000.
