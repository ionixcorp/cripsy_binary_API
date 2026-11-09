# Stream en vivo: velas multi-tamaño y ticks

Dos mecanismos nativos, sin polling:

## 1. Velas en vivo (`subscribe_candles`)

Una única suscripción entrega **todos los tamaños** de vela de un activo
(1 s, 5 s, 1 m, 5 m, ... hasta diario). El adaptador filtra por el tamaño
pedido: `interval = 0` significa "todos los tamaños".

```rust
let mut stream = broker.subscribe_candles(CandleSubscribe {
    symbol: "USDZAR-OTC".into(),
    interval: 60,           // velas de 1 minuto
}).await?;

while let Some(vela) = stream.next().await {
    let vela = vela?;       // Candle { from, to, open, close, high, low, volume }
    // estrategia...
}
```

## 2. Ticks de precio (`Event::PriceTick`)

Cada mensaje del stream trae el precio spot (`value`, `bid`, `ask`) con marca
temporal en nanosegundos. La sesión lo emite como evento:

```rust
let mut eventos = broker.events().await?;
while let Some(ev) = eventos.next().await {
    if let Event::PriceTick { symbol, tick } = ev {
        // tick.price, tick.bid, tick.ask, tick.at_ms
    }
}
```

El bus de eventos tiene buffer acotado (1024 eventos). Si tu consumidor se
atrasa más que el buffer, se descartan los eventos intermedios y el stream
entrega `Event::EventsLost { count }` con la cuenta de los perdidos; el stream
sigue abierto. Consume sin bloqueos largos entre lectura y lectura.

Medido en vivo en IQ Option (2026-10-27, varios mercados OTC):

| Activo (mercado) | Mensajes/s | Cambios de precio/s |
|---|---|---|
| XAUUSD-OTC (materia prima) | 3.67 | 3.20 |
| ETHUSD-OTC (cripto) | 3.40 | 2.80 |
| EURUSD-OTC (forex) | 3.20 | 2.67 |

Hallazgos clave:

- En IQ Option la densidad (~3/s por activo) **no depende** de suscribir
  multi-tamaño o un tamaño específico: da igual.
- **Caudal proporcional al número de activos**: con 20 activos suscritos se
  midieron 61 mensajes/s (3.07 por activo).
- Escala verificada hasta **75 activos en una sola sesión**: 0 rechazos,
  5338 velas en 25 s (2.85 velas/s por activo). No se probó por encima de esa
  cifra, así que no se documenta un techo.
- La densidad exacta depende de la liquidez y el horario del activo; los
  ejemplos miden mercados OTC de fin de semana.

## 3. Densidad medida en Deriv

Gateway público, ventana de 8 s, 2026-11-09:

| Activo | Clase | Ticks | Densidad | Hueco medio |
|---|---|---|---|---|
| `R_100` | índice sintético (2 s) | 4 | ≈0,5/s | 1 993 ms |
| `1HZ100V` | índice sintético (1 s) | 8 | 1,0/s | 998 ms |
| `1HZ10V` | índice sintético (1 s) | 8 | 1,0/s | 1 002 ms |
| `frxAUDCAD` | forex | 8 | 1,0/s | 1 002 ms |
| `cryBTCUSD` | cripto | 8 | 1,0/s | 998 ms |
| `frxXAGUSD` | materia prima | 6–8 | 0,75–1,0/s | 1 000–1 183 ms |

La cadencia la fija el producto: los índices de 2 s entregan un tick cada 2 s;
forex y cripto llegan agregados a ~1/s por el gateway público. No es polling:
se entrega el stream del bróker tal cual. La materia prima osciló entre 0,75 y
1,0/s entre pasadas. Faltan mediciones con credenciales y en horario de mercado
pleno; no se extrapolan.

## Sin sondeo

Cada mensaje del WebSocket se procesa al instante y se emite como evento: no
hay bucles de consulta ni latencia artificial añadida. La densidad que mides es
la que el bróker publica.
