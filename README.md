# cripsy_binary_API

[![Licencia: MIT](https://img.shields.io/badge/licencia-MIT-blue.svg)](LICENSE)
[![Rust 1.86+](https://img.shields.io/badge/rust-1.86%2B-orange.svg)](https://www.rust-lang.org)
[![Versión](https://img.shields.io/badge/versi%C3%B3n-0.1.0-informational.svg)](Cargo.toml)
[![CI](https://github.com/ionixcorp/cripsy_binary_API/actions/workflows/ci.yml/badge.svg)](https://github.com/ionixcorp/cripsy_binary_API/actions)

**La API en Rust para operar opciones binarias con datos y eventos en tiempo
real.** Un núcleo `async` y una fachada bloqueante sobre el mismo motor, dominio
tipado de extremo a extremo y **cero polling**: todo llega por eventos.

Gratuita, de código abierto (MIT) y sin dependencias heredadas. Se consume como
librería.

```rust
let broker = IqOptionBroker::nuevo(BrokerParams::IqOption(cfg)).await?;
let orden  = broker.buy_binary(BuyBinaryRequest {
    symbol: "EURUSD-OTC".into(), amount: 1.0, direction: Direction::Call,
    expiration_minutes: 1, option_type: OptionType::Binary,
}).await?;

// El resultado llega solo, como evento. Sin bucles, sin sleeps.
while let Some(Event::OrderClosed { id, result }) = broker.events().await?.next().await {
    println!("orden {id}: {result:?}");
    break;
}
```

## 🚀 Qué puedes hacer con ella

| | Capacidad | Detalle |
|---|---|---|
| 🔐 | **Sesión completa** | Login, WebSocket, perfil, balances con saldo retenido/bonos y cambio demo ↔ real en caliente |
| 📊 | **791 activos** | Catálogo vivo con payout, duraciones, mínimos/máximos, sector, proveedor y horarios |
| 🌍 | **Mercados reales** | 248 forex · 165 cripto · 165 acciones · 102 índices · 62 materias primas (medido contra el bróker) |
| 🎚️ | **Tres grupos operables** | Clásicas (turbo/binary), **blitz** con duraciones en segundos y **digitales v2** |
| 🕯️ | **Velas históricas** | De 1 segundo a 1 día · hasta 7 días en 1 s · descarga paginada con detección de huecos |
| ⚡ | **Stream sin polling** | Velas multi-tamaño y **ticks nativos ~3/s por activo**; 75 activos en una sola sesión |
| 🎯 | **Cuatro tipos de orden** | Clásicas (turbo/binary), digitales v2, **blitz v2** y venta anticipada (buyback) |
| 📈 | **Resultado por evento** | Cierre tipado (`Win`/`Loss`/`Equal`) con caché consultable sin latencia de red |
| 🧾 | **Analítica de cuenta** | Historial de posiciones con PnL realizado, invertido y apalancamiento |
| 🌐 | **Datos de plataforma** | Perfil fundamental del activo (nombre, ticker, variaciones), operaciones de terceros (live-deal) y cambios de comisión |
| 🧠 | **Señales del bróker** | Humor de traders y activos más populares, emitidos como eventos |
| 🔁 | **Dos formas de consumo** | `async`/`await` o `SyncBroker` bloqueante, mismo núcleo |
| 🧩 | **Multibróker** | Trait `Broker` + registro de adaptadores: añadir un bróker no toca dominio ni transporte |
| 🛡️ | **Uso responsable** | Revisor de configuración con avisos (TLS, timeouts, cuenta real) y texto de garantía listo para mostrar |

### Por qué no es «una más»

- **Nada de polling.** Cada petición se correlaciona por `request_id` y los
  resultados llegan por eventos. No hay `while` preguntando «¿ya cerró?».
- **Datos que otras APIs no exponen:** metadatos operativos por activo (ventana
  de buyback, deadtime, proveedor de precio, rollovers de blitz), perfil
  fundamental vía GraphQL, comisiones en vivo y operaciones de terceros.
- **Dominio tipado:** `OrderId`, `Payout`, `Position`, `Asset`… nada de
  diccionarios crudos en la frontera pública.
- **Blitz de verdad:** duraciones en segundos con `expired` alineado y
  resolución del activo contra el grupo correcto del catálogo.

## 📦 Instalación

```powershell
git clone https://github.com/ionixcorp/cripsy_binary_API.git
cd cripsy_binary_API
.\installation\install.ps1        # guiado, con aprobación en cada paso
```

Manual:

```powershell
cargo build --release                                           # async
cargo build --release --no-default-features --features blocking # sync
```

Credenciales en `.env` (ignorado por git, nunca en el código):

```dotenv
IQOPTION_EMAIL = "tu-correo@ejemplo.com"
IQOPTION_PASSWORD = "tu-contrasena"
```

## ✅ Compruébalo tú mismo en 30 segundos

```powershell
.\scripts\conectar.ps1        # sesión, balance y catálogo
.\scripts\balance.ps1         # balance y tipo de cuenta
.\scripts\verificar_todo.ps1  # valida TODAS las capacidades documentadas
```

`verificar_todo.ps1` recorre la API de punta a punta y devuelve `[OK]`/`[FALLO]`
por capacidad: sesión, catálogo, mercados, velas históricas, descarga paginada,
payout, perfil, historial, suscripciones, stream, ticks y —en cuenta demo—
órdenes clásicas, digitales, blitz y buyback. Añade `-SoloLectura` para omitir
las órdenes.

> Última pasada registrada (cuenta demo, 8 nov 2026): **28 comprobaciones OK,
> 0 fallos, 1 aviso** (el bróker no expone indicadores para ese activo).

## 🧭 Uso

Ejemplos ejecutables (código completo en `examples/`):

```powershell
cargo run --example ejemplo_lectura      # async: sesión, catálogo, velas, stream
cargo run --example ejemplo_sincrono     # sync: sin tokio ni await
cargo run --example ejemplo_orden_demo   # orden demo y cierre por evento
```

Toda la guía de usuario, con ejemplos completos y los detalles que el bróker
exige, está en **[`docs/`](docs/)**:

| Documento | Contenido |
|---|---|
| [Introducción](docs/01_introduccion.es.md) | Qué es, estado real de cada componente y qué no hace |
| [Sesión y cuentas](docs/02_account/01_sesion_cuentas.es.md) | Conexión, demo/real, balances |
| [Históricos](docs/03_candle_and_stream/01_historicos.es.md) | Límites verificados y descarga masiva |
| [Stream y ticks](docs/03_candle_and_stream/02_stream_ticks.es.md) | Velas multi-tamaño y densidad de ticks |
| [Fachada síncrona](docs/03_candle_and_stream/03_fachada_sincrona.es.md) | Consumo bloqueante |
| [Digital](docs/04_digital/01_digital.es.md) · [Binary](docs/05_binary/01_binary.es.md) · [Blitz](docs/06_blitz/01_blitz.es.md) | Los tres tipos de orden |
| [Herramientas](docs/07_herramientas/01_herramientas.es.md) | Instalación, scripts y cómo añadir un bróker |
| [Indicadores](docs/08_indicadores/01_indicadores.es.md) | Estado real del canal de indicadores |

**Documentación en línea:** <https://ionixcorp.github.io/cripsy_binary_API/>

**Verla en local** (un solo HTML, sin dependencias):

```powershell
.\scripts\build_docs.ps1 -Lang es    # genera docs/site/es/index.html
```

- **VS Code:** instala la extensión **Live Server** (Go Live) y pulsa *Go Live*
  sobre `index.html` o `docs/site/es/index.html`.
- **Manual:** abre `docs/site/es/index.html` con doble clic — funciona también
  sin servidor (`file://`): la navegación es por hash y no hace `fetch`.

## 🔌 Consumirla desde tu proyecto

```toml
[dependencies]
cripsy-binary-api = { path = "../cripsy_binary_API", features = ["async"] }
```

- Fija la versión (tag o commit): una actualización no debe romperte sin avisar.
- Usa el trait `Broker` y los tipos de dominio; no hables directo con el
  WebSocket ni construyas sobres de protocolo.
- Interpreta los montos con la traducción del adaptador (micro-unidades,
  `crédito total = stake + ganancia`).
- Si necesitas una capacidad nueva o encuentras un fallo, abre un issue o un PR.

## 🗂️ Estructura

```
src/domain/      modelo puro: activos, velas, órdenes, posiciones, eventos
src/broker/      trait Broker, configuración, adaptadores y fachada sync
src/transport/   HTTP/WebSocket, reintentos con backoff y router request_id
examples/        uso real + generador de documentación
docs/            guía de usuario (Markdown, fuente de verdad)
scripts/         compuerta de calidad, docs y validadores listos para usar
installation/    instalación guiada y verificación de entorno
```

## 🧪 Calidad

```powershell
.\scripts\verify.ps1   # fmt + clippy -D warnings + 61 tests + build blocking + docs
```

CI en GitHub Actions (Linux y Windows) ejecuta lo mismo en cada push, más la
generación de la documentación. Sin credenciales: los tests no tocan la red.

## ⚖️ Garantía y descargo

**Lo documentado funciona.** Si consumes la librería como dependencia, el trait
`Broker` y los canales documentados en `docs/`, obtienes el comportamiento
descrito. Queda fuera de la garantía puentear el contrato (hablar directo con el
WebSocket), usar canales no documentados, interpretar montos sin la traducción
del adaptador o modificar el crate en local.

```rust
use cripsy_binary_api::{mensaje_consumidor, revisar_config};

println!("{}", mensaje_consumidor());
for aviso in &revisar_config(&cfg).advertencias {
    eprintln!("aviso de uso: {aviso}");
}
```

## 🔒 Seguridad

- Credenciales solo en `.env` (git-ignored); la librería las recibe por entorno.
- TLS con verificación de certificados activa por defecto (`verify_tls`).
- Los ejemplos usan **cuenta demo**; operar con dinero real exige elegir
  `AccountSelection::Real` de forma explícita.

## 🧱 Base conceptual

El diseño se apoya en el conocimiento público del protocolo acumulado por la
comunidad, en particular **la API original de IQ Option publicada en GitHub**.
Este proyecto es una reimplementación moderna desde cero: no contiene código
heredado de aquella, ni la usa como dependencia.

## ⚠️ Aviso

Este software no es asesoría financiera. Operar opciones binarias implica riesgo
de pérdida total del capital y puede estar restringido por la legislación de tu
país o por los términos del bróker. Usa cuenta demo y verifica las condiciones
antes de operar.

## 📄 Licencia

MIT — ver [`LICENSE`](LICENSE).
