# Sesión y cuentas

## Conectar

La conexión combina un login HTTP y un WebSocket:

1. `POST https://auth.iqoption.com/api/v2/login` con `{identifier, password}`.
   La respuesta trae la cookie `ssid` (también en el cuerpo).
2. Se abre `wss://iqoption.com/echo/websocket` y se envía
   `{"name":"ssid","msg":"<SSID>"}` como primer mensaje.
3. El bróker responde con `profile`, que incluye los balances y el
   `balance_id` activo.

La sesión queda operativa cuando el catálogo de activos se ha recibido
(`get-initialization-data` v3.0).

## Cuentas: demo y real

El perfil contiene varias cuentas. El tipo se identifica por `balance.type`:

| `type` | Cuenta |
|---|---|
| 1 | Real |
| 4 | Demo (practica) |
| otros | tratados como real |

Al conectar puedes elegir la cuenta con `AccountSelection`:

```rust
let config = IqOptionConfig {
    email: std::env::var("IQOPTION_EMAIL")?,
    password: std::env::var("IQOPTION_PASSWORD")?,
    account: AccountSelection::Practice,  // demo explícita
    ..IqOptionConfig::default()
};

let broker = IqOptionBroker::nuevo(BrokerParams::IqOption(config)).await?;
```

## Consultas disponibles

Todas son `async` y devuelven `Result<_, BrokerError>`:

| Llamada | Qué devuelve |
|---|---|
| `session().await?` | Copia de la sesión activa (`balance_id`, tipo de cuenta, offset de hora) |
| `get_balance().await?` | Monto, moneda y tipo de la cuenta activa |
| `get_account_type().await?` | `Practice` o `Real` |
| `switch_account(kind).await?` | Cambia a demo/real (`api_profile_changebalance`) |
| `list_assets().await?` | Todos los activos del catálogo |
| `asset_status(symbol).await?` | `Open` / `Closed` / `Suspended` según el bróker |

Nota sobre el catálogo: el bróker publica el mismo símbolo en hasta tres grupos
(binary, turbo y blitz) con **metadatos distintos** entre ellos. El catálogo
contiene una entrada por grupo; para operar blitz usa la de
`AssetClass::Blitz` (ver la sección 06).

## Notas importantes

- Toda orden lleva `user_balance_id`: sin cuenta activa el bróker la rechaza.
- La expiración se calcula con la **hora del servidor** (`timeSync`), no con el
  reloj local.
- Las cuentas demo tienen saldo independiente; las operaciones de prueba se ven
  solo cuando la demo es la cuenta activa.

## Eventos de comisiones y operaciones de terceros

Dos canales más, ambos con suscripción explícita (los eventos salen por
`broker.events()`):

```rust
// Comisiones por tipo de instrumento (binary-option, turbo-option, digital-option)
broker.subscribe_commission_changes().await?;
while let Some(ev) = broker.events().await?.next().await {
    if let Event::CommissionChanged(c) = ev {
        // c.symbol, c.instrument_type, c.commission_value
    }
}

// Operaciones de terceros (live-deal) para un activo concreto
broker.subscribe_live_deals("EURUSD-OTC").await?;
while let Some(ev) = broker.events().await?.next().await {
    if let Event::LiveDeal(deal) = ev {
        // deal.symbol, deal.direction, deal.amount, deal.created_at
    }
}
```

- `commission-changed` emite `Event::CommissionChanged` con la comisión en
  puntos (payout = 100 − comisión).
- `live-deal` / `live-deal-binary-option-placed` emiten `Event::LiveDeal`
  (operaciones de otros usuarios: señal de flujo del mercado).
- El volumen depende de la actividad del bróker: en horarios tranquilos puede
  no llegar nada (verificado en vivo: suscripción aceptada, 0 eventos en
  ventana de fin de semana).
