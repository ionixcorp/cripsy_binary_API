#!/usr/bin/env pwsh
# Instalación guiada de cripsy_binary_API (PowerShell nativo).
#
# Reglas: cada acción que modifica el sistema se ejecuta SOLO con tu aprobación
# explícita. Nada se instala en silencio. Es idempotente: puedes reejecutarlo.
#
# Ejemplos:
#   .\install.ps1                      # completo (con prueba de humo)
#   .\install.ps1 -ConPruebaDeHumo:$false   # sin contacto con el bróker
#   .\install.ps1 -ConPuerta:$true          # además ejecuta la compuerta de calidad

param(
    [bool]$ConPruebaDeHumo = $true,
    [bool]$ConPuerta = $false
)

$ErrorActionPreference = "Stop"
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $raiz

function Aprobar($mensaje) {
    $respuesta = Read-Host "$mensaje [s/N]"
    return ($respuesta -match '^[sSyY]')
}

Write-Host "== Instalación guiada de cripsy_binary_API =="
Write-Host "Directorio: $raiz"
Write-Host ""

# ---------- 1. Entorno ----------
Write-Host "1/6 Verificando el entorno..."
& (Join-Path $PSScriptRoot "check_env.ps1")
$entorno_ok = ($LASTEXITCODE -eq 0)
if (-not $entorno_ok) {
    Write-Host ""
    if (Aprobar "Faltan herramientas. ¿Deseas que el instalador intente resolverlas?") {
        if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
            if (Get-Command winget -ErrorAction SilentlyContinue) {
                if (Aprobar "Instalar Rust con winget (Rustlang.Rustup)?") {
                    winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements
                    Write-Host "  Rust instalado. Abre una terminal nueva si cargo no aparece en esta sesión."
                }
            } else {
                Write-Host "  winget no disponible. Descarga rustup-init desde https://rustup.rs y vuelve a ejecutar."
            }
        }
        # Refrescar PATH de esta sesión
        $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")
        & (Join-Path $PSScriptRoot "check_env.ps1")
        if ($LASTEXITCODE -ne 0) {
            Write-Host "El entorno sigue incompleto; revisa los pasos anteriores."
            exit 1
        }
    } else {
        Write-Host "Instalación cancelada por el usuario."
        exit 1
    }
}

# ---------- 2. .env ----------
Write-Host ""
Write-Host "2/6 Configurando credenciales..."
if (-not (Test-Path -LiteralPath ".env")) {
    if (Test-Path -LiteralPath ".env.example") {
        if (Aprobar "No existe .env. ¿Crearlo a partir de .env.example?") {
            Copy-Item -LiteralPath ".env.example" -Destination ".env"
            Write-Host "  .env creado. Edítalo con tus credenciales de IQ Option."
            if (Aprobar "¿Abrir .env ahora para editarlo?") {
                notepad .env
            }
        } else {
            Write-Host "  Sin .env: la prueba de humo se omitirá."
            $ConPruebaDeHumo = $false
        }
    }
} else {
    Write-Host "  .env ya existe."
}

# ---------- 3. Compilación ----------
Write-Host ""
Write-Host "3/6 Compilando la librería (release)..."
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "La compilación falló. Revisa los errores de cargo."
    exit 1
}
cargo build --release --no-default-features --features blocking
if ($LASTEXITCODE -ne 0) {
    Write-Host "La compilación de la variante sincrona falló."
    exit 1
}
Write-Host "  Compilación OK (async + blocking)."

# ---------- 4. Documentación navegable ----------
Write-Host ""
Write-Host "4/6 Generando la documentación navegable..."
& (Join-Path $PSScriptRoot "..\scripts\build_docs.ps1") -Lang es
if ($LASTEXITCODE -ne 0) {
    Write-Host "  La generación de documentación falló (no es bloqueante)."
} else {
    Write-Host "  Documentación generada en docs/site/es/."
}

# ---------- 5. Prueba de humo ----------
if ($ConPruebaDeHumo -and (Test-Path -LiteralPath ".env")) {
    Write-Host ""
    Write-Host "5/6 (opcional) Prueba de humo contra IQ Option (solo lectura)..."
    if (Aprobar "¿Ejecutar la prueba de conexión real (usa tus credenciales)?") {
        $env:IQOPTION_EMAIL = ((Get-Content .env | Where-Object { $_ -match '^IQOPTION_EMAIL' }) -replace '.*=\s*"' -replace '"$')
        $env:IQOPTION_PASSWORD = ((Get-Content .env | Where-Object { $_ -match '^IQOPTION_PASSWORD' }) -replace '.*=\s*"' -replace '"$')
        cargo run --quiet --release --example ejemplo_lectura 2>&1 | Select-Object -Last 8
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  Conexión OK: login, catálogo y velas verificados."
        } else {
            Write-Host "  La prueba de humo falló (revisa credenciales o conectividad)."
        }
    } else {
        Write-Host "  Prueba de humo omitida."
    }
} else {
    Write-Host ""
    Write-Host "5/6 (opcional) Prueba de humo omitida (sin .env o no aprobada)."
}

# ---------- 6. Compuerta opcional ----------
if ($ConPuerta) {
    Write-Host ""
    Write-Host "6/6 (opcional) Compuerta de calidad..."
    & (Join-Path $PSScriptRoot "..\scripts\verify.ps1")
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  La compuerta falló. Revisa los errores."
        exit 1
    }
    Write-Host "  Compuerta verde."
    $pasos = 6
} else {
    $pasos = 6
}

# ---------- Estado para el portal ----------
$estado = @{
    installed = $true
    at = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    toolchain = (& rustc --version) 2>$null
    docs = "docs/site/es/index.html"
} | ConvertTo-Json
$dir_site = Join-Path $raiz "docs/site"
New-Item -ItemType Directory -Path $dir_site -Force | Out-Null
# installed.js (evita el bloqueo CORS de file://)
Set-Content -LiteralPath (Join-Path $dir_site "installed.js") -Value "var INSTALL_STATE = $estado;"
Set-Content -LiteralPath (Join-Path $dir_site "state.json") -Value $estado

Write-Host ""
Write-Host "== Instalación completa =="
Write-Host "  Documentación:  index.html (raíz) o docs/site/es/index.html"
Write-Host "  Reglas de uso:  README.md"
Write-Host "  Verificar:      .\scripts\verify.ps1"
Write-Host "  Comprobar:      cargo test"
Write-Host "  Ejemplos:       cargo run --example ejemplo_lectura"
exit 0

