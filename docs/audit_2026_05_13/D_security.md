# D 領域 セキュリティ監査 (2026-05-13)

**対象**: hellowork-deploy (`rust_dashboard` crate, axum web app, Render 無料プラン本番)
**監査範囲**: src/, tests/, docs/ (secret 履歴 grep), .env*, Cargo.toml
**手法**: read-only (build/test 実行なし)

---

## サマリ

| 優先度 | 件数 | 主な内容 |
|---|---|---|
| P0 | 1 | git 履歴に E2E credentials 平文残存 (revoke/rotation 状況不明) |
| P1 | 4 | path traversal (geojson), Cookie Secure=false, 公開 api/v1 認証なし+rate limit なし, USER_EMAIL 未エスケープ反映 |
| P2 | 3 | security headers 不在 (CSP/HSTS/XFO/XCTO), audit DB Origin 検証で `null`/欠落許可, OPTIONS preflight CORS 設定なし |

CSRF / SQL injection / stored-XSS は今回の検査範囲で重大な未対応箇所は確認されず (CSRF Origin/Referer 検証実装済、SQL は parameterized + 内部ホワイトリスト統制)。

---

## P0: git 履歴に E2E credentials 平文残存

**evidence**:
- commit `db81296` (2026-05-12) で `docs/ROUND12_REVIEW_REQUEST_2026_05_12.md:30` の `s_fujimaki@f-a-c.co.jp` / `fac_2026` 平文を redact
- `git log --all -p -S "fac_2026"` で `db81296^` 以前 (`3e74ee5` 含む) に平文が残存
- public repo (`makimaki1006/HR_HR`) 履歴に commit が残っているため、redact は表層的措置のみ

**risk**: 当該アカウントで本番 `https://hr-hw.onrender.com` にログイン可能。`fac_2026` は弱パスワード (推測容易、辞書攻撃に脆弱)。アカウントロックアウト (5回) は IP 単位なのでパスワードスプレー (異 IP) に耐えない。

**remediation**:
1. **即時 rotation**: `s_fujimaki@f-a-c.co.jp` の認証情報を変更。`AUTH_PASSWORD_HASH` (bcrypt) を新値で `.env` 更新、Render 環境変数を更新
2. **強度向上**: 12 文字以上+記号必須+辞書語禁止
3. **git 履歴消去 (可能なら)**: `git filter-repo` で当該文字列を全 commit から除去し force-push。ただし fork/clone への漏出は不可逆
4. **検出**: pre-commit hook (`gitleaks`/`trufflehog`) を `.git/hooks` または CI に導入

CLAUDE.md (memory) でも「rotation 未済なら P0」と明記済 — 本監査では rotation 完了の証拠を確認できず、未完扱い。

---

## P1: GeoJSON API path traversal の防御欠落

**evidence**: `src/handlers/api.rs:21-44`

```rust
pub async fn get_geojson(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Json<Value> {
    let geojson_dir = "static/geojson";
    let path = format!("{geojson_dir}/{filename}");
    match std::fs::read_to_string(&path) { ... }
}
```

**risk**: axum `Path<String>` は単一セグメントを取るが、URL エンコード経由で `..%2F..%2F..%2Fetc%2Fpasswd` 等が `filename` に入る可能性がある。ファイル拡張子/canonical path/prefix チェックが一切ない。`std::fs::read_to_string` は任意ファイルを読み出し可能。

**impact**: 認証必須ルート配下なので外部攻撃者には届かないが、認証済みユーザーから `Cargo.toml` / `.env` (取り外し済) / 監査 DB token を含む env 系ファイルの読み出しリスク。

**remediation**:
- `filename` を許可リスト (例: `^[A-Za-z0-9_-]+\.(json|geojson)$`) で正規表現バリデーション
- `Path::canonicalize()` 後に `static/geojson` 配下であることを `starts_with` で検証
- もしくは `tower_http::services::ServeFile` でディレクトリ配信に置換

---

## P1: Cookie `with_secure(false)` (Render 本番 HTTPS 環境)

**evidence**: `src/lib.rs:60-62`

```rust
let session_layer = SessionManagerLayer::new(session_store)
    .with_secure(false)
    .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));
```

**risk**: 本番は HTTPS だが Cookie に `Secure` 属性が付かない。中間者攻撃 + HTTP downgrade (例: 攻撃者制御の WiFi が `Location: http://hr-hw.onrender.com` を返す) でセッション ID が漏出。`SameSite`/`HttpOnly` も明示設定なし (tower-sessions のデフォルト挙動依存 - 仕様要確認)。

**remediation**:
- 本番では `.with_secure(true)`、開発のみ env で切替
- `.with_http_only(true)` 明示 (XSS時の Cookie 窃取防止)
- `.with_same_site(SameSite::Lax)` 以上を明示 (CSRF 二重防御)

---

## P1: `/api/v1/*` 認証なし + rate limit なし

**evidence**: `src/lib.rs:373-393`

```rust
// JSON REST API v1（認証不要 - MCP/AI連携用）
let api_v1 = Router::new()
    .route("/api/v1/companies", get(handlers::api_v1::search_companies))
    .route("/api/v1/companies/{corporate_number}", ...)
    ...;

Router::new()
    .route("/health", get(health_check))
    .route("/login", ...)
    .merge(api_v1)            // ← auth middleware 適用なし
    .merge(protected_routes)
    ...
```

**risk**: 198K 社の SalesNow 企業データ (法人番号/従業員数/売上レンジ/credit score 等) が誰でも無制限に取得可能。bot による全件 dump (corporate_number は 13桁数字、列挙可能) → 競合事業者へのデータ流出。rate limit なし → Render 無料プラン枯渇 + Turso クォータ消費。

**remediation**:
- API key 認証 (header `X-API-Key`) を追加。`accounts` テーブルの `api_key` カラムで管理
- `tower-governor` 等で IP/key 単位の rate limit (e.g. 60 req/min)
- 最小限の response field に絞る (credit_score / salesnow_score 等の集計情報は要否再検討)

---

## P1: USER_EMAIL を HTML テンプレートに未エスケープ反映

**evidence**:
- `src/lib.rs:801` `replace("{{USER_EMAIL}}", &user_email)`
- `templates/dashboard_inline.html:65` `<span>ログイン: {{USER_EMAIL}}</span>`
- email は `validate_email_domain` で `@` を含む形式チェックのみ、domain allowlist に `"*"` 設定時 (`src/auth/mod.rs:34`) は任意ドメイン許可

**risk**: email local-part に `<` `>` `"` 等を含む RFC 5321 違反値を attacker が登録 (例: `"><script>alert(1)</script>"@evil.com`) → ログイン後 dashboard に reflective XSS。allowlist が `*` の運用なら攻撃者が任意ドメイン登録可能。

**impact**: stored XSS ではないが、self-XSS / open redirect 等の入口になりうる。

**remediation**:
- `helpers::escape_html(&user_email)` を `lib.rs:801` で適用 (既に他箇所で同関数を使用、`src/handlers/helpers.rs` 該当)
- email バリデーションを RFC 5321 準拠の文字種制限 (`^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$`) に強化

---

## P2: security headers の不在

**evidence**: `src/lib.rs` 全文 grep で `Content-Security-Policy` / `X-Frame-Options` / `Strict-Transport-Security` / `X-Content-Type-Options` のいずれもセットされていない。`tower_http::set_header::SetResponseHeaderLayer` は `Cache-Control` のみに使用 (line 368-371)。

**risk**:
- CSP 不在 → reflective XSS が成立した場合に外部 script ロードで escalation 可能
- X-Frame-Options 不在 → clickjacking (login 画面を iframe に埋め込んで credentials を奪取)
- HSTS 不在 → 初回接続時の HTTP downgrade 攻撃が可能

**remediation**: 共通 layer で以下を追加:
- `Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; img-src 'self' data: https:;` (ECharts/Tailwind の inline 許容を吟味)
- `X-Frame-Options: DENY`
- `X-Content-Type-Options: nosniff`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- `Referrer-Policy: strict-origin-when-cross-origin`

---

## P2: CSRF check で Origin/Referer 欠落時に通過

**evidence**: `src/lib.rs:461-466`

```rust
None => {
    // Origin/Referer 無し = curl/API client/モバイルアプリ等
    Ok(())
}
```

**risk**: 一部古いブラウザや特殊な navigation (meta refresh 等) で Origin/Referer が落ちる場合があり、その挙動を悪用される可能性。POST へのリクエストでヘッダ欠落を許可するのは原則禁止。

**remediation**: state-changing POST には独立した CSRF token (double-submit cookie or synchronizer token) を導入し、Origin/Referer 検証は二重防御として併用。最低限、`None` ケースは拒否しブラウザ以外 (curl) には別途 API key 認証経路を提供。

---

## P2: CORS 設定なし (許容ポリシー不明)

**evidence**: `Cargo.toml:11` で `tower-http` の `cors` feature は有効だが `src/` 全文で `CorsLayer` 使用箇所なし。

**risk**: 現状は同一オリジン専用で問題はないが、将来的に `--api_v1` を別ドメインから呼ぶ場合に CORS misconfig が発生しやすい。明示拒否ポリシーを設定すべき。

**remediation**: `CorsLayer::new().allow_origin(ALLOWED_ORIGINS).allow_methods([Method::GET]).allow_credentials(false)` を明示。

---

## 監査外 / 既知の SECURE 項目

| 項目 | 状態 | 根拠 |
|---|---|---|
| SQL injection | SECURE | `build_filter_clause` (overview.rs:253) で全 user input を `?N` parameterized。SQL 文字列フォーマットは内部 ホワイトリスト列 (condition_gap.rs:297, workstyle.rs:349-366) のみ |
| password storage | SECURE | bcrypt 使用 (`auth/mod.rs:48`)、cost は不明 (デフォルト 12 想定) |
| rate limit (login) | OK | IP 単位 5 回 → 5分 lockout (`config.rs:107-114`) |
| audit log | OK | IP hash 化 (`audit.hash_ip`)、PII を生 IP で保存しない |
| body size limit | OK | 20MB 制限 (`lib.rs:40, 267`) |
| CSRF (Origin check) | PARTIAL | 実装済だが `None` 通過が懸念 (P2 参照) |

---

## 推奨優先順序

1. **P0 fac_2026 revoke** (1 時間)
2. **P1 Cookie Secure + api/v1 API key + USER_EMAIL escape** (各 30 分)
3. **P1 geojson path traversal** (1 時間 - 許可リスト導入)
4. **P2 security headers** (30 分 - middleware layer 追加)
5. **P2 CSRF token / CORS 明示** (検討事項)

---

## 検査範囲外 / 未検査

- Cargo.lock の CVE スキャン (cargo audit 未実行 — read-only 制約のため)
- E2E test ファイル群 (e2e_*.py) 内の credentials 漏出
- Render 環境変数の値 (アクセス権限なし)
- `target/`, `node_modules/` (build artifact)
- log ファイル (`server.err`, `*.out.log`) の PII 漏出 — 別途 grep 推奨
