# L 領域 i18n / 文字種 監査レポート
監査日: 2026-05-13  
対象 crate: `rust_dashboard`  
監査者: Code Review Agent (read-only)

---

## 1. Critical Issues (P0)

### L-01 シングルクォート HTML エスケープの二重実装 — `&#39;` vs `&#x27;` 混在
**優先度: P1**

同一プロジェクト内でシングルクォートの HTML エスケープ表現が 2 種類混在している。

| ファイル | 行 | 表記 |
|---|---|---|
| `src/handlers/helpers.rs` | 55 | `"&#x27;"` (16 進) |
| `src/handlers/competitive/utils.rs` | 9 | `"&#x27;"` (16 進) |
| `src/handlers/balance.rs` | 496, 570 | `"&#39;"` (10 進) |
| `src/handlers/analysis/render/subtab5_anomaly.rs` | 1785, 1836, 2064, 2199 | `"&#39;"` (10 進) |
| `src/handlers/analysis/render/subtab7.rs` | 342, 419 | `"&#39;"` (10 進) |
| `src/handlers/insight/render.rs` | 1184 | `"&#39;"` (10 進) |
| `src/handlers/region/karte.rs` | 882 | `"&#39;"` (10 進) |
| `src/handlers/survey/integration.rs` | 879, 1005 | `"&#39;"` (10 進) |
| `src/handlers/survey/render.rs` | 791, 885 | `"&#39;"` (10 進) |

`&#x27;` と `&#39;` は HTML 上等価だが、統一エスケープ関数 `escape_html`（`helpers.rs:55`）は `&#x27;` を使い、各ハンドラでの手動置換は `&#39;` を使う。コードレビューの一貫性が低い。

**推奨**: `escape_html` に統一し、各所の手動 `.replace('\'', "&#39;")` を廃止する。

---

## 2. Important Issues (P1)

### L-02 月表記の表記ゆれ — `ヶ月` / `ケ月` / `か月` / `カ月` の混在（UI 表示）
**優先度: P1**

賞与月数・試用期間・時系列期間の UI 表示に複数の表記が混在。

| ファイル | 行 | 表記 |
|---|---|---|
| `src/handlers/demographics.rs` | 286–289 | `'1ヶ月以下'`, `'2~3ヶ月'`, `'4~6ヶ月'` |
| `src/handlers/diagnostic.rs` | 244, 1175, 1183 | `ヶ月` |
| `src/handlers/company/render.rs` | 353–355, 401 | `1ヶ月`, `3ヶ月`, `6ヶ月` |
| `src/handlers/company/handlers.rs` | 232 | `従業員増減率(3ヶ月)` (CSV ヘッダ) |
| `src/handlers/survey/salary_parser.rs` | 82, 107 | パーサは `ヶ月`, `ケ月`, `か月`, `カ月`, `ヵ月`, `箇月` の 6 形式を正規化 |
| `src/handlers/survey/report_html_qa_test.rs` | 1025–1028 | テストが `3ヶ月`, `3か月`, `3カ月` の OR 条件で検証（揺れを許容） |

**問題点**: `salary_parser.rs` の入力正規化（HW データ由来の多形式を正規化すること）は適切。しかしシステム生成 UI ラベル（`demographics.rs`、`company/render.rs`、CSV ヘッダ）は統一すべきで、`ヶ月` に統一されていない。

**推奨**: UI 表示・CSV ヘッダは `ヶ月` に統一する。`report_html_qa_test.rs` の OR 条件はパーサ入力テストのみに限定し、UI 出力テストは `ヶ月` を固定で確認する。

---

### L-03 範囲記号のゆれ — `~`（ASCII チルダ）と `〜`（波ダッシュ）の混在
**優先度: P1**

UI に表示される範囲表記に ASCII `~`（U+007E）と全角 `〜`（U+301C）が混在する。

| ファイル | 行 | 表記 | 文脈 |
|---|---|---|---|
| `src/handlers/balance.rs` | 157, 162, 182–187 | `~5人`, `300人~`, `~100万`, `1億~` | SQL ラベル |
| `src/handlers/demographics.rs` | 178–181, 287–289 | `2~3人`, `4~5人`, `2~3ヶ月` | SQL ラベル |
| `src/handlers/workstyle.rs` | 280 | `80~100日` | SQL ラベル |
| `src/handlers/competitive/analysis.rs` | 124 | `~15万`, `15~20万` | Rust 配列ラベル |
| `src/handlers/overview.rs` | 583–584 | `~15万`, `15~20万` | SQL ラベル |
| `src/handlers/survey/salary_parser.rs` | 507–508 | `~15万`, `15~20万` | Rust 文字列 |
| `src/handlers/diagnostic.rs` | 398 | `A〜B` | 全角波ダッシュ |
| `src/handlers/guide.rs` | 237, 384 | `A=容易〜D=困難`, `受付日〜有効期限` | 全角波ダッシュ |
| `src/handlers/competitive/render.rs` | 162 | `{}〜{}件` | 全角波ダッシュ |
| `src/handlers/company/fetch.rs` | 223, 225 | `2〜5秒`, `30〜100秒` | コメント |

**問題点**: 給与帯バケット（`~15万`）はユーザーに直接見える文字列。ASCII `~` と全角 `〜` が混在すると、PDF レポートや画面表示の見た目が不統一になる。日本語文脈では全角 `〜` が標準的。

**推奨**: UI 表示ラベルは全角 `〜` に統一する。コード内コメントは許容範囲。

---

### L-04 「ポイント」と `%` の単位混在（パーセントポイント差分）
**優先度: P1**

パーセントポイントの差分に「ポイント」という日本語単語と `%` 記号が混在する。

```
// src/handlers/company/fetch.rs:1355
"御社は地域平均より{:.1}ポイント下回っています"   // 差分に「ポイント」
"御社は地域の{}業界平均を{:.1}ポイント上回る成長率です"

// src/handlers/handlers.rs:pct関数
pub fn pct(v: f64) -> String { format!("{:.1}%", v * 100.0) }  // 通常は % 記号
```

業界文脈（成長率差分）では "pp" または "ポイント" どちらかに統一すべき。現状 `company/fetch.rs` の提案ポイント文に「ポイント」を使い、他箇所は `%` の差分を直接表示している。

**推奨**: パーセントポイント差を表す場面では `{:.1}%ポイント` または `{:.1}pp` に統一する。

---

### L-05 日付フォーマット混在 — 内部 DB と UI 表示が `YYYYMM` / `YYYY/MM` / `YYYY-MM`
**優先度: P1**

| ファイル | 行 | 形式 | 文脈 |
|---|---|---|---|
| `src/handlers/trend/helpers.rs` | 24–28 | `YYYYMM`（整数）→ `YYYY/MM` ラベル変換 | UI 表示 |
| `src/handlers/recruitment_diag/market_trend.rs` | 242–258 | `YYYY-MM` 文字列 → `YYYY/MM` 正規化 | UI 表示 |
| `src/handlers/company/render.rs` | 307 | `YYYY-MM-DD or YYYY/MM/DD or YYYY` を複数パターン受容 | 企業年齢計算 |
| `src/handlers/jobmap/correlation.rs` | 421 | `"2019-01〜2021-12 / HW snapshot"` | JSON フィールド |
| `src/handlers/jobmap/flow_handlers.rs` | 267 | `"2019-01〜2021-12"` | JSON フィールド |
| `src/handlers/jobmap/flow_types.rs` | 123 | `"2019-01〜2021-12"` | 構造体定数 |
| `src/config.rs` | 7 | `YYYY-MM-DD形式` | 有効期限設定 |
| `src/handlers/analysis/fetch/mod.rs` | 332–334 | `2024Q4` | 外国人在留データの `survey_date` |
| `src/handlers/analysis/render/mod.rs` | 560, 575 | `2024Q1`, `2024Q3` | テストデータ `survey_date` |

**問題点**:  
- UI ラベルは `YYYY/MM` に統一されているが、DB カラム `survey_date` は `2024Q4` 形式（四半期）が混在。  
- `YYYY-MM-DD` と `YYYY/MM/DD` の両方が企業設立年月日として許容されている（`company/render.rs:307`）。  
- 外部統計の `survey_date` が `YYYYQn` 形式でありトレンドタブの `YYYYMM` と体系が異なる。

**推奨**: UI 表示は `YYYY/MM` に統一。DB 内の `survey_date` カラムは形式を設計書で明示し、フロント変換層で統一出力する。`YYYYQn` は別フィールドとして区別する。

---

### L-06 CSV エンコーディング混在 — `utf-8-sig` / `cp932` / `utf-8` が共存
**優先度: P1**

| スクリプト | エンコーディング | 備考 |
|---|---|---|
| `scripts/fetch_boj_tankan.py:225` | `utf-8-sig` (出力) | BOJ TANKAN データ |
| `scripts/fetch_foreign_residents.py:245,302` | `utf-8-sig` | 外国人在留者データ |
| `scripts/fetch_census_demographics.py:200,289` | `utf-8-sig` | 国勢調査 |
| `scripts/fetch_industry_structure.py:320` | `utf-8-sig` | 産業構造データ |
| `scripts/import_ssdse_to_db.py:67` | `cp932` 優先フォールバック | SSDSE-A（Shift-JIS 由来） |
| `scripts/ts_phase0_validate.py:37,54` | `cp932` | HW 時系列 CSV（元データ） |
| `scripts/ts_phase1_extract.py:148` | `cp932, errors='replace'` | HW 時系列抽出 |
| `scripts/build_commute_flow_summary.py:272` | `utf-8` | 通勤フロー |
| `scripts/build_municipality_code_master.py:251` | `utf-8` | 市区町村マスタ |

**問題点**: 生成 CSV が `utf-8-sig`（BOM 付き）と `utf-8`（BOM なし）に分かれている。Rust 側（`src/`）は BOM を意識しないまま読み込む可能性がある。`cp932` 起源の HW 時系列 CSV は `errors='replace'` で読み込まれており、文字化けリスクがある。

**推奨**:  
1. スクリプト出力は `utf-8`（BOM なし）に統一する（Excel 互換が必要な場合のみ `utf-8-sig`）。  
2. `ts_phase1_extract.py` の `errors='replace'` をログ付き `errors='strict'` に変更し、文字化け行を明示的に検出する。

---

## 3. Suggestions (P2)

### L-07 テスト関数名に日本語漢字が混在
**優先度: P2**

```
// src/handlers/survey/report_html/market_intelligence.rs:4309
fn print_summary_uses_該当なし_for_zero_priority_sa()
```

Rust の関数名に日本語を使うことは技術的には有効だが、変数名・関数名は英語という CLAUDE.md ルールに違反している。他の全テスト関数は英語（snake_case）で命名されており、孤立した例外となっている。

**推奨**: `print_summary_uses_gaitou_nashi_for_zero_priority_sa` のようにローマ字転写する。

---

### L-08 絵文字を HTML エンティティで埋め込み（`&#x1F3E2;` 等）
**優先度: P2**

```
// src/handlers/company/render.rs:85
format!(" <span ...>&#x1F3E2; {}</span>", ...)
// src/handlers/company/render.rs:485
r#" <a ...>&#x1F517;</a>"#
// src/handlers/company/render.rs:911
<h4 ...>&#x1F4A1; 提案ポイント</h4>
// src/handlers/company/render.rs:1031
<h4 ...>&#x1F4CA; ...</h4>
// src/handlers/company/render.rs:1079
<h4 ...>&#x1F4B0; ...</h4>
```

絵文字を `&#x1F4A1;` のような数値参照で埋め込んでいる。UTF-8 ソースファイルに直接 `💡` と書いた方が視認性が高く、Rust の文字列リテラルは UTF-8 として完全に安全。コードレビュー時の可読性が低い。

**推奨**: UTF-8 ソース中に直接絵文字を書く（例: `💡 提案ポイント`）。ただし、CLAUDE.md の「絵文字はユーザーが明示的に要求した場合のみ」ルールに従い、絵文字使用自体を廃止する選択肢も検討する。

---

### L-09 「Qn 四半期」フォーマットが `survey_date` カラムに混在
**優先度: P2**

外国人在留者データの `survey_date` に `2024Q4` 形式が使われており、他のトレンドデータの `snapshot_id`（YYYYMM 整数）と体系が全く異なる。

```
// src/handlers/analysis/fetch/mod.rs:332–334
('都道府県', 'visa', 0, '2024Q4'),
('東京都', '永住者', 100000, '2024Q4'),
('北海道', '技能実習', 5000, '2024Q4')
```

フォーマットが異なるカラムに同じ列名 `survey_date` を使うと、JOIN や比較で混乱が生じる恐れがある。

**推奨**: 四半期データは `survey_quarter`（例: `2024Q4`）、月次データは `snapshot_id`（YYYYMM 整数）と列名で区別する。

---

### L-10 資本金バケットラベルの数字が半角と表記ゆれ（上限表記のみ）
**優先度: P2**

```
// src/handlers/balance.rs:182–187
'~100万', '~500万', '~1000万', '~5000万', '~1億', '1億~'
```

`~1000万` と `~1億` が混在するバケット（10,000万 = 1億）は金額の桁が一貫しておらず、UIの並び順と視認性に影響する。また給与バケット（`src/handlers/competitive/analysis.rs:124` の `~15万`, `15~20万`）では万円省略形が使われているが、千万・億の境界だけ漢数字に切り替わる。

**推奨**: バケット境界を `~5,000万`, `5,000万~1億`, `1億~` のように統一するか、全て万円表記（`~10,000万`）に揃える。

---

## 4. 指摘なし / 良好な実装

- **都道府県名の join key 統一**: `東京都`/`神奈川県` など都道府県名は全箇所で「都/道/府/県」付きフルネームを使用。「東京」のみの短縮形は SQL join key では検出されなかった（P0 該当なし）。
- **数値桁区切り**: `format_number()` 関数（`src/handlers/helpers.rs:100–110`）が全数値表示に一貫適用されており、コンマ区切りは統一。
- **コメント言語**: `src/` 全域でコメントは日本語、変数名・関数名は英語という CLAUDE.md ルールがほぼ遵守されている（L-07 の例外 1 件のみ）。
- **HTML エスケープ体制**: `escape_html` および `escape_url_attr` が適切に実装され、XSS 対策は機能している（表現方法の揺れは L-01 で指摘）。
- **改行コード**: `\r\n` 混在ファイルは `src/` に検出されなかった。
- **全角英数字**: コード・UI ラベルとも全角英数字（Ａ-Ｚ、０-９）は検出されなかった。
- **UTF-8 徹底**: Rust ソースは全て UTF-8。Python スクリプトの Windows cp932 対応は `reconfigure(encoding="utf-8")` で適切に処理されている。
- **YYYY-MM-DD 形式**: 認証設定（`config.rs`）、ISO-8601 ログ（`audit/mod.rs`）は一貫して ISO 8601 を使用。

---

## 5. 推奨アクション（優先順）

| 優先 | ID | アクション | ファイル |
|---|---|---|---|
| P1 | L-02 | UI ラベルの月表記を `ヶ月` に統一 | `demographics.rs`, `company/render.rs`, `company/handlers.rs` |
| P1 | L-03 | UI 表示ラベルの範囲記号を全角 `〜` に統一 | `balance.rs`, `demographics.rs`, `competitive/analysis.rs`, `overview.rs` |
| P1 | L-01 | シングルクォートエスケープを `&#x27;` に統一 | `balance.rs`, `subtab5_anomaly.rs`, `subtab7.rs` 等 9 箇所 |
| P1 | L-05 | `survey_date` フォーマット仕様を設計書に明記 | `analysis/fetch/mod.rs`, `analysis/render/mod.rs` |
| P1 | L-06 | スクリプト出力 CSV を `utf-8`（BOM なし）に統一 | `scripts/fetch_*.py`, `ts_phase1_extract.py` |
| P1 | L-04 | パーセントポイント差の表記を `%ポイント` または `pp` に統一 | `company/fetch.rs` |
| P2 | L-07 | テスト関数名の日本語漢字をローマ字転写 | `survey/report_html/market_intelligence.rs:4309` |
| P2 | L-09 | `survey_quarter` / `snapshot_id` カラム名を区別 | `analysis/fetch/mod.rs` |
| P2 | L-10 | 資本金バケットラベルの桁表記を統一 | `balance.rs` |
| P2 | L-08 | 絵文字 HTML エンティティを UTF-8 直書きまたは廃止 | `company/render.rs` |
