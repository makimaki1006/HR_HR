# Plan P3: V2 HW Dashboard コード健全性整理プラン

**作成日**: 2026-04-26
**作成者**: Refactoring Expert (Agent P3)
**対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`)
**前提監査**:
- `docs/audit_2026_04_24/00_overall_assessment.md`
- `docs/audit_2026_04_24/team_delta_codehealth.md` (技術負債スコア 6.0/10 Moderate)
**並行作業**:
- 親セッション: P0 緊急バグ (jobmap Mismatch #1/#4, MF-1, vacancy_rate 概念混乱)
- Agent P1/P2: PDF 設計仕様書 (`docs/pdf_design_spec_2026_04_24.md`) — `survey/report_html.rs` 全面再構成
- Turso 5/1 リセット: CTAS fallback 14 箇所の戻し作業 (別 PR)

**スコープ**: 中長期の構造リファクタリング (コード編集は実施せず計画のみ)
**制約**: 既存 647 テスト破壊禁止 / `[lints.clippy] unwrap_used = "allow"` 段階移行尊重 / PDF 仕様書衝突回避

---

## 0. エグゼクティブサマリ

| カテゴリ | 課題数 | 推奨着手 | 推定総工数 |
|---|---|---|---|
| P1 即着手 (1 週間以内) | 5 件 (#1〜#5) | 親セッション P0 完遂後 | 1.5 〜 2 人日 |
| P2 1〜2 週間以内 | 4 件 (#6〜#9) | PDF 再構成と協調 | 4 〜 5 人日 |
| P2 2〜4 週間 (継続的改善) | 4 件 (#10〜#13) | 別タイムボックス | 3 〜 4 人日 |
| **合計** | **13 課題** | | **8.5 〜 11 人日** |

### 全体方針

1. **PDF 仕様書 (Agent P1/P2) と衝突しない順序**: `survey/report_html.rs` 内の dead code 削除 (#2) を P1/P2 着手前に完了。再構成中は当該ファイルへの他改変停止
2. **ビッグバンを避ける**: ファイル分割 (#6, #7, #8, #9) は **モジュール境界を先に確定** → 関数移動 → import 修正 → テスト の 1 PR を 1 サブタブ単位で
3. **bug marker (#[ignore]) の運用ルール化** を環境変数統合 (#1) と同じ PR で実施し、CI が「気づける」状態に戻す
4. **環境変数移行 (#1) は backwards-compat 維持**: `.env` フォーマット非破壊 (環境変数名は維持、読出し場所のみ変更)

---

## 1. P1 課題 (1 週間以内)

### #1 環境変数 4 個 を `AppConfig` に統合

#### 現状

| 環境変数 | 読出し位置 | 現コード | 行数影響 |
|---|---|---|---|
| `TURSO_EXTERNAL_URL` | `src/main.rs:83` | `std::env::var("TURSO_EXTERNAL_URL").ok()` | match 内 30 行 |
| `TURSO_EXTERNAL_TOKEN` | `src/main.rs:84` | 同上 | 同上 |
| `SALESNOW_TURSO_URL` | `src/main.rs:113, 125` | 二重読出し (line 125 はログ用) | match 内 35 行 |
| `SALESNOW_TURSO_TOKEN` | `src/main.rs:114` | 同上 | 同上 |

`src/config.rs:48-115` の `AppConfig::from_env()` には `audit_turso_url/token` (line 104-105) が既に同じパターンで定義されており、4 個追加するための実績テンプレが存在。

#### リスク

| リスク | 評価 |
|---|---|
| 本番影響 | 🟢 低 (環境変数名は維持、読出し時点のみ shift) |
| 起動時動作変更 | 🟢 None (`Option<TursoDb>` の生成ロジックは `main.rs` に残し、`AppConfig` は値保持のみ) |
| テスト破壊 | 🟢 None (`config.rs` のテストは clear_env() に 4 keys 追加するだけ) |
| `from_env()` 副作用 | 🟢 None (既存と同じく `String::default()` フォールバック) |

#### 修正手順 (3 commit に分割)

**Commit 1**: `AppConfig` フィールド追加
- `src/config.rs:13-46` の struct に 4 フィールド追加 (`turso_external_url, turso_external_token, salesnow_turso_url, salesnow_turso_token: String`)
- `src/config.rs:48-115` の `from_env()` 末尾に 4 行追加 (既存 `audit_turso_url` と同パターン)
- `src/config.rs:127-145` の `clear_env()` に 4 keys 追加

**Commit 2**: `main.rs` の読出しを置換
- `src/main.rs:82-109` の `match (std::env::var("TURSO_EXTERNAL_URL").ok(), ...)` を `match (Some(config.turso_external_url.clone()), Some(config.turso_external_token.clone()))` に変更
- `src/main.rs:111-145` の SalesNow 同様に置換
- `src/main.rs:125` の二重読出しを `config.salesnow_turso_url.clone()` に統一

**Commit 3**: 起動時警告 (#10 と統合可能)
- `from_env()` 末尾で 4 個全空かつ非テスト時に `tracing::warn!("Turso external/SalesNow not configured")` を出す

#### 検証

```bash
cargo check                              # 型エラーなし
cargo test --lib config::tests           # 既存 2 テスト + 4 keys clear 確認
cargo test --lib                         # 全 647 テスト維持
```

#### 工数 + 推奨担当

- **0.5 人日** (約 4 時間)
- **手作業推奨** (Morphllm は環境変数の semantic 移動に向かない)
- 親セッションの P0 完了後に着手

#### Backwards-compat

- ✅ 環境変数名 (`TURSO_EXTERNAL_URL` 等) は不変
- ✅ `.env` ファイル / Render dashboard / docker 環境設定の変更不要
- ✅ 未設定時の挙動 (`None` で動作続行) は不変

---

### #2 `render_section_hw_enrichment_legacy_unused` 削除

#### 現状

- ファイル: `src/handlers/survey/report_html.rs:1493-1640` (約 147 行)
- 直前コメント `line 1491`: `// === 以下は旧実装（未使用、将来削除予定） ===`
- 現行版: 同ファイル `render_section_hw_enrichment` (line 1375-1492, 約 117 行) が使用中
- `#[allow(dead_code)]` 付き (`team_delta_codehealth.md:1.1` 参照)

#### リスク

| リスク | 評価 |
|---|---|
| 機能影響 | 🟢 None (呼出 0 件) |
| コンパイル時間 | 🟡 -2〜3% 想定 (147 行 + ヘルパ) |
| PDF 再構成衝突 | 🔴 **重要**: P1/P2 着手後に削除すると merge conflict 必発 |

#### 修正手順

**🔴 タイミング厳守**: PDF 再構成 (Agent P2) **着手前** に 1 commit で削除

```
Commit: chore(survey): remove render_section_hw_enrichment_legacy_unused (-147 lines)
- Delete fn render_section_hw_enrichment_legacy_unused (line 1493-1640)
- Delete comment "// === 以下は旧実装" (line 1491)
- Verify no callers via `grep -r "render_section_hw_enrichment_legacy_unused" src/`
```

#### 検証

```bash
grep -rn "render_section_hw_enrichment_legacy_unused" src/  # → 0 件
cargo build --release                                       # 警告ゼロ
cargo test --lib survey::                                   # report_html_qa_test.rs 維持
```

#### 工数 + 推奨担当

- **0.25 人日** (1〜2 時間, grep + 削除 + 検証)
- **手作業** (削除のみ)

#### Backwards-compat

- ✅ git 履歴に残るため復元容易
- ✅ public API への影響なし (`pub` でない関数)

---

### #3 `src/handlers/diagnostic.rs.bak` 削除

#### 現状

| ファイル | サイズ | 修正日 | 用途 |
|---|---|---|---|
| `src/handlers/diagnostic.rs.bak` | 37,265 B | 2026-03-31 | Phase 6 拡張前のバックアップ |
| `src/handlers/diagnostic.rs` | 47,292 B | 現行 | 現行版 |

`.gitignore` に `*.bak` パターン未登録 → 既にコミット済み (誤コミット既往)。

#### リスク

| リスク | 評価 |
|---|---|
| 機能影響 | 🟢 None (`.bak` は Rust コンパイル対象外) |
| 誤参照 | 🟡 IDE が `.bak` を Rust file 認識する場合あり |
| 再発防止 | 🔴 `.gitignore` 未強化なら同種の `.bak` が再度コミットされる |

#### 修正手順 (#4 と同 PR 推奨)

**Commit**: `chore: cleanup .bak file and reinforce .gitignore`
1. `git rm src/handlers/diagnostic.rs.bak`
2. `.gitignore` 強化 (#4)
3. リポジトリ全体 `.bak` / `.old` スキャン: `find . -name "*.bak" -o -name "*.old" | grep -v target/`

#### 検証

```bash
find . -name "*.bak" -not -path "./target/*"  # → 0 件
find . -name "*.old" -not -path "./target/*"  # → 0 件
cargo build --release                          # 警告ゼロ
```

#### 工数 + 推奨担当

- **0.25 人日**
- **手作業**

#### Backwards-compat

- ✅ git 履歴で復元可能 (`git show HEAD~1:src/handlers/diagnostic.rs.bak`)

---

### #4 `.gitignore` 強化

#### 現状

`.gitignore` 39 行 (build artifacts / data / .DS_Store 等のみ)。以下が **未登録**:

| 汚染パターン | 件数 (確認済) | リスク |
|---|---|---|
| `*.png` (E2E 結果スクリーンショット) | 138 個 (ルート直下) | 🔴 リポジトリ膨張 |
| `_*_mock.csv` (`_final_mock`, `_jobbox_mock`, `_mixed_mock`, `_survey_mock`) | 4 個 | 🟡 |
| `_sec_tmp/` ディレクトリ (encoding/CSRF/spoof テスト CSV) | 約 14 個 | 🟡 |
| `*.bak` / `*.old` | 1 個 (#3) + 既往リスク | 🔴 |
| `chart_verify*.png`, `check_*.png`, `d??_*.png` | 上記 138 内 | - |

#### リスク

| リスク | 評価 |
|---|---|
| 既存ファイル削除誤爆 | 🔴 **重要**: 単純な `.gitignore` 追加では既コミット済みファイルは tracked のまま。意図して `git rm --cached` する必要 |
| memory ルール `feedback_git_safety.md` (本番 geojson 消失事故) | 🔴 「git add -A 禁止」遵守でも `.gitignore` 強化が抜け穴塞ぎになる |

#### 修正手順 (3 段階)

**Stage 1**: `.gitignore` 追記 (commit 1)
```gitignore
# E2E test artifacts (auto-generated)
*.png
chart_verify*.png
check_*.png
d??_*.png

# Test mocks
_*_mock.csv
_sec_tmp/

# Backups
*.bak
*.old

# Coverage / lcov
*.profraw
target/llvm-cov/
```

**Stage 2**: 既コミット済みファイルの cache 解除 (commit 2)
```bash
# 重要: 削除ではなく untrack のみ
git ls-files | grep -E '\.(png|bak|old)$' | xargs git rm --cached
git ls-files | grep -E '_.*_mock\.csv$' | xargs git rm --cached
git ls-files | grep '^_sec_tmp/' | xargs git rm --cached
```

**🔴 注意**: `static/geojson/` 配下の `.png` (アイコン等) があれば `!static/geojson/*.png` で例外指定 (要事前確認)。

**Stage 3**: pre-commit hook 推奨 (`.git/hooks/pre-commit`) — オプション
- `.bak`, `.old` ファイル add 検出時 abort

#### 検証

```bash
git status --ignored | grep -E '\.(png|bak|old)$'           # 全件 ignored 表示
git ls-files | grep -E '\.(png|bak|old)$' | grep -v static/  # 0 件
```

#### 工数 + 推奨担当

- **0.5 人日** (Stage 1+2 の慎重な検証含む)
- **手作業** (memory ルール `feedback_git_safety.md` 由来で `git add -A` 禁止 → ファイル名指定で 1 件ずつ)

#### Backwards-compat

- ⚠️ E2E スクリーンショット消失リスク: ローカル開発者が手元で見たい場合は `--force` 取得必要
- ✅ ビルド成果物への影響なし

---

### #5 dead route 6 件 の生死判定

#### 現状

`src/lib.rs` で以下 6 ルートが定義されているが `templates/dashboard_inline.html:70-89` のナビには **存在しない**:

| ルート | ハンドラ | テンプレート | UI 経路 |
|---|---|---|---|
| `/tab/overview` | `handlers::overview::tab_overview` | `templates/tabs/overview.html` | ❌ |
| `/tab/balance` | `handlers::balance::tab_balance` | `templates/tabs/balance.html` | ❌ |
| `/tab/workstyle` | `handlers::workstyle::tab_workstyle` | `templates/tabs/workstyle.html` | ❌ |
| `/tab/demographics` | `handlers::demographics::tab_demographics` | `templates/tabs/demographics.html` | ❌ |
| `/tab/trend` | `handlers::trend::tab_trend` | (`trend/render.rs`) | ❌ ナビ非搭載 |
| `/tab/insight` | `handlers::insight::tab_insight` | (`insight/render.rs`) | ❌ ナビ非搭載 |

加えて `/api/insight/report` 系は **外部 API として現役の可能性**:
- `README.md:21-22`: 「`/api/insight/report` - JSON API」「`/api/insight/report/xlsx` - Excel ダウンロード」
- `docs/openapi.yaml:193, 200`: OpenAPI で公開
- `e2e_api_excel.py:184-188, 259-264`: E2E テストが叩いている
- `docs/contract_audit_2026_04_23.md:30`: 「frontend consumer なし」だが OpenAPI に載っている = MCP/AI 連携用

E2E テストファイルでも `/tab/overview, /tab/balance, /tab/workstyle` は呼ばれている (`e2e_8fixes_verify.py:146-148`, `e2e_chart_json_verify.py:88-90`)。**E2E が dead route を直接叩いている** = テストは通るが UI からは到達不可、という乖離。

#### リスク

| リスク | 評価 |
|---|---|
| 削除誤爆 | 🔴 **高**: insight/trend は `/tab/insight` 削除しても `/api/insight/widget`, `/api/insight/report*` は維持必要 |
| E2E 破壊 | 🟡 `e2e_*.py` 多数が `/tab/overview` 等を直接 GET。削除なら更新必要 |
| 復活パス | 🟢 ナビに 1 行追加で UI 復活可能 |
| 外部利用者 | 🔴 **未確認**: `/api/insight/report*` を Render ログ /nginx ログで確認するまで削除禁止 |

#### 修正手順 (4 段階, 段階的検証)

**Stage 1: 外部利用ログ確認 (削除前の必須前提)**
```bash
# Render ログ (URL タブから過去 7 日アクセス)
# pattern: /api/insight/report または /api/insight/report/xlsx
# pattern: /tab/overview, /tab/balance, /tab/workstyle, /tab/demographics, /tab/trend, /tab/insight

# nginx access.log (もしあれば)
grep -E '(/api/insight/report|/tab/(overview|balance|workstyle|demographics|trend|insight))' \
  /var/log/nginx/access.log | awk '{print $7}' | sort | uniq -c
```

判定基準 (Stage 1 結果に基づき分岐):
- A. UI から復活すべき (高頻度アクセスあり) → ナビに追加 (#6 と同じ PR)
- B. 外部 API のみ生存 → `/tab/*` のみ削除、`/api/insight/report*` 維持
- C. 完全 dead → ハンドラ + テンプレ + ルート全削除

**Stage 2: テンプレートと V1 由来コード調査** (`templates/tabs/CLAUDE.md` 確認)
- `templates/tabs/overview.html` の変数 `{{AVG_AGE}}=月給`, `{{MALE_COUNT}}=正社員数` は V1 求職者ダッシュボードの遺物 (team α 報告)
- 誤使用すると即事故 → V1 残骸であれば削除推奨

**Stage 3: 削除 (判定 C の場合のみ)**

各ルート 1 commit で分割:
```
Commit 3-1: feat: remove dead route /tab/overview
- src/lib.rs: route definition 削除
- src/handlers/overview.rs: 削除 (1299 行)
- src/handlers/mod.rs: pub mod overview; 削除
- templates/tabs/overview.html: 削除
- e2e_*.py: /tab/overview 行を削除/コメントアウト
```

同様に balance, workstyle, demographics, trend (insight は API 残存のため別扱い)。

**Stage 4: insight タブの特殊処理**
- `/tab/insight` UI ハンドラのみ削除
- `/api/insight/report`, `/api/insight/report/xlsx`, `/report/insight`, `/api/insight/widget/*`, `/api/insight/subtab/*` は **維持**
- `tab_insight` 関数のみ削除、他は `pub fn` 残置

#### 検証

```bash
# 削除後
cargo test --lib                                    # 全 647 → 修正後の値
cargo build --release                               # 警告ゼロ
grep -rn "tab_overview\|/tab/overview" src/         # 0 件
python e2e_chart_json_verify.py                     # E2E 失敗 OK (pattern 削除済)
```

#### 工数 + 推奨担当

- **Stage 1**: 0.25 人日 (ログ確認, ユーザー手動)
- **Stage 2**: 0.25 人日 (テンプレ調査)
- **Stage 3**: 1.0 人日 (5 ルート × 0.2 人日)
- **Stage 4**: 0.25 人日 (insight 特殊処理)
- **合計**: **約 1.75 人日**
- **手作業推奨** (Morphllm は外部利用判定に不向き、削除自体は単純)

#### Backwards-compat

- ⚠️ Stage 1 結果次第: 外部利用者ありなら方針変更
- ⚠️ E2E テスト 4 ファイル更新必要 (`e2e_8fixes_verify.py`, `e2e_chart_json_verify.py`, `e2e_c1_c4_coverage.py`, `docs/E2E_TEST_PLAN*.md`)
- 🔴 ナビからアクセス不可 = 既に UI 上は dead = エンドユーザー影響なし

---

## 2. P2 課題 (1〜2 週間以内)

### #6 `survey/report_html.rs` (3,912行) の section 分割

#### 現状

| 関数 | 行数 (推定) | 役割 |
|---|---|---|
| `render_survey_report_page` | line 100-240 (約 140 行) | エントリポイント。20 セクションを順次呼出 |
| `render_section_executive_summary` | 902-1233 (332 行) | exec summary |
| `render_section_summary` | 1234-1374 (141 行) | overview |
| `render_section_hw_enrichment` | 1375-1492 (117 行) | HW 統合 |
| `render_section_hw_enrichment_legacy_unused` | 1493-1640 (147 行) | 削除対象 (#2) |
| `render_section_hw_comparison` | 1795-2172 (377 行) | HW 比較 |
| `render_section_salary_stats` | 2173-2298 (125 行) | 給与統計 |
| `render_section_employment` | 2394-2483 (89 行) | 雇用形態 |
| `render_section_emp_group_native` | 2299-2393 (94 行) | 雇用グループ |
| `render_section_region` | 2484-2534 (50 行) | 地域 |
| `render_section_municipality_salary` | 2993-3046 (53 行) | 市区町村給与 |
| `render_section_min_wage` | 2863-2992 (129 行) | 最賃 |
| `render_section_company` | 2535-2704 (169 行) | 企業 |
| `render_section_tag_salary` | 3047-3169 (122 行) | タグ別給与 |
| `render_section_job_seeker` | 3322-3868 (546 行) | 求職者 |
| `render_section_salesnow_companies` | 3170-3279 (109 行) | SalesNow |
| `render_section_notes` | 3280-3321 (41 行) | 注記 |
| `render_section_scatter` | 2705-2862 (157 行) | 散布図 |
| `render_css` | 343-902 (約 545 行) | CSS 定義 |
| その他: `compute_mode` (3513-, 約 439 行), 共通ヘルパ | - | - |

#### リスク

| リスク | 評価 |
|---|---|
| **PDF 再構成衝突** | 🔴 **最重要**: Agent P2 が `report_html.rs` 全面再構成中。本作業は **その後** に実施 |
| 既存テスト破壊 | 🟡 `report_html_qa_test.rs` (1241 行 / 48,857 B) が pub でない関数を含めて検証している可能性 |
| import チェーン | 🟡 ヘルパ関数 (`compute_mode`, `linear_regression_points` 等) の可視性調整が必要 |

#### 修正手順 — モジュール境界 (PDF 再構成完了後)

```
src/handlers/survey/
├── report_html.rs                         # 残置: render_survey_report_page (entry, 約 140 行)
└── report_html/                           # ★新ディレクトリ
    ├── mod.rs                             # pub use sections::*; pub use style::render_css;
    ├── style.rs                           # render_css (545 行)
    ├── helpers.rs                         # compute_mode, linear_regression_points (約 1000 行)
    └── sections/
        ├── mod.rs
        ├── executive_summary.rs           # render_section_executive_summary (332 行)
        ├── summary.rs                     # render_section_summary
        ├── hw_enrichment.rs               # render_section_hw_enrichment (117 行)
        ├── hw_comparison.rs               # render_section_hw_comparison (377 行)
        ├── salary.rs                      # render_section_salary_stats + emp_group_native
        ├── employment.rs                  # render_section_employment
        ├── region.rs                      # render_section_region + municipality_salary + min_wage
        ├── company.rs                     # render_section_company + salesnow_companies + tag_salary
        ├── scatter.rs                     # render_section_scatter
        ├── job_seeker.rs                  # render_section_job_seeker (546 行)
        └── notes.rs                       # render_section_notes
```

**1 PR = 1 セクション** で段階的移行 (12 PR)。各 PR で:
1. 該当 section をサブモジュールに移動 (関数本体 + ヘルパ)
2. 可視性調整 (`pub(super)` で十分なものは `pub` 化しない)
3. テスト実行 (`cargo test --lib survey::report_html_qa_test`)
4. 失敗があればロールバック

#### 検証

```bash
cargo test --lib survey::report_html_qa_test  # 全パス維持
cargo test --lib survey::                     # 関連 200+ テスト維持
cargo clippy --release                        # 警告ゼロ維持
wc -l src/handlers/survey/report_html.rs      # 140 行程度に縮減
```

#### 工数 + 推奨担当

- **2.5 人日** (12 PR × 0.2 人日, ヘルパ抽出含む)
- **手作業推奨** (Morphllm は関数移動 + import 修正の自動化に不向き、テスト破壊リスク高)
- **🔴 着手タイミング**: PDF 再構成 (Agent P2) **完了後 + 1 週間 cooldown**

#### Backwards-compat

- ✅ public API 不変 (`render_survey_report_page` のみ呼出)
- ✅ HTML 出力バイト一致テストでもパス維持

---

### #7 `analysis/render.rs` (4,594行) のサブタブ単位分割

#### 現状

`src/handlers/analysis/handlers.rs:136-145` で 7 サブタブに分岐:

```rust
match id {
    1 => render_subtab_1(&db, &pref, &muni),
    2 => render_subtab_2(...),
    ...
    7 => super::render::render_subtab_7(...),
}
```

`render.rs` は `render_subtab_1〜7` + 35 個の `render_*_section` 関数を抱える。

35 セクションのうち判明分 (`grep` 結果):
- vacancy / resilience / transparency / temperature / competition / cascade / anomaly
- salary_structure / salary_competitiveness / compensation / text_quality / keyword_profile
- employer_strategy / monopsony / spatial_mismatch / minimum_wage / wage_compliance
- job_openings_ratio / labor_stats / prefecture_stats / population / demographics
- establishment / turnover / household_spending / business_dynamics / climate
- care_demand / region_benchmark / fulfillment / mobility / shadow_wage
- education / household_type / foreign_residents / land_price / regional_infra
- social_life / boj_tankan

#### リスク

| リスク | 評価 |
|---|---|
| 大規模 PR | 🔴 4,594 行 → サブタブ単位でも 1 PR 約 600 行 |
| ヘルパ共有 | 🟡 `kpi`, `build_commute_sankey`, `build_butterfly_pyramid` は複数 section から呼ばれる可能性 |
| テスト | 🟡 公開関数は `render_subtab_1〜7` のみ。section 関数は `pub(super)` 想定 |

#### 修正手順 — モジュール境界

```
src/handlers/analysis/
├── handlers.rs                  # tab_analysis, analysis_subtab (現行維持)
├── fetch.rs                     # query_3level (現行維持) ※ #8 で別途再構成
├── render.rs                    # 残置: render_subtab_1〜7 (各々 30〜80 行のオーケストレータ化)
└── render/                      # ★新ディレクトリ
    ├── mod.rs
    ├── helpers.rs               # kpi, build_commute_sankey, build_butterfly_pyramid
    └── sections/
        ├── mod.rs
        ├── vacancy.rs           # render_vacancy_section + resilience + transparency + temperature + competition + cascade + anomaly  (subtab 1 系)
        ├── salary.rs            # render_salary_structure_section + salary_competitiveness + compensation  (subtab 2 系)
        ├── text.rs              # render_text_quality_section + keyword_profile + employer_strategy + monopsony + spatial_mismatch  (subtab 3 系)
        ├── wage.rs              # render_minimum_wage_section + wage_compliance + job_openings_ratio + labor_stats + prefecture_stats  (subtab 4 系)
        ├── population.rs        # render_population_section + demographics + establishment + turnover  (subtab 5 系)
        ├── household.rs         # household_spending + business_dynamics + climate + care_demand  (subtab 5 系)
        ├── benchmark.rs         # region_benchmark + fulfillment + mobility + shadow_wage  (subtab 6 系)
        └── extra.rs             # education + household_type + foreign_residents + land_price + regional_infra + social_life + boj_tankan  (subtab 7 系)
```

**🔴 重要 (1 PR の粒度)**: サブタブ 1 つ分ずつ (1〜7 で 7 PR)。各 PR:
1. 当該サブタブが呼ぶ section 関数群を 1 ファイルに移動
2. `render.rs` 内 `render_subtab_N` は `super::render::sections::xxx::render_*_section` 呼出に修正
3. ヘルパ重複は 2 件目移動時に `helpers.rs` に統一

#### 検証

```bash
cargo test --lib analysis::                # 関連テスト維持
cargo build --release                      # 警告ゼロ
wc -l src/handlers/analysis/render.rs      # 4,594 → 約 600 行
```

#### 工数 + 推奨担当

- **2.0 人日** (7 PR × 0.3 人日)
- **手作業推奨**
- **着手タイミング**: PDF 再構成と独立 (異なるファイル)

#### Backwards-compat

- ✅ public API 不変 (`render_subtab_1〜7` 維持)
- ✅ HTML 出力不変

---

### #8 `analysis/fetch.rs` (1,897行) のサブタブ単位分割

#### 現状

`src/handlers/analysis/fetch.rs:48` で `fn query_3level` のみ pub 関数として grep 検出。実態は 22 個の private fetch 関数 (`fetch_vacancy`, `fetch_resilience` 等) を持つ可能性 (`team_delta_codehealth.md:8.1` 「22 fetch 関数」記載)。

#### リスク

| リスク | 評価 |
|---|---|
| 関数命名規約 | 🟡 fetch 関数の命名が `fetch_*` で統一されていない可能性 |
| クエリ重複 | 🟢 重複 SQL があれば統合機会 |
| ヘルパ関数 | 🟢 `query_3level` は共通化済 |

#### 修正手順

`render/sections/` と同じ粒度で `fetch/sections/` に分割。サブタブ単位で 7 PR。

```
src/handlers/analysis/
└── fetch/
    ├── mod.rs                   # pub use; common helper (query_3level)
    └── sections/
        ├── vacancy.rs           # subtab 1 系の SQL クエリ群
        ├── salary.rs            # subtab 2 系
        ├── text.rs              # subtab 3 系
        ├── wage.rs              # subtab 4 系
        ├── population.rs        # subtab 5 系
        ├── benchmark.rs         # subtab 6 系
        └── extra.rs             # subtab 7 系
```

#### 検証

```bash
cargo test --lib analysis::
cargo build --release
```

#### 工数 + 推奨担当

- **1.5 人日**
- **手作業推奨**
- **着手タイミング**: #7 と同 sprint 推奨 (相互依存解消で同時に進む)

#### Backwards-compat

- ✅ pub API 不変 (`query_3level`)

---

### #9 `render_insight_report_page` (868行) の責務分割

#### 現状

`src/handlers/insight/render.rs:244-1112` の単一関数 (約 868 行)。`team_delta_codehealth.md:8.2` 記載。

```rust
pub(crate) fn render_insight_report_page(
    insights: &[Insight],
    ctx: &InsightContext,
    pref: &str,
    muni: &str,
) -> String { ... }
```

#### リスク

| リスク | 評価 |
|---|---|
| テスト困難 | 🔴 単一関数のため部分検証不可 |
| 変更影響 | 🟡 PDF 出力 (`/report/insight`) に直結 |
| 派生関数命名 | 🟢 `render_insight_*` 命名規約あり |

#### 修正手順

責務分割 (1 PR で 4 commit に分割可能):

1. `render_insight_header(ctx, pref, muni) -> String` (約 80 行)
2. `render_insight_kpi_section(insights) -> String` (約 100 行)
3. `render_insight_pattern_groups(insights) -> String` (約 600 行) — さらに 4 group に分割可能
4. `render_insight_footer(ctx) -> String` (約 80 行)
5. `render_insight_report_page` が上記 4 を結合する 30 行のオーケストレータ化

#### 検証

```bash
cargo test --lib insight::
# HTML 出力バイト一致テスト (もし存在すれば) で diff チェック
```

#### 工数 + 推奨担当

- **1.5 人日**
- **手作業推奨**
- **着手タイミング**: #7 と並行可能

#### Backwards-compat

- ✅ pub API 不変 (`render_insight_report_page`)

---

## 3. P2 継続的改善 (2〜4 週間)

### #10 `format!` → `write!` バルク変換 (329 箇所)

#### 現状

`html.push_str(&format!(...))` パターンが 329 箇所。`team_delta_codehealth.md:7.3` 内訳:
- `analysis/render.rs`: 188 (57%)
- `survey/report_html.rs`: 150
- `insight/render.rs`: 89
- `company/render.rs`: 80

#### リスク

| リスク | 評価 |
|---|---|
| 性能利得 | 🟢 数 ms 規模 (中間 String 確保削減) |
| HTML 出力差 | 🟢 None (同等の文字列生成) |
| エスケープ漏れ | 🟡 `write!` マクロは format args の評価順は同じだが、誤改変リスクあり |

#### 修正手順 (Morphllm 推奨)

**Step 1**: 各ファイル冒頭に `use std::fmt::Write;` を追加

**Step 2**: パターン置換 (Morphllm bulk transform):
```rust
// Before
html.push_str(&format!("<div>{}</div>", x));
// After
let _ = write!(html, "<div>{}</div>", x);
```

**🔴 注意**:
- `write!(String, ...)` は `Result<(), fmt::Error>` を返すが `String` の write は実質エラー発生しない → `let _ = ` または `.unwrap()` (`Cargo.toml` 設定で許容)
- ヒット箇所が `Vec<String>::push(format!(...))` の場合は対象外 (Vec 構築は別パターン)

**Step 3**: ファイル単位で順次変換 (4 PR):
- PR 1: `analysis/render.rs` (188 箇所)
- PR 2: `survey/report_html.rs` (150 箇所) — #6 の section 分割 **後** に実施
- PR 3: `insight/render.rs` (89 箇所) — #9 の責務分割 **後**
- PR 4: `company/render.rs` (80 箇所)

#### 検証

```bash
cargo test --lib                       # 全 647 維持
# ベンチマーク (オプション)
cargo bench --bench render_bench       # 数 ms 改善測定
```

#### 工数 + 推奨担当

- **1.0 人日** (Morphllm 利用前提)
- **Morphllm bulk transform 推奨** (パターン明確で機械的、レビュー容易)
- **着手タイミング**: #6/#7/#9 完了後 (ファイル分割中の merge conflict 回避)

#### Backwards-compat

- ✅ HTML 出力バイト不変

---

### #11 `config.rs` 起動時警告 (audit_ip_salt)

#### 現状

`src/config.rs:106-107`:
```rust
audit_ip_salt: env::var("AUDIT_IP_SALT")
    .unwrap_or_else(|_| "hellowork-default-salt".to_string()),
```

本番で `AUDIT_IP_SALT` 未設定なら同一 salt → レインボーテーブル攻撃可能 (`team_delta_codehealth.md:4.3`)。警告ログなし。

#### リスク

| リスク | 評価 |
|---|---|
| セキュリティ | 🔴 **重要**: salt 既知化 → IP 復元可能 |
| 既存運用影響 | 🟢 警告のみ追加、動作不変 |

#### 修正手順 (1 commit)

`src/config.rs::from_env()` 末尾に追加:

```rust
// 起動時セキュリティ警告
const DEFAULT_IP_SALT: &str = "hellowork-default-salt";
let cfg = Self { /* ... 既存 */ };
if cfg.audit_ip_salt == DEFAULT_IP_SALT {
    tracing::warn!(
        "AUDIT_IP_SALT がデフォルト値です。本番では必ず固有 salt を設定してください"
    );
}
if cfg.auth_password.is_empty() && cfg.auth_password_hash.is_empty() {
    tracing::warn!("AUTH_PASSWORD / AUTH_PASSWORD_HASH 未設定です");
}
cfg
```

#### 検証

```bash
cargo test --lib config::tests
# 起動時ログ確認
RUST_LOG=warn cargo run --release
```

#### 工数 + 推奨担当

- **0.25 人日**
- **手作業**
- **#1 と同 PR で実施推奨** (`AppConfig::from_env()` の同時改変)

#### Backwards-compat

- ✅ 動作不変、警告のみ追加

---

### #12 bug marker `#[ignore]` 運用ルール化

#### 現状

`#[ignore]` 付き bug marker テスト 2 件 (`team_delta_codehealth.md:2.5`):
- `bug_marker_seekers_marker_name_key_MISSING_bug_marker` (jobmap Mismatch #1)
- `bug_marker_labor_flow_returns_municipality_key` (Mismatch #4)

**問題**: 修正完了 PR と一緒に `#[ignore]` を外す運用がない → CI silent。memory ルール `feedback_agent_contract_verification.md` (採用診断 8 パネル全滅事故) と同根原因。

#### 修正手順 (3 段階)

**Stage 1: ドキュメント化** (`docs/contract_audit_2026_04_23.md` 改訂 or 新規 `docs/bug_marker_workflow.md`)

```markdown
# Bug Marker テスト運用ルール

## 原則
- `#[ignore]` 付き bug marker テストは「現在失敗するが修正方針を保留」を表す
- バグ修正 PR では同じ commit で `#[ignore]` を **必ず削除** する
- 24 時間以上 `#[ignore]` 状態の bug marker は CI で warning 通知

## チェックリスト
- [ ] バグ修正 commit で当該テストの `#[ignore]` 行を削除した
- [ ] `cargo test --include-ignored` でローカル全パス確認
- [ ] PR description に `closes bug_marker_*` を明記
```

**Stage 2: CI scripts 追加** (`scripts/check_ignored_tests.sh`)

```bash
#!/bin/bash
# 24h 以上更新されていない #[ignore] を warn
git log --since="24 hours ago" --diff-filter=A --name-only | grep -q "tests"
ignored=$(grep -rn "^\s*#\[ignore" src/ | wc -l)
if [ "$ignored" -gt 0 ]; then
    echo "⚠️ ${ignored} ignored tests detected. Review:"
    grep -rn "^\s*#\[ignore" src/
fi
```

**Stage 3: GitHub Actions 統合** (`.github/workflows/ci.yml`)
```yaml
- name: Check ignored tests
  run: bash scripts/check_ignored_tests.sh
```

#### 工数 + 推奨担当

- **0.5 人日** (ドキュメント + script + CI 統合)
- **手作業**
- **着手タイミング**: 親セッションの jobmap Mismatch 修正 PR と同期

#### Backwards-compat

- ✅ 既存テスト動作不変
- ⚠️ CI に warning 段階追加 (失敗化は段階移行)

---

### #13 `*.unwrap()` 256 箇所の段階的削減ロードマップ

#### 現状

`team_delta_codehealth.md:7.1` 内訳:

| ファイル | 件数 | 経路 |
|---|---|---|
| `pattern_audit_test.rs` | 75 | テスト (許容) |
| `insight/handlers.rs` | 26 | **production** |
| `survey/handlers.rs` | 25 | **production** |
| `local_sqlite.rs` | 24 | 大半 `#[cfg(test)]` |
| `recruitment_diag/handlers.rs` | 23 | **production** |
| `trend/tests.rs` | 22 | テスト |
| `region/karte.rs` | 13 | **production** |
| `survey/report_html.rs` | 13 | **production** |
| その他 | 35 | 混在 |
| **合計** | **256** | - |

`Cargo.toml:79` `unwrap_used = "allow"` 段階移行中。

#### リスク

| リスク | 評価 |
|---|---|
| 本番 panic | 🟡 graceful degradation (`team_beta_system.md:3`) で大半は handle されているが、`unwrap()` は最終ガード破り |
| 段階移行ペース | 🟢 production 約 100 箇所 / 月 1 ファイル ≒ 10 ヶ月 |

#### 修正手順 (4 sprint に分割)

**Sprint 1 (Week 1-2)**: `local_sqlite.rs` 24 件
- 大半 `#[cfg(test)]` で許容、production 経路のみ `?` 演算子化
- 期待削減: -15 件

**Sprint 2 (Week 3-4)**: `insight/handlers.rs` 26 件
- DB クエリ unwrap → `Result` 伝播 → graceful 空応答
- 期待削減: -20 件

**Sprint 3 (Week 5-6)**: `survey/handlers.rs` 25 件
- CSV パース unwrap → エラー応答
- 期待削減: -20 件

**Sprint 4 (Week 7-8)**: `recruitment_diag/handlers.rs` 23 件 + `region/karte.rs` 13 件
- 採用診断 8 panel 並列ロード経路の unwrap 削減
- 期待削減: -30 件

**Sprint 5 (Week 9-10)**: 残箇所 + `Cargo.toml` 設定変更
- `unwrap_used = "allow"` → `unwrap_used = "warn"` 昇格
- 残 unwrap は `#[allow(clippy::unwrap_used)]` で個別正当化

#### 検証

```bash
cargo clippy --release --all-features -- -W clippy::unwrap_used 2>&1 | grep -c "unwrap_used"
# 数値が sprint ごとに減少することを確認
```

#### 工数 + 推奨担当

- **5 sprint × 1.0 人日 = 5.0 人日** (継続改善, 別タイムボックス)
- **手作業推奨** (Result 伝播の意図解釈が機械化困難)
- **着手タイミング**: P1 完遂 + #6/#7/#8/#9 完了後の並行 sprint

#### Backwards-compat

- ✅ Result 伝播でも空応答化すれば外部 API 不変
- ⚠️ エラーログ増加可能性 (graceful degradation 経路で `tracing::warn!` 追加)

---

## 4. 全体ロードマップ

```
Week 1 (2026-04-26 〜 05-02): P0 完遂中。本プランは P1 着手準備のみ
   親セッション: jobmap Mismatch #1, #4 / MF-1 / vacancy_rate
   P3 タスク: なし (待機)

Week 2 (05-03 〜 05-09): P1 一気呵成
   #1 環境変数統合          (0.5 人日)
   #2 dead code 削除        (0.25 人日)  ★ PDF 再構成前に必達
   #3 .bak 削除             (0.25 人日)
   #4 .gitignore 強化       (0.5 人日)
   #5 dead route 判定       (1.75 人日, Stage 1 ログ確認含む)
   #11 config 警告          (0.25 人日)  ★ #1 と同 PR
   #12 bug marker ルール    (0.5 人日)   ★ 親セッション修正 PR と同期

Week 3-4 (05-10 〜 05-23): PDF 再構成 (Agent P1/P2)
   P3 待機 (report_html.rs への他改変停止)

Week 5-6 (05-24 〜 06-06): P2 大規模分割
   #6 report_html.rs 分割   (2.5 人日)   ★ PDF 完了 + 1 週間 cooldown 後
   #7 analysis/render.rs    (2.0 人日)   並行可
   #8 analysis/fetch.rs     (1.5 人日)   並行可
   #9 insight/render.rs     (1.5 人日)   並行可

Week 7 (06-07 〜 06-13): バルク変換
   #10 format! → write!      (1.0 人日)  Morphllm

Week 8-12 (06-14 〜 07-18): 継続改善
   #13 unwrap 削減 5 sprint  (5.0 人日)
```

---

## 5. テスト破壊リスク評価

| 課題 | 影響テスト | 破壊リスク | 緩和策 |
|---|---|---|---|
| #1 環境変数 | `config::tests` | 🟢 低 | `clear_env()` に 4 keys 追加 |
| #2 dead code 削除 | `report_html_qa_test.rs` | 🟢 低 (関数未参照) | grep で呼出 0 確認 |
| #3 .bak 削除 | なし | 🟢 None | - |
| #4 .gitignore | E2E スクリーンショット | 🟡 中 | ローカル開発者へ事前周知 |
| #5 dead route 削除 | `e2e_*.py` 4 ファイル | 🔴 高 | E2E pattern 削除 + ナビ復活も検討 |
| #6 section 分割 | `report_html_qa_test.rs` (1241 行) | 🔴 高 | 1 PR 1 セクション + HTML byte-diff テスト |
| #7 analysis 分割 | `tests.rs` (analysis 系) | 🟡 中 | サブタブ単位 PR |
| #8 fetch 分割 | 同上 | 🟡 中 | 同上 |
| #9 insight 関数分割 | `insight/render.rs` テスト | 🟢 低 | 部分関数化、出力不変 |
| #10 format! 変換 | 全 render テスト | 🟡 中 | HTML byte-diff スナップショット |
| #11 config 警告 | `config::tests` | 🟢 低 | tracing log capture 不要 |
| #12 bug marker ルール | 既存テスト | 🟢 None | ドキュメント + script のみ |
| #13 unwrap 削減 | sprint ごと | 🟡 中 | sprint 単位で全テスト確認 |

---

## 6. 親セッションへの申し送り Top 5

### 即着手 (今週中, 親セッション P0 完了後すぐ)

#### 1. **#2 `render_section_hw_enrichment_legacy_unused` 削除 (0.25 人日)**
- **理由**: PDF 再構成 (Agent P2) 着手前に必達。後で削除すると merge conflict 必発
- **着手者**: P3 または親セッションどちらでも可
- **依存**: なし
- **commit**: `chore(survey): remove dead legacy unused renderer (-147 lines)`

#### 2. **#1 環境変数 4 個 を `AppConfig` に統合 (0.5 人日) + #11 起動時警告 (0.25 人日)**
- **理由**: テスト容易性 + セキュリティ (audit_ip_salt 警告)
- **着手者**: P3 推奨 (`AppConfig` 単一責任の整理)
- **依存**: なし
- **commit 数**: 3 (`feat(config): add 4 turso fields` / `refactor(main): use AppConfig` / `feat(config): warn on default ip_salt`)

### 1 週間以内 (Week 2 中)

#### 3. **#5 dead route 6 件 の生死判定 — Stage 1 (ログ確認, 0.25 人日, ユーザー手動)**
- **理由**: 削除前提の Stage 2-4 が全て Stage 1 結果に依存
- **着手者**: ユーザー手動 (Render dashboard / nginx ログ参照)
- **アウトプット**: 「削除可」「ナビ復活」「API のみ生存」の 3 択判定
- **未確認のまま削除は禁止** (`/api/insight/report*` の MCP/AI 連携利用が判明済み)

#### 4. **#4 `.gitignore` 強化 + #3 `.bak` 削除 (0.75 人日)**
- **理由**: memory ルール `feedback_git_safety.md` (本番 geojson 消失事故) 直系
- **着手者**: P3 推奨
- **🔴 注意**: `git rm --cached` は誤爆リスク高、ファイルパターン 1 件ずつ確認 (memory ルール「git add -A 禁止」遵守)

### 2 週間以内 (Week 3 までに準備完了)

#### 5. **#12 bug marker `#[ignore]` 運用ルール化 (0.5 人日)**
- **理由**: jobmap Mismatch #1/#4 修正 PR と同期で `#[ignore]` 削除運用を確立。CI silent 失敗の再発防止
- **着手者**: P3 + 親セッション (修正 PR と統合)
- **依存**: 親セッションの Mismatch 修正 PR 完了
- **アウトプット**: `docs/bug_marker_workflow.md` + `scripts/check_ignored_tests.sh`

---

## 7. 制約遵守チェック

| 制約 | 遵守確認 |
|---|---|
| コード編集禁止 | ✅ 本ドキュメントは計画のみ、コード変更なし |
| 既存 647 テスト破壊しない設計 | ✅ Section 5 で全 13 課題のリスク評価実施 |
| `[lints.clippy] unwrap_used = "allow"` 段階移行尊重 | ✅ #13 で 5 sprint の段階移行ロードマップ提示 |
| PDF 設計仕様書衝突回避 | ✅ #2/#6 はタイミング指定 (PDF 着手前 / 完了後) |
| memory `feedback_git_safety.md` 整合 | ✅ #4 で `git rm --cached` の慎重運用明記 |
| memory `feedback_agent_contract_verification.md` 整合 | ✅ #12 で bug marker 運用ルール化 |
| backwards-compat | ✅ 全課題で環境変数名・public API・HTML 出力の不変性確認 |

---

## 8. 残課題 (本プラン対象外)

| 項目 | 理由 | 後継担当 |
|---|---|---|
| `cargo build / test` 実行ログ取得 | sandbox 制約 | ユーザー手動 |
| `physicians` テーブル単位検証 (MF-1) | 親セッション P0 担当 | 親セッション |
| CTAS fallback 14 箇所戻し作業 | 5/1 Turso リセット待ち | 別 PR (親セッション or 別 agent) |
| ルート `CLAUDE.md` (40+ 日未更新) 再構成 | ドキュメント担当 | Team β / 別 agent |
| `templates/` 配下 dead 確認 | フロント監査スコープ | 別 Team |
| `static/js/` dead code | 同上 | 別 Team |
| 統合 PDF レポート機能新規実装 | 機能追加スコープ (Plan P3 はリファクタリング限定) | 別企画 |
| 47 県横断比較ビュー新規実装 | 同上 | 別企画 |

---

**作成完了**: 2026-04-26
**ファイル**: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\plan_p3_code_health.md`
