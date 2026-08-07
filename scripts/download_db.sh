#!/bin/bash
# GitHub Releaseから hellowork.db.gz をダウンロード
# DB_RELEASE_URL が設定されていればそこから、なければ最新Releaseから取得

set -e

DB_GZ="data/hellowork.db.gz"
DB_FILE="data/hellowork.db"

# 既存DBがあっても常に最新版をダウンロード
if [ -f "$DB_GZ" ]; then
    echo "Removing old DB gz: $DB_GZ ($(du -h "$DB_GZ" | cut -f1))"
    rm -f "$DB_GZ"
fi

# 解凍済みDBも削除（起動時に新しいgzから再解凍させる）
if [ -f "$DB_FILE" ]; then
    echo "Removing old decompressed DB: $DB_FILE ($(du -h "$DB_FILE" | cut -f1))"
    rm -f "$DB_FILE"
fi

# URL決定: 環境変数 > ビルド引数 > デフォルト（最新Release）
REPO="makimaki1006/HR_HR"
ASSET_NAME="hellowork.db.gz"
DOWNLOAD_ACCEPT=""

# GitHub API認証ヘッダー（レート制限回避）
AUTH_HEADER=""
if [ -n "$GITHUB_TOKEN" ]; then
    AUTH_HEADER="Authorization: token $GITHUB_TOKEN"
    echo "Using GITHUB_TOKEN for API authentication"
fi

# Common settings for transient Render/GitHub connection failures.
CURL_COMMON_ARGS=(
    --fail
    --silent
    --show-error
    --location
    --connect-timeout 30
)

fetch_release_info() {
    if [ -n "$AUTH_HEADER" ]; then
        curl "${CURL_COMMON_ARGS[@]}" -H "$AUTH_HEADER" \
            "https://api.github.com/repos/${REPO}/releases/latest"
    else
        curl "${CURL_COMMON_ARGS[@]}" \
            "https://api.github.com/repos/${REPO}/releases/latest"
    fi
}

if [ -n "$DB_RELEASE_URL" ]; then
    URL="$DB_RELEASE_URL"
    echo "Downloading DB from specified URL: $URL"
else
    # GitHub API で最新ReleaseのアセットURLを取得
    echo "Fetching latest release info from $REPO..."

    API_ATTEMPT=1
    while ! API_RESPONSE=$(fetch_release_info); do
        if [ "$API_ATTEMPT" -ge 5 ]; then
            echo "ERROR: Failed to fetch GitHub release info after ${API_ATTEMPT} attempts."
            exit 1
        fi
        echo "Release API attempt ${API_ATTEMPT} failed; retrying..."
        sleep $((API_ATTEMPT * 5))
        API_ATTEMPT=$((API_ATTEMPT + 1))
    done

    # レート制限チェック
    if echo "$API_RESPONSE" | grep -q "API rate limit exceeded"; then
        echo "WARNING: GitHub API rate limit exceeded, trying direct URL..."
        # フォールバック: 既知の最新タグで直接URL構築
        URL="https://github.com/${REPO}/releases/download/db-v2.0/${ASSET_NAME}"
        echo "Trying fallback URL: $URL"
    elif [ -n "$GITHUB_TOKEN" ]; then
        # 非公開リポジトリ (2026-08-07〜): browser_download_url は 404 になるため、
        # アセットのAPI URL (…/releases/assets/ID) + Accept: application/octet-stream で落とす
        ASSET_API_URL=$(echo "$API_RESPONSE" \
            | grep -B3 "\"name\": *\"${ASSET_NAME}\"" \
            | grep -o "https://api.github.com/repos/${REPO}/releases/assets/[0-9]*" \
            | head -1)
        if [ -z "$ASSET_API_URL" ]; then
            # 並び順の揺れに備えて逆方向でも探す
            ASSET_API_URL=$(echo "$API_RESPONSE" \
                | grep -o "https://api.github.com/repos/${REPO}/releases/assets/[0-9]*" \
                | head -1)
        fi
        if [ -z "$ASSET_API_URL" ]; then
            echo "ERROR: Could not find asset API url for $ASSET_NAME in latest release."
            echo "API response (first 500 chars): $(echo "$API_RESPONSE" | head -c 500)"
            exit 1
        fi
        URL="$ASSET_API_URL"
        DOWNLOAD_ACCEPT="application/octet-stream"
        echo "Downloading DB via authenticated asset API: $URL"
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

# ダウンロード（リダイレクト・途中再開対応）
# Resume the 310MB+ release asset after curl 56 or another transient failure.
DOWNLOAD_ARGS=(
    --fail
    --show-error
    --location
    --connect-timeout 30
    --continue-at -
    --output "$DB_GZ"
)

DOWNLOAD_ATTEMPT=1
while true; do
    if [ -n "$AUTH_HEADER" ] && [ -n "$DOWNLOAD_ACCEPT" ]; then
        curl "${DOWNLOAD_ARGS[@]}" -H "$AUTH_HEADER" -H "Accept: $DOWNLOAD_ACCEPT" "$URL" && break
    elif [ -n "$AUTH_HEADER" ]; then
        curl "${DOWNLOAD_ARGS[@]}" -H "$AUTH_HEADER" "$URL" && break
    else
        curl "${DOWNLOAD_ARGS[@]}" "$URL" && break
    fi

    if [ "$DOWNLOAD_ATTEMPT" -ge 8 ]; then
        echo "ERROR: DB download failed after ${DOWNLOAD_ATTEMPT} attempts."
        exit 1
    fi

    PARTIAL_BYTES=$(stat -c%s "$DB_GZ" 2>/dev/null || echo 0)
    echo "Download attempt ${DOWNLOAD_ATTEMPT} failed at ${PARTIAL_BYTES} bytes; resuming..."
    sleep $((DOWNLOAD_ATTEMPT * 5))
    DOWNLOAD_ATTEMPT=$((DOWNLOAD_ATTEMPT + 1))
done

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
