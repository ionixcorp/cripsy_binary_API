#!/usr/bin/env pwsh
# Valida de punta a punta todas las capacidades documentadas de la API.
# Sin -SoloLectura también envía órdenes de 1 USD en cuenta DEMO.
param([switch]$SoloLectura)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "_env.ps1")

$argumentos = @()
if ($SoloLectura) { $argumentos += "--solo-lectura" }

Write-Host ""
Write-Host "Verificando la API contra el bróker (cuenta DEMO)..."
cargo run --quiet --example verificar_todo -- @argumentos
$codigo = $LASTEXITCODE

if ($codigo -eq 0) {
    Write-Host ""
    Write-Host "RESULTADO: la API funciona como esta documentada."
} else {
    Write-Host ""
    Write-Host "RESULTADO: hay capacidades que no respondieron (ver [FALLO] arriba)."
}
exit $codigo
