#!/usr/bin/env pwsh
# Compuerta local completa: fmt + clippy + test + build (ambas variantes).
$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)

Write-Host "== cargo fmt --check =="
cargo fmt --check
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "== documentación navegable (build_docs) =="
pwsh -NoProfile -File (Join-Path $PSScriptRoot "build_docs.ps1") -Lang es
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "== cargo clippy (async, por defecto) =="
cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "== cargo test (async) =="
cargo test
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "== cargo build (variante sincrona/blocking) =="
cargo build --no-default-features --features blocking
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "== todo verde =="
