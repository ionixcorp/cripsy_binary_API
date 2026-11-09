# Opciones digitales

Las digitales usan el **protocolo v2**: la compra se arma con el instrumento
que publica el stream de precios del bróker.

## Flujo

1. Suscribirse a `price-splitter.client-price-generated` (el adaptador lo hace
   solo al comprar).
2. Esperar el mensaje del periodo deseado y tomar la entrada **spot**
   (`strike == "SPT"`): de ahí salen `call.symbol` o `put.symbol` (el
   `instrument_id`) y el `instrument_index`.
3. Enviar `digital-options.place-digital-option` **v2.0** con:

```json
{
  "user_balance_id": <id_de_tu_perfil>,
  "instrument_id": "do1380A20261027D002400T1MCSPT",
  "asset_id": 1380,
  "instrument_index": 4046465,
  "amount": "1.0"
}
```

Detalles que el bróker exige (verificados en vivo):

- `amount` viaja como **texto** ("1.0"); como número responde
  `INVALID_SCHEMA`.
- El `instrument_index` debe salir del **mismo** mensaje de precio que el
  instrumento: los índices rotan.
- Si el instrumento elegido no es el spot, el bróker responde
  `bad quote` (status 4008).

## Llamada

```rust
let orden = broker.buy_digital_spot(BuyDigitalRequest {
    symbol: "USDZAR-OTC".into(),
    amount: 1.0,
    direction: Direction::Call,
    duration_minutes: 1,
    strike: 0.0,   // lo elige el price-splitter
}).await?;
```

## Resultado

Las digitales cierran por el canal de cartera `position-changed`, no por
`option-closed` como el resto. El adaptador correlaciona la orden por
`instrument_id` y emite `Event::OrderClosed` igual que las demás, así que el
consumidor no ve la diferencia:

```rust
if let Event::OrderClosed { id, result } = /* ... */ {
    // result: Win(neto) | Loss(monto) | Equal(monto) | Pending
}
```

El resultado también queda en el cache: `get_operation_result(&id)` devuelve
el mismo valor sin consultar al bróker (sin polling).

## Semántica de montos

- `close_profit` / `win_amount` son el **crédito total** (inversión +
  ganancia), no la ganancia.
- El adaptador ya devuelve la **ganancia neta** (crédito − inversión).
- En `socket-option-closed` el campo `amount` viene en micro-unidades
  (1 000 000 = 1 USD); `sum` es la inversión legible.
