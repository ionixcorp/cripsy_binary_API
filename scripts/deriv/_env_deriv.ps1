# Carga .env para Deriv. Exige token solo cuando se va a operar.
param([switch]$Operar)
$raiz = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location -LiteralPath $raiz

$archivo = Join-Path $raiz ".env"
if (-not (Test-Path -LiteralPath $archivo)) {
    Write-Host "Falta .env en $raiz"
    Write-Host "Copia .env.example a .env; el modo publico no necesita claves."
    exit 1
}

foreach ($linea in Get-Content -LiteralPath $archivo) {
    if ($linea -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"?([^"]*)"?\s*$') {
        Set-Item -Path "Env:$($matches[1])" -Value $matches[2].Trim()
    }
}

if ($Operar) {
    foreach ($clave in @("DERIV_TOKEN", "DERIV_APPID")) {
        if (-not (Test-Path "Env:$clave")) {
            Write-Host "Falta $clave en .env (necesario para operar)."
            exit 1
        }
    }
    Write-Host "Credenciales de Deriv cargadas desde .env (no se muestran)."
} else {
    Write-Host "Modo solo lectura: no se exigen credenciales de Deriv."
}
