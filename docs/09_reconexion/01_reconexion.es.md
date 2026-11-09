# Reconexión de sesión

Desde la versión 0.2.0 la sesión no se queda caída cuando el WebSocket se
interrumpe: una tarea supervisora la restablece y el consumidor recibe el aviso
por el mismo stream de eventos que ya usa.

## Qué ocurre cuando cae la conexión

1. El bucle de despacho termina y se descartan las peticiones en vuelo
   (`fail_all`), de modo que ninguna espera queda colgada hasta su timeout.
2. Se emite `Event::Disconnected { motivo }` con el motivo observado: error de
   lectura, error de escritura o cierre por parte del bróker.
3. La sesión vuelve a autenticarse con las credenciales de la configuración y
   abre un WebSocket nuevo con reintentos y backoff exponencial
   (`backoff_base_ms` … `backoff_max_ms`).
4. Con el socket nuevo se rehace el handshake: `ssid` → perfil (sesión y
   balances) y datos de inicialización (catálogo de activos).
5. Se reproduce el registro de suscripciones completo.
6. Se emite `Event::Reconnected { intentos, downtime_ms }` y el despacho
   continúa.

Los streams de velas entregados antes de la caída **siguen siendo válidos**: no
se cierran y vuelven a recibir datos cuando la suscripción se reproduce.

## Peticiones durante la caída

Mientras no hay socket, `solicitar` y las suscripciones devuelven
`BrokerError::NotConnected` de inmediato. No se encola nada a propósito: una
orden guardada para más tarde se ejecutaría contra un mercado distinto del que
motivó la decisión, que en opciones binarias con expiración corta cambia el
resultado por completo.

La cola de salida está acotada (`TransportConfig::cola_salida`, 256 por
defecto). Si el productor supera esa capacidad sin que el socket consuma, la
llamada espera en lugar de crecer en memoria sin límite.

## Credenciales rechazadas

Un fallo de autenticación es terminal: la reconexión se abandona y la sesión
queda cerrada. Reintentar el login en bucle es la forma más rápida de que el
bróker bloquee la cuenta, así que el supervisor no lo hace. Los fallos de red o
de handshake sí se reintentan.

## Personalización

```rust
use cripsy_binary_api::broker::config::{IqOptionConfig, TransportConfig};

let cfg = IqOptionConfig {
    transport: TransportConfig {
        backoff_base_ms: 250,   // primera espera
        backoff_max_ms: 8_000,  // techo
        cola_salida: 256,       // capacidad de la cola de salida
        ..TransportConfig::default()
    },
    ..IqOptionConfig::default()
};
```

## Reconciliación de cierres

Una orden se abre y se registra como pendiente. Si su cierre ocurre durante la
caída, el evento nunca llega; al reconectar la sesión consulta el historial de
posiciones del balance activo y recupera esos cierres:

- Solo se completan identificadores que esta sesión abrió y siguen pendientes.
  Un cierre ajeno no aparece en la caché del consumidor.
- El resultado se publica como `Event::OrderClosed`, igual que un cierre
  normal, así que el consumidor no necesita una ruta distinta.
- `Win` lleva la ganancia neta, `Loss` el importe perdido y `Equal` el importe
  devuelto, calculados desde el PnL de la posición.

La consulta es paginada (100 posiciones por página, hasta 10 páginas) y se
detiene en cuanto encuentra todos los cierres pendientes: una sesión caída
durante horas no bloquea el despacho recorriendo el historial completo.

Si el historial no responde, la reconexión continúa y se registra el aviso en
`tracing`; las órdenes afectadas quedan como estaban.

## Qué no resuelve

- **Órdenes en vuelo**: se descartan con error y no se reenvían. Una orden
  aceptada por el bróker sí se reconcilia al volver.
- **Huecos de datos**: las velas y ticks ocurridos durante la caída no llegan
  por el stream; se recuperan con `get_candles` o la descarga paginada.
- **Cierres fuera de la ventana del historial**: la reconciliación recorre
  hasta 10 páginas de 100 posiciones y se detiene al encontrar lo que busca o
  al recibir una página incompleta. Un cierre más antiguo exige consultar el
  historial con `get_position_history`.

## Observabilidad

Los avisos de reconexión se registran con `tracing` (`warn` al perder el
socket, `info` al restablecerlo, `error` si se abandona). El consumidor que
quiera métricas propias puede contar `Disconnected` frente a `Reconnected` y
usar `downtime_ms` para medir la indisponibilidad real.

## Estado verificado

La suite viva fuerza la caída del socket y comprueba el ciclo completo:
`Disconnected`, reconexión, operativa posterior y replay de una suscripción.
Última ejecución: 1 intento de reconexión, 5.710 ms sin conexión y vela
recibida tras volver; 34 comprobaciones, 0 fallos.
