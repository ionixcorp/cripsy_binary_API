# Instalación

La instalación de `cripsy_binary_API` es **guiada y con aprobación**: ningún
paso que modifique tu sistema se ejecuta sin que lo confirmes.

## Requisitos

| Herramienta | Para qué | Mínimo |
|---|---|---|
| Rust (rustup + cargo) | Compilar la librería | 1.86 |
| git | Control de versiones | cualquiera |
| Credenciales del bróker | Probar la conexión | cuenta demo recomendada |

## Instalación rápida

```powershell
.\install.ps1                              # completo (con comprobación de conexión)
.\install.ps1 -ConPruebaDeHumo:$false      # sin contacto con el bróker
.\install.ps1 -ConPuerta:$true             # además ejecuta la compuerta de calidad
```

Qué hace, paso a paso:

1. **Verifica el entorno** (`check_env.ps1`): Rust, cargo, git y `.env`.
2. **Ofrece instalar lo que falte** (por ejemplo Rust vía winget) — solo si
   apruebas la pregunta.
3. **Crea `.env`** a partir de `.env.example` y te lo abre para editarlo si
   quieres.
4. **Compila** la librería en release, en sus dos variantes (async y blocking).
5. **Genera la documentación navegable** (`docs/site/es/`) y, si lo apruebas,
   ejecuta el ejemplo de conexión (`ejemplo_lectura`): login real + WebSocket +
   catálogo + velas. Es de solo lectura.
6. **Registra el estado**: escribe `docs/site/installed.js`, que el portal
   `index.html` lee para saber que la API está instalada.

El instalador es idempotente: puedes reejecutarlo sin romper nada.

## Instalar sin modificar nada

```powershell
.\check_env.ps1
```

Solo informa del estado y no instala ni escribe nada.

## Primer contacto después de instalar

```powershell
# compuerta de calidad
..\scripts\verify.ps1

# documentación navegable (regenerar)
..\scripts\build_docs.ps1 -Lang es

# ejemplos de uso
cargo run --example ejemplo_lectura
cargo run --example ejemplo_sincrono
```

## Notas

- Las credenciales viven solo en `.env` (ignorado por git). Nunca se escriben
  en el código ni en los volcados.
- Windows PowerShell 5.1 funciona, pero se recomienda PowerShell 7+.
