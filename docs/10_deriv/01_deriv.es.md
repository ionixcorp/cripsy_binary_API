# Deriv

Segundo bróker soportado, sobre la **API moderna** (`developers.deriv.com`):
REST para cuentas y OTP, WebSocket JSON para mercado y ejecución.

## Alcance

- **Operación:** opciones binarias **rise/fall** (`CALL`/`PUT`) en todos los
  activos que el bróker habilite.
- **Datos:** catálogo, velas y **ticks nativos de cualquier activo**; el
  consumidor puede analizar todo el mercado aunque solo opere rise/fall.
- **Fuera de alcance por ahora:** multiplicadores, acumuladores, dígitos,
  barreras y demás familias. No se simulan sobre rise/fall; se añadirán como
  tipos de dominio cuando se amplíe el producto.

## Dos modos

| Modo | Requisitos | Qué permite |
|---|---|---|
| Público | ninguno | catálogo, velas, ticks y cotización de payout |
| Autenticado | PAT con scope `trade` (o OAuth) + App ID | además balance, compra, venta e historial |

El modo público es el valor por defecto: si no hay token, la sesión no inventa
una cuenta ni permite operar. Las claves son tuyas y viven en `.env` (ignorado
por git); la plantilla está en `.env.example`.

## Conexión

```rust
use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};

// Solo mercado (gratuito, sin credenciales).
let broker = DerivBroker::connect(BrokerParams::Deriv(DerivConfig::publico())).await?;

// Cuenta demo o real: el OTP se pide por REST y la URL del socket es efímera.
let broker = DerivBroker::connect(BrokerParams::Deriv(DerivConfig {
    token: std::env::var("DERIV_TOKEN").ok(),
    app_id: std::env::var("DERIV_APPID").ok(),
    account_id: std::env::var("DERIV_ACCOUNT_ID").ok(),
    ..DerivConfig::default()
})).await?;
```

- El **PAT** exige la cabecera `Deriv-App-ID`; OAuth no.
- El OTP **caduca a los 120 s y es de un solo uso**; al reconectar se pide uno
  nuevo. La API lo hace sola: tú no ves ni gestionas el OTP.

## Capacidades

| Operación | Método | Estado |
|---|---|---|
| Catálogo de activos | `list_assets` | verificado |
| Estado de mercado | `asset_status` | verificado |
| Velas históricas | `get_candles` | verificado (granularidades 1 m – 1 d) |
| Velas en vivo | `subscribe_candles` | implementado |
| Ticks nativos | `subscribe_ticks` → `Event::PriceTick` | verificado |
| Payout cotizado | `get_payout` | verificado (por importe y duración) |
| Compra rise/fall | `buy_binary` | verificado en demo |
| Venta anticipada | `sell_option` | verificado en demo |
| Cierre por evento | `events` → `Event::OrderClosed` | verificado |
| Historial | `get_position_history` | verificado (`profit_table`) |
| Balance y sesión | `get_balance`, `session` | verificado |
| Reconexión con OTP nuevo | automática | verificado (1 intento) |
| Reconciliación de cierres | automática | verificado (cierre durante 72 s sin red) |

No soportado y con rechazo explícito: `buy_blitz`, `buy_digital_spot`,
indicadores técnicos, perfil de activo, live deals y comisiones.

## Densidad de ticks (medida)

Gateway público, ventana de 8 s, 9 de noviembre de 2026:

| Activo | Clase | Ticks | Densidad | Hueco medio |
|---|---|---|---|---|
| `R_100` | índice sintético | 4 | ~0,5/s | 1 993 ms |
| `1HZ100V` | índice sintético 1 s | 8 | 1,0/s | 998 ms |
| `1HZ10V` | índice sintético 1 s | 8 | 1,0/s | 1 002 ms |
| `frxAUDCAD` | forex | 8 | 1,0/s | 1 002 ms |
| `cryBTCUSD` | cripto | 8 | 1,0/s | 998 ms |
| `frxXAGUSD` | materia prima | 6–8 | 0,75–1,0/s | 1 000–1 183 ms |

La cadencia es del producto (el índice de 2 s entrega uno cada 2 s) y el
gateway público agrega forex y cripto a ~1 tick/s. **No es polling**: se
entrega el stream del bróker tal cual. Con credenciales, la densidad puede
variar; se documentará cuando se mida.

## Límites del bróker

| Grupo | Por minuto | Por hora |
|---|---|---|
| `proposal`, `proposal_open_contract`, `buy`, `sell` | 360 | 14 400 |
| `balance`, `statement` | 100 | 2 000 |
| `portfolio`, `profit_table` | 30 | 1 500 |
| Resto (`contracts_for`, velas, símbolos) | 220 | 14 400 |

La API aplica el **80 %** de cada cupo y espera en vez de fallar. Un `RateLimit`
del bróker nunca provoca el reenvío de una orden.

## Reconexión y reconciliación

- Al caer el socket se emite `Event::Disconnected`; la reconexión pide un OTP
  nuevo, restaura sesión y balance, y **reproduce las suscripciones**.
- Medición: 1 intento y 2 396 ms sin conexión en una caída forzada.
- Si un contrato cierra sin conexión, al volver se consulta su estado y, si ya
  no está activo, el historial paginado; el cierre se publica **una sola vez**
  (`Event::OrderClosed`) con el importe neto. Verificado con 72 s de caída.
- Las órdenes nunca se reenvían: sin socket, `buy_binary` devuelve
  `NotConnected`.

## Cuentas e identificadores

Deriv identifica cuentas con cadenas (`DOT…`, `VRTC…` demo; `CR…` real). Hasta
que el contrato admita cadenas, `Session.balance_id` y `AccountBalance.id`
exponen un **sustituto determinista** documentado, y `Asset.opcode` es `0`
porque Deriv no publica identificador numérico. Usa `symbol`/`account_id`.

## Fachada sincrónica

`SyncBroker` sigue siendo exclusiva de IQ Option: existe para consumidores
heredados. Deriv es async, como la API oficial; no hay envoltorio bloqueante.

## Verificación

```powershell
.\scripts\verificar_deriv.ps1              # incluye órdenes y caídas en demo
.\scripts\verificar_deriv.ps1 -SoloLectura # solo mercado y ticks
```

Las pruebas viven en `examples/deriv_*.rs` y leen `.env` sin imprimir claves.
