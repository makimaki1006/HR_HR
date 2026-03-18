#!/bin/bash
# GitHub Releaseから hellowork.db.gz をダウンロード
# DB_RELEASE_URL が設定されていればそこから、なければ最新Releaseから取得

set -e

DB_GZ="data/hellowork.db.gz"

# 既存DBがあっても常に最新版をダウンロード
if [ -f "$DB_GZ" ]; then
    echo "Removing old DB: $DB_GZ ($(du -h "$DB_GZ" | cut -f1))"
    rm -f "$DB_GZ"
fi

# URL決定: 環境変数 > ビルド引数 > デフォルト（最新Release）
REPO="makimaki1006/HR_HR"
ASSET_NAME="hellowork.db.gz"

# GitHub API認証ヘッダー（レート制限回避）
AUTH_HEADER=""
if [ -n "$GITHUB_TOKEN" ]; then
    AUTH_HEADER="Authorization: token $GITHUB_TOKEN"
    echo "Using GITHUB_TOKEN for API authentication"
fi

if [ -n "$DB_RELEASE_URL" ]; then
    URL="$DB_RELEASE_URL"
    echo "Downloading DB from specified URL: $URL"
else
    # GitHub API で最新ReleaseのアセットURLを取得
    echo "Fetching latest release info from $REPO..."

    if [ -n "$AUTH_HEADER" ]; then
        API_RESPONSE=$(curl -sL -H "$AUTH_HEADER" \
            "https://api.github.com/repos/${REPO}/releases/latest")
    else
        API_RESPONSE=$(curl -sL \
            "https://api.github.com/repos/${REPO}/releases/latest")
    fi

    # レート制限チェック
    if echo "$API_RESPONSE" | grep -q "API rate limit exceeded"; then
        echo "WARNING: GitHub API rate limit exceeded, trying direct URL..."
        # フォールバック: 既知の最新タグで直接URL構築
        URL="https://github.com/${REPO}/releases/download/db-v2.0/${ASSET_NAME}"
        echo "Trying fallback URL: $URL"
    else
        RELEASE_URL=$(echo "$API_RESPONSE" \
            | grep -o "https://github.com/${REPO}/releases/download/[^\"]*${ASSET_NAME}" \
            | head -1)

        if [ -z "$RELEASE_URL" ]; then
            echo "WARNING: Could not find $ASSET_NAME in latest release, trying fallback..."
            echo "API response (first 500 chars): $(echo "$API_RESPONSE" | head -c 500)"
            # フォールバック: 直接URL
            URL="https://github.com/${REPO}/releases/download/db-v2.0/${ASSET_NAME}"
            echo "Trying fallback URL: $URL"
        else
            URL="$RELEASE_URL"
            echo "Downloading DB from latest release: $URL"
        fi
    fi
fi

# ダウンロード（リダイレクト対応、リトライ3回）
if [ -n "$AUTH_HEADER" ]; then
    curl -L -H "$AUTH_HEADER" --retry 3 --retry-delay 5 -o "$DB_GZ" "$URL"
else
    curl -L --retry 3 --retry-delay 5 -o "$DB_GZ" "$URL"
fi

# サイズ確認
SIZE=$(du -h "$DB_GZ" | cut -f1)
echo "Downloaded: $DB_GZ ($SIZE)"

# 最低限のサイズチェック（10MB未満なら失敗とみなす）
BYTES=$(stat -c%s "$DB_GZ" 2>/dev/null || stat -f%z "$DB_GZ" 2>/dev/null || echo 0)
if [ "$BYTES" -lt 10000000 ]; then
    echo "ERROR: Downloaded file is too small (${BYTES} bytes). Download may have failed."
    rm -f "$DB_GZ"
    exit 1
fi

echo "DB download complete."
