# Deriv

Segundo bróker soportado, sobre la **API moderna** (`developers.deriv.com`):
REST para cuentas y OTP, WebSocket JSON para mercado y ejecución.

## Alcance

- **Operación:** opciones binarias **rise/fall** (`CALL`/`PUT`) en todos los
  activos que el bróker habilite.
- **Datos:** catálogo, velas y **ticks nativos de cualquier activo**; el
  consumidor puede analizar todo el mercado aunque solo opere rise/fall.
- **Fuera del producto:** multiplicadores, acumuladores, dígitos, barreras y
  demás familias. No se simulan sobre rise/fall; se añadirán como tipos de
  dominio cuando se amplíe el producto. No es una limitación técnica del
  adaptador, es el alcance actual del contrato.

## Dos modos

| Modo | Requisitos | Qué permite |
|---|---|---|
| Público | ninguno | catálogo, velas, ticks y cotización de payout |
| Autenticado | PAT con scope `trade` (o OAuth) + App ID | además balance, compra, venta e historial |

El modo público es el valor por defecto: si no hay token, la sesión no inventa
una cuenta ni permite operar. Las claves son tuyas y viven en `.env` (ignorado
por git); la plantilla está en `.env.example`.

## Secciones

| Documento | Contenido |
|---|---|
| `docs/deriv/02_cuenta/01_cuentas_identificadores.es.md` | Conexión, PAT, OTP, cuentas y sustitutos deterministas |
| `docs/deriv/03_mercado/01_catalogo_velas_ticks.es.md` | Catálogo, velas, ticks, payout y densidad medida |
| `docs/deriv/04_ordenes/01_rise_fall_historial.es.md` | Compra/venta rise/fall, cierre por evento e historial |
| `docs/deriv/05_operativa/01_reconexion_reconciliacion.es.md` | OTP nuevo, replay de suscripciones y reconciliación |
| `docs/deriv/06_limites/01_limites.es.md` | Cupos, capacidades no soportadas e identificadores |

## Estado verificado

| Componente | Estado |
|---|---|
| Modo público (sin credenciales) | Verificado: catálogo, velas, payout y ticks |
| Sesión autenticada (OTP + App ID) | Verificada en cuenta demo |
| Compra y venta rise/fall | Verificadas en cuenta demo |
| Cierre por evento e historial | Verificados (`profit_table` paginado) |
| Reconexión con OTP nuevo | Verificada (2 396 ms, replay de suscripciones) |
| Reconciliación de cierres | Verificada con 72 s sin red y un solo evento |
| Blitz, digital, indicadores, live-deals | No soportado; rechazo explícito |

## Qué no hace

- No calcula indicadores ni construye estrategias.
- No convierte rise/fall en otros productos.
- No reenvía órdenes tras una caída: una orden diferida operaría contra otro
  mercado.
