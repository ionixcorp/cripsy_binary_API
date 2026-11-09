# Indicadores técnicos del bróker

## La respuesta corta

El bróker **sí tiene un canal** de indicadores técnicos, pero hoy rechaza todos
los activos. Nuestra API lo implementa igual: cuando el bróker lo habilite,
funciona sin cambios.

## Qué se verificó

El canal existe y responde con mensajes tipados:

- Petición: `sendMessage` con `trading-signals.get-technical-indicators`
  v1.0 y body `{"id": <active_id>}`.
- Respuesta: mensaje `technical-indicators` con `msg.indicators` (forma libre
  del bróker) o `msg.message` de error.

Errores observados en la validación (2026-10-27):

| Prueba | Respuesta del bróker |
|---|---|
| Varias docenas de activos (OTC y no-OTC, abiertos y cerrados) | `Active is not supported: active id 'X'` (status 4150) |
| `id` como texto | `INVALID_SCHEMA` (debe ser entero) |
| versión 2.0 | sin respuesta |

Es decir: el canal está vivo pero el servicio no entrega indicadores a esta
cuenta/estado actual. No es un problema de formato.

## Cómo usarlo cuando esté disponible

```rust
let indicadores = broker.get_technical_indicators("EURUSD-OTC").await?;
// indicadores.indicators trae la forma libre del bróker
```

La API es de transporte: **no calcula ni configura indicadores**, entrega lo
que el bróker publique. Si el bróker no soporta el activo, recibes
`BrokerError::Rejected` con el mensaje textual del bróker.

## Alternativa: calcular los tuyos

Si prefieres no depender del canal, puedes calcular indicadores a partir de:

- **Histórico**: hasta 1s × 7 días por activo (ideal para backtests densos).
- **Stream en vivo**: ~3 ticks/s por activo para tiempo real.
- **Mood y popularidad**: `Event::TradersMoodChanged` y
  `Event::TopAssetUpdated` vienen del bróker como señales adicionales.

Con velas de 1 segundo reconstruidas desde el histórico puedes alimentar
cualquier indicador (medias, RSI, bandas, pivotes) con resolución de un segundo.
