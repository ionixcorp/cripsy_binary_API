# Cuentas e identificadores

## Conexión

```rust
use cripsy_binary_api::broker::{Broker, BrokerParams, DerivBroker, DerivConfig};

// Solo mercado (gratuito, sin credenciales).
let broker = DerivBroker::connect(BrokerParams::Deriv(DerivConfig::publico())).await?;

// Cuenta demo o real: el OTP se pide por REST y la URL del socket es efímera.
let broker = DerivBroker::connect(BrokerParams::Deriv(DerivConfig {
    token: std::env::var("DERIV_TOKEN").ok(),
    app_id: std::env::var("DERIV_APPID").ok(),
    account_id: std::env::var("DERIV_ACCOUNT_ID").ok(),
    ..DerivConfig::default()
})).await?;
```

- El **PAT** exige la cabecera `Deriv-App-ID`; OAuth no.
- El OTP **caduca a los 120 s y es de un solo uso**; al reconectar se pide uno
  nuevo. La API lo hace sola: tú no ves ni gestionas el OTP.
- Sin `token` no se abre sesión autenticada: el modo público es de solo lectura.

## Variables de entorno

```dotenv
DERIV_TOKEN = "tu-token-con-scope-trade"
DERIV_APPID = "tu-app-id"
DERIV_ACCOUNT_ID = "DOT00000000"
```

`DERIV_ACCOUNT_ID` es opcional si la cuenta activa del token ya es la correcta.
Las claves se leen del entorno y nunca se imprimen ni se serializan.

## Cuentas

Deriv identifica cuentas con cadenas (`DOT…`, `VRTC…` demo; `CR…` real). Hasta
que el contrato admita cadenas, `Session.balance_id` y `AccountBalance.id`
exponen un **sustituto determinista** documentado, y `Asset.opcode` es `0`
porque Deriv no publica identificador numérico. Usa `symbol`/`account_id`.

El modo demo o real lo determina la cuenta asociada al token; no se elige con
`AccountSelection` como en IQ Option. `session()` y `get_balance()` devuelven la
cuenta activa y su saldo. `switch_account` no está soportado por el adaptador:
Deriv no ofrece ese cambio en esta integración.
