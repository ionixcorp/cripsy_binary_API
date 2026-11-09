# Mercado: catálogo, velas y ticks

## Capacidades

| Operación | Método | Estado |
|---|---|---|
| Catálogo de activos | `list_assets` | verificado |
| Estado de mercado | `asset_status` | verificado |
| Velas históricas | `get_candles` | verificado (granularidades 1 m – 1 d) |
| Velas en vivo | `subscribe_candles` | implementado |
| Ticks nativos | `subscribe_ticks` → `Event::PriceTick` | verificado |
| Payout del catálogo | `get_payout` | verificado |
| Cotización por expiración | `quote_payout` | verificado (rise/fall; digital no disponible) |

El histórico de Deriv no ofrece velas de 1 segundo en esta integración: la
granularidad mínima es **1 minuto** y la máxima **1 día**. El límite de velas por
petición lo aplica el bróker.

## Velas

```rust
let velas = broker.get_candles(CandleRequest {
    symbol: "R_100".into(),
    interval: 60,
    count: 100,
    end_time: None,
}).await?;
```

`interval` va en segundos y `end_time` es el inicio de una vela cerrada. La
suscripción en vivo usa `CandleSubscribe` y entrega solo el tamaño pedido.

## Ticks nativos

```rust
broker.subscribe_ticks("R_100").await?;
let mut eventos = broker.events().await?;
while let Some(ev) = eventos.next().await {
    if let Event::PriceTick { symbol, tick } = ev {
        // tick.price, tick.bid, tick.ask, tick.at_ms
    }
}
```

No hay polling: se entrega el stream del bróker tal cual.

## Payout

`get_payout(symbol, seconds)` devuelve el payout publicado en el catálogo para
el activo. El parámetro de expiración no altera la consulta; los cambios en vivo
llegan por `Event::PayoutChanged`.

```rust
let payout = broker.get_payout("R_100", 60).await?;
println!("{} %", payout.profit_percent);
```

### Cotización por expiración

`quote_payout(symbol, OptionType::Binary, seconds, amount)` consulta una
propuesta real de Deriv (`proposal`) para rise/fall y la traduce a `CALL`.
`amount` debe ser positivo e influye en la propuesta; no se usa el payout del
catálogo como sustituto.

```rust
use cripsy_binary_api::domain::OptionType;

let quote = broker.quote_payout("R_100", OptionType::Binary, 60, 1.0).await?;
if quote.available {
    println!("{:?} %", quote.profit_percent);
} else {
    println!("sin cotización para esta combinación");
}
```

Digital y Blitz devuelven `available: false`; la compra de esos productos sigue
rechazada. `PayoutQuote` no transporta dirección y la referencia es `CALL`: no
se asume que `PUT` pague el mismo porcentaje. En Deriv, `min_amount` y
`max_amount` quedan vacíos porque la propuesta no los publica.

## Densidad medida

Gateway público, ventana de 8 s, 9 de noviembre de 2026:

| Activo | Clase | Ticks | Densidad | Hueco medio |
|---|---|---|---|---|
| `R_100` | índice sintético | 4 | ~0,5/s | 1 993 ms |
| `1HZ100V` | índice sintético 1 s | 8 | 1,0/s | 998 ms |
| `1HZ10V` | índice sintético 1 s | 8 | 1,0/s | 1 002 ms |
| `frxAUDCAD` | forex | 8 | 1,0/s | 1 002 ms |
| `cryBTCUSD` | cripto | 8 | 1,0/s | 998 ms |
| `frxXAGUSD` | materia prima | 6–8 | 0,75–1,0/s | 1 000–1 183 ms |

La cadencia es del producto (el índice de 2 s entrega uno cada 2 s) y el
gateway público agrega forex y cripto a ~1 tick/s. **No es polling**: se
entrega el stream del bróker tal cual. Con credenciales, la densidad puede
variar; se documentará cuando se mida.
