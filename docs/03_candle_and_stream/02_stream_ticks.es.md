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
atrasa más que el buffer, el stream se cierra y hay que volver a llamar
`events()`: consume los eventos sin bloqueos largos entre lectura y lectura.

Medido en vivo (2026-10-27, varios mercados OTC):

| Activo (mercado) | Mensajes/s | Cambios de precio/s |
|---|---|---|
| XAUUSD-OTC (materia prima) | 3.67 | 3.20 |
| ETHUSD-OTC (cripto) | 3.40 | 2.80 |
| EURUSD-OTC (forex) | 3.20 | 2.67 |

Hallazgos clave:

- La densidad (~3/s por activo) **no depende** de suscribir multi-tamaño o un
  tamaño específico: da igual.
- **Sin límite de caudal por conexión**: con 20 activos suscritos se midieron
  61 mensajes/s (3.07 por activo); el caudal escala linealmente.
- Escala verificada hasta **75 activos en una sola sesión**: 0 rechazos,
  5338 velas en 25 s (2.85 velas/s por activo).
- La densidad exacta depende de la liquidez y el horario del activo; los
  ejemplos miden mercados OTC de fin de semana.

## Sin sondeo

Cada mensaje del WebSocket se procesa al instante y se emite como evento: no
hay bucles de consulta ni latencia artificial añadida. La densidad que mides es
la que el bróker publica.
