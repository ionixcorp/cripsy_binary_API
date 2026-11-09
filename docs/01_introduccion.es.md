# Qué es cripsy_binary_API

`cripsy_binary_API` es una API moderna, en Rust, para operar contra brókers de
opciones binarias. Diseñada desde cero, con estas propiedades:

- **Núcleo asíncrono**: el adaptador es `async`/`await` nativo.
- **Fachada sincrónica**: `broker::sync::SyncBroker` permite consumir la misma
  API con llamadas bloqueantes, sin tokio ni `await` (solo IQ Option).
- **Sin estado global estático y sin polling**: cada petición crea un canal y la
  respuesta se correlaciona por `request_id`; los resultados de las órdenes
  llegan por eventos.
- **Multibróker**: una interfaz común (`Broker`) con adaptadores por bróker;
  IQ Option y Deriv integrados, con las capacidades exclusivas de IQ Option en
  `IqOptionExtras`.
- **Streams nativos**: IQ Option mide ~3 ticks/s por activo en OTC y admite
  75 activos por sesión sin rechazos; Deriv entrega la cadencia del producto
  (0,5–1 tick/s en la medición pública). Sin polling en ninguno de los dos.
- **Tipado de extremo a extremo**: nada de diccionarios crudos en la frontera
  pública.

## Estado actual

### IQ Option

| Componente | Estado |
|---|---|
| Autenticación y sesión (login + WSS + perfil) | Listo, verificado contra el bróker |
| Catálogo de activos (binary, turbo y blitz) | Listo, verificado contra el bróker |
| Velas históricas (hasta 1s × 7 días) | Listo, verificado contra el bróker |
| Stream de velas multi-tamaño y ticks | Listo, verificado contra el bróker |
| Órdenes clásicas (turbo/binary) | Listo, verificado en cuenta demo |
| Órdenes digitales (protocolo v2) | Listo, verificado en cuenta demo |
| Órdenes blitz (protocolo v2) | Listo, verificado en cuenta demo (apertura + cierre por evento) |
| Venta anticipada (buyback) | Lista, verificada en cuenta demo |
| Perfil fundamental del activo | Listo, verificado contra el bróker |
| Historial de posiciones | Listo, verificado (PnL realizado) |
| Live-deals y comisiones | Suscripción lista; el volumen de eventos depende del bróker |
| Resultados por evento y caché | Listo, verificado en cuenta demo |
| Cotización por expiración (`quote_payout`) | Digital verificada; binaria, turbo y blitz devuelven no disponible |
| Fachada sincrónica (`SyncBroker`) | Lista, verificada contra el bróker |
| Indicadores técnicos del bróker | Canal implementado; el bróker rechaza hoy todos los activos |

### Deriv

| Componente | Estado |
|---|---|
| Modo público (sin credenciales) | Listo, verificado: catálogo, velas, payout y ticks |
| Cotización por expiración (`quote_payout`) | Rise/fall verificada; digital devuelve no disponible |
| Sesión autenticada (OTP + App ID) | Lista, verificada en cuenta demo |
| Compra y venta rise/fall | Listas, verificadas en cuenta demo |
| Cierre por evento e historial | Listos, verificados (`profit_table` paginado) |
| Reconexión con OTP nuevo | Lista, verificada (2 396 ms, replay de suscripciones) |
| Reconciliación de cierres | Lista, verificada con 72 s sin red y un solo evento |
| Blitz, digital, indicadores, live-deals | No soportado; rechazo explícito |

## Qué no hace

- **No calcula indicadores**: entrega los que el bróker publique, o nada si el
  bróker no los soporta. Puedes calcular los tuyos con el histórico y el stream.
- **No es una interfaz gráfica ni un bot**: es una librería para que tú
  construyas encima.
- **No gestiona tu dinero**: envía órdenes al bróker cuando se lo pides.

## Cómo leer esta documentación

Todas las llamadas del trait `Broker` son `async` y devuelven
`Result<_, BrokerError>` (salvo `name()` e `is_connected()`).

| Grupo | Documento | Contenido |
|---|---|---|
| General | `docs/01_introduccion.es.md` | Alcance, estado y qué no hace |
| IQ Option | `docs/iqoption/01_cuenta/01_sesion_cuentas.es.md` | Conexión, cuentas demo/real y balances |
| IQ Option | `docs/iqoption/02_mercado/01_historicos.es.md` | Históricos, límites, velas en vivo, ticks y fachada sincrónica |
| IQ Option | `docs/iqoption/03_ordenes/01_binary.es.md` | Clásicas, digitales v2 y blitz |
| IQ Option | `docs/iqoption/04_operativa/02_reconexion.es.md` | Indicadores del bróker y reconexión |
| Deriv | `docs/deriv/01_introduccion.es.md` | Alcance, modos público/autenticado y estado |
| Deriv | `docs/deriv/02_cuenta/01_cuentas_identificadores.es.md` | PAT, OTP, cuentas e identificadores |
| Deriv | `docs/deriv/03_mercado/01_catalogo_velas_ticks.es.md` | Catálogo, velas, ticks, payout y densidad |
| Deriv | `docs/deriv/04_ordenes/01_rise_fall_historial.es.md` | Rise/fall, cierre por evento e historial |
| Deriv | `docs/deriv/05_operativa/01_reconexion_reconciliacion.es.md` | Reconexión y reconciliación de cierres |
| Deriv | `docs/deriv/06_limites/01_limites.es.md` | Cupos y capacidades no soportadas |
| Python | `docs/python/01_python.es.md` | PyO3/maturin: instalación, superficie y límites |
| General | `docs/herramientas/01_herramientas.es.md` | Instalación, scripts y cómo añadir otro bróker |

## Requisitos

- Rust 1.86 o superior (`rustup`).
- Credenciales del bróker (cuenta demo recomendada; en Deriv el modo público
  no necesita ninguna).
- Windows, Linux o macOS.

## Limitaciones conocidas

- **Reconexión automática (0.2.0, ampliada a Deriv en 0.3.0)**: si el WebSocket cae, la sesión
  reautentica, restaura perfil, catálogo y suscripciones, y lo informa con
  `Event::Disconnected` y `Event::Reconnected { intentos, downtime_ms }`.
  Mientras no hay socket, las llamadas devuelven `BrokerError::NotConnected`.
- **Órdenes en vuelo**: se descartan con error y no se reenvían al reconectar;
  reenviarlas significaría operar con un mercado distinto al decidido.
- **Huecos de stream**: la reproducción restablece las suscripciones, pero los
  datos ocurridos durante la caída se recuperan con el histórico, no por el
  stream.
- **Resultado de operación desde caché**: `get_operation_result` lee la caché
  alimentada por eventos; no consulta al bróker. Sin el stream activo, una orden
  puede quedarse en `Pending`.
- **Payout del catálogo y cotización**: `get_payout` devuelve el payout
  publicado para el activo; el parámetro de expiración no cambia la consulta.
  Para una combinación concreta, `quote_payout` consulta una cotización por
  expiración y tipo. Si el bróker no la ofrece, devuelve `available: false` con
  `profit_percent: None`; no se inventa un porcentaje. La referencia actual es
  `CALL` y no se asume simetría con `PUT`. Los cambios en vivo de payout llegan
  por `Event::PayoutChanged`.
- **Suscripciones sin baja explícita**: al soltar el `CandleStream` la
  suscripción se retira en el siguiente despacho de velas.
- **Pérdida de eventos**: si el consumidor se atrasa más que el buffer (1024),
  el stream entrega `Event::EventsLost { count }` y continúa.
- **Indicadores técnicos**: dependen de lo que el bróker publique para cada
  activo; hoy los rechaza para los activos probados.
- **Deriv sin fachada sincrónica**: `SyncBroker` es exclusivo de IQ Option.
- **Deriv solo rise/fall**: blitz, digital, indicadores y live-deals se rechazan
  de forma explícita en lugar de simularse.
- **Identificadores de Deriv**: las cuentas son cadenas; `balance_id` y
  `Asset.opcode` usan un sustituto determinista documentado.
