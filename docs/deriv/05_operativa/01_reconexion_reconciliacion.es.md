# Reconexión y reconciliación

## Qué ocurre cuando cae el socket

1. Se emite `Event::Disconnected` con el motivo observado.
2. La sesión pide un **OTP nuevo** por REST (el anterior es de un solo uso) y
   vuelve a autenticarse.
3. Restaura la sesión y el balance, y **reproduce las suscripciones** de velas y
   ticks.
4. Se emite `Event::Reconnected { intentos, downtime_ms }`.

Medición en una caída forzada: **1 intento y 2 396 ms** sin conexión, con vela
recibida después de volver.

## Peticiones durante la caída

Mientras no hay socket, `buy_binary`, `sell_option` y las suscripciones devuelven
`BrokerError::NotConnected`. No se encola nada: diferir una orden cambia el
mercado y el resultado. Las órdenes en vuelo se descartan con error y **no se
reenvían**.

## Reconciliación de cierres

Una orden se abre y queda pendiente. Si su cierre ocurre durante la caída, el
evento nunca llega; al reconectar la sesión consulta el estado del contrato y,
si ya no está activo, el historial paginado:

- Solo se completan identificadores que esta sesión abrió y siguen pendientes.
- El cierre se publica como `Event::OrderClosed`, igual que un cierre normal.
- `Win` lleva la ganancia neta, `Loss` el importe perdido y `Equal` el importe
  devuelto.

Verificado con **72 s sin red**: un solo `OrderClosed` `Loss(1.0)` y delta de
balance −1,00, sin reenviar la orden.

## Límites

La reconciliación recorre páginas del historial y se detiene al encontrar los
cierres pendientes. Un cierre más antiguo que la ventana consultada exige leer
`get_position_history` explícitamente. Si el historial no responde, la
reconexión continúa y las órdenes afectadas quedan como estaban.
