#!/usr/bin/env pwsh
# Verificación de entorno de cripsy_binary_API (sin instalar nada).
$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)

function Comprobar($nombre, $valor) {
    if ($valor) {
        Write-Host "  [OK]   $nombre"
        return $true
    }
    Write-Host "  [FALTA] $nombre"
    return $false
}

Write-Host "== Entorno de cripsy_binary_API =="
$ok = $true
$ok = (Comprobar "Rust (rustup)" (Get-Command rustup -ErrorAction SilentlyContinue)) -and $ok
$ok = (Comprobar "cargo" (Get-Command cargo -ErrorAction SilentlyContinue)) -and $ok
$ok = (Comprobar "git" (Get-Command git -ErrorAction SilentlyContinue)) -and $ok

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $version = (& cargo --version) 2>$null
    Write-Host "  [info] $version"
    # rustc 1.86 mínimo
    $rustc = (& rustc --version) 2>$null
    if ($rustc -match "1\.(8[6-9]|9\d+|\d\d\d+)") {
        Write-Host "  [OK]   $rustc (>= 1.86)"
    } else {
        Write-Host "  [AVISO] $rustc — se recomienda Rust 1.86+"
    }
}

$env_file = Join-Path (Get-Location) ".env"
if (Test-Path -LiteralPath $env_file) {
    Write-Host "  [OK]   .env presente"
} else {
    Write-Host "  [FALTA] .env (cópialo de .env.example)"
    $ok = $false
}

Write-Host ""
if ($ok) {
    Write-Host "Entorno listo."
    exit 0
}
Write-Host "Entorno incompleto. Ejecuta .\installation\install.ps1 para resolverlo."
exit 1
