# Team δ: Code Health Audit Report

**監査日**: 2026-04-24
**監査者**: Refactoring Expert (Team δ)
**対象**: V2 HW Dashboard (`hellowork-deploy/`)
**Layer**: L7 Tech Debt / Maintainability
**制約**: 読み取り・grep のみ。コード編集禁止。
**配置先 (要望)**: `docs/audit_2026_04_24/team_delta_codehealth.md` — sandbox 書込制限により worktree に出力。手動コピー要。

---

## エグゼクティブサマリ

### 技術負債スコア: **6.0 / 10** (Moderate Health, 部分的注意領域あり)

| 評価 | スコア | 根拠 |
|------|--------|------|
| ✅ 良好 | 8/10 | 依存関係の使用率, 警告抑制ポリシー, 契約テストの整備 |
| 🟡 普通 | 6/10 | unwrap/clone の絶対数, ファイル肥大化, dead code 管理 |
| 🔴 注意 | 4/10 | ドキュメント乖離, 未解消の契約ミスマッチ, ワークスペース汚染 |

### 全体所見
1. **`#[allow(dead_code)]` が 11 箇所**: ほとんどは「将来用」の正当化付き。1 箇所のみ `_legacy_unused` で削除予定 (147 行)。
2. **テスト 647 件**: 充実しているが `#[ignore]` 付き bug marker が 2 件残置 (Mismatch #1, #4 未解消)。
3. **ファイル肥大化**: 4,594 行の `analysis/render.rs` と 3,912 行の `survey/report_html.rs` が要注意。後者は PDF 設計仕様書 (2026-04-24) で全面再構成予定。
4. **ドキュメント乖離**: ルート `CLAUDE.md` (2026-03-14) は `insight/survey/recruitment_diag/region/trend/SalesNow` を一切記載しておらず、現実装から大きく乖離。
5. **環境変数の未文書化**: `TURSO_EXTERNAL_URL/TOKEN`, `SALESNOW_TURSO_URL/TOKEN` が `config.rs` に存在せず `main.rs` で直接 `std::env::var` 読出し。
6. **ワークスペース汚染**: ルート直下に 138 個の PNG, 4 個の `_*_mock.csv`, `_sec_tmp/` が `.gitignore` 未登録のまま放置。

---

## 1. Dead Code

### 1.1 `#[allow(dead_code)]` 一覧 (合計 11 箇所)

| ファイル:行 | 関数/型 | 種別 | 評価 |
|------|--------|------|------|
| `src/db/turso_http.rs:108` | (関数) | 内部 API | 🟢 利用可能性あり |
| `src/db/turso_http.rs:198` | (関数) | 内部 API | 🟢 利用可能性あり |
| `src/handlers/competitive/fetch.rs:16` | `struct PostingRow` | 全フィールド未使用扱い | 🟡 26 フィールド構造体、使われているフィールドもあるが allow で全体抑制 |
| `src/handlers/competitive/fetch.rs:56` | `SalaryStats::bonus_rate` | 単一フィールド | 🟢 OK |
| `src/handlers/insight/pattern_audit_test.rs:155, 396` | テストヘルパー | テスト内部 | 🟢 OK |
| `src/handlers/jobmap/fetch.rs:28` | `struct DetailRow` | 全フィールド | 🟡 26 フィールド・一部使用、契約 #3 未返却フィールド多数を含む |
| `src/handlers/recruitment_diag/render.rs:26` | `fn job_type_options_html` | UI 補助 (50行) | 🟡 「将来用」コメント。Agent D テンプレが独自実装したため重複 |
| `src/handlers/recruitment_diag/render.rs:60` | `fn emp_type_options_html` | UI 補助 (20行) | 🟡 同上 |
| `src/handlers/survey/location_parser.rs:771` | `fn resolve_city_alias` | 略称マッピング | 🟢 政令市名→ '市' 補完。実装中拡張用 |
| `src/handlers/survey/report_html.rs:1492` | `fn render_section_hw_enrichment_legacy_unused` | **147 行の旧実装** | 🔴 **削除推奨** |

### 1.2 `_legacy_unused` 関数 (削除推奨)

**`src/handlers/survey/report_html.rs:1493`** `render_section_hw_enrichment_legacy_unused()` (1493〜1640 行、約 147 行)
- コメント: `// === 以下は旧実装（未使用、将来削除予定） ===` (line 1491)
- 現行実装は同ファイル `render_section_hw_enrichment` (line 1375) に存在。
- **問題**: 削除タイミングを明記せず放置。`report_html.rs` は PDF 仕様書 (2026-04-24) で全面再構成予定のため、いずれにせよ削除される可能性が高いが、それまで毎回コンパイル時間に乗る。
- **推奨**: 次回 commit で削除。git 履歴に残るため復元は容易。

### 1.3 バックアップファイル (削除推奨)

| ファイル | サイズ | 修正日 | 評価 |
|------|------|------|------|
| `src/handlers/diagnostic.rs.bak` | 37,265 B | 2026-03-31 | 🔴 **削除推奨**。現行 `diagnostic.rs` (47,292 B) と並存。Phase 6 拡張版コメントから旧版と判明 |
| `data/hellowork.db.gz.bak` | (未確認) | - | 🟡 DB ロールバック用なら正当だが命名規則を整備すべき |

`.gitignore` に `*.bak` パターンが**ない**ため、誤コミットリスクあり。

### 1.4 コメントアウトされた大きなブロック

`TODO|FIXME|XXX|HACK` の検出件数: **2 件のみ** (`src/handlers/insight/phrase_validator.rs` のみ)。
コメントアウト主体のブロックは目視確認した範囲内では検出されず。**🟢 GOOD**

---

## 2. テストカバレッジ

### 2.1 統計

| 項目 | 件数 |
|------|------|
| `#[test]` / `#[tokio::test]` 総数 | **647** |
| テストファイル数 | **10** (専用) + 多数の `#[cfg(test)] mod tests` |
| `#[ignore]` 付き bug marker | **2** (jobmap contract Mismatch #1, #4) |

### 2.2 専用テストファイル

| ファイル | サイズ | 用途 |
|------|--------|------|
| `handlers/global_contract_audit_test.rs` | 19,670 B | 横断契約テスト (採用診断事故対応) |
| `handlers/insight/pattern_audit_test.rs` | 51,604 B | Insight 22 パターン検証 |
| `handlers/jobmap/flow_audit_test.rs` | 26,736 B | flow CTAS fallback 検証 |
| `handlers/recruitment_diag/contract_tests.rs` | (中) | 採用診断 8 panel 契約 (2026-04-23 事故直接対応) |
| `handlers/region/karte_audit_test.rs` | (中) | 地域カルテ |
| `handlers/survey/parser_aggregator_audit_test.rs` | 38,684 B | survey 解析 |
| `handlers/survey/report_html_qa_test.rs` | 48,857 B | レポート HTML QA |
| `handlers/survey/location_parser_realdata_test.rs` | (小) | 実データ駆動 |
| `handlers/competitive/tests.rs` | (小) | 競合 |
| `handlers/trend/tests.rs` | 18,974 B | トレンド |

### 2.3 カバレッジが薄い領域 (推測)

- **`src/handlers/admin/*`**: テスト未確認。管理画面のため影響範囲限定
- **`src/handlers/my/*`**: 同上、自己サービス
- **`src/handlers/api_v1.rs`**: OpenAPI 文書化済みとされるが対応テストファイルなし
- **`src/main.rs`** の起動シーケンス: spawn_blocking + Turso 初期化失敗フォールバックの統合テスト未確認

### 2.4 Mock の使用状況

実 DB 依存ではなく `tempfile::NamedTempFile::new()` で都度 SQLite を生成する設計。`global_contract_audit_test.rs:147` 等で実証されており **🟢 GOOD**。memory ルール `feedback_test_data_validation.md`「実データで検証」を満たしている。

### 2.5 既知の `#[ignore]` (未解消バグマーカー)

| テスト | ファイル:行 | 内容 |
|------|----------|------|
| `bug_marker_seekers_marker_name_key_MISSING_bug_marker` | (推定 `global_contract_audit_test.rs`) | Mismatch #1: backend `municipality` キーのみ、frontend `e.name` 期待 |
| `bug_marker_labor_flow_returns_municipality_key` | `global_contract_audit_test.rs:452` | Mismatch #4: backend `location` キーのみ、frontend `data.municipality` 期待 |

**問題**: `docs/contract_audit_2026_04_23.md` で報告されたが、実コード (`jobmap/handlers.rs:399`, `jobmap/company_markers.rs:128`) は**未修正**。bug marker テストは `#[ignore]` のままなので CI 通過しているが、フロントエンドでは `undefined` が出続けている。

---

## 3. 依存関係の整合性

### 3.1 `Cargo.toml` 依存と実コード使用率

| crate | 用途 | 使用箇所 | 評価 |
|------|------|---------|------|
| axum / tokio / tower / tower-http / tower-sessions / http | Web | 多数 | 🟢 必須 |
| askama / askama_axum | テンプレ | (要確認) | 🟢 |
| serde / serde_json | シリアライズ | 多数 | 🟢 必須 |
| rusqlite (bundled) / r2d2 / r2d2_sqlite | DB | `db/local_sqlite.rs` 中心 | 🟢 必須 |
| reqwest (blocking, json) | Turso HTTP | `db/turso_http.rs` | 🟢 必須 |
| dashmap | キャッシュ | `db/cache.rs` | 🟢 必須 |
| bcrypt | 認証 | `auth/mod.rs` (1 ファイル) | 🟢 |
| dotenvy | env ロード | (推定 main.rs) | 🟢 |
| tracing / tracing-subscriber | ログ | 全域 | 🟢 |
| flate2 | gzip 解凍 | DB/GeoJSON 起動時 | 🟢 |
| **time = "0.3"** | tower-sessions Expiry | `lib.rs:61` のみ | 🟡 chrono と重複機能だが API 要件で必須 |
| csv | survey upload | `survey/handlers.rs` | 🟢 |
| rand | bootstrap stats | `survey/statistics.rs` | 🟢 |
| axum-extra (multipart) | アップロード | survey | 🟢 |
| **rust_xlsxwriter** | Excel エクスポート | `insight/export.rs` (1 ファイル) | 🟢 |
| chrono | 時刻 | 11 ファイル | 🟢 必須 |
| **uuid (v4)** | UUID 生成 | `survey/handlers.rs`, `audit/mod.rs` (2 ファイル) | 🟢 |
| urlencoding | URL エンコード | (要確認) | 🟢 |
| **sha2** | ハッシュ | `audit/mod.rs` (1 ファイル) | 🟢 IP ハッシュ用 |

**結論**: 未使用 crate は検出されず。`time` と `chrono` の併存は tower-sessions の API 要件 (`time::Duration::hours(24)`) に起因し、置換不可。

### 3.2 `unused_imports` warning

`grep -r "unused_imports" src/` で 0 件。`Cargo.toml` の `[lints.clippy]` に明記の通り `redundant_clone` / `needless_collect` を warn で運用。

### 3.3 警告抑制ポリシー

```toml
[lints.clippy]
unwrap_used = "allow"   # 既存コード配慮
expect_used = "allow"
panic = "allow"
redundant_clone = "warn"
needless_collect = "warn"
```

🟡 **評価**: `unwrap_used = "allow"` は段階移行の妥当な設計。ただし「段階的に warn へ昇格予定」とコメントされながら作業計画は不在 (TODO ファイルなし)。

---

## 4. 設定・環境変数

### 4.1 `config.rs` で管理されている環境変数 (15 個)

`PORT`, `AUTH_PASSWORD`, `AUTH_PASSWORD_HASH`, `AUTH_PASSWORDS_EXTRA`, `ALLOWED_DOMAINS`, `ALLOWED_DOMAINS_EXTRA`, `HELLOWORK_DB_PATH`, `CACHE_TTL_SECS`, `CACHE_MAX_ENTRIES`, `RATE_LIMIT_MAX_ATTEMPTS`, `RATE_LIMIT_LOCKOUT_SECONDS`, `AUDIT_TURSO_URL`, `AUDIT_TURSO_TOKEN`, `AUDIT_IP_SALT`, `ADMIN_EMAILS`

### 4.2 🔴 `config.rs` 外で直接 `env::var` 読出し (4 個)

| 環境変数 | 読出し位置 | リスク |
|---------|----------|--------|
| `TURSO_EXTERNAL_URL` | `main.rs:83` | 🔴 設定一元管理を逸脱 |
| `TURSO_EXTERNAL_TOKEN` | `main.rs:84` | 🔴 同上 |
| `SALESNOW_TURSO_URL` | `main.rs:113, 125` | 🔴 同上, ログ用に再読込み |
| `SALESNOW_TURSO_TOKEN` | `main.rs:114` | 🔴 同上 |

**なぜ問題か**: `AppConfig` 単一構造を経由する原則が破綻。テスト容易性低下、`from_env()` で一括検証不可、未設定時のフォールバック方針が `main.rs` に散在。

**修正方針**: `AppConfig` に
```rust
pub turso_external_url: String,
pub turso_external_token: String,
pub salesnow_turso_url: String,
pub salesnow_turso_token: String,
```
を追加し、`main.rs` を書換え。空文字列で「未設定」を表すパターンは既存 `audit_turso_url` と同じ。

### 4.3 ハードコード値 (セキュリティリスク評価)

| 値 | ファイル:行 | リスク評価 |
|------|----------|----------|
| `"hellowork-default-salt"` (`AUDIT_IP_SALT` のデフォルト値) | `config.rs:107` | 🟡 **要注意**。本番で `AUDIT_IP_SALT` を設定し忘れると同一 salt で IP ハッシュ化、レインボーテーブル攻撃可能。本番では絶対設定が必要だが、コード上は警告ログがない。 |
| `"f-a-c.co.jp,cyxen.co.jp"` (`ALLOWED_DOMAINS` のデフォルト) | `config.rs:76` | 🟢 業務関連、明示的 |
| `9216` (PORT デフォルト) | `config.rs:55` | 🟢 OK |

**修正方針**: `audit_ip_salt` が「default」のままなら起動時 `tracing::warn!` を出す。

### 4.4 README / docs での文書化状況

- `README.md`: 未確認 (本監査では未読)
- `CLAUDE.md` ルート: 環境変数の言及なし。`feedback_turso_priority.md` メモリでは「Turso 優先」原則が示されているが、`TURSO_EXTERNAL_URL` 等の名前は本ドキュメントに登場しない
- `docs/USER_GUIDE.md` / `docs/USER_MANUAL.md`: 未確認

---

## 5. ドキュメント整合性

### 5.1 `CLAUDE.md` (ルート) の最新性

**最終更新**: 2026-03-14 → **40 日以上未更新**

| 項目 | 現実装 | CLAUDE.md 記載 | 評価 |
|------|--------|---------------|------|
| ハンドラ | `insight/`, `survey/`, `recruitment_diag/`, `region/`, `trend/`, `company/`, `admin/`, `my/` | 記載なし | 🔴 **乖離大** |
| SalesNow 統合 | `main.rs:111-145` で実装済み | 記載なし | 🔴 |
| 認証 | bcrypt + 外部パスワード + ドメイン許可 | あり (簡易) | 🟢 |
| 分析テーブル | "31 テーブル" と記載 | Round 1-3 で 14 + 10 追加されたはず | 🔴 数値乖離 |
| タブ数 | "8 タブ + 6 サブタブ + 市場診断" | 9 タブ実装済 (採用診断含む) | 🟡 |

🔴 **重大**: `recruitment_diag` (採用診断) は 2026-04-23 contract audit で大事故対応した中核機能。それすら CLAUDE.md に未記載。**マスターリファレンスとして機能していない**。

### 5.2 `docs/CLAUDE.md` / `src/handlers/CLAUDE.md`

両者とも空のテンプレート (`*No recent activity*`)。**実質ドキュメントなし**。

### 5.3 `docs/contract_audit_2026_04_23.md` のミスマッチ解消状況

| Mismatch | 内容 | 解消状況 (2026-04-24) |
|---------|------|--------------------|
| **#1** | `/api/jobmap/seekers` で `municipality` キーのみ、frontend `e.name` 期待 | 🔴 **未解消** (`handlers.rs:399` 確認、`"municipality": m_name` のみ) |
| **#2** | `/api/jobmap/seekers` `flows` キー未実装 (silent empty) | 🟡 要件確認待ち |
| **#3** | `/api/jobmap/detail-json/{id}` 7 フィールド欠落 | 🟡 未確認 (DetailRow 構造体は `#[allow(dead_code)]` 付きで一部存在) |
| **#4** | `/api/jobmap/labor-flow` `location` キーのみ、frontend `municipality` 期待 | 🔴 **未解消** (`company_markers.rs:128` 確認、`"location": loc` のみ) |
| **#5** | `center` 形式 object/array 不統一 | 🔵 観察のみ、現状動作中 |

**問題**: 監査から 1 日経過 (2026-04-23 → 2026-04-24) で **#1, #4 は backend 1 行追加で済む修正** が未着手。bug marker テストは `#[ignore]` のため CI 警告も出ていない。

### 5.4 `docs/pdf_design_spec_2026_04_24.md` と現実装

- 設計仕様書: `report_html.rs` を全面再構成し A4 縦 PDF 用に再設計 (Agent P1 設計、P2 実装、P3 QA 体制)
- 現実装: `report_html.rs` は HEAD で 3,912 行。仕様書記載の「2530 行」と既に乖離 (= 仕様書執筆後にも追記が入った可能性)。
- 🟡 注意: 仕様書通りに作業すると `render_section_hw_enrichment_legacy_unused` (147 行) や中間追加コードを巻き込む可能性。再構成前に dead code 削除を推奨。

### 5.5 `docs/flow_ctas_restore.md` の手順現実性

- 対象 `v2_flow_city_agg` / `v2_flow_mesh3km_agg` CTAS 戻し手順
- 各 FALLBACK 箇所に `// FALLBACK: GROUP BY, replace with CTAS after May 1` コメント完備 (`flow.rs:88, 112, 137, 163, 196, 213, 229, 238, 266, 281`, `flow_context.rs:51, 138`)
- ✅ **手順現実的**: コメントマーカー方式で grep 一発復元可能
- 期日 (2026-05-01 Turso リセット) の管理は手動。今日 (2026-04-25) 時点で 1 週間内に対応必要。

### 5.6 memory feedback rule の遵守状況

| ルール | 違反例 |
|------|--------|
| `feedback_dedup_rules.md` (employment_type 含めた dedup) | Rust 側ではコンパイル時保証、SQL 側で要確認 |
| `feedback_never_guess_data.md` (推測禁止) | コード上の問題なし |
| `feedback_test_data_validation.md` (要素存在ではなくデータ妥当性) | global_contract_audit_test.rs は内容アサート実施、🟢 GOOD |
| `feedback_agent_contract_verification.md` (並列 agent 後の cross-check) | global_contract_audit_test.rs で対応済み、🟢 GOOD |

---

## 6. ビルド警告

実行不可 (sandbox 制限) のため静的解析のみ。`Cargo.toml` 設定:

```toml
[lints.clippy]
unwrap_used = "allow"
expect_used = "allow"
panic = "allow"
redundant_clone = "warn"
needless_collect = "warn"
```

### 6.1 静的解析で検出した潜在警告

| 警告種別 | 候補数 | 影響 |
|---------|--------|------|
| dead_code (`#[allow(dead_code)]` で抑制) | 11 箇所 | 🟢 すべて意図的抑制 |
| unused_imports | 0 (grep) | 🟢 |
| `format!` の濫用 (`html.push_str(&format!())` パターン) | **329 箇所** | 🟡 `write!` で代替可、性能微改善 |
| `format!` 全体 | **1,319 箇所** | - |
| `.clone()` | 389 箇所 | 🟡 多くは String 渡し、JSON 値の deep-clone 含む |
| `.unwrap()` | **256 箇所** | 🟡 多くは test、production 経路の panic 可能性は要個別精査 |
| `.expect()` | 25 箇所 | 🟢 OK (失敗時メッセージ付き panic) |

### 6.2 抑制 (`#[allow]`) の妥当性評価

- ✅ `#[allow(dead_code)]` が付いている struct (`PostingRow`, `DetailRow`) は SQL select 結果を保持するためフィールドが将来追加されうる、合理的
- 🔴 `_legacy_unused` は削除タイミング明示なしで悪い慣習

---

## 7. Rust スタイル

### 7.1 `unwrap()` 使用の注意箇所 (Production code)

| ファイル | 件数 | 評価 |
|------|------|------|
| `src/handlers/insight/pattern_audit_test.rs` | 75 | 🟢 テスト |
| `src/handlers/insight/handlers.rs` | 26 | 🟡 production, 要精査 |
| `src/handlers/survey/handlers.rs` | 25 | 🟡 同上 |
| `src/db/local_sqlite.rs` | 24 | 🟢 大半が `#[cfg(test)]` |
| `src/handlers/recruitment_diag/handlers.rs` | 23 | 🟡 production |
| `src/handlers/trend/tests.rs` | 22 | 🟢 テスト |
| `src/handlers/region/karte.rs` | 13 | 🟡 |
| `src/handlers/global_contract_audit_test.rs` | 13 | 🟢 テスト |
| `src/handlers/survey/report_html.rs` | 13 | 🟡 |
| `src/handlers/recruitment_diag/contract_tests.rs` | 12 | 🟢 テスト |

**総計**: 256 件 (34 ファイル)。production 経路で実 panic 可能性のある箇所は handlers/* で多めだが、`Cargo.toml` のポリシー通り「段階移行中」と扱う。

### 7.2 `clone()` の濫用

389 箇所、最多は:
- `recruitment_diag/handlers.rs`: 23
- `recruitment_diag/render.rs:48`, `analysis/render.rs:48`: 多い
- 多くは `String → owned struct field` への移動。`Cow<'_, str>` で削減可能だが、変更コスト > 性能利得。

### 7.3 `format!` を `write!` で代替できる箇所

`html.push_str(&format!(...))` パターン: **329 箇所** (主要箇所):
- `analysis/render.rs`: 188 (= 全体の 57%)
- `survey/report_html.rs`: 150
- `insight/render.rs`: 89
- `company/render.rs`: 80

**改善方針**: `use std::fmt::Write;` を追加し `write!(html, "...")` に置換すれば中間 String 確保を 0 にできる。Morphllm 等のバルク変換ツールを使えば 1 PR で完了可能。性能改善は数 ms 規模だが、慣習として推奨。

### 7.4 `String` vs `&str` の使い分け

目視確認した範囲では適切。serde 構造体は `String`、関数引数は `&str` を多用。

---

## 8. ファイル肥大化

### 8.1 1500 行超ファイル一覧

| ファイル | 行数 | バイト | 評価 |
|------|------|--------|------|
| `src/handlers/analysis/render.rs` | **4,594** | 204,989 | 🔴 単一ファイルが 6 サブタブ × 28 セクション render を抱える。サブタブ単位で分割可能 |
| `src/handlers/survey/report_html.rs` | **3,912** | 147,641 | 🔴 PDF 仕様書 (2026-04-24) で全面再構成予定。再構成時に section 単位へ分割推奨 |
| `src/handlers/analysis/fetch.rs` | 1,897 | 81,079 | 🟡 22 fetch 関数。サブタブ単位で分割可能 |
| `src/handlers/insight/pattern_audit_test.rs` | 1,767 | 51,604 | 🟡 テスト (許容) |
| `src/handlers/insight/engine.rs` | 1,740 | 59,870 | 🟡 22 パターン分析エンジン、責務集中 |
| `src/handlers/insight/render.rs` | 1,605 | 69,867 | 🟡 |
| `src/handlers/region/karte.rs` | 1,511 | 56,703 | 🟡 |

参考: 1000 行超は `company/render.rs` (1365), `survey/location_parser.rs` (1313), `overview.rs` (1299), `survey/aggregator.rs` (1259), `survey/report_html_qa_test.rs` (1241), `diagnostic.rs` (1203), `lib.rs` (1178), `jobmap/handlers.rs` (1103), `competitive/fetch.rs` (1033)。

### 8.2 200 行超 単一関数

| 関数 | ファイル | 確認/推定行数 | 評価 |
|------|--------|---------|------|
| `render_insight_report_page` | `src/handlers/insight/render.rs:244` | **~868 (確認済み)** | 🔴 line 244 → 1112、責務分割推奨 |
| `render_boj_tankan_section` | `src/handlers/analysis/render.rs:8611` (推定) | ~684 | 🔴 |
| `station_map` (const テーブル) | `src/handlers/survey/location_parser.rs` | ~640 | 🟡 静的データ定義のため許容 |
| `linear_regression_points` | `src/handlers/survey/aggregator.rs` | ~576 | 🟡 数学計算、分割困難 |
| `render_css` | `src/handlers/survey/report_html.rs:343` | ~545 | 🟡 CSS 文字列定義。`style.rs` への分離推奨 |
| `render_workstyle` | `src/handlers/workstyle.rs` | ~527 | 🔴 |
| `compute_mode` | `src/handlers/survey/report_html.rs:3513` | ~439 | 🟡 ヒストグラム計算 |
| `aggregate_records_core` | `src/handlers/survey/aggregator.rs` | ~380 | 🔴 |
| `evaluate_diagnostic` | `src/handlers/diagnostic.rs` | ~372 | 🔴 6 軸レーダー診断、責務分割可 |
| `build_app` | `src/lib.rs` | ~357 | 🔴 ルーター定義。エンドポイント単位で extern fn 化推奨 |

**注意**: 一部は awk 概算 (次関数までの距離) であり、関数末尾の `}` 位置は必ずしもそうではない。再構成計画立案時に正確な行数を再計測すること (`tokei` ツール推奨)。

### 8.3 ワークスペース汚染

ルート直下:
- 138 個の `*.png` (E2E 結果スクリーンショット: `d??_*.png`, `check_*.png`, `chart_verify*.png`)
- 4 個の `_*_mock.csv` (`_final_mock`, `_jobbox_mock`, `_mixed_mock`, `_survey_mock`)
- `_sec_tmp/` ディレクトリ (encoding/CSRF/spoof テスト用 CSV、約 14 個)
- `chart_verify*.png`, `check_*.png`, `d??_*.png` 等のテスト出力

`.gitignore` に `*.png`, `_*_mock.csv`, `_sec_tmp/`, `*.bak` のいずれも未登録。

**memory `feedback_git_safety.md`** の「git add -A 禁止」原則と整合するが、ワークスペース汚染自体が起きやすい状態。

---

## 優先 Top 10 リファクタリング項目

### 🔴 P0 (即対応推奨)

1. **契約 Mismatch #1, #4 の修正** (3 行追加)
   - **Why**: bug marker テストが #[ignore] のまま 1 日放置。ユーザーには `undefined: 0人` 表示が出続けている。
   - **How**:
     - `src/handlers/jobmap/handlers.rs:399` に `"name": m_name,` を追加 (既存 `municipality` は維持)
     - `src/handlers/jobmap/company_markers.rs:128` に `"municipality": muni,` を追加 (既存 `location` は維持)
     - `global_contract_audit_test.rs:451` 等の `#[ignore]` を外す

2. **環境変数の `config.rs` 統合**
   - **Why**: `TURSO_EXTERNAL_URL/TOKEN`, `SALESNOW_TURSO_URL/TOKEN` が `main.rs` 直接読出し。テスト不可、設定一元管理逸脱。
   - **How**: `AppConfig` に 4 フィールド追加し `from_env()` で読出し。`main.rs:79-145` を書換え。

3. **ルート `CLAUDE.md` の更新**
   - **Why**: 2026-03-14 から 40 日未更新。`insight/survey/recruitment_diag/region/trend/SalesNow` を一切記載しておらず、新規参入者・自分自身の事故再発リスク。
   - **How**: 現実装ベースで 8 タブ + 採用診断 + SalesNow 統合 + Round 1-3 の数値で書き直す。Memory `feedback_*.md` への参照リンクも追加。

### 🟡 P1 (1〜2 週間)

4. **`render_section_hw_enrichment_legacy_unused` 削除** (147 行)
   - **Why**: dead code。コンパイル時間/レビューノイズの増加。
   - **How**: git rm 1 commit。`#[allow(dead_code)]` 抑制も一緒に消える。

5. **`src/handlers/diagnostic.rs.bak` 削除**
   - **Why**: 37KB のバックアップファイル。`.gitignore` に `*.bak` がないため誤コミット既往リスク。
   - **How**: 削除 + `.gitignore` に `*.bak`, `*.old` 追加。

6. **`.gitignore` 強化** (ワークスペース汚染対策)
   - **Why**: 138 個 PNG, `_*_mock.csv`, `_sec_tmp/` が未除外。`feedback_git_safety.md` の事故記憶。
   - **How**:
     ```gitignore
     # E2E artifacts
     *.png
     d??_*.png
     check_*.png
     chart_verify*.png

     # Test mocks
     _*_mock.csv
     _sec_tmp/

     # Backups
     *.bak
     *.old
     ```

7. **`src/handlers/survey/report_html.rs` の section 分割** (PDF 再構成と合わせて)
   - **Why**: 3,912 行の単一ファイル。`render_section_*` が 25 個ある (line 902, 1234, 1375, 1493, 1641, 1795, 2173, 2299, 2394, 2484, 2535, 2705, 2829, 2863, 2993, 3047, 3170, 3280, 3322 等)
   - **How**: 各 `render_section_*` を `report_html/sections/` 配下のサブモジュールへ移動。`render_css` (545 行) は `style.rs` へ。

### 🟢 P2 (時間あれば)

8. **`render_insight_report_page` (868 行) の責務分割**
   - **Why**: insight/render.rs:244 から ~1112 まで単一関数。テスト困難、変更影響範囲大。
   - **How**: ヘッダ / KPI / セクション群に分け、`render_insight_*` 関数群へ抽出。

9. **`html.push_str(&format!())` → `write!(html, ...)` バルク変換** (329 箇所)
   - **Why**: 中間 String 確保が無駄。慣用的でない。
   - **How**: `use std::fmt::Write;` 追加 + Morphllm/sed バルク置換。性能利得は数 ms だが、レビューしやすくなる。

10. **`config.rs` 起動時警告の追加**
    - **Why**: `audit_ip_salt` が default のまま本番動作するとレインボーテーブル攻撃容易。
    - **How**: `AppConfig::from_env()` 末尾で default 値検出時に `tracing::warn!` を出す。`AUTH_PASSWORD` 未設定時の警告も同様に。

---

## 残課題 (本監査ではカバーしきれなかった項目)

| 項目 | 理由 | 推奨アクション |
|------|------|--------------|
| `cargo build` warning 実行確認 | bash 実行制限のため未実施 | ユーザー手動 `cargo build --release 2>&1 \| grep warning` |
| `cargo test --lib --release` の現状 | 同上 | ユーザー手動。期待値: 645+ pass |
| function ごとの正確な行数測定 | awk 概算のみ | `tokei` ツール導入推奨 |
| `templates/` 配下の重複/未使用 HTML | 監査スコープ外 | 別 Team で実施推奨 |
| `static/js/` 配下の dead code | 同上 | フロントエンドコード健康度監査が必要 |
| `git status` での untracked 確認 | bash 制限 | ユーザー手動 |
| README.md と実環境変数の照合 | 未読 | 次回監査 |
| API_v1 の OpenAPI 完全性 | 未確認 | `docs/openapi.yaml` と `api_v1.rs` の cross-check |

---

## 申し送り

### 親 Team へ

1. **2026-04-23 contract audit ミスマッチ #1, #4 が 1 日経過しても未修正**。コード変更は backend に 1 行追加のみで完了する。Team α (機能担当) に最優先タスクとして引き継ぎ推奨。
2. **`CLAUDE.md` ルート (2026-03-14) は今日のシステムを表していない**。マスターリファレンスが機能していない。Team β (ドキュメント) に再構成依頼推奨。
3. **`flow_ctas_restore.md` の期日 2026-05-01** が 1 週間以内。Turso 無料枠リセット直後に CTAS 投入と Rust コード戻しが必要。マイルストーン化推奨。

### Team β (Test Coverage) との連携

- bug marker test (`#[ignore]`) が 2 件残置。修正完了 PR と一緒に外す運用にすべき。CI が「動いていないことに気づかせる仕組み」として機能していない現状を共有。

### 配置先メモ

本ファイルは指示の `docs/audit_2026_04_24/team_delta_codehealth.md` に書き込む予定であったが、sandbox 書込制限により worktree (`agent-a96d0ab5a54d58e78/`) に出力。手動でコピー要。
