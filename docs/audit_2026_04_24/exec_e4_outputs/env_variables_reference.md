# 環境変数 完全リファレンス (19 個)

**最終更新**: 2026-04-26
**対象範囲**: V2 ハローワークダッシュボードで使用される全環境変数 (config.rs 15 + main.rs 4)
**根拠**: `src/config.rs:52-108`、`src/main.rs:83-145`、`src/lib.rs:39`
**マスター**: ルート [`CLAUDE.md`](../CLAUDE.md) §8

---

## 0. 種別サマリ

| 種別 | 数 | 説明 |
|------|---|------|
| config.rs (`AppConfig::from_env`) | 15 | 統合管理、テスト容易、デフォルト値あり |
| main.rs 直接読出 (🔴 統合違反) | 4 | Turso 系。P0 #4 で config.rs に統合予定 |
| `src/lib.rs` 内ハードコード定数 | 1 | `UPLOAD_BODY_LIMIT_BYTES` (env 化候補) |

---

## 1. config.rs 管理 (15 個)

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照行 |
|---|------|----------|------|-------------|--------|
| 1 | `PORT` | `9216` | HTTP リッスンポート | デフォルト使用 | `config.rs:52-55` |
| 2 | `AUTH_PASSWORD` | `""` | 平文パスワード (社内・無期限) | 認証 OFF (`auth_password.is_empty() && auth_password_hash.is_empty()` 時) | `config.rs:56` |
| 3 | `AUTH_PASSWORD_HASH` | `""` | bcrypt ハッシュ (社内・無期限、Cargo.toml `bcrypt = "0.16"`) | 同上 | `config.rs:57` |
| 4 | `AUTH_PASSWORDS_EXTRA` | `""` | 外部期限付きパスワード `pass1:2026-06-30,pass2:2026-12-31` 形式 | 外部認証なし | `config.rs:58` |
| 5 | `ALLOWED_DOMAINS` | `f-a-c.co.jp,cyxen.co.jp` | 社内ドメイン (カンマ区切り) | デフォルト 2 ドメイン | `config.rs:75` |
| 6 | `ALLOWED_DOMAINS_EXTRA` | `""` | 外部追加ドメイン | 追加なし | `config.rs:80` |
| 7 | `HELLOWORK_DB_PATH` | `data/hellowork.db` | SQLite ファイルパス | デフォルト | `config.rs:86` |
| 8 | `CACHE_TTL_SECS` | `1800` (30 分) | DashMap TTL | デフォルト | `config.rs:88` |
| 9 | `CACHE_MAX_ENTRIES` | `3000` | DashMap 最大エントリ | デフォルト | `config.rs:92` |
| 10 | `RATE_LIMIT_MAX_ATTEMPTS` | `5` | ログイン失敗上限 | デフォルト | `config.rs:96` |
| 11 | `RATE_LIMIT_LOCKOUT_SECONDS` | `300` (5 分) | ロックアウト秒数 | デフォルト | `config.rs:100` |
| 12 | `AUDIT_TURSO_URL` | `""` | 監査 DB URL | 監査機能 OFF (`/admin/*` 403、活動記録 OFF) | `config.rs:104` |
| 13 | `AUDIT_TURSO_TOKEN` | `""` | 監査 DB トークン | 同上 | `config.rs:105` |
| 14 | `AUDIT_IP_SALT` | `hellowork-default-salt` | IP ハッシュ用 salt | ⚠ デフォルトのままだとレインボーテーブル攻撃容易、本番では必須変更 | `config.rs:106` |
| 15 | `ADMIN_EMAILS` | `""` | 管理者メール (カンマ区切り) | role=admin 付与なし | `config.rs:108` |

---

## 2. main.rs 直接読出 (4 個、🔴 config.rs 統合違反、P0 #4)

| # | 変数 | 用途 | 未設定時影響 | 参照行 |
|---|------|------|-------------|--------|
| 16 | `TURSO_EXTERNAL_URL` | country-statistics URL | 外部統計タブ全機能 OFF (詳細分析 / 地域カルテ / 採用診断 / 媒体分析の HW 統合 / 一部 insight) | `main.rs:83` |
| 17 | `TURSO_EXTERNAL_TOKEN` | country-statistics トークン | 同上 | `main.rs:84` |
| 18 | `SALESNOW_TURSO_URL` | SalesNow URL | 企業検索タブ機能 OFF + 採用診断 Panel 4 (競合) + 地図 labor-flow / company-markers が空応答 | `main.rs:113, 125` |
| 19 | `SALESNOW_TURSO_TOKEN` | SalesNow トークン | 同上 | `main.rs:114` |

🔴 **修正対象**: `team_delta_codehealth.md §4.2` 推奨どおり、`AppConfig` に `turso_external_url/token`, `salesnow_turso_url/token` を追加し、`from_env()` で一括検証。テスト容易性向上 + 未設定時警告ログを追加。

---

## 3. ハードコード定数

### 3.1 `UPLOAD_BODY_LIMIT_BYTES`

`src/lib.rs:39`:
```rust
pub const UPLOAD_BODY_LIMIT_BYTES: usize = 20 * 1024 * 1024; // 20MB
```

`/api/survey/upload` のみ適用。20MB 超は 413 即拒否。env 化候補 (P2)。

---

## 4. 設定パターン早見表

### 4.1 ローカル開発 (認証 OFF、Turso なし)

```bash
# 必須なし、デフォルト値で起動
cargo run
# → http://localhost:9216
# → 認証 OFF (未推奨だが起動可能)
# → Turso 系全機能 OFF (詳細分析・地域カルテ・採用診断 等は空応答)
```

### 4.2 ローカル開発 (認証 ON、Turso 接続)

```bash
# Windows PowerShell
$env:AUTH_PASSWORD = "dev-password"
$env:TURSO_EXTERNAL_URL = "libsql://country-statistics-xxx.turso.io"
$env:TURSO_EXTERNAL_TOKEN = "..."
$env:SALESNOW_TURSO_URL = "libsql://salesnow-xxx.turso.io"
$env:SALESNOW_TURSO_TOKEN = "..."
$env:AUDIT_TURSO_URL = "libsql://audit-xxx.turso.io"
$env:AUDIT_TURSO_TOKEN = "..."
$env:AUDIT_IP_SALT = "$(uuidgen)"   # ⚠ 本番危険デフォルトを必ず変更
$env:ADMIN_EMAILS = "admin@example.com"
cargo run
```

### 4.3 本番 (Render Free)

| 設定 | 値 |
|------|-----|
| `PORT` | (Render が自動設定、9216 のまま) |
| `AUTH_PASSWORD_HASH` | bcrypt ハッシュ (sync:false) |
| `AUTH_PASSWORDS_EXTRA` | `clientA:2026-06-30,clientB:2026-12-31` |
| `ALLOWED_DOMAINS` | `f-a-c.co.jp,cyxen.co.jp` (デフォルト) |
| `TURSO_EXTERNAL_URL` / `_TOKEN` | (sync:false) |
| `SALESNOW_TURSO_URL` / `_TOKEN` | (sync:false) |
| `AUDIT_TURSO_URL` / `_TOKEN` | (sync:false) |
| `AUDIT_IP_SALT` | UUID 生成 (sync:false) |
| `ADMIN_EMAILS` | 管理者メール |

⚠ Docker Build Argument: `GITHUB_TOKEN` (download_db.sh のレート制限回避)

---

## 5. 検証チェックリスト

起動ログで以下を確認:
```
[INFO] AppConfig loaded: port=9216, ...
[INFO] Local DB connected: data/hellowork.db (469027 rows)
[INFO] Turso country-statistics: connected
[INFO] Turso salesnow: connected
[INFO] Turso audit: connected
```

未設定時の warning 例:
```
[WARN] TURSO_EXTERNAL_URL not set; external statistics tabs will return empty
[WARN] SALESNOW_TURSO_URL not set; company search and recruitment_diag panel 4 will be empty
[WARN] AUDIT_TURSO_URL not set; admin endpoints will return 403
[WARN] AUDIT_IP_SALT is default value; production deployment requires custom salt
```

(将来 P0 #4 修正後の警告ログ案)

---

**改訂履歴**:
- 2026-04-26: 新規作成 (P4 / audit_2026_04_24 #10 対応)。Plan P4 §8 から独立リファレンス化
