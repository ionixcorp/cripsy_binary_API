# Historial de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/)
y versionado semántico.

## [0.2.0] - 2026-11-09

### Añadido

- Reconexión automática de la sesión: tras una caída del WebSocket la tarea
  del socket reautentica, restaura perfil, balances y catálogo, y reproduce
  las suscripciones antes de volver a despachar. Backoff exponencial con
  saturación; un rechazo de credenciales no se reintenta.
- `Event::Disconnected { motivo }` y `Event::Reconnected { intentos,
  downtime_ms }` para que el consumidor distinga una caída de un cierre.
- Registro de suscripciones (velas, live-deals, comisiones, price-splitter y
  cierres de posición) que se usa como fuente del replay.
- `BrokerError::NotConnected`: las llamadas fallan de inmediato mientras no
  haya socket, en vez de encolarse para ejecutarse con datos obsoletos.
- `TransportConfig::cola_salida` (256 por defecto): capacidad de la cola de
  salida del socket.
- `docs/09_reconexion/01_reconexion.es.md` con el diseño y los límites del
  supervisor de sesión.
- `Broker::buy_blitz` con `BuyBlitzRequest`: la duración viaja en segundos y el
  payout se puede indicar de forma explícita. `buy_binary` deja de transportar
  segundos en `expiration_minutes` y delega el blitz en la nueva petición.
- `AccountType::Unknown` para los tipos de balance que el adaptador no reconoce.
- `Event::EventsLost { count }`: informa de la pérdida de continuidad del bus de
  eventos sin cerrar el stream.
- Reintentos con backoff en el handshake del WebSocket.
- Documento de limitaciones conocidas en `docs/01_introduccion.es.md` y en el
  README.

### Corregido

- `switch_account` propaga el error del refresco de balances y comprueba que el
  balance solicitado quede activo antes de actualizar la sesión; ya no devuelve
  `Ok(())` cuando el bróker no confirmó el cambio.
- Un tipo de balance desconocido se mapea a `AccountType::Unknown` en lugar de
  interpretarse como cuenta real.
- `get_account_type` devuelve error si la sesión no está inicializada, en vez de
  asumir cuenta de práctica.
- El router retira las solicitudes que expiran y no registra receptores para los
  mensajes que no esperan respuesta (suscripciones).
- Al cerrarse el socket se descartan las solicitudes pendientes, de modo que
  ninguna espera queda colgada hasta su timeout.
- Las suscripciones a velas cuyos streams fueron soltados se retiran en el
  siguiente despacho de velas.
- Documentación corregida: `get_operation_result` lee la caché local (no hace
  consulta remota), `get_payout` devuelve el payout del catálogo y la densidad
  de ticks se describe con la medición real, sin afirmar ausencia de límite.

### Cambiado (incompatible)

- `Event` pasa a ser `#[non_exhaustive]` e incorpora variantes nuevas: los
  `match` exhaustivos sobre `Event` deben añadir un brazo `_`.
- La cola de salida del socket deja de ser ilimitada; una ráfaga superior a
  `TransportConfig::cola_salida` aplica presión al productor.
- Migración: sustituir los `match` exhaustivos sobre `Event` y, si se construye
  `TransportConfig` con literal de struct, añadir `cola_salida`.

## [0.1.0] - 2026-11-08

### Añadido

- Cliente Rust para la API de IQ Option con núcleo `async` y fachada `blocking`.
- Trait `Broker` multibróker y dominio tipado.
- Sesión, perfil, balances y cambio de cuenta.
- Catálogo de activos con metadatos operativos.
- Velas históricas, descarga paginada, stream multi-tamaño y ticks nativos.
- Órdenes clásicas (turbo/binary), digitales v2, blitz v2 y venta anticipada.
- Resultados por evento con caché, historial de posiciones, perfil del activo,
  comisiones y live-deals.
- Ejemplos ejecutables, scripts de verificación y documentación navegable.
