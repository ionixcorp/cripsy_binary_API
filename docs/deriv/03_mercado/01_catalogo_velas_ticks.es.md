# Mercado: catálogo, velas y ticks

## Capacidades

| Operación | Método | Estado |
|---|---|---|
| Catálogo de activos | `list_assets` | verificado |
| Estado de mercado | `asset_status` | verificado |
| Velas históricas | `get_candles` | verificado (granularidades 1 m – 1 d) |
| Velas en vivo | `subscribe_candles` | implementado |
| Ticks nativos | `subscribe_ticks` → `Event::PriceTick` | verificado |
| Payout cotizado | `get_payout` | verificado |

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

`get_payout(symbol, seconds)` devuelve la cotización publicada para el activo y
la duración pedida. No se consulta por importe ni por strike; el importe se elige
al comprar y el bróker puede rechazarlo según sus mínimos y máximos.

```rust
let payout = broker.get_payout("R_100", 60).await?;
println!("{} %", payout.profit_percent);
```

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
