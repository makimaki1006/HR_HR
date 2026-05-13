# J 領域監査: 運用 / デプロイ (hellowork-deploy)

**監査日**: 2026-05-13
**対象**: `.github/workflows/`, `Dockerfile`, `render.yaml`, `scripts/`, `src/main.rs`, `src/config.rs`, `README.md`, `docs/runbook*`
**手法**: read-only static review。ビルド/デプロイ操作なし。

---

## サマリー (TL;DR)

CI/CD 基盤は最低限整っているが、**fail-fast 不在による silent degradation** が最大のリスク。本番 secret (`TURSO_EXTERNAL_TOKEN`, `SALESNOW_TURSO_TOKEN`) が `.env` (gitignore 済・未 track) に平文存在し、リポ内に残存している（公開履歴露出は確認できないが現在ワークツリーに JWT が記載）。Render `healthCheckPath: /health` は実装されているが、外部 Turso 未接続でも `degraded` を返すだけで Render は healthy 扱いし続ける。CI は `cargo test --lib` のみで E2E は nightly cron のみ＝ main push 時の回帰検知が欠落。Dockerfile は `cargo build --release` をビルダーで完結させており Windows の exe ロック問題は無関係。rollback 手順は `docs/POST_RELEASE_MONITORING_CHECKLIST.md` に存在するが、Render 側 manual rollback の具体操作手順記述なし。

---

## P0 (本番 deploy 失敗・secret 漏出リスク)

### J-P0-1: 本番 secret が `.env` に平文格納（git 未 track だがワークツリー残存）
**証拠**: `.env:5,8` に Turso JWT (`TURSO_EXTERNAL_TOKEN`, `SALESNOW_TURSO_TOKEN`) が平文。`.gitignore:6` で除外、`git ls-files .env` で未 track 確認済。
**リスク**: 開発機が侵害されればトークン即漏出。Render 上は env vars (`sync: false`) なので本番直接ではないが、共有/バックアップ経路で露出可能性。D 領域 (secret rotate 未実施) と同根。
**推奨**: `.env` 内の token を直ちに rotate。`.env.example:2-3` のように placeholder のみワークツリー残置。

### J-P0-2: 起動時 fail-fast 不在 → 致命的 misconfig で silent start
**証拠**: `src/config.rs:67,97,125-128` 全 env が `unwrap_or_default()`。`src/main.rs:38-77` HW DB 読み込み失敗時も `None` で続行 (`tracing::warn!("HelloWork DB not available")`).
`src/lib.rs:921-942` `/health` は `db_connected:false` でも HTTP 200 を返す。`render.yaml:7` の `healthCheckPath: /health` は 200 で healthy と判定するため、DB ロード失敗状態が長時間放置され得る。
**リスク**: `download_db.sh` 失敗 → ビルド継続 → 起動 → `/health` 200 (degraded) → Render は healthy → 全機能 404/空応答。
**推奨**: `/health` で `db_connected:false` の場合 HTTP 503 を返却。`AUTH_PASSWORD` 未設定時は起動拒否。

---

## P1 (rollback 手順不在 / CI test 抜け)

### J-P1-1: CI が unit test のみ、E2E は nightly cron のみ
**証拠**: `.github/workflows/ci.yml:23` `cargo test --lib` のみ。E2E (`e2e_*.py`, `tests/e2e/*.spec.ts`) は `regression.yml:4-6` の `cron: "0 0 * * *"` で 24h 周期。main push → auto deploy (README:74) との間に E2E gate なし。
**リスク**: 朝の push で本番が壊れても翌 09:00 JST まで検知遅延。`feedback_e2e_chart_verification.md` (19/24 ブランク見逃し) の再発条件成立。
**推奨**: ci.yml に `npx playwright test tests/e2e/regression_2026_04_26.spec.ts` を追加 (BASE_URL=localhost のスモークのみ)。

### J-P1-2: CI security-audit が `continue-on-error: true`
**証拠**: `.github/workflows/ci.yml:31`. 既知脆弱性発見してもジョブ赤化しない設計。
**リスク**: 依存 CVE が放置される。
**推奨**: `.cargo/audit.toml` ignore リストを明示化し、ignore 外脆弱性は fail。

### J-P1-3: Render rollback 具体手順の記載不足
**証拠**: `docs/POST_RELEASE_MONITORING_CHECKLIST.md:87-99` に「rollback 判断基準」「ユーザー承認後実施」は記載があるが、Render ダッシュボードでの具体操作 (Manual Deploy → Previous Deploy 選択) や DB 側 rollback (download_db.sh の `DB_VERSION`/`DB_RELEASE_URL` 切替) の手順書なし。
**リスク**: 障害時に当番が手探り。MTTR 増大。
**推奨**: `docs/RUNBOOK_ROLLBACK.md` 新設。Render rollback、`DB_VERSION` 巻き戻し、Turso スキーマ ALTER 戻しを章立て。

### J-P1-4: `download_db.sh` のフォールバック URL ハードコード
**証拠**: `scripts/download_db.sh:52,63` `db-v2.0` タグ固定。`Dockerfile:46` の `ARG DB_VERSION="2.2-pyramid9-force"` と不一致。
**リスク**: GitHub API rate limit 時に古い v2.0 DB をダウンロードして起動 → データ古い状態で本番稼働。検知困難。
**推奨**: フォールバック URL を `DB_VERSION` から動的構築、もしくは fallback 失敗時は build fail。

---

## P2 (健全性)

### J-P2-1: `.env.example` と実 `.env` の乖離
**証拠**: `.env.example:6` `LOCAL_DB_PATH=data/job_postings_minimal.db` (旧名)。実コードは `HELLOWORK_DB_PATH` (`src/config.rs:97`, `.env:3`)。`SALESNOW_*`, `TURSO_EXTERNAL_*` は `.env.example` に存在しない。
**推奨**: `.env.example` を `docs/env_variables_reference.md` (README:48) と同期。19 個全列挙。

### J-P2-2: `/health` で外部 DB (Turso/SalesNow/Audit) の状態を返さない
**証拠**: `src/lib.rs:924-941` `state.hw_db` のみチェック。`turso_db`, `salesnow_db`, `audit` は無視。
**リスク**: Turso 接続切れで企業検索/外部統計が空応答でも `/health` healthy。

### J-P2-3: Render free plan の cold start に対する起動時間の警鐘なし
**証拠**: `src/main.rs:34-36` 起動時に `decompress_geojson_if_needed` + `precompress_geojson` + DB 解凍を同期実行。`feedback_render_cold_start_timeout.md` で navigationTimeout 60s 設定済だが、Dockerfile の `data/geojson_gz/` (Dockerfile:42) サイズ次第で 60s 超過リスク。
**推奨**: `tracing::info!` で各段階の elapsed を出力し、cold start ベンチマークを `docs/` に記録。

### J-P2-4: `regression.yml` で E2E_EMAIL/E2E_PASS 未設定時に `exit 0`
**証拠**: `.github/workflows/regression.yml:58-61`. silent skip でテスト未実行が success 扱い。
**推奨**: `exit 1` または `::error::` 出力で notify。

### J-P2-5: build と稼働 exe ロック競合 (Windows dev) の対策記載なし
**証拠**: `feedback_release_build_exe_lock.md` 該当。Dockerfile (Linux) では非問題だが、開発者向け README.md:41 の `cargo build --release` 実行前に `taskkill` 等の注意書きなし。
**推奨**: README に `os error 5` 検知 grep tip 追記。

### J-P2-6: `RUST_LOG=info` 固定、本番調査時の動的切り替え手順なし
**証拠**: `render.yaml:14`, `Dockerfile:60`. Render env vars で上書き可能だが手順書なし。
**推奨**: runbook に `RUST_LOG=debug` 一時切替手順記載。

---

## 良好事項

- `.env` が gitignore 済 (`.gitignore:6`) かつ git 未 track。
- Dockerfile マルチステージ構成、`GITHUB_TOKEN` をビルド後に空文字化 (`Dockerfile:56`)。
- `download_db.sh` にサイズ検証 (10MB 未満で fail, `scripts/download_db.sh:84-89`)。
- Slack 通知の `SLACK_WEBHOOK_URL` 未設定時の graceful skip。
- 監査ログ自動 purge (`src/main.rs:213-220`)。
- Render `healthCheckPath: /health` 設定済。

---

## scope と限界

- 検査範囲: 設定ファイル / shell script / main.rs エントリポイント / CI workflow / README。
- 未検査: Render ダッシュボード設定値の実際の状態、GitHub Actions secrets の現在値、過去 deploy 履歴。
- 未確認: `docs/env_variables_reference.md` 実在性 (README に記載のみ)、`scripts/setup.sh` の内容。

## 推奨優先順位

1. J-P0-1 token rotate (即時)
2. J-P0-2 `/health` 503 化と起動時 fail-fast (1 日)
3. J-P1-3 rollback runbook (1 週)
4. J-P1-1 CI への E2E smoke 追加 (1 週)
5. J-P1-4 download_db.sh フォールバック修正 (任意のタイミング)
