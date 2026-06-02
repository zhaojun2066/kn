#!/usr/bin/env bash
# Build AI Profile Manager desktop app
set -e
cd "$(dirname "$0")"

echo "==> Building AI Profile Manager Desktop"

# Install frontend deps
echo "==> Installing npm dependencies..."
npm ci --silent

# Build frontend
echo "==> Building frontend..."
npm run build

# Build Tauri
echo "==> Building Tauri backend..."
cd src-tauri
cargo build --release

echo ""
echo "==> Bundling application..."
cargo tauri build 2>&1 | tail -20

echo ""
echo "==> Build complete!"
echo "Check src-tauri/target/release/bundle/ for packages."
