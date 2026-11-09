# Carga las credenciales de .env en la sesión actual (nunca las imprime).
$raiz = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location -LiteralPath $raiz

$archivo = Join-Path $raiz ".env"
if (-not (Test-Path -LiteralPath $archivo)) {
    Write-Host "Falta .env en $raiz"
    Write-Host "Copia .env.example a .env y completalo:  Copy-Item .env.example .env"
    exit 1
}

foreach ($linea in Get-Content -LiteralPath $archivo) {
    if ($linea -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"?([^"]*)"?\s*$') {
        Set-Item -Path "Env:$($matches[1])" -Value $matches[2].Trim()
    }
}

foreach ($clave in @("IQOPTION_EMAIL", "IQOPTION_PASSWORD")) {
    if (-not (Test-Path "Env:$clave")) {
        Write-Host "Falta $clave en .env"
        exit 1
    }
}

Write-Host "Credenciales cargadas desde .env (no se muestran)."
