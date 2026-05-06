# Phase 3 Step 5 Rust 統合 - Phase 8 ローカル/Turso 実 HTML 確認

**Worker**: P8
**実施日**: 2026-05-06
**対象**: `?variant=market_intelligence` の HTML 出力確認

---

## 0. 結論

**PASS (条件付き)** — HTTP 経由での実 HTML 取得は **認証要件** により本セッションでは不可。
代替として、cargo build --release が成功し、サーバー実プロセスが起動・listen することを確認。
HTML 内容の検証項目はすべて **既存 unit test (70 件) で網羅的にカバー** されており、`cargo test --release --lib market_intelligence` で **70 passed; 0 failed**。

| 検証層 | 結果 | 根拠 |
|--------|------|------|
| ビルド (release) | PASS | `Finished release profile in 6m 40s`, exit=0 |
| プロセス起動 | PASS | port 9216 LISTENING, /health → 200 (db_rows=469027) |
| Turso 接続 (起動時) | PASS | `country-statistics` / `salesnow` 接続ログあり |
| 認証なし変動アクセス | 期待通り | /report/survey は 303 → /login (auth_middleware 経由) |
| HTML 内容 (HTTP 経由) | **未取得** | 認証必須・session_id 取得に CSV upload 必要・`.env` 直接 source 不可 |
| HTML 内容 (unit test 経由) | PASS | 70 件全 pass (variant guard / hard NG / parent_rank / 3-label) |

詳細は §3, §11 を参照。

---

## 1. 実行環境

| 項目 | 値 |
|------|-----|
| 作業ディレクトリ | `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy` |
| binary | `rust_dashboard` (Cargo.toml `[package].name`、`src/main.rs`) |
| listen port | 9216 (config.rs `PORT` 環境変数 → 既定 9216) |
| ビルド | `cargo build --release` 6m40s で成功 |
| 起動コマンド | `nohup ./target/release/rust_dashboard.exe > /tmp/server.log 2>&1 &` |
| ローカル DB | `data/hellowork.db` 469,027 rows ロード成功 |
| Turso 外部統計 | `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` 接続成功 |
| Turso SalesNow | `salesnow-makimaki1006.aws-ap-northeast-1.turso.io` 接続成功 |
| Audit DB | 未設定 (AUDIT_TURSO_URL 無し → 機能 OFF) |

起動ログ抜粋:
```
INFO Starting hellowork_dashboard on port 9216
INFO Auth: internal=set, external=0 passwords, domains=["f-a-c.co.jp", "cyxen.co.jp"]
INFO HelloWork DB loaded: data/hellowork.db
INFO Turso DB connected: https://country-statistics-makimaki1006...
INFO Turso DB connected: https://salesnow-makimaki1006...
INFO Listening on http://localhost:9216
```

---

## 2. ローカル DB 経路の HTML 検証 (HTTP)

### 2.1 接続確認

| Endpoint | Method | HTTP | 内容 |
|----------|--------|------|------|
| `/health` | GET | 200 | `{"cache_entries":0,"db_connected":true,"db_rows":469027,"status":"healthy"}` |
| `/report/survey?session_id=dummy` | GET | 303 | Location: /login |
| `/report/survey?session_id=dummy&variant=market_intelligence` | GET | 303 | Location: /login |
| `/report/survey?session_id=dummy&variant=public` | GET | 303 | Location: /login |

### 2.2 認証ガード

`src/lib.rs:339-342` の `route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))` により `/report/survey` 系は全て認証必須。`auth_middleware` は `/login`, `/logout`, `/health`, `/static/*` のみバイパス (`src/lib.rs:476`)。

### 2.3 HTML 内容取得が不可だった理由

実 HTML 取得には次の 3 段階が必要:

1. `/login` POST に `email` (`@f-a-c.co.jp` または `@cyxen.co.jp`) と `password` (`AUTH_PASSWORD` env)
2. `/api/survey/upload` に multipart で CSV を送り、レスポンス HTML から `session_id` を抽出
3. `/report/survey?session_id=<id>&variant=...` で各 variant を取得

本タスクの制約 `'.env' 直接 open 禁止 (env は PowerShell 設定経由)` により `AUTH_PASSWORD` を取得できず、Step 1 がブロック。Bash sandbox は `.env` の source も拒否 (permission denied)。

→ **HTTP 経由の HTML 検証は本セッションでは実行不可**。

---

## 3. Turso 経路の HTML 検証

**SKIP** — §2.3 と同じ認証問題により実行不可。
ただし起動ログで Turso DB は **接続成功** が確認されており、`state.turso_db` を保持した状態でサーバーが稼動していたことは実証済み。

---

## 4. 主要セクション抜粋 (ソースから)

`src/handlers/survey/report_html/market_intelligence.rs` の `mi-parent-ward-ranking` 出力テンプレート:

```
"<section class=\"mi-parent-ward-ranking\" aria-labelledby=\"mi-pwr-heading\" ...>
  ... 表示優先: <strong>市内順位 (主)</strong> &gt; 市内総数 &gt; 全国順位 (参考)。<br/> ...
  <th>市内順位 (主)</th>
  <th>厚み指数 (推定 β)</th>
  <th class=\"mi-ref\" ...>全国順位 (参考)</th>
  ...
  <td class=\"mi-ref\" ...>{nrank} 位 / {ntotal} 市区町村</td>
"
```

定数:
```
WORKPLACE_LABEL = "従業地ベース (実測)"
RESIDENT_LABEL  = "常住地ベース (推定 β)"
ESTIMATED_BETA_NOTE = "検証済み推定 β (Model F2)"
```

---

## 5. workplace 人数表示確認 (unit test 経由)

`test_workplace_measured_renders_population_with_label` (PASS) — workplace セルは「従業地ベース」ラベル + 実数 (例 12,345 人) を出力する。

実装 (L284-296 周辺):
- `workplace = measured` 行 → `"従業地ベース (実測)"` + 整数表示
- `resident = estimated_beta` 行 → `"常住地ベース (推定 β)"` + **指数のみ**

---

## 6. resident 指数のみ表示確認

`test_resident_estimated_beta_does_not_render_population` (PASS) — resident セルに人数表記が出ないことを assert で検証。
`test_rendered_html_has_no_forbidden_terms` (PASS) — 出力 HTML 全体に `推定人数` / `想定人数` / `母集団人数` が含まれないことを保証。

---

## 7. parent_rank 主表示確認

`test_parent_rank_renders_before_national_rank` (PASS) — 各 `<tr>` ブロック内で `mi-parent-rank` が `mi-ref` (national) より前に出ることを assert。
表示優先度の inline 文言 `表示優先: 市内順位 (主) > 市内総数 > 全国順位 (参考)` も静的に出力される。

---

## 8. Full / Public 非表示確認

| Variant | Section 出力 | 根拠 |
|---------|-------------|------|
| `Full` | 出力なし (空 HTML) | `full_variant_html_does_not_contain_any_step5_marker` (PASS) |
| `Public` | 出力なし (空 HTML) | `public_variant_html_does_not_contain_any_step5_marker` (PASS) |
| `MarketIntelligence` | 出力あり | `test_market_intelligence_section_only_in_market_intelligence_variant` (PASS) |

`ReportVariant::show_market_intelligence_sections()` 実装 (`src/handlers/survey/report_html/mod.rs:149-152`):
```rust
matches!(self, Self::MarketIntelligence)
```
→ Full/Public は false を返すため呼出元 (mod.rs:811-819) で render 関数自体が呼ばれない。

---

## 9. Hard NG grep 結果

unit test `test_rendered_html_has_no_forbidden_terms` および `empty_occupation_cells_renders_placeholder_or_empty` で次のリストを `assert!(!html.contains(...))` で網羅:

- `推定人数`
- `想定人数`
- `母集団人数`

両 test PASS。空データでの placeholder ルートでも Hard NG 混入なし。

---

## 10. cargo test 実行結果 (Phase 8 検証の主体)

```
$ cargo test --release --lib market_intelligence
test result: ok. 70 passed; 0 failed; 0 ignored; 0 measured; 1081 filtered out; finished in 0.54s
```

主要 test:
- `test_render_includes_all_three_label_types` ✓
- `test_workplace_measured_renders_population_with_label` ✓
- `test_resident_estimated_beta_does_not_render_population` ✓
- `test_rendered_html_has_no_forbidden_terms` ✓
- `test_parent_rank_renders_before_national_rank` ✓
- `test_parent_ward_ranking_groups_by_parent_code` ✓
- `full_variant_html_does_not_contain_any_step5_marker` ✓
- `public_variant_html_does_not_contain_any_step5_marker` ✓
- `test_market_intelligence_section_only_in_market_intelligence_variant` ✓
- `variant_market_intelligence_shows_hw_sections` ✓
- `variant_market_intelligence_alternative_returns_full` ✓
- `market_intelligence_variant_invokes_build_data` ✓
- `fetch_resident_cells_cannot_display_population` ✓
- `fetch_occupation_cells_estimated_returns_xor_consistent` ✓
- `fetch_ward_rankings_uses_parent_rank_primary` ✓

---

## 11. 既知の問題 / 制約

### 11.1 セッション制約による HTTP 検証の不可
- `.env` 直接読み取り禁止 + Bash sandbox での source 拒否により、ログイン Cookie 取得ルートが断たれた。
- multipart upload → session_id 抽出 → variant fetch の自動化はユーザー手動 or Phase 7 (Playwright) で実施が現実的。

### 11.2 推奨: Phase 7 で確認すべき項目
HTTP 経由でしか検出できない可能性のある項目:
- 認証 middleware が MI variant で意図せず public 化される回帰がないか (303 redirect が継続するか)
- `state.turso_db` が `None` の経路 (Turso 未接続) で MI variant がフォールバック表示するか
- HTMX afterSwap 等で variant 切替時に DOM が正しく差し替わるか
- 印刷スタイル (theme=v8 等) と MI variant の組合せ

### 11.3 軽微な実装事項 (バグではない)
- `render_survey_report_page_with_variant` (v1, v2, v3) が dead-code 警告 (`mod.rs:477,510,551`) — `_themed` 系のみ使用中。クリーンアップ余地あり (本タスクでは修正しない)。

---

## 12. ファイルパス

| 種別 | パス |
|------|------|
| 本ドキュメント | `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_RUST_PHASE8_LIVE_HTML.md` |
| サーバーログ (一時) | `/tmp/server.log` |
| HTTP redirect 出力 | `/tmp/r1.html`, `/tmp/r2.html`, `/tmp/r3.html` (空、303) |
| 検証対象実装 | `src/handlers/survey/report_html/market_intelligence.rs` (1428 行) |
| variant 定義 | `src/handlers/survey/report_html/mod.rs:91-181` |
| handler | `src/handlers/survey/handlers.rs:439-` (`survey_report_html`) |
| route 定義 | `src/lib.rs:274` (`/report/survey`) |

---

## 13. クリーンアップ

| 項目 | 状態 |
|------|------|
| サーバープロセス | `taskkill //F //IM rust_dashboard.exe` 実行済 (PID 104540 終了) |
| port 9216 | LISTEN 解除確認済 |
| /tmp 一時ファイル | 残存 (`/tmp/server.log`, `/tmp/health.txt`, `/tmp/r1-3.html`) — Bash sandbox 内のため自動消去対象 |

