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
| Fachada sincrónica (`SyncBroker`) | Lista, verificada contra el bróker |
| Indicadores técnicos del bróker | Canal implementado; el bróker rechaza hoy todos los activos |

### Deriv

| Componente | Estado |
|---|---|
| Modo público (sin credenciales) | Listo, verificado: catálogo, velas, payout y ticks |
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

| Sección | Contenido |
|---|---|
| 02 · Account | Conexión, cuentas (demo/real), balances |
| 03 · Candle & Stream | Históricos, límites, velas en vivo, ticks y fachada sincrónica |
| 04 · Digital | Compra digital v2 y ciclo de vida |
| 05 · Binary | Órdenes clásicas turbo/binary |
| 06 · Blitz | Órdenes blitz y rollovers |
| 07 · Herramientas | Instalación, verificación y cómo añadir otro bróker |
| 08 · Indicadores | Indicadores del bróker: estado y alternativas |
| 09 · Reconexión | Supervisión de sesión, replay de suscripciones y límites |
| 10 · Deriv | Adaptador Deriv: alcance, credenciales, densidad y límites |
| 11 · Python | Binding PyO3/maturin: instalación, superficie y límites |

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
- **Payout del catálogo**: `get_payout` devuelve el payout publicado para el
  activo; el parámetro de expiración no cambia la consulta. Los cambios en vivo
  llegan por `Event::PayoutChanged`.
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
