# ===== ビルドステージ =====
FROM rust:latest AS builder

# ビルドに必要なシステムライブラリ
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# ソースコード + テンプレート（include_str!がコンパイル時に参照）+ Cargo.lock（依存バージョン固定）
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY templates/ templates/
RUN cargo build --release

# ===== ランタイムステージ =====
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# バイナリ
COPY --from=builder /app/target/release/rust_dashboard .

# テンプレート（ランタイムでは不要だがディレクトリ構造維持）
COPY templates/ templates/

# 静的ファイル（CSS/JS/ガイド画像）※GeoJSONは除外、gzから起動時解凍
COPY static/css/ static/css/
COPY static/js/ static/js/
COPY static/guide/ static/guide/

# 圧縮データ（起動時に自動解凍）
COPY data/geojson_gz/ data/geojson_gz/

# DB: GitHub Releaseからダウンロード（Git LFS不要）
# DB_VERSION を変更するとDockerキャッシュが無効化され、最新DBがダウンロードされる
ARG DB_VERSION="2.2-pyramid9-force"
ARG DB_RELEASE_URL=""
ARG GITHUB_TOKEN=""
ENV GITHUB_TOKEN=${GITHUB_TOKEN}
COPY scripts/download_db.sh scripts/download_db.sh
RUN chmod +x scripts/download_db.sh && \
    mkdir -p data && \
    echo "DB_VERSION=${DB_VERSION}" && \
    scripts/download_db.sh
# トークンをイメージに残さない
ENV GITHUB_TOKEN=""

EXPOSE 9216

ENV RUST_LOG=info

CMD ["./rust_dashboard"]
