# Herramientas del proyecto

Todo lo necesario para instalar, verificar y desarrollar está en tres lugares:

- `installation/` — instalación guiada y verificación del entorno.
- `scripts/` — compuerta de calidad, generación de la documentación y scripts
  listos para usar contra el bróker.
- `examples/` — uso real de la API (lectura, consumo sincrónico y una orden demo).

## Instalación guiada

```powershell
.\installation\install.ps1
```

El instalador, **pidiendo tu aprobación en cada paso**:

1. Verifica el entorno (`check_env.ps1`): Rust, cargo, git y `.env`.
2. Si falta una herramienta, ofrece instalarla (rustup por winget o descarga
   oficial) — solo si apruebas.
3. Compila la librería en modo release, en sus dos variantes (async y blocking).
4. Crea `.env` a partir de `.env.example` para tus credenciales.
5. Genera la documentación navegable y, si lo apruebas, ejecuta
   `ejemplo_lectura` para comprobar la conexión (solo lectura).
6. Escribe `docs/site/installed.js` con el estado: el portal `index.html`
   detecta la instalación por ese archivo.

## Verificación sin instalar

```powershell
.\installation\check_env.ps1
```

## Compuerta de calidad

```powershell
.\scripts\verify.ps1
```

Ejecuta `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, el build de la variante `blocking` y la generación de la
documentación. Es el estándar mínimo antes de dar un cambio por bueno.

## Regenerar la documentación navegable

```powershell
.\scriptsuild_docs.ps1 -Lang es
```

Convierte `docs/**/*.es.md` en un único HTML navegable en `docs/site/es/`. El
punto de entrada es `index.html` en la raíz del proyecto. El Markdown es la
fuente de verdad; el HTML es generado y nunca se edita a mano.

Para leerlo: en VS Code instala la extensión **Live Server** (Go Live) y pulsa
*Go Live* sobre el `index.html`; o abre el archivo con doble clic, porque
funciona también sin servidor (`file://`, navegación por hash y sin `fetch`).

## Scripts listos para usar

Todas leen tus credenciales de `.env` y operan en cuenta **demo**:

| Script | Bróker | Qué hace |
|---|---|---|
| `scripts\conectar.ps1` | IQ Option | Sesión, balance, catálogo, velas y stream (solo lectura) |
| `scripts\balance.ps1` | IQ Option | Balance, tipo de cuenta y `balance_id` |
| `scripts\operar.ps1` | IQ Option | Orden de 1 USD en demo y cierre por evento (pide confirmación) |
| `scripts\verificar_iqoption.ps1` | IQ Option | Valida sus capacidades y reporta `[OK]`/`[FALLO]` |
| `scripts\verificar_deriv.ps1` | Deriv | Público, ticks, densidad, órdenes y caídas en demo |
| `scripts\verificar_todo.ps1` | Ambos | Ejecuta los dos verificadores en secuencia |

```powershell
.\scripts\verificar_iqoption.ps1            # IQ Option, incluye órdenes demo
.\scripts\verificar_iqoption.ps1 -SoloLectura
.\scripts\verificar_deriv.ps1               # Deriv, incluye órdenes y caídas
.\scripts\verificar_deriv.ps1 -SoloLectura  # solo mercado y ticks
.\scripts\verificar_todo.ps1                # ambos
```

`verificar_deriv.ps1` usa el modo público si no hay token; para operar exige
`DERIV_TOKEN` y `DERIV_APPID` en `.env`.

## Probar contra el bróker

Los ejemplos leen las credenciales del entorno y operan en cuenta demo:

```powershell
cargo run --example ejemplo_lectura     # IQ Option: sesión, catálogo, velas y stream
cargo run --example ejemplo_sincrono    # IQ Option: el mismo consumo, bloqueante
cargo run --example ejemplo_orden_demo  # IQ Option: compra demo y cierre
cargo run --example verificar_todo      # IQ Option: verificación integral

cargo run --example deriv_conectar      # Deriv: público o cuenta demo
cargo run --example deriv_ticks         # Deriv: ticks nativos y reconexión
cargo run --example deriv_densidad      # Deriv: densidad por activo
cargo run --example deriv_orden_demo    # Deriv: compra/venta demo
cargo run --example deriv_reconciliacion # Deriv: cierre ocurrido sin red
```

`ejemplo_orden_demo` mueve saldo de práctica: ejecútalo solo si eres el dueño de
la cuenta o tienes autorización explícita.

## Agregar otro bróker

1. Crear `src/broker/<nombre>/` con su `Config` (añadir la variante a
   `BrokerParams`).
2. Implementar el trait `Broker` (async) y `subscribe_ticks`.
3. Escribir el mapper dominio ↔ protocolo del bróker.
4. Si hay capacidades exclusivas, exponerlas en un trait propio (como
   `IqOptionExtras`) en vez de ampliar `Broker`.
5. Registrar en `BrokerRegistry` y añadir su verificador en `scripts/`.

No se modifica ni el dominio ni el transporte: los brókers nuevos se añaden
como adaptadores.

## Garantía y descargo

La API expone el contrato listo para mostrar desde tu código:

```rust
use cripsy_binary_api::{mensaje_consumidor, revisar_config};

println!("{}", mensaje_consumidor());
let reporte = revisar_config(&tu_config);
for aviso in &reporte.advertencias {
    eprintln!("aviso de uso: {aviso}");
}
```

Si sigues el flujo documentado, lo documentado funciona. Cualquier otro uso
queda fuera de la garantía: ver la sección «Garantía y descargo» del `README.md`.
