# ===== フロントエンド (React) ビルドステージ (Phase 0-5, 2026-09-29) =====
# frontend/ を Vite でビルドし、成果物 (static/app/ = ハッシュ付き JS/CSS + .vite/manifest.json)
# だけをランタイムに渡す。Node 本体と node_modules はランタイムに入らない。
# Rust の builder とは依存が無いので、BuildKit なら並列に走る。
# node のメジャーは CI (.github/workflows/ci.yml の frontend ジョブ) の 22 と揃える。
# vite.config.ts の outDir は '../static/app' なので、リポジトリと同じ並び
# (/src/frontend → /src/static/app) で置く。
FROM node:22-bookworm-slim AS web
WORKDIR /src/frontend
# package*.json が変わらない限り npm ci の層はキャッシュに乗る
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build \
    && test -f /src/static/app/.vite/manifest.json

# ===== ビルドステージ =====
# ベースを固定する理由が 2 つある。
#  1) glibc: rust:latest は Debian 13 (trixie/glibc 2.41) に上がっており、
#     ランタイムの debian:bookworm-slim (glibc 2.36) より新しい。新しい glibc で
#     リンクしたバイナリは古い glibc では起動しない可能性がある。bookworm 版の
#     rust イメージを使ってランタイムと glibc を揃える。
#  2) キャッシュ: latest はタグの中身が動くので、動いた瞬間に下の依存ビルド層まで
#     まるごと無効化される。バージョンを固定するとそれが起きない。
FROM node:22-bookworm-slim AS pdf
WORKDIR /app
COPY scripts/pdf/package.json scripts/pdf/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts

FROM rust:1.98-slim-bookworm AS builder

# ビルドに必要なシステムライブラリ
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# ---- 依存クレートだけを先にビルドする層 ----
# Cargo.toml / Cargo.lock が変わらない限り、この RUN はキャッシュに乗る。
# ソースを 1 文字直しただけで 312 個の依存クレートを再ビルドしないための層。
#
# 仕組み: 中身が空の src/main.rs と src/lib.rs を置いて依存だけコンパイルする。
#   - src/bin/ を置かないので、cargo のターゲット自動検出には何も引っかからない
#   - ビルド対象を --bin rust_dashboard に固定しているのでダミーはこの 2 つで足りる
#     (cargo-chef を使わずに済むのはこのため。複数バイナリを全部ビルドするなら
#      ダミーも全部用意する必要があり、その場合は cargo-chef の方が向く)
#   - 最後の cargo clean -p はダミー本体の成果物だけを捨てる。依存 312 個は残る。
#     これをやらないと、COPY したソースの mtime がダミーのビルド時刻より古い場合に
#     cargo が「変更なし」と誤判定し、空のダミーバイナリをそのまま出荷してしまう。
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && : > src/lib.rs \
    && cargo build --release --locked --bin rust_dashboard \
    && cargo clean --release -p rust_dashboard \
    && rm -rf src

# ---- アプリ本体 ----
# ソースコード + テンプレート（include_str!がコンパイル時に参照）
COPY src/ src/
# HRハッカー本文の列順をフロントと共有（include_str! の入力）
COPY frontend/src/screens/job-copy/hrhCopyColumns.json frontend/src/screens/job-copy/hrhCopyColumns.json
COPY templates/ templates/
# include_str! で driver/data.rs が wage_census → 国勢調査中分類のマッピングを参照
COPY data/wage_census_to_occupation_middle_map.json data/wage_census_to_occupation_middle_map.json
COPY data/minimum_wage_rates.csv data/minimum_wage_rates.csv
# include_str! で media_engine/handlers.rs がキーワード需要ビューアの UI を埋め込む
COPY static/keywords.html static/keywords.html
# include_str! で job_gen が求人票生成 UI と知識データ (NGワード/職種知識) を埋め込む
COPY static/jobgen.html static/jobgen.html
COPY static/jobgen_competitive_beta.html static/jobgen_competitive_beta.html
COPY static/jobgen_applicant_journey_beta.html static/jobgen_applicant_journey_beta.html
COPY assets/ assets/
# include_str! で採用提案の試作モックページを埋め込む
COPY static/proposal_mock.html static/proposal_mock.html
# include_str! で競合調査ダッシュボードのCSS・タブ操作を埋め込む
COPY static/css/competitor-dashboard.css static/css/competitor-dashboard.css
COPY static/js/competitor-tabs.js static/js/competitor-tabs.js
# 競合調査のキーワード図の再描画 (competitor_report.rs が include_str! で埋め込む。2026-10-06 #79 で追加。
# ここに無いと Render のビルドだけが「ファイルが無い」で失敗する。ローカルと CI はリポジトリ全体があるので通る)
COPY static/js/competitor-keywords.js static/js/competitor-keywords.js
COPY scripts/pdf/render.cjs scripts/pdf/render.cjs

# touch は上の cargo clean と同じ目的の二重の保険。COPY はコンテキスト側の mtime を
# そのまま持ち込むため、キャッシュ層のビルド時刻より古いソースが来ることがある。
#
# ランタイムに載せるのは rust_dashboard 1 本だけ。--bin を付けないと
# src/bin/ の 10 本 (probe_* / gen_vrt_fixtures 等) も一緒にビルド・リンクされる。
RUN touch src/main.rs src/lib.rs \
    && cargo build --release --locked --bin rust_dashboard

# ===== ランタイムステージ =====
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    chromium \
    fonts-noto-cjk \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=pdf /usr/local/bin/node /usr/local/bin/node
COPY --from=pdf /app/node_modules/playwright-core /app/node_modules/playwright-core
# Playwright は executablePath に絶対パスが要る (PATH を引かない)。起動に使うパスが実在することをビルド時に確かめる
ENV PDF_CHROMIUM_PATH=/usr/bin/chromium
RUN node --version && "$PDF_CHROMIUM_PATH" --version

# バイナリ
COPY --from=builder /app/target/release/rust_dashboard .

# テンプレート（ランタイムでは不要だがディレクトリ構造維持）
COPY templates/ templates/

# 静的ファイル（CSS/JS/ガイド画像）※GeoJSONは除外、gzから起動時解凍
COPY static/css/ static/css/
COPY static/js/ static/js/
COPY static/guide/ static/guide/

# React 画面のビルド成果物。/app/{screen} が起動時に static/app/.vite/manifest.json を読む。
# 無いと /app/* は「フロントエンド未ビルド」の注記を返す (起動は止まらない)。
COPY --from=web /src/static/app/ static/app/

# 圧縮データ（起動時に自動解凍）
COPY data/geojson_gz/ data/geojson_gz/

# キーワード需要ビューア: /api/regions が実行時に読む市区町村重心CSV
COPY data/media_engine/ data/media_engine/

# Indeed 採用市場データ（分析層・gz 5.4MB）。起動時に data/indeed_insights.db へ展開する。
COPY data/indeed_insights.db.gz data/indeed_insights.db.gz

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
