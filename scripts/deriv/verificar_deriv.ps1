#!/usr/bin/env pwsh
# Verifica el adaptador Deriv contra el broker real.
# Sin -SoloLectura ejecuta ordenes y caidas forzadas en cuenta DEMO.
param([switch]$SoloLectura)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "_env_deriv.ps1") -Operar:(-not $SoloLectura)

$fallos = 0
function Probar([string]$ejemplo) {
    Write-Host ""
    Write-Host "== $ejemplo =="
    cargo run --quiet --example $ejemplo
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[FALLO] $ejemplo"
        $script:fallos++
    } else {
        Write-Host "[OK] $ejemplo"
    }
}

Probar "deriv_conectar"
Probar "deriv_ticks"
Probar "deriv_densidad"

if (-not $SoloLectura) {
    Probar "deriv_orden_demo"
    Probar "deriv_reconexion"
    Probar "deriv_reconciliacion"
}

if ($fallos -eq 0) {
    Write-Host ""
    Write-Host "RESULTADO: Deriv responde como esta documentado."
} else {
    Write-Host ""
    Write-Host "RESULTADO: $fallos prueba(s) de Deriv fallaron."
}
exit $fallos
