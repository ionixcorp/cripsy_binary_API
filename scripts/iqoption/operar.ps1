#!/usr/bin/env pwsh
# Abre una orden clásica de 1 USD en cuenta DEMO y espera el cierre por evento.
# Mueve saldo de práctica, nunca dinero real.
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "_env.ps1")

Write-Host ""
Write-Host "Vas a enviar una orden de 1.0 USD (cuenta DEMO, turbo 1 minuto)."
$respuesta = Read-Host "Continuar? [s/N]"
if ($respuesta -notmatch '^[sSyY]') {
    Write-Host "Cancelado."
    exit 0
}

cargo run --quiet --example ejemplo_orden_demo
exit $LASTEXITCODE
