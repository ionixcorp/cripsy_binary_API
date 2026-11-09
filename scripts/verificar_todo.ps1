#!/usr/bin/env pwsh
# Compuerta viva completa: IQ Option y Deriv, en secuencia.
# Sin -SoloLectura tambien envia ordenes de 1 USD en cuentas DEMO.
param([switch]$SoloLectura)

$ErrorActionPreference = "Stop"

Write-Host "===== IQ Option ====="
& (Join-Path $PSScriptRoot "iqoption\verificar_iqoption.ps1") -SoloLectura:$SoloLectura
$iq = $LASTEXITCODE

Write-Host ""
Write-Host "===== Deriv ====="
& (Join-Path $PSScriptRoot "deriv\verificar_deriv.ps1") -SoloLectura:$SoloLectura
$deriv = $LASTEXITCODE

if ($iq -eq 0 -and $deriv -eq 0) {
    Write-Host ""
    Write-Host "RESULTADO: ambos brokers responden como esta documentado."
    exit 0
}
Write-Host ""
Write-Host "RESULTADO: hay fallos (IQ Option=$iq, Deriv=$deriv)."
exit 1
