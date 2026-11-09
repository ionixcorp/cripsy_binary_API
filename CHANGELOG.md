# Historial de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/)
y versionado semántico.

## [0.3.3] - 2026-11-09

### Cambiado

- Release de alineación de crates.io: el mismo código de `0.3.2` se publica
  desde un commit limpio, con la documentación revisada y la metadata de
  procedencia corregida. No hay cambios de API ni de comportamiento.
- El workflow de wheels solo construye y adjunta artefactos cuando el tag `v*`
  coincide con la versión de `python/Cargo.toml`; los tags de solo crate se
  omiten.

### Verificación

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  (89 passed), `cargo test --features blocking` (89 passed) y build `blocking`.
- Documentación navegable: 18 páginas.
- `cargo package` verificado sin `.env`, `target`, `python/` ni artefactos.
- `cargo publish` desde el commit limpio; el tag `v0.3.3` no dispara wheels de
  Python.

## [0.3.2] - 2026-11-09

### Añadido

- `PayoutQuote` como cotización explícita por expiración y tipo de opción.
  Distingue una cotización real de la ausencia de datos con `available: false`
  y `profit_percent: None`; no fabrica un `0.0`.
- `Broker::quote_payout` con implementación por defecto que devuelve
  `PayoutQuote::unavailable`; los adaptadores que pueden cotizar lo sobrescriben.
- Deriv: cotización real por `proposal` para `OptionType::Binary` (traducida a
  `CALL`); digital y blitz devuelven no disponible y los rechazos del bróker se
  propagan.
- IQ Option: cotización digital por `price-splitter.client-price-generated`,
  con espera acotada de 15 s; binaria, turbo y blitz devuelven no disponible.

### Documentado

- La cotización de IQ usa el lado `CALL` del strike spot y `PayoutQuote` no
  transporta dirección: no se asume simetría con `PUT`.
- Notas de release en `docs/releases/0.3.2.md` y base del plan 0.4.0 actualizada
  a 0.3.2.

### Verificación

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  (89 passed) y `cargo build --no-default-features --features blocking`.
- Prueba viva IQ: 25 OK, 0 fallos y 1 aviso documentado; Digital
  `available=true` y Binary `available=false`.
- Prueba viva Deriv: payout rise/fall 60 s y 300 s `available=true`; Digital
  `available=false`; ticks y conexión autenticada OK.
- `cargo package --allow-dirty` y `cargo publish --dry-run --allow-dirty`
  completados; el paquete no incluye `.env`, `target`, `python/` ni artefactos.

## [0.3.1] - 2026-11-09

### Añadido

- `Broker::iqoption_extras` como acceso opcional y compatible a las capacidades
  exclusivas de IQ Option desde un `Box<dyn Broker>`; los adaptadores que no las
  ofrecen devuelven `None`.
- Binding Python: `buy_digital`, `buy_blitz` e `indicators`, con rechazo
  explícito en Deriv en lugar de simular esas capacidades.
- Wheels para Linux x64/arm64, Windows x64/arm64 y macOS Intel/arm64, además del
  sdist.
- Publicación en PyPI del paquete Python 0.3.1.

### Cambiado

- Documentación separada por bróker: `docs/iqoption/` y `docs/deriv/`, con
  subcarpetas de cuenta, mercado, órdenes y operativa; Python y herramientas
  tienen su propia carpeta.
- Scripts globales en `scripts/` y verificadores/cargadores por bróker en
  `scripts/iqoption/` y `scripts/deriv/`.
- Assets fuera de `docs/`: el banner y el QR viven en `img/`.
- El generador de documentación agrupa por bróker y ordena la navegación.

### Corregido

- La documentación de Deriv ya no afirma que el payout se consulte por importe:
  `get_payout` usa activo y duración.
- Enlaces y rutas de scripts actualizados tras la reorganización.

## [0.3.0] - 2026-11-09

### Añadido

- Adaptador **Deriv** (API moderna): conexión OTP por REST, gateway público sin
  credenciales, catálogo, velas, cotización de payout, compra y venta rise/fall,
  cierre por `proposal_open_contract`, historial paginado y reconexión con OTP
  nuevo más replay de suscripciones.
- `Broker::subscribe_ticks` con entrega por `Event::PriceTick`; Deriv implementa
  el stream nativo y el resto de adaptadores hereda un rechazo explícito.
- `DerivConfig` con `Debug` redactado y sin serializar el token; límites de tasa
  por clase al 80 % de lo documentado por el bróker.
- `BrokerError` conserva su taxonomía; las capacidades no soportadas responden
  con el mensaje del bróker o con el rechazo por defecto.

### Cambiado (incompatible)

- Las capacidades exclusivas de IQ Option (`buy_blitz`, `buy_digital_spot`,
  indicadores técnicos, perfil de activo, live deals y comisiones) salen de
  `Broker` y pasan a `IqOptionExtras`. En `IqOptionBroker` siguen disponibles
  como métodos inherentes, por lo que el código que usa el tipo concreto no
  cambia; quien las invocaba sobre `Box<dyn Broker>` debe usar
  `Box<dyn IqOptionExtras>` o el tipo concreto.
- Migración: `use cripsy_binary_api::broker::IqOptionExtras;` solo si se consume
  el trait de forma genérica.

### Documentado

- Alcance del producto: operación rise/fall, datos para cualquier activo y
  densidad de ticks nativa como diferencial; expansión futura fuera de alcance.
- `docs/deriv/01_introduccion.es.md`: uso de Deriv, credenciales, límites y
  diferencias con IQ Option.
- Tabla de densidad medida por clase de activo y separación de los scripts de
  verificación (`verificar_iqoption.ps1`, `verificar_deriv.ps1`,
  `verificar_todo.ps1`).
- Reconciliación de Deriv verificada en vivo: un cierre ocurrido durante 72 s
  sin conexión se publica una sola vez, sin reenviar la orden.


## [0.2.1] - 2026-11-09

### Añadido

- Prueba del recorrido multi-página de la reconciliación con historiales
  simulados: encuentra cierres en páginas posteriores, corta al localizarlos y
  respeta el límite de diez páginas.

### Corregido

- Cifras de verificación de la documentación embebida: la medición final de la
  caída forzada es 4.938 ms sin conexión, con 34 comprobaciones y 0 fallos.

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
- Reconciliación al reconectar: las órdenes abiertas cuyo cierre ocurrió sin
  conexión se recuperan del historial de posiciones (paginado, hasta 10 páginas
  de 100) y se publican como `Event::OrderClosed`, sin reenviar nada al bróker.
- `IqOptionBroker::simular_caida_de_conexion` (marcado `#[doc(hidden)]`) para
  ejercitar la recuperación en pruebas de resiliencia.
- La verificación viva incluye una caída forzada del socket con comprobación de
  `Disconnected`, `Reconnected`, operativa posterior y replay de suscripciones.
- `docs/iqoption/04_operativa/02_reconexion.es.md` con el diseño y los límites del
  supervisor de sesión, más la sección de continuidad del README.
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
