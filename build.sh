#!/usr/bin/env bash
# WebAssembly 版をビルドして dist/ に静的サイトを出力する。
# Cloudflare Pages ではビルドコマンドに `bash build.sh`、出力ディレクトリに `dist` を指定する。
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
  else
    echo "Rust が無いので rustup で導入する"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    . "$HOME/.cargo/env"
  fi
fi
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

# ブラウザ版はゲーム本体（lib）だけを、デバッグ情報なしで作る（Windows 版の設定とは別）
cargo build --locked --release --lib --target wasm32-unknown-unknown --config profile.release.debug=false

rm -rf dist
mkdir -p dist
cp web/index.html web/main.js web/browser-recovery.js web/manifest.webmanifest web/ikusa-192.png web/ikusa-512.png web/ikusa-apple.png web/ikusa-cover.jpg dist/
cp assets/branding/ikusa.ico dist/favicon.ico
cp target/wasm32-unknown-unknown/release/yyikusa.wasm dist/
# wasm-opt があればさらに小さくする（任意）
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -O3 dist/yyikusa.wasm -o dist/yyikusa.wasm || true
fi
cat > dist/_headers <<'H'
/*
  Cache-Control: no-cache
/yyikusa.wasm
  Content-Type: application/wasm
  Cache-Control: no-cache
H
# Workers Static Assets / Pages 上限は1ファイル25 MiB。
for file in dist/*; do
  if [ "$(wc -c < "$file")" -gt $((25 * 1024 * 1024)) ]; then
    echo "Cloudflare static asset exceeds 25 MiB: $file" >&2
    exit 1
  fi
done
ls -la dist
