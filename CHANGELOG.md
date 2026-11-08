# Historial de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/)
y versionado semántico.

## [Sin publicar]

### Añadido

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
