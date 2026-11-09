# Límites y capacidades no soportadas

## Cupos del bróker

| Grupo | Por minuto | Por hora |
|---|---|---|
| `proposal`, `proposal_open_contract`, `buy`, `sell` | 360 | 14 400 |
| `balance`, `statement` | 100 | 2 000 |
| `portfolio`, `profit_table` | 30 | 1 500 |
| Resto (`contracts_for`, velas, símbolos) | 220 | 14 400 |

La API aplica el **80 %** de cada cupo y espera en vez de fallar. Un `RateLimit`
del bróker nunca provoca el reenvío de una orden.

## Capacidades no soportadas

| Capacidad | Respuesta |
|---|---|
| Blitz, digitales, multiplicadores, acumuladores, dígitos, barreras | `Unsupported` / rechazo explícito |
| Indicadores técnicos | no ofrecidos por esta integración |
| Perfil de activo | no ofrecido |
| Live deals y comisiones | no ofrecidos |
| Fachada sincrónica `SyncBroker` | exclusiva de IQ Option |

`SyncBroker` no se extiende a Deriv: existe para consumidores heredados de IQ
Option y Deriv mantiene la API async. No es una limitación pendiente, es una
decisión de compatibilidad.

## Identificadores

- Las cuentas son cadenas (`DOT…`, `VRTC…`, `CR…`).
- `Session.balance_id` y `AccountBalance.id` usan un sustituto determinista.
- `Asset.opcode` es `0`; usa `symbol`.
- El `account_id` autenticado es la referencia fiable de la cuenta.

## Datos medidos

La densidad de ticks se midió con el gateway público y ventana de 8 s. No se
extrapola a cuentas autenticadas ni a horario de mercado pleno; cuando se mida,
se documentará con la misma distinción entre observado y derivado.
