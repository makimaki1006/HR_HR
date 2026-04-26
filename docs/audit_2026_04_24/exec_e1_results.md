# E1 縮退版 実装結果（src/handlers/** スコープのみ）

実施日: 2026-04-26
担当: Frontend Architect agent (E1)
スコープ: `src/handlers/**` のみ。`templates/` への変更は親セッションが担当。

---

## 1. 実装した課題と file:line

### 課題1: 詳細分析タブのフッター注意書き追加（相関≠因果 / HW掲載求人ベース）

- ファイル: `src/handlers/analysis/handlers.rs`
- 場所: 96行目以降（`/api/insight/widget/analysis` のロード trigger 直後 → 閉じ `</div>` の直前）
- 実装内容: `<div class="mt-6 p-3 bg-slate-900/40 border-l-4 border-amber-500 ...">` で「⚠️ 本分析の前提」セクションを追加
  - 「本分析はハローワーク掲載求人ベースです。民間求人サイト（Indeed等）は含まれません。」
  - 「相関関係と因果関係は別物のため、本ダッシュボードでは『傾向』『可能性』表現に留めています。」

### 課題2: 市場概況タブ H2 直下に HW 限定 banner 追加

- ファイル: `src/handlers/overview.rs`
- 場所: H2 (`📊 地域概況`) と既存説明文の間（869〜871 行付近）
- 実装内容: `<div class="p-3 bg-amber-900/20 border-l-4 border-amber-500 ...">` で警告 banner 挿入
  - 「⚠️ 本ダッシュボードはハローワーク掲載求人のみが対象です。民間求人サイト（Indeed・求人ボックス・自社サイト等）の求人は含まれません。」

### 課題3: 平均月給(下限) KPI に tooltip 追加

- ファイル: `src/handlers/overview.rs`
- 場所: 887 行目付近の stat-card div
- 実装内容:
  - stat-card 自体に `title="HW求人は市場実勢より給与を低めに設定する慣習があります"` 属性追加
  - ラベル横に `<span title="...">ⓘ</span>` のインフォアイコン追加（hover で tooltip 表示）

### 課題4: タブ呼称統一（src/handlers/** のみ）

| ファイル | 行 | 旧 | 新 |
|---|---|---|---|
| `src/handlers/competitive/render.rs` | 29 | `/// 競合調査タブの初期HTML` | `/// 求人検索タブの初期HTML` |
| `src/handlers/competitive/render.rs` | 524 | `<title>競合調査レポート - ...` | `<title>求人検索レポート - ...` |
| `src/handlers/competitive/render.rs` | 545 | `<h1>競合調査レポート</h1>` | `<h1>求人検索レポート</h1>` |
| `src/handlers/competitive/handlers.rs` | 22 | `/// タブ8: 競合調査...` | `/// タブ8: 求人検索...` |
| `src/handlers/competitive/fetch.rs` | 64 | `/// 競合調査の基本統計` | `/// 求人検索タブの基本統計` |
| `src/handlers/competitive/fetch.rs` | 166 | `... 競合調査フィルタ用)` | `... 求人検索フィルタ用)` |
| `src/handlers/company/render.rs` | 8 | `<h2>🔎 企業分析` | `<h2>🔎 企業検索` |
| `src/handlers/company/handlers.rs` | 24 | `/// タブ: 企業分析...` | `/// タブ: 企業検索...` |
| `src/handlers/insight/render.rs` | 163 | `"competitive" => "競合"` | `"competitive" => "求人検索"` |
| `src/handlers/insight/render.rs` | 167 | `"survey" => "競合調査"` | `"survey" => "媒体分析"` |
| `src/handlers/survey/mod.rs` | 1, 3 | `競合調査モジュール`, `競合調査レポート` | `媒体分析モジュール`, `媒体分析レポート` |
| `src/handlers/survey/handlers.rs` | 16 | `/// 競合調査タブ` | `/// 媒体分析タブ` |
| `src/handlers/survey/handlers.rs` | 321 | `/// 競合調査PDF/印刷用...` | `/// 媒体分析PDF/印刷用...` |
| `src/handlers/survey/handlers.rs` | 332 | `// 監査: 競合調査レポート生成` | `// 監査: 媒体分析レポート生成` |
| `src/handlers/survey/handlers.rs` | 460 | `/// 競合調査レポートを HTML...` | `/// 媒体分析レポートを HTML...` |
| `src/handlers/survey/report.rs` | 24 | `"title": "競合調査 統合レポート"` | `"title": "媒体分析 統合レポート"` |

**注意**: 以下は変更していません。
- `src/handlers/recruitment_diag/competitors.rs` の競合企業ランキング Panel 4 関連（固有名詞、指示で除外）
- `src/handlers/balance.rs` の H2「🏢 企業分析」（指示の検索ワードは「企業調査」のみで、「企業分析」は含まれない／現タブ名「企業分析」のまま親セッション側で nav を変更する想定）
- `src/handlers/company/render.rs` 1329, 1344 行の「企業分析レポート」（個別企業の詳細レポートタイトルで、analysis 自体が意味のあるラベル）
- `src/handlers/guide.rs` および `src/handlers/market.rs` の「🏢 企業分析」表記（balance.rs と同じ理由で保留。親側のナビ統一に追随予定）
- `src/handlers/survey/report_html.rs:139` のコメント（履歴記録のため保持）
- `src/handlers/survey/report_html_qa_test.rs`（テストファイル、QA 検証用に「競合調査」を文字列リテラルとして使用中）

### 課題5: 媒体分析タブ集計値に「外れ値除外（IQR法）」UI 文言追加

- ファイル: `src/handlers/survey/render.rs`
- 場所:
  - 給与統計（月給換算）カード（341 行付近）: `<h3>給与統計（月給換算）<span ...>外れ値除外（IQR法）</span></h3>`
  - 分布カード（405 行付近）: `<h3>分布<span ...>外れ値除外（IQR法）適用済</span></h3>`
- 実装内容: カード見出しに `<span class="ml-2 text-[10px] font-normal text-slate-500">外れ値除外（IQR法）...</span>` を追記

### 課題6: `render_no_db_data("雇用形態別分析")` → `"詳細分析"` に修正

- ファイル: `src/handlers/analysis/handlers.rs`
- 場所: 23 行（DB 未接続時の早期 return）, 36 行（spawn_blocking 失敗時の fallback）
- 実装内容: ラベル文字列を `"雇用形態別分析"` から `"詳細分析"` に変更

---

## 2. cargo build / cargo test 結果

### cargo build --lib

```
errors: 0
warnings: 既存ワーニングのみ（dead_code, unused_import）
```

### cargo test --lib

```
test result: ok. 646 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

**645 → 646 testsへの増加なし**。既存テスト全件パス。失敗 0 件。

**survey QA テスト（report_html_qa_test）**: 59/59 passed
- `assert_no_forbidden_word(&html, "ハローワーク競合調査")` ✅
- `assert_no_forbidden_word(&html, "競合調査分析")` ✅
- 他 57 件すべて pass

---

## 3. 親セッションへの統合手順（worktree → main）

worktree パス: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\` （実際の編集先）

以下のファイルを worktree から main repo にコピー（または直接 main 側で同じ編集を実施）：

```
src/handlers/analysis/handlers.rs
src/handlers/overview.rs
src/handlers/competitive/render.rs
src/handlers/competitive/handlers.rs
src/handlers/competitive/fetch.rs
src/handlers/company/render.rs
src/handlers/company/handlers.rs
src/handlers/insight/render.rs
src/handlers/survey/mod.rs
src/handlers/survey/handlers.rs
src/handlers/survey/render.rs
src/handlers/survey/report.rs
```

**統合後チェックコマンド**:
```bash
cargo build --lib
cargo test --lib
```

**親セッションが担当する `templates/` 側との整合**:
- 親 nav バー: `求人検索` / `企業検索` / `媒体分析` のラベルへ統一する想定
- 親側で `templates/tabs/competitive.html` の H2 が「競合調査」を含む場合は同様に「求人検索」へ更新が必要
- 親側で `templates/tabs/company.html` 等が H2 を持つ場合も同様

**重複チェック対象（親が編集後に grep）**:
```
grep -rn "競合調査\|企業調査" templates/ src/
```
→ `recruitment_diag/competitors.rs`（許可済み 固有名詞）と `survey/report_html_qa_test.rs`（テスト assertion）以外で hit が無いことを確認。

---

## 4. 補足: vacancy_rate 関連

P0 で完了済とのため、本セッションでは触っていません。
