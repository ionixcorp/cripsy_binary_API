# Herramientas del proyecto

Todo lo necesario para instalar, verificar y desarrollar está en tres lugares:

- `installation/` — instalación guiada y verificación del entorno.
- `scripts/` — compuerta de calidad y generación de la documentación.
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
.\scriptserify.ps1
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

## Probar contra el bróker

Los ejemplos leen las credenciales del entorno y operan en cuenta demo:

```powershell
cargo run --example ejemplo_lectura     # sesión, catálogo, velas y stream
cargo run --example ejemplo_sincrono    # el mismo consumo, bloqueante
cargo run --example ejemplo_orden_demo  # compra demo y cierre por evento
```

`ejemplo_orden_demo` mueve saldo de práctica: ejecútalo solo si eres el dueño de
la cuenta o tienes autorización explícita.

## Agregar otro bróker

1. Crear `src/broker/<nombre>/` con su `Config` (añadir la variante a
   `BrokerParams`).
2. Implementar el trait `Broker` (async).
3. Escribir el mapper dominio ↔ protocolo del bróker.
4. Registrar en `BrokerRegistry`.

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
