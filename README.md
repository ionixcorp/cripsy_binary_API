# cripsy_binary_API

Librería **Rust** para operar contra brókers de opciones binarias: núcleo
asíncrono (`async`/`await`), fachada bloqueante para quien no usa tokio, y un
modelo de dominio tipado de extremo a extremo.

Gratuita y de código abierto bajo licencia MIT. Se consume **como dependencia**:
no se copia ni se modifica.

## Qué hace

| Área | Capacidad |
|------|-----------|
| Cuenta | Login HTTP + WebSocket, perfil, balances, cambio demo/real |
| Catálogo | Activos binary, turbo y blitz con payout, duraciones, límites y metadatos |
| Velas históricas | Desde 1 segundo hasta diario, con paginación y ayudante de descarga |
| Stream en vivo | Velas multi-tamaño y ticks de precio (~3/s por activo), sin polling |
| Órdenes | Clásicas (turbo/binary), digitales v2, blitz v2 y venta anticipada (buyback) |
| Resultados | Cierre de cada orden por evento, con caché consultable sin latencia de red |
| Análisis de cuenta | Historial de posiciones con PnL, saldos y tipo de cuenta |
| Datos de mercado | Perfil fundamental del activo, operaciones de terceros (live-deal) y cambios de comisión |
| Arquitectura | Interfaz común `Broker` y registro de adaptadores (multibróker) |

Características de diseño:

- **Sin estado global y sin polling.** Cada petición crea un canal y la
  respuesta se correlaciona por `request_id`; los resultados llegan por eventos.
- **Tipado de extremo a extremo.** Nada de diccionarios crudos en la frontera
  pública: el adaptador traduce el protocolo del bróker a tipos de dominio.
- **Dos formas de consumo del mismo núcleo.** `async` para aplicaciones
  concurrentes y `SyncBroker` para código bloqueante.

## Requisitos

- Rust 1.86 o superior (`rustup`).
- Credenciales del bróker (empieza con **cuenta demo**).
- Windows, Linux o macOS.

## Instalación

Instalación guiada en PowerShell, con aprobación en cada paso:

```powershell
.\installation\install.ps1                 # completo
.\installation\install.ps1 -ConPruebaDeHumo:$false   # sin contactar al bróker
.\installation\check_env.ps1               # solo verificar el entorno
```

O manualmente:

```powershell
cargo build --release                                  # variante async
cargo build --release --no-default-features --features blocking   # variante bloqueante
```

## Configuración de credenciales

Copia `.env.example` a `.env` y complétalo. `.env` está ignorado por git:

```dotenv
IQOPTION_EMAIL = "tu-correo@ejemplo.com"
IQOPTION_PASSWORD = "tu-contrasena"
```

La librería **no lee** el archivo por ti: exporta las variables en el entorno
(el instalador y los scripts de este repositorio lo hacen por ti).

## Uso

### Núcleo async

```rust
use cripsy_binary_api::broker::config::{AccountSelection, BrokerParams, IqOptionConfig};
use cripsy_binary_api::broker::iq_option::session::IqOptionBroker;
use cripsy_binary_api::broker::trait_::Broker;
use cripsy_binary_api::domain::CandleRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = IqOptionConfig {
        email: std::env::var("IQOPTION_EMAIL")?,
        password: std::env::var("IQOPTION_PASSWORD")?,
        account: AccountSelection::Practice, // demo explícita
        ..IqOptionConfig::default()
    };

    let mut broker = IqOptionBroker::nuevo(BrokerParams::IqOption(cfg)).await?;

    let balance = broker.get_balance().await?;
    println!("balance: {} {}", balance.amount, balance.currency);

    let velas = broker
        .get_candles(CandleRequest {
            symbol: "EURUSD-OTC".into(),
            interval: 60,
            count: 5,
            end_time: None,
        })
        .await?;
    println!("velas: {}", velas.len());

    broker.disconnect().await?;
    Ok(())
}
```

### Fachada síncrona

El mismo núcleo, sin tokio ni `await`:

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
    let balance = broker.get_balance()?;
    println!("balance: {} {}", balance.amount, balance.currency);
    broker.disconnect()?;
    Ok(())
}
```

### Ejemplos ejecutables

```powershell
cargo run --example ejemplo_lectura        # sesión, catálogo, histórico y stream
cargo run --example ejemplo_sincrono       # mismo consumo, bloqueante
cargo run --example ejemplo_orden_demo     # compra en demo y cierre por evento
```

## Consumirla desde tu proyecto

```toml
# Cargo.toml de TU proyecto
[dependencies]
cripsy-binary-api = { path = "../cripsy_binary_API", features = ["async"] }
```

Buenas prácticas:

1. **Fija la versión** que consumes (tag o commit concreto), no la punta de la
   rama: una actualización no debe romperte sin avisar.
2. **No modifiques este repositorio** ni parchees el crate localmente. Tus
   herramientas y ejemplos viven en tu proyecto. Si necesitas una capacidad
   nueva o encuentras un fallo, abre un issue o un pull request aquí.
3. **Usa el trait `Broker`** y los tipos de dominio; no hables directamente con
   el WebSocket ni construyas sobres de protocolo por tu cuenta.
4. Los montos deben interpretarse con la traducción del adaptador (micro-unidades
   en algunos mensajes, `crédito total = stake + ganancia` en los cierres).

## Documentación

| Documento | Contenido |
|-----------|-----------|
| `docs/01_introduccion.es.md` | Qué es, estado de cada componente y cómo leer la guía |
| `docs/02_account/` | Conexión, cuentas demo/real, balances |
| `docs/03_candle_and_stream/` | Históricos con sus límites, streams y fachada síncrona |
| `docs/04_digital/` | Compra digital v2 y ciclo de vida |
| `docs/05_binary/` | Órdenes clásicas turbo/binary |
| `docs/06_blitz/` | Órdenes blitz y rollovers |
| `docs/07_herramientas/` | Instalación, compuerta de calidad y cómo añadir un bróker |
| `docs/08_indicadores/` | Indicadores del bróker: estado real y alternativas |

Versión navegable (un solo HTML, sin dependencias externas):

```powershell
.\scripts\build_docs.ps1 -Lang es   # genera docs/site/es/index.html
```

Abre `index.html` en la raíz del proyecto: detecta la instalación y enlaza la
documentación generada.

## Estructura del proyecto

```
src/domain/      modelo de dominio puro (sin brókers ni red)
src/broker/      trait Broker, configuración y adaptadores por bróker
src/transport/   HTTP/WebSocket, reintentos y router de request_id
examples/        uso real de la API + generador de documentación
docs/            guía de usuario (Markdown, fuente de verdad)
installation/    instalación guiada y verificación de entorno
scripts/         compuerta de calidad y generación de documentación
```

## Verificación

```powershell
.\scripts\verify.ps1      # fmt + clippy -D warnings + test + build blocking + docs
cargo test                # tests unitarios del crate
```

## Garantía y descargo

**Lo documentado funciona.** Si usas la librería como dependencia, el trait
`Broker` y los canales documentados en `docs/`, el comportamiento descrito es
el que obtienes: sesión, catálogo, velas históricas y en vivo, ticks, órdenes
clásicas/digitales/blitz y ciclo de vida por eventos.

**Queda fuera de la garantía** puentear el contrato (hablar directo con el
WebSocket), usar canales no documentados, interpretar montos sin la traducción
del adaptador o modificar el crate en local. Los problemas derivados de esos
usos no son responsabilidad de los mantenedores.

La API expone el texto de garantía y un revisor de configuración para que puedas
mostrarlos desde tu aplicación:

```rust
use cripsy_binary_api::{mensaje_consumidor, revisar_config};

println!("{}", mensaje_consumidor());
let cfg = cripsy_binary_api::broker::IqOptionConfig::default();
for aviso in &revisar_config(&cfg).advertencias {
    eprintln!("aviso de uso: {aviso}");
}
```

## Seguridad

- Las credenciales viven en `.env` (ignorado por git) y se leen como variables
  de entorno. Nunca en el código, en los ejemplos ni en los logs.
- La conexión usa TLS con verificación de certificados por defecto
  (`verify_tls`).
- Las operaciones con dinero real requieren que elijas `AccountSelection::Real`
  de forma explícita; los ejemplos y el instalador usan **demo**.

## Base conceptual

El diseño se apoya en el conocimiento público del protocolo acumulado por la
comunidad, en particular **la API original de IQ Option publicada en GitHub**.
Este proyecto es una reimplementación moderna desde cero: no contiene código
heredado de aquella, ni la usa como dependencia, ni reproduce su arquitectura
interna.

## Contribuir

1. Abre un issue describiendo el caso de uso y la evidencia del comportamiento
   del bróker.
2. Mantén `.\scripts\verify.ps1` en verde: `fmt`, `clippy -D warnings`, tests y
   build de la variante `blocking`, además de la generación de documentación.
3. Un cambio de contrato incompatible exige versión mayor y nota de migración.

## Aviso

Este software no es asesoría financiera. Operar opciones binarias implica riesgo
de pérdida total del capital y puede estar restringido por la legislación de tu
país o por los términos del bróker. Usa cuenta demo y verifica las condiciones
del bróker antes de operar.

## Licencia

MIT — ver [`LICENSE`](LICENSE).
