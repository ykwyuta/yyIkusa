$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    cargo build --locked --release --lib --target wasm32-unknown-unknown --config profile.release.debug=false
    if ($LASTEXITCODE -ne 0) { throw 'WebAssembly build failed' }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    foreach ($name in @('index.html','main.js','browser-recovery.js','manifest.webmanifest','ikusa-192.png','ikusa-512.png','ikusa-apple.png')) {
        Copy-Item -LiteralPath (Join-Path web $name) -Destination dist -Force
    }
    Copy-Item -LiteralPath assets/branding/ikusa.ico -Destination dist/favicon.ico -Force
    Copy-Item -LiteralPath target/wasm32-unknown-unknown/release/yyikusa.wasm -Destination dist -Force
    Set-Content -LiteralPath dist/_headers -Encoding utf8 -Value "/*`n  Cache-Control: no-cache`n/yyikusa.wasm`n  Content-Type: application/wasm`n  Cache-Control: no-cache"
    foreach ($file in Get-ChildItem -LiteralPath dist -File) {
        if ($file.Length -gt 25MB) { throw "Static asset exceeds 25 MiB: $($file.Name)" }
    }
} finally { Pop-Location }
