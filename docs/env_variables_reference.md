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

## 2d. HubSpot API の鍵 (CRM 専用鍵は任意)

`HubSpotApiConfig::from_env()` は従来の鍵、`crm_from_env()` は CRM 専用鍵を優先して読む。前後の空白を落とし、空文字は未設定扱い。`Debug` は値を伏せ、起動ログには選択した変数名だけを出す。変更後は再起動する。

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照 |
|---|------|----------|------|-------------|------|
| 25 | `HUBSPOT_ACCESS_TOKEN` | `""` | 従来の共通 Bearer トークン。求人文面・営業KPI など CRM 以外の機能と、専用鍵がない場合の CRM に使用 | 専用鍵もなければ CRM は 503 `not_configured`。専用鍵だけあれば CRM は利用可能 | `src/config.rs` / `src/main.rs` |
| 25b | `HUBSPOT_CRM_ACCESS_TOKEN` | `""` | CRM 専用 private app の Bearer トークン。`/api/crm/*`、メール記録、返信取り込み、保存待ちの再送で優先。共通鍵の値は変更しない | `HUBSPOT_ACCESS_TOKEN` に戻る | 同上 / `src/crm/` |
| 26 | `CRM_METADATA_ALLOWED_EMAILS` | `""` | Google Workspace ログインと既存 RBAC に加えるメールの絞り込み (カンマ区切り、完全一致)。変更後は再起動 | 追加のメール絞り込みなし。RBAC は必須 | `src/crm/rbac.rs` |
| 26b | `JOB_COPY_ALLOWED_EMAILS` | (使わない) | 2026-10-21 から読まない。案件管理の閲覧は、会社ドメイン (`ALLOWED_DOMAINS`) の Google ログインで無効化されていない人なら全員可 (パスワードログイン・`ALLOWED_DOMAINS_EXTRA` の社外ドメインは不可)。Render に値が残っていても効かないので削除してよい | — | `src/handlers/job_copy_live.rs` |

### 専用 private app に付けるスコープ

現在の `/crm/v3`・`/crm/v4` 呼び出しに対応する Legacy Private App の構成。読み取りと書き込みを下記のとおり付ける。Gmail 送信の権限は Google 側で別に取得する。

| 種別 | スコープ | 実際の API / 利用機能 |
|------|----------|---------------------|
| 読み取り | `crm.objects.deals.read` | deals の GET・batch/read・search、取引プロパティ定義・グループ、パイプライン、関連ラベル / batch/read。架電一覧・詳細・段階ルール |
| 読み取り | `crm.objects.contacts.read` | contacts の GET・batch/read・search、プロパティ定義・関連。担当者詳細・宛先・活動 (calls / notes / tasks / meetings / emails) の取得 |
| 読み取り | `crm.objects.companies.read` | companies の GET・batch/read、プロパティ定義・関連。会社情報 |
| 読み取り | `crm.objects.owners.read` | `GET /crm/v3/owners`。担当者一覧・操作者の owner ID |
| 読み取り | `sales-email-read` | emails の GET・batch/read・search の本文取得。活動ログ・送信/返信の Message-ID 重複確認 |
| 書き込み | `crm.objects.deals.write` | `PATCH /crm/v3/objects/deals/{id}`。段階・取引項目の保存 / 再送、メールの取引への関連付け |
| 書き込み | `crm.objects.contacts.write` | `PATCH /crm/v3/objects/contacts/{id}`、`POST /crm/v3/objects/emails`、emails の取引・コンタクトへの関連付け。担当者項目・送信/受信メール記録 / 再送 |
| 書き込み | `crm.objects.companies.write` | `PATCH /crm/v3/objects/companies/{id}`。会社項目の保存 / 再送 |

定義の読み取りは上記オブジェクトの読み取り権限で行える。プロパティ・パイプラインの作成変更、Call 作成、マーケティングメール送信、求人 listing の権限は CRM 専用鍵に不要。実装は `src/hubspot/client.rs` と `src/crm/{routes,workspace,write,email}.rs`。

参照: [スコープ一覧](https://developers.hubspot.com/docs/apps/developer-platform/build-apps/authentication/scopes)、[メール記録 API](https://developers.hubspot.com/docs/api-reference/legacy/crm/activities/emails/guide)、[メール本文の権限](https://developers.hubspot.com/changelog/announcement-new-scope-required-to-get-the-content-of-email-engagements)、[公式 OpenAPI 定義](https://github.com/HubSpot/HubSpot-public-api-spec-collection/tree/master/PublicApiSpecs/CRM)。

### 2d-1. Headless CRM の書き込み (3 個、2026-10-09 追加、ADR-018)

`src/crm/write.rs` `WriteConfig::from_env()` が読む (ルーターを作るときに 1 回。変更後は再起動)。**既定は書き込み OFF**。

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照 |
|---|------|----------|------|-------------|------|
| 27 | `CRM_WRITES_ENABLED` | `""` (OFF) | `1` で `PATCH /api/crm/deals/{id}` を全案件で開く | 閉じている。許可リストの案件以外は 403 `writes_disabled` (HubSpot も監査 DB も触らない) | `src/crm/write.rs` |
| 28 | `CRM_WRITE_DEAL_ALLOWLIST` | `""` | 栓が閉じていても書ける案件 ID (カンマ区切り)。本番で 1 件だけ試すときに使う | 空 | `src/crm/write.rs` |
| 29 | `CRM_PENDING_MAX` | `50000` | 一時障害で再送待ち (`crm_pending_operations.status = 'pending'`) にできる件数の上限。超えたら 503 `queue_full` で何も積まない | 既定 | `src/crm/write.rs` |
| 29b | `CRM_WRITE_RATE_PER_MIN` | `200` | 1 人あたりの保存受付の上限 (1 分あたり、最小 1)。超えたら 429 `rate_limited` で何も保存しない (再送待ちにも積まない) | 既定。0・数でない値も既定 | `src/crm/write.rs` |

書き込みには HubSpot の書き込みスコープ (`crm.objects.deals.write` ほか、担当者・会社も書くなら `contacts.write` / `companies.write`) が選択された CRM の鍵 (`HUBSPOT_CRM_ACCESS_TOKEN`、未設定時は `HUBSPOT_ACCESS_TOKEN`) に必要。監査 Turso に表 `crm_pending_operations` を起動時に自動作成する (`CREATE TABLE IF NOT EXISTS`、手動作業なし)。

### 2d-2. HubSpot 呼び出しの関所 (6 個、2026-10-08 追加)

`src/hubspot/gateway.rs` `GatewayConfig::from_env()` と `src/hubspot/client.rs` `base_url_from_env()` が読む。このプロセスで同じ鍵の HubSpot 呼び出しが同じ流量制限を共有し、異なる鍵は別の枠を持つ。CRM は専用鍵を優先し、求人票コピー・営業KPI 直読みは従来の鍵を使う。範囲外・数字以外は既定値に戻して warn を出す。設計は `docs/architecture/headless-crm-design.md` §18。選択された CRM の鍵の観測値は `/api/admin/hubspot-usage` (管理画面 `/app/admin?view=hubspot`)。

| # | 変数 | デフォルト | 用途 | 未設定時影響 | 参照 |
|---|------|----------|------|-------------|------|
| 27 | `HUBSPOT_APP_RATE_PER_SEC` | `8` (範囲 1〜19) | Search 以外の呼び出しの、鍵ごとの 1 秒あたりの上限 (どの 1 秒を切り取っても超えない) | 既定値 | `src/hubspot/gateway.rs` |
| 28 | `HUBSPOT_APP_RATE_PER_10S` | `80` (範囲 1〜190) | 鍵ごとの 10 秒あたりの上限。HubSpot の上限 190 / 10 秒のうち残りは鍵を共有する外部バッチの分 | 既定値 | 同上 |
| 29 | `HUBSPOT_APP_SEARCH_PER_SEC` | `3` (範囲 1〜5) | Search の 1 秒あたりの上限 (鍵ごとの枠。HubSpot 側はアカウントで 5 / 秒なので、別の鍵も含む合計は共有) | 既定値 | 同上 |
| 30 | `HUBSPOT_INTERACTIVE_MAX_WAIT_MS` | `5000` (範囲 100〜60000) | 画面の操作 (CRM・求人票コピーの読み取り) が順番を待てる上限。超える・超えると見込まれるときは HubSpot を呼ばずに 503 `hubspot_busy` (画面は「HubSpot が混み合っています。少し待ってから再試行してください」)。CRM のレコード読み取りの同時実行枠の待ちもこの時間まで | 既定値 | 同上 / `src/crm/routes.rs` `acquire_read_slot` |
| 31 | `HUBSPOT_BACKGROUND_MAX_WAIT_MS` | `60000` (範囲 100〜600000) | 背景の取得 (定義系キャッシュの先読み・営業KPI 直読み) が待てる上限 | 既定値 | 同上 |
| 32 | `HUBSPOT_BASE_URL` | `""` (= `https://api.hubapi.com`) | HubSpot API の接続先の差し替え。負荷試験の偽 HubSpot (`scripts/loadtest/`) 用。`http://` / `https://` で始まる値だけ使う。**鍵 (Bearer) はこの接続先に送られる**ので本番では設定しない | `https://api.hubapi.com` | `src/hubspot/client.rs` `base_url_from_env()` |

専用鍵を設定すると CRM と既存機能の短時間の枠・待ち行列・429 停止を分離する。同じ鍵を両変数に設定した場合は同じ枠。各枠の設定値は既定の 8 / 秒・80 / 10 秒・Search 3 / 秒を維持する。**日次上限は同一アカウントの全 private app で共有され、Search の公式上限もアカウントで 5 / 秒**なので、鍵を分けてもこれらの総量は増えない。日次の別枠や新しい保存先は追加しない。429 の再試行は従来どおり最大 2 回。ユーザー側の準備: 上記権限の private app を用意し、任意の専用鍵を実行環境に設定する。未設定なら従来の鍵で動く。

参照: [HubSpot 利用上限](https://developers.hubspot.com/docs/developer-tooling/platform/usage-guidelines)、[Search の上限](https://developers.hubspot.com/docs/api-reference/latest/crm/search-the-crm)。

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
$env:TURSO_EXTERNAL_URL = "libsql://country-statistics-<org>.turso.io"
$env:TURSO_EXTERNAL_TOKEN = "..."
$env:SALESNOW_TURSO_URL = "libsql://salesnow-<org>.turso.io"
$env:SALESNOW_TURSO_TOKEN = "..."
$env:AUDIT_TURSO_URL = "libsql://audit-<org>.turso.io"
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


## 再起動後のログイン維持 (`SESSION_COOKIE_KEY`)

| 変数 | 形式 | 未設定・不正な場合 |
|------|------|------------------|
| `SESSION_COOKIE_KEY` | 64 バイト以上の暗号学的乱数を標準 base64 化した固定鍵 | 起動時に warn。従来のメモリ認証に戻る (再起動でログアウト) |

Render の環境変数に設定する。ローカルで次のコマンドを実行し、出力を秘密値として登録する。

```sh
openssl rand -base64 64 | tr -d '\n'
```

鍵はリポジトリ、ログ、PR、共有画面に載せない。同じサービスの全インスタンスと再デプロイで同じ値を使う。
鍵を変更すると既存の認証 Cookie は無効になり、再ログインが必要になる。自動生成して起動ごとに変更しない。

メール、ログイン方法、認証の有効期限、社外パスワードの利用期限と、既存ログイン履歴への参照を
認証付き暗号化した `hr_login` Cookie に持つ。HttpOnly、Path=/、本番は Secure・SameSite=Strict
(開発は SameSite=Lax)。期限は従来と同じ24時間で、既存メモリセッションの更新条件に合わせて延長する。
復元だけでは元の期限を延長しない。CSV、ヘッダーフィルタ、OIDC 開始時の一時情報は永続化しない。

**既存監査 DB の接続 (`AUDIT_TURSO_URL` / `AUDIT_TURSO_TOKEN`) も必要**。
新しい DB・表は作らず、メモリの本人・ログイン方法・ログイン履歴の参照が Cookie と一致しない場合だけ、
既存 `login_sessions` の本人・方法・成功状態・`ended_at` を1回照合して復元する。
一致する通常要求と Cookie の更新では履歴を再照会しない。
役割は Cookie に持たず、既存の認可処理で確認する。アカウント無効化の既存照会と社外パスワード期限の確認は維持する。
このため「照会を省く」の対象は追加したログイン履歴の確認であり、画面固有の DB 処理や既存の無効化照会までなくなるわけではない。
ログアウト・再ログインで元の履歴を終了する。成功した失効は同じプロセス内にも24時間保持し、
同じ履歴から復元して作られた複数のメモリ状態、更新前後の Cookie の再送を拒否する。
再起動後や未復元の別インスタンスでも、DB の終了日時を見て拒否する。
**別インスタンスで既に確認済みのメモリには即時失効を通知しない**。そのメモリが消失・期限切れになるまでは継続できる。
既存履歴の1年保持による削除も、次の復元時に再ログインが必要になる。

監査 DB が起動時に未接続なら warn を出して Cookie を発行せず、従来のメモリ認証だけに戻る。
ログイン履歴の保存に失敗した場合も Cookie は発行しない。
メモリの認証が Cookie と一致していれば、監査 DB 障害中も追加の履歴確認による503を返さない。
アカウント無効化の確認は従来どおり実行し、その照会失敗時の既存方針は変えない。
メモリからの復元が必要で履歴の確認が失敗した場合や、発行済み Cookie の鍵を失った状態のログアウトは503で拒否し、
Cookie は保持して回復後に再試行できる。認証 Cookie を使ったログアウトの失効記録が失敗した場合も503で、完了扱いにはせず再試行が必要。
鍵なしのメモリ認証では従来どおり、監査の書き込み失敗時もメモリのログイン状態を削除する。

初回導入と参照を持たない旧形式 Cookie からの更新では、一度再ログインが必要。その後は同じ鍵・同じ既存監査 DB の
再デプロイで復元できる。失効方式は [ADR-020](architecture/architecture-decisions.md#adr-020--認証-cookie-の失効を既存ログイン履歴で確認する)、復元時だけの確認に変更する理由・制約・戻すコストは ADR-021 を参照。


### CRM メール送信 (既定 OFF、ADR-022)

| 変数 | 内容 |
|---|---|
| `CRM_EMAIL_ENABLED` | `1` のときのみ有効。未設定なら実データの画面にメール送信を出さない |
| `CRM_EMAIL_TOKEN_KEY` | Base64 形式のランダムな 32 バイト AES-256-GCM 鍵。既存の Cookie / Scout の鍵を流用しない。欠落・不正なら無効。設定後の変更は保存済み token を復号できなくなるため、先に許可を取り消して再許可する |
| `CRM_EMAIL_GOOGLE_CLIENT_ID` | Gmail 追加許可専用の OAuth クライアント ID（Internal プロジェクト）。専用 ID と secret が両方あれば認可・token 交換・refresh に使用し、ID token の aud もこの ID に一致させる。両方未設定なら従来の `GOOGLE_OIDC_*` を使用。片方だけなら warn を出して Gmail 機能を無効化（空白だけは未設定扱い） |
| `CRM_EMAIL_GOOGLE_CLIENT_SECRET` | 専用クライアントの secret。サーバだけが使用し、ブラウザ・ログには出さない。上記 ID と必ずセットで設定 |
| `CRM_EMAIL_REDIRECT_URL` | Google Cloud の承認済み URI と一致する追加許可 callback。例 `https://hr-hw.onrender.com/api/crm/gmail/callback`。HTTPS 必須 (ローカルの localhost / 127.0.0.1 は HTTP 可) |

既存 `GOOGLE_OIDC_CLIENT_ID` / `GOOGLE_OIDC_CLIENT_SECRET` / `GOOGLE_OIDC_REDIRECT_URL` / `GOOGLE_OIDC_HOSTED_DOMAIN` と `AUDIT_TURSO_URL` / `AUDIT_TURSO_TOKEN` も必要。ログインのクライアント・OIDC callback は維持する。Gmail 専用クライアントを設定する場合は、そのクライアントに `CRM_EMAIL_REDIRECT_URL` を登録する（従来設定を使う場合はログイン用クライアントへ Gmail 用 URI を追加する）。署名・iss・exp・nonce・hd・email_verified・操作者本人一致の検証は維持する。取り消しは専用クライアントで得た refresh token を Google revoke endpoint に渡す（Google の revoke は client_id / secret を受け取らない）。クライアントを変更すると既存許可は使えないため、許可のある環境では取り消して再許可する。Gmail API を有効化し、同意画面は会社の Internal とする。要求する追加スコープは gmail.send と gmail.readonly。HubSpot に emails の読み取り・検索・作成・関連付けができる既存アプリの権限も必要 (本 PR では本番の権限は変更しない)。

許可の開始・callback の間だけ `hrhr_gmail_tx` (HttpOnly / SameSite=Lax / 五分 / 本番 Secure) を使う。既存ログインの Cookie を変更しない。送信の操作台帳を監査 DB に記録できなければメールを送らない。計測表だけの保存失敗は計測なしで送る。ローカル試験は偽 Gmail・偽 HubSpot・SQLite に裏打ちした偽 Turso だけを使う。

### CRM メール計測 (既定 OFF)

| 変数 | 既定 | 用途 |
|---|---|---|
| `CRM_EMAIL_ENABLED` | 未設定 | `1` のときだけメール機能と計測を有効化。計測は既存 `AUDIT_TURSO_URL` / `AUDIT_TURSO_TOKEN` を使用。Renderへの設定はユーザーが行う |
| `CRM_EMAIL_TOKEN_KEY` | 未設定 | Gmail 許可情報を暗号化する鍵 (送信PRで検証)。未設定なら計測表の初期化・計測・表示も無効 |

計測だけの独立したDB・鍵・公開URL用の環境変数は増やさない。送信時の `base_url` は信頼できるアプリ設定から渡し、受信要求の Host ヘッダーから作らない。

### トークの振り返り (既定 OFF、ADR-033)

| 変数 | 既定 | 用途 |
|------|------|------|
| `CALL_REVIEW_ENABLED` | 未設定 | `1` のときだけ worker が判定する。未設定なら worker は動かず、summary は空の 200 (`enabled: false`)、calls と詳細は 503 `not_configured`。既存の監査 DB (`AUDIT_TURSO_URL` / `AUDIT_TURSO_TOKEN`) が必要 |
| `CALL_REVIEW_MODEL` | `MiniMax-M3` | 判定に使う MiniMax のモデル名 |
| `CALL_REVIEW_INTERVAL_SECS` | `600` | 対象の Call を読みに行く間隔 (秒)。30 以上。30 未満は既定の 600 |
| `CALL_REVIEW_BACKFILL_DAYS` | `14` | 初回に過去何日まで遡るか。1〜62。範囲外は既定の 14 |
| `CALL_REVIEW_PER_PERSON_DAY` | `10` | 1 人 1 日 (JST) あたりに判定する電話の最大本数。新しい電話から。1〜1000、範囲外・数字でない値は既定の 10 |
| `CALL_REVIEW_CONCURRENCY` | `4` | 同時に判定する本数。1〜64、範囲外・数字でない値は既定の 4 |
| `CALL_CONTACT_BACKFILL_DAYS` | `7` | 接触・日程切りの判定を、初回に過去何日まで遡るか (以後は今日と昨日だけ見直す)。1〜62、範囲外・数字でない値は既定の 7。`CALL_REVIEW_ENABLED=1` のときだけ動く |
| `CALL_CONTACT_CONCURRENCY` | `16` | 接触・日程切りの判定を同時に進める本数。1〜64、範囲外・数字でない値は既定の 16 |
| `CALL_REVIEW_TEAMS` | 未設定 | 対象の HubSpot チーム名 (カンマ区切り、前後の空白は除く)。未設定なら `BPO_リクロジ` と `新規営業` |
| `ZOOM_ACCOUNT_ID` | 未設定 | Zoom Server-to-Server OAuth のアカウント ID。Zoom Phone の録音・文字起こしの読み取りに使う |
| `ZOOM_CLIENT_ID` | 未設定 | 同 OAuth のクライアント ID |
| `ZOOM_CLIENT_SECRET` | 未設定 | 同 OAuth のクライアント secret。サーバだけが使い、ブラウザ・ログには出さない |
| `MINIMAX_API_KEY` | (なし) | 判定に使う MiniMax の鍵 (月額の枠)。未設定なら worker は起動せず警告を出すだけ |
| `MINIMAX_BASE_URL` | `https://api.minimax.io/v1` | MiniMax の API の場所 (OpenAI 互換) |

Zoom のアプリには Zoom Phone の録音・文字起こしを読むスコープを付ける。値は Render の環境変数に設定する (リポジトリには書かない)。
