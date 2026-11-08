#!/usr/bin/env pwsh
# Muestra el balance, el tipo de cuenta y el balance_id de la sesión.
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "_env.ps1")
cargo run --quiet --example estado_cuenta
exit $LASTEXITCODE
