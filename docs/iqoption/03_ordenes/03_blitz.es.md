# Opciones blitz

Blitz es el tipo más nuevo del protocolo (`option_type_id` **12**, mensaje
`binary-options.open-option` **v2.0**) y tiene sus propias reglas. Los errores
de implementación típicos se documentan aquí porque el bróker los reporta con
mensajes engañosos.

## Reglas verificadas

1. **El grupo blitz tiene metadatos propios.** El mismo símbolo aparece en
   binary, turbo y blitz con duraciones distintas (binary declara
   `[900]`; blitz `[30, 45, 60, 120, 180, 300]`, y los rollovers añaden
   600 y 900). Resuelve el activo contra el grupo blitz (`AssetClass::Blitz`),
   nunca contra binary.
2. **`expired` debe ir alineado** al múltiplo de la duración:
   `(ahora_servidor / duracion + 1) * duracion`. Un `expired` desalineado
   recibe `active is suspended` — que parece un estado del mercado pero es un
   error de forma.
3. **`profit_percent` va como entero** (el bróker rechaza "86.0").

## Llamada

```rust
let orden = broker.buy_binary(BuyBinaryRequest {
    symbol: "USDZAR-OTC".into(),
    amount: 1.0,
    direction: Direction::Call,
    expiration_minutes: 30,        // para blitz: SEGUNDOS (ver nota)
    option_type: OptionType::Blitz,
}).await?;
```

Nota de nomenclatura: en `BuyBinaryRequest`, el campo `expiration_minutes`
transporta **segundos** cuando `option_type` es `Blitz` (las duraciones del
catálogo blitz están en segundos).

## Rollovers

La respuesta de apertura del bróker trae `rollover_params` (`deadtime`,
`limit`, `offset`, `offset_from_expiration`, `amount_multiplier`): son los
parámetros de renovación automática que el bróker aplica a la orden. Hoy
`buy_binary` devuelve solo el `OrderId`; los parámetros de rollover viajan en
la respuesta del bróker pero **no se exponen aún** en la API pública.

## Estado actual

Blitz está verificado **de extremo a extremo en cuenta demo**: apertura
(`status 2000`, `type: "blitz"`, respuesta con `rollover_params`) y cierre por
evento con resultado y delta de balance coincidente.

El bróker puede reportar `active is suspended` / `asset is not available at the
moment` para activos concretos: es un estado **por activo** que el bróker
decide (OTC blitz opera 24/7, no es cierre de mercado). Si recibes ese error,
prueba otro activo o más tarde; el mensaje de error es literalmente el del
bróker.

Si una compra blitz se rechaza, revisa antes tu petición: las causas
confirmadas fueron duración no admitida por el grupo blitz y `expired`
desalineado (arriba), no el estado del mercado.
