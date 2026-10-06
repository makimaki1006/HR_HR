# 競合調査レポート：コミット担当セッションへの引き継ぎ

作成日：2026-10-05

## 1. 現在の状態

| 項目 | 内容 |
| --- | --- |
| 作業フォルダ | `C:/dev/competitor_insights` |
| ブランチ | `feat/competitor-report-insights` |
| 基準HEAD | `17532dc5d3abbc653e4d679be25dc2be445e05da` |
| 変更の状態 | **すべて未コミット。新規ファイルもある** |
| コミット・Push・マージ・Render反映 | このセッションでは未実施。別セッションが担当 |
| 共有側の作業フォルダ | `C:/Users/fuji1/orca/workspaces/HR_HR/gurnard`。今回の変更を書き戻していない |

**取り込むものは作業フォルダの未コミット差分と新規ファイル。現在のHEADやブランチ名をcherry-pickするだけでは、今回の変更は入らない。**

コミット先に別セッションの修正がある場合は差分を統合し、既存ファイルを丸ごと上書きしない。基準HEADには独立した競合調査ページと直接PDFダウンロードの実装が既にある。古いチェックアウトへ取り込む場合は、その前提機能・ルート・依存関係も確認する。

## 2. ユーザーの意図と反映内容

Excelを基準とする競合調査を独立機能として維持し、各タブで給与・待遇、Google検索需要、Indeed採用市場、人口、採用のヒントを確認できるようにする。個別HTMLで改善した表示をアプリのレポート生成処理に移植済み。

- **キーワード**：表は上位10語、グラフは上位20語。件数と割合を表示し、余白を減らした。全体と先頭の比較は共通の0〜100%軸。
- **給与の縦棒**：空の給与区間を残す。ピークの帯・件数・割合・母数を表示し、同数ピークも保持。
- **時系列**：Googleの月間検索数、Indeedの求人数・閲覧人数・募集企業数・1求人あたり閲覧人数をSVGで描画。元の数値表は残す。
- **人口**：都道府県指定時はその地域、未指定時は全国の市区町村データを既存DBから合算。男女比・年齢構成比・人口ピラミッドを表示。構成比の分母は総人口とし、年齢区分の合計との差は別行で示す。
- **補足の簡略化**：長い注意書きを短くし、タブを「給与・待遇」「採用のヒント」などに整理。未取得の関連語は空の見出しを表示しない。取得済みの関連語は表示する。
- **PDF**：直接ダウンロードの既存経路を維持。5タブをA3横5ページに収める。20語のグラフの高さもページ収容の計算に反映。
- **CSV**：Excel原本で使われていたIndeed SPの`css-f6zp9m`列名を認識。職種名と求人URLを保持し、異なる求人キーを誤って重複削除しない。
- **レスポンシブ**：SVGの再描画と`contain:size`により、画面幅の切替でキーワード欄の高さが増幅する問題を修正。

Rust/Axumを維持。React移行・CRM・認証の変更はない。DBへの書き込み、新しい永続ストア、外部サービスへの公開は実施していない。

## 3. コミットに含めるアプリ実装

パスはすべて作業フォルダからの相対パス。

| ファイル | 状態 | 内容 |
| --- | --- | --- |
| `src/handlers/competitor.rs` | 変更 | 全国人口の取得、人口の総数・基準日をレポートへ渡す |
| `src/handlers/regional_analysis/fetch.rs` | 変更 | 人口のNULLを保持する集計、地域／全国の総人口集計、SQLの反例テスト |
| `src/handlers/survey/upload.rs` | 変更 | 原本SP列名への対応と求人キーの重複処理テスト |
| `src/handlers/survey/report_html/navy_report/competitor_report.rs` | 変更 | 各タブ、給与・キーワード・時系列・人口の描画と検証用出力 |
| `src/handlers/survey/report_html/navy_report/competitor_keywords.rs` | **新規** | 件数・占有率の比較、20語のグラフと安全なJSON埋め込み |
| `src/handlers/survey/report_html/navy_report/competitor_consultation.rs` | **新規** | 給与の比較、訴求の確認候補、採用のヒント |
| `src/handlers/survey/report_html/navy_report/competitor_trends.rs` | **新規** | Google／Indeedの時系列SVGと欠測・ゼロの反例テスト |
| `src/handlers/survey/report_html/navy_report/competitor_population.rs` | **新規** | 総人口を分母にした構成比、人口ピラミッド、不整合の反例テスト |
| `static/js/competitor-keywords.js` | **新規** | 表示サイズに合わせたSVG再描画 |
| `static/css/competitor-dashboard.css` | 変更 | グラフ、人口構成比、採用のヒント、スマホ・印刷レイアウト |
| `templates/competitor.html` | 変更 | 短い案内、全国人口表示に合う説明、5タブPDFの案内 |
| `src/handlers/competitor_pdf.rs` | 変更 | 20語のグラフと採用のヒントを固定ページへ収容 |
| `scripts/pdf/render.cjs` | 変更 | PDFの5ページ・5パネル検証 |

**`competitor_report.rs`だけを取り込まないこと。** `#[path]`で読む4つのRustモジュール、`include_str!`で読むCSS・JS、PDFヘルパーをセットで含める。新規ファイルは通常の`git diff`には出ないため、`git status --short`でも確認する。

## 4. テスト・文書

### テストファイル

- `scripts/pdf/validation.test.cjs`：5ページへの対応、Playwrightモジュールの指定。
- `tests/e2e/pr/competitor.spec.ts`：5タブへの更新。認証付きE2Eは今回未実行。
- **新規** `scripts/verify_competitor_app_visuals.py`：アプリ生成HTMLで、20語の値・時系列の全プロット・人口の割合・表示幅・タブ操作を検証。
- **新規** `scripts/verify_competitor_keyword_preview.py`：以前の567件Excel原本を独立照合した検証スクリプト。古い注記・出力構成への前提が残るため、最新表示の検証には上記の新しいスクリプトを使う。

### 意図した表示変更に合わせて更新したgolden HTML（15本）

フォルダ：`tests/fixtures/competitor/golden/`

```text
h1_sp_utf8_monthly.html
h2_sp_utf8_hourly.html
h3_plain_no_sp_monthly.html
h4_missing_salary_column_200.html
h9_ten_thousand_rows.html
pdf_document_r1.html
pdf_document_r2.html
r1_monthly_google_ok_indeed_ok_pop_ok.html
r2_hourly_google_missing_indeed_none_pop_unavailable.html
r3_monthly_google_error_indeed_ok_pop_ok_escaped_title.html
r4_hourly_google_timeout_indeed_no_series_pop_ok.html
r5_monthly_google_not_requested_indeed_ok_pop_unavailable.html
r6_hourly_google_ok_indeed_ok_pop_ok.html
r7_monthly_no_sp_data_all_external_missing.html
r8_hourly_no_sp_data_google_ok.html
```

比較を無効にしたのではない。更新後、`COMPETITOR_UPDATE_GOLDEN`を設定しない通常モードで70件成功を確認した。

### 文書

- この引き継ぎ文書。
- `docs/competitor-report-enhancement-2026-10-05.md`：末尾の「公開用HTMLの表示改善をアプリに反映」が最新。
- `docs/competitor-report-review-three-loops-2026-10-05.md`：以前の3回のレビュー・修正・逆証明の記録。
- `docs/marketing-trial-monthly-manufacturing-logistics-2026-10-05.md`：トライアル作成時の履歴。追加のExcel等も作った時点の記録であり、現在の納品物を示すものではない。

以前の文書にある「上位10語」「4ページ」「人口未取得」は当時の状態。最新はグラフ20語、5タブ・5ページ、取得できた人口の基準日・割合を表示する実装。

## 5. 最新の検証結果

| 確認 | 結果 | ローカルログ・証拠 |
| --- | --- | --- |
| `cargo test --lib competitor`（golden更新なし） | **70成功・0失敗・1 ignored** | `target/app-report-tests-normal.log` |
| `cargo test --lib population_report_tests` | **2成功・0失敗** | `target/population-report-tests.log` |
| 実際のRust→Node→EdgeでのPDF生成 | **1成功・0失敗**。通常版とデータ行数の多い検証版 | `target/app-report-pdf-tests.log` |
| ブラウザと値の照合 | 製造・物流ともに20語、Indeed4グラフ、Google12か月、人口18本の棒の値を照合 | `target/app-report-preview/visual-verification.json` |
| 画面幅・操作 | 320／900／1600px、5タブ、キーボード操作、ページの横はみ出しなし。幅変更時の高さも確認 | `scripts/verify_competitor_app_visuals.py` |
| PDF本文 | 通常版・検証版ともA3横5ページ、文字のページ外はみ出しなし | `target/app-report-preview/manufacturing/report.pdf`、`report.rich.pdf` |
| 実人口DBに対する集計SQL | 47都道府県・1,741市区町村・126,146,099人。男女合計一致、基準日2020-10-01 | `target/marketing-trials/population-national.json`、`target/app-report-manifest.json` |
| 整形・差分 | 変更Rustの`rustfmt --check`、JSの`node --check`、`git diff --check`成功 | ローカル実行 |

浮動小数の値は丸め誤差を許容して照合。月・件数は厳密に照合する。欠測をゼロにしない、欠測では線を切る、総人口と男女合計が矛盾した場合は割合を作らない等の反例も検証済み。

Rust全体のテスト、認証付きアプリ全体のE2E、Google本番APIへの再問い合わせ、Renderでの動作確認は未実施。生成HTMLをEdgeで開いた検証と、本番ログインからのE2Eを混同しない。

## 6. 再確認コマンド

通常の検証ではgolden更新を有効にしない。

```powershell
Set-Location 'C:/dev/competitor_insights'
Remove-Item Env:COMPETITOR_UPDATE_GOLDEN -ErrorAction SilentlyContinue
cargo test --lib competitor
cargo test --lib population_report_tests
node --check static/js/competitor-keywords.js
git diff --check
```

ローカルの保存実データを使ってアプリHTMLを再生成する場合（外部APIを呼ばない）：

```powershell
$env:COMPETITOR_TRIAL_MANIFEST='C:/dev/competitor_insights/target/app-report-manifest.json'
cargo test --lib export_marketing_trials_when_requested -- --nocapture
Remove-Item Env:COMPETITOR_TRIAL_MANIFEST
$env:PYTHONIOENCODING='utf-8'
python scripts/verify_competitor_app_visuals.py
```

マニフェスト・元CSV・保存DBはGit管理外なので、このPCにある場合にだけ実行できる。CIの必須テストはそのローカル資料に依存しない。

PDFの実生成確認にはNode・Edge／Chromium・Playwrightが必要：

```powershell
$env:COMPETITOR_PDF_TEST_HTML='C:/dev/competitor_insights/target/app-report-preview/manufacturing/report.html'
$env:PDF_PLAYWRIGHT_MODULE='C:/Users/fuji1/orca/workspaces/HR_HR/gurnard/target/competitor-release/node_modules/playwright-core'
cargo test --lib export_fixed_pdf_from_real_report -- --ignored
```

他の環境では`PDF_PLAYWRIGHT_MODULE`をその環境のモジュールパスへ変更し、必要なら`PDF_CHROMIUM_PATH`も指定する。

## 7. 公開用HTMLとコミット対象外のもの

ユーザーが広告のフックとして使用する、シンプルにした個別HTML：

- 製造：`C:/dev/competitor_insights/target/marketing-trials/manufacturing/製造.html`
- 物流：`C:/dev/competitor_insights/target/marketing-trials/logistics/物流.html`

公開・ホスティングは未実施。これらはGit管理外の成果物で、アプリ実装のコミット対象ではない。人口は2020-10-01基準の全国合算。CSVは月給表記のみ、重複除外後は製造163件・物流279件。Google／Indeedの保存市場データは月給限定ではない。

**月給だけに絞ったのは、この2職種のトライアルデータ。アプリ全体の既存の月給換算規則は変更していない。** 全国人口の実装も2020年の数値をハードコードせず、既存DBのデータを集計する。自治体行そのものの欠落を完全に検知する仕組みではない。

原本CSV、DB、`target/`の生成HTML・PDF・画像・ログ、古いExcel・CSV・ZIPはコミットしない。

以下の未追跡Pythonスクリプトはトライアル作成履歴で、アプリ稼働に必要なファイルではない：

- `scripts/build_marketing_trial_artifacts.py`
- `scripts/update_trial_html_visuals.py`
- `scripts/simplify_trial_html.py`

再現用として含めるかはコミット担当が判断する。前者はExcel等も生成し、後二者は旧名`report.html`を想定する。現在の`製造.html`・`物流.html`をそのまま再生成する操作として実行しない。ユーザーは個別HTMLだけを求め、追加成果物の作成を取りやめるよう指示している。

## 8. コミット担当への作業順序

1. 作業フォルダの差分・新規ファイルと、コミット先の最新実装を比較する。
2. 上記のアプリ実装・テスト・最新文書を取り込む。別セッションの修正を保持し、Rustモジュール・CSS・JS・PDFヘルパーの依存を揃える。
3. 取り込み先で競合調査・人口SQLのテストを通常モードで実行する。統合時にHTMLが変わった場合はgolden差分の理由を確認する。
4. `git add -A`／`git add .`は使わず、対象ファイルを明示してステージする。
5. `git diff --cached --stat`、`git diff --cached --check`で削除・生成物混入・新規モジュール漏れを確認する。
6. 別セッションでまとめてコミット・Push・必要なマージを行う。
7. Render反映後に、認証付き画面でCSV→5タブ・全国人口・直接PDFダウンロードを確認する。

最低賃金やCRMなど別セッションの変更を、このレポート差分で置き換えない。DBの更新や公開用HTMLのホスティングはこの引き継ぎに含めて実施した扱いにしない。
