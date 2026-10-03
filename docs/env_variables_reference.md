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
| 5 | `ALLOWED_DOMAINS` | `f-a-c.co.jp` | 社内ドメイン (カンマ区切り) | 2026-08-10 に cyxen.co.jp を削除 | `config.rs:93` |
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

## 2b. handler 直接読出 (1 個、config.rs 統合違反、2026-09-28 追加)

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照行 |
|---|------|----------|------|-------------|--------|
| 20 | `HUBSPOT_PORTAL_ID` | `23708633` (リクロジ事業部) | コンサルダッシュボードの「HubSpot で開く」「HS」リンク先 `https://app.hubspot.com/contacts/<portal_id>/record/0-3/<deal_id>/` の portal_id。全 `/api/consulting/*` の `meta.hubspot_portal_id` に載せる。`/api/crm/*` の `deep_link` (`record/0-1|0-2|0-3/{id}/`) にも使う | 既定値を使う（公開して困る値ではない。2026-09-28 藤巻さん確認）。空白だけでも既定値 | `src/hubspot/deep_link.rs` `hubspot_portal_id()` (cs_dashboard からはここを経由) |

`freshen()`（全 API の meta を組む関数）は `AppState` を受け取らず `Sheets` だけで動くため、`AppConfig` を通さず `std::env::var` を直接読んでいる。統合するなら `freshen` の 7 か所の呼び出しに config を通す必要がある（UI/UX 改善の範囲外として据え置き）。

## 2c. Google Workspace OIDC ログイン (4 個、2026-09-29 追加、ADR-017)

`src/config.rs` `GoogleOidcConfig::from_env()` が読む。**4 つ全部が空でないときだけ有効**。1 つでも欠けると OIDC は無効で、ログイン画面に Google ボタンを出さず `/auth/google/login` `/auth/google/callback` は 404、パスワードログインは従来どおり (一部だけ設定されていると起動時に warn)。

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照 |
|---|------|----------|------|-------------|------|
| 21 | `GOOGLE_OIDC_CLIENT_ID` | `""` | Google Cloud の OAuth クライアント ID (ウェブアプリ)。ID token の `aud` と照合 | OIDC 無効 | `src/config.rs` / `src/auth/google_oidc.rs` |
| 22 | `GOOGLE_OIDC_CLIENT_SECRET` | `""` | 同クライアントのシークレット。code → token 交換でサーバだけが使う (ブラウザに渡さない。ログにも出さない) | 同上 | 同上 |
| 23 | `GOOGLE_OIDC_REDIRECT_URL` | `""` | 承認済みリダイレクト URI と完全一致させる。本番 `https://hr-hw.onrender.com/auth/google/callback` | 同上 | 同上 |
| 24 | `GOOGLE_OIDC_HOSTED_DOMAIN` | `""` | ID token の `hd` クレームと email のドメインをこれと照合 (例 `f-a-c.co.jp`)。`ALLOWED_DOMAINS` は流用しない (`*` 設定で hd 検証が無効化されるのを避けるため) | 同上 | 同上 |
| 25 | `CSRF_EXTRA_ORIGINS_DEBUG` | `""` | **debug ビルド専用** (release では読まない)。CSRF の許可 Origin に追加する (カンマ区切り、例 `http://localhost:9217`)。PR 時 E2E の POST 用 | 追加なし | `src/lib.rs` (`origin_allowed`) |

ユーザー側の準備 (Google Cloud): OAuth 同意画面を「内部」で作成 → OAuth クライアント ID (ウェブアプリ) を作成 → 承認済みリダイレクト URI に上の URL を登録 → 4 つを Render の環境変数に設定 (`render.yaml` は `sync: false` で名前だけ)。

## 2d. HubSpot CRM API (1 個、2026-09-29 追加、Headless CRM PR2)

`src/config.rs` `HubSpotApiConfig::from_env()` が読む。前後の空白は落とし、空文字は未設定扱い。`Debug` 出力ではトークンを `***` に伏せる。

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照 |
|---|------|----------|------|-------------|------|
| 25 | `HUBSPOT_ACCESS_TOKEN` | `""` | HubSpot CRM API の Bearer トークン (`Authorization: Bearer`)。Legacy Private App / static auth アプリ / Service Key のどれでも同じ形で扱う。スコープは読み取りのみ (`crm.objects.contacts.read` / `crm.objects.companies.read` / `crm.objects.deals.read` / `crm.objects.owners.read`) を推奨し、書き込みスコープは PR4 まで付けない。秘密情報のためログ・API 応答に出さない | `/api/crm/*` は 503 `not_configured`。他機能には影響なし | `src/config.rs` / `src/hubspot/` / `src/crm/` |
| 26 | `CRM_METADATA_ALLOWED_EMAILS` | `""` | `/api/crm/*` (定義 `metadata` とレコード読み取り) を読める人のメール (カンマ区切り、大文字小文字を区別しない完全一致)。Google Workspace OIDC ログインであることも必須 (共有・外部パスワードは 403、未ログインは JSON 401)。役割 (RBAC) の本実装までの暫定。変更後は再起動 | 空なら全員 403 | `src/crm/rbac.rs` `CrmAccess::from_env()` |

鍵は既存の HubSpot Service Key (sales-automation-api) を共有する (2026-09-29 ユーザー決定 P-2。HR_HR 専用キーは発行しない)。既存バッチ群とレート上限 (10 秒あたりの上限、Search 5 req/秒/アカウント) を共有するため、クライアントは Search を 1 req/秒に絞り、429 は Retry-After (無ければ最低 1 秒) を待って最大 2 回だけ retry する。ユーザー側の準備: 同じ値を Render の環境変数に設定 (`render.yaml` は `sync: false` で名前だけ)。

> ⚠ この文書の見出しの「19 個」は 2026-04-26 時点の数。その後 `config.rs` に Turso 系が入り（§2 の 4 個は今は `AppConfig::from_env` にある）、`src/` の `env::var` の名前は 2026-09-28 時点で 43 個。全体の棚卸しは別作業。

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
| `ALLOWED_DOMAINS` | `f-a-c.co.jp` (デフォルト) |
| `TURSO_EXTERNAL_URL` / `_TOKEN` | (sync:false) |
| `SALESNOW_TURSO_URL` / `_TOKEN` | (sync:false) |
| `AUDIT_TURSO_URL` / `_TOKEN` | (sync:false) |
| `AUDIT_IP_SALT` | UUID 生成 (sync:false) |
| `ADMIN_EMAILS` | 管理者メール |
| `GOOGLE_OIDC_CLIENT_ID` / `_CLIENT_SECRET` / `_REDIRECT_URL` / `_HOSTED_DOMAIN` | Google ログイン (§2c、sync:false) |
| `HUBSPOT_ACCESS_TOKEN` | HubSpot CRM API 読み取り (§2d、sync:false) |

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
