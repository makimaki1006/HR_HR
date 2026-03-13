#!/bin/bash
# GitHub Releaseから hellowork.db.gz をダウンロード
# DB_RELEASE_URL が設定されていればそこから、なければ最新Releaseから取得

set -e

DB_GZ="data/hellowork.db.gz"

# 既にDBが存在する場合はスキップ
if [ -f "$DB_GZ" ]; then
    echo "DB already exists: $DB_GZ ($(du -h "$DB_GZ" | cut -f1))"
    exit 0
fi

# URL決定: 環境変数 > ビルド引数 > デフォルト（最新Release）
REPO="makimaki1006/HR_HR"
ASSET_NAME="hellowork.db.gz"

if [ -n "$DB_RELEASE_URL" ]; then
    URL="$DB_RELEASE_URL"
    echo "Downloading DB from specified URL: $URL"
else
    # GitHub API で最新ReleaseのアセットURLを取得
    echo "Fetching latest release info from $REPO..."
    RELEASE_URL=$(curl -sL \
        "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep -o "https://github.com/${REPO}/releases/download/[^\"]*${ASSET_NAME}" \
        | head -1)

    if [ -z "$RELEASE_URL" ]; then
        echo "ERROR: Could not find $ASSET_NAME in latest release of $REPO"
        echo "Please set DB_RELEASE_URL or create a release with the DB file"
        exit 1
    fi
    URL="$RELEASE_URL"
    echo "Downloading DB from latest release: $URL"
fi

# ダウンロード（リダイレクト対応、リトライ3回）
curl -L --retry 3 --retry-delay 5 -o "$DB_GZ" "$URL"

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
