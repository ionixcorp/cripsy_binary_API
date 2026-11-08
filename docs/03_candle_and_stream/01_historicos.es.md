# Históricos: límites verificados

Los datos históricos se piden con `get_candles(...)`. Los límites están
**verificados en vivo** contra el bróker (2026-10-27):

| Tamaño de vela | Profundidad máxima |
|---|---|
| 1 segundo | **7 días exactos** (a 14 días: 0 velas) |
| 1 minuto | entre 45 y 90 días |
| 5 minutos | entre 270 y 330 días |
| 15 min, 1 h, 4 h, 1 día | **más de 2 años** |

Reglas prácticas:

- `count` máximo por petición: **1000 velas** (pedir más devuelve 1000).
- `to` debe ser el **cierre de una vela ya cerrada**. Con size=1 funciona
  `(ahora/60)*60 - 90`; con `to` reciente la respuesta llega vacía.
- Los datos son contiguos: sin huecos dentro de la ventana disponible.

Tamaños disponibles: 1, 5, 15, 30, 60, 300, 900, 3600, 14400 y 86400 s.

## Descargar toda la semana en velas de 1 s

Iterando `end_time` hacia atrás, `count × interval` segundos por petición:

```rust
let mut fin = None;
loop {
    let velas = broker.get_candles(CandleRequest {
        symbol: "USDZAR-OTC".into(),
        interval: 1,
        count: 1000,
        end_time: fin,
    }).await?;
    if velas.is_empty() { break; }
    fin = Some(velas.last().unwrap().from);
    // ~605 peticiones cubren los 7 días completos de este activo
}
```

El protocolo también permite paginar por id (`to_id`), pero **nuestra API aún
no lo expone**: la descarga se hace iterando `end_time` hacia atrás.

## Nota sobre densidad en histórico

El histórico es de velas: **no existe densidad de tick en el pasado**. La
granularidad fina máxima es la vela de 1 segundo. Para densidad intrasegundo
hay que usar el stream en vivo (ver la próxima página).
