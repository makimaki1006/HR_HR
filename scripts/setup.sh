#!/usr/bin/env bash
# 初回開発者向けセットアップスクリプト
set -e

echo "=== HR_HR Setup ==="

# Rust確認
if ! command -v cargo &> /dev/null; then
    echo "ERROR: Rustが未インストール。https://rustup.rs/ から"
    exit 1
fi

# Python + Playwright
if ! command -v python &> /dev/null; then
    echo "ERROR: Python未インストール"
    exit 1
fi

pip install -q playwright openpyxl pypdf
python -m playwright install chromium

# DBファイル確認
if [ ! -f "data/hellowork.db" ]; then
    echo "WARN: data/hellowork.db が存在しません"
    if [ -f "data/hellowork.db.gz" ]; then
        echo "展開中..."
        gunzip -k data/hellowork.db.gz
    fi
fi

# ビルド
cargo build

echo "=== Setup完了 ==="
echo "起動: cargo run"
echo "テスト: cargo test"
echo "E2E: python e2e_final_verification.py"
