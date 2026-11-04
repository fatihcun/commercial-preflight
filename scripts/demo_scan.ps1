$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
& cargo run --locked -- scan `
  tests/fixtures/Project_Orion_Quote_v0.1.xlsx `
  --policy config/company_policy_orion.json `
  --as-of 2026-10-30
exit $LASTEXITCODE
