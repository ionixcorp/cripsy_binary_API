#!/usr/bin/env pwsh
# Genera la documentación navegable (Markdown → single-file HTML).
param([string]$Lang = "es")
$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
cargo run --quiet --example build_docs -- --lang $Lang --out docs/site
