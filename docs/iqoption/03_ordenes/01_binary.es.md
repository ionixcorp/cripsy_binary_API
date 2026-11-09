# Opciones clásicas (turbo / binary)

Las clásicas usan el mensaje `binary-options.open-option` v1.0. La
clasificación la hace la duración, igual que la plataforma:

- **≤ 5 minutos → turbo** (`option_type_id` 3).
- **> 5 minutos → binary** (`option_type_id` 1).

La expiración se calcula con la tabla de minutos/cuartos de hora del bróker:
la API reproduce la lógica verificada (candidatos de minuto para turbo, de
cuarto de hora con 5 minutos de margen para binary) y elige el más cercano a
la duración pedida.

## Llamada

```rust
let orden = broker.buy_binary(BuyBinaryRequest {
    symbol: "USDZAR-OTC".into(),
    amount: 1.0,
    direction: Direction::Put,
    expiration_minutes: 1,
    option_type: OptionType::Binary,
}).await?;
```

El bróker confirma con `buyComplete`; la orden queda identificada por
`OrderId`.

## Ciclo de vida por eventos

El resultado llega por eventos, no por polling:

1. `option-opened` — apertura tipada (`option_type`, `option_type_id`, montos).
2. `option-closed` — resultado final con `result` (win/loose/equal) y montos.
3. `socket-option-closed` — evento equivalente con el detalle micro-unitario.

```rust
let mut eventos = broker.events().await?;
while let Some(ev) = eventos.next().await {
    if let Event::OrderClosed { id, result } = ev {
        println!("orden {id}: {result:?}");
    }
}
```

`get_operation_result(&id)` consulta el cache alimentado por esos eventos
(siempre disponible, sin latencia de red).

## Validaciones antes de enviar

- El activo debe estar en estado `Open`; si no, error `MarketClosed`.
- Se valida el monto mínimo del catálogo (`min_amount`) cuando el bróker lo
  publica; el máximo lo aplica el bróker (responde con su mensaje de rechazo).
- En blitz se valida además que la duración esté entre las admitidas por el
  grupo blitz (`expiration_times` ∪ `rollovers`).
