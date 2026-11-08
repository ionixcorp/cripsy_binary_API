# Qué es cripsy_binary_API

`cripsy_binary_API` es una API moderna, en Rust, para operar contra brókers de
opciones binarias. Diseñada desde cero, con estas propiedades:

- **Núcleo asíncrono**: el adaptador es `async`/`await` nativo.
- **Fachada sincrónica**: `broker::sync::SyncBroker` permite consumir la misma
  API con llamadas bloqueantes, sin tokio ni `await`.
- **Sin estado global estático y sin polling**: cada petición crea un canal y la
  respuesta se correlaciona por `request_id`; los resultados de las órdenes
  llegan por eventos.
- **Multibróker**: una interfaz común (`Broker`) con adaptadores por bróker;
  el primero es IQ Option.
- **Streams nativos de alta densidad**: ~3 ticks de precio por segundo por
  activo en la medición documentada; hasta 75 activos por sesión sin rechazos.
- **Tipado de extremo a extremo**: nada de diccionarios crudos en la frontera
  pública.

## Estado actual

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

## Requisitos

- Rust 1.86 o superior (`rustup`).
- Credenciales del bróker (se recomienda cuenta demo para empezar).
- Windows, Linux o macOS.

## Limitaciones conocidas

- **Sin reconexión automática**: si el WebSocket cae, la sesión queda
  desconectada (`is_connected()` devuelve `false`), las peticiones en vuelo
  terminan con error y el consumidor debe reconectar y volver a suscribirse.
  El handshake inicial sí reintenta con backoff.
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
