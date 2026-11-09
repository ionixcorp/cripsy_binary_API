# Órdenes rise/fall e historial

## Compra

```rust
use cripsy_binary_api::domain::{BuyBinaryRequest, Direction, OptionType};

let id = broker.buy_binary(BuyBinaryRequest {
    symbol: "R_100".into(),
    amount: 1.0,
    direction: Direction::Call,
    expiration_minutes: 1,
    option_type: OptionType::Binary,
}).await?;
```

`expiration_minutes` se expresa en minutos. El adaptador valida que el activo
esté abierto y que el importe respete el mínimo publicado; el máximo lo aplica
el bróker.

## Cierre por evento

El resultado llega por el stream de eventos, no por polling:

```rust
let mut eventos = broker.events().await?;
while let Some(ev) = eventos.next().await {
    if let Event::OrderClosed { id, result } = ev {
        // result: Win(neto) | Loss(monto) | Equal(monto) | Pending
    }
}
```

`get_operation_result(&id)` lee la caché alimentada por esos eventos. Sin stream
activo una orden puede permanecer en `Pending`.

## Venta anticipada

```rust
broker.sell_option(&id).await?;
```

El bróker la admite dentro de su ventana de venta; si ya no es vendible responde
con su mensaje de rechazo.

## Historial

`get_position_history(symbol, limit, offset)` consulta `profit_table` paginado.
`limit` es el tamaño de página y `offset` el desplazamiento. El PnL se deriva de
los importes que publica el bróker y se conserva la diferencia entre observado y
derivado.

## Reconciliación

Si el contrato cierra sin conexión, al volver se consulta su estado y, si ya no
está activo, el historial paginado. El cierre se publica **una sola vez** como
`Event::OrderClosed`. Ver `docs/deriv/05_operativa/01_reconexion_reconciliacion.es.md`.

## Fuera de alcance

`buy_blitz`, `buy_digital_spot`, indicadores técnicos, perfil de activo, live
deals y comisiones no están soportados por Deriv y se rechazan de forma
explícita; no se simulan sobre rise/fall.
