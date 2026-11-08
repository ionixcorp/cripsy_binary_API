#!/usr/bin/env pwsh
# Conecta al bróker y muestra sesión, balance, catálogo, velas y stream.
# Solo lectura: no envía órdenes.
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "_env.ps1")
cargo run --quiet --example ejemplo_lectura
exit $LASTEXITCODE
