# UI-1: 媒体分析タブ UI 強化 実装結果

**日付**: 2026-04-26
**担当範囲**: `src/handlers/survey/render.rs` + 関連 (`mod.rs`, 新規テストファイル)
**目的**: 媒体分析タブ (`/tab/survey`) のアップロード前/後 UI、結果サマリの読みやすさ、グラフ・図・表記の説明能力を強化する。「数値だけ並ぶ画面」から「物語を伝える画面」へ。

---

## 1. 変更ファイル一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/handlers/survey/render.rs` | 編集 | アップロードフォーム / 分析結果 UI の全面強化 |
| `src/handlers/survey/mod.rs` | 編集 | 新規テストモジュールの登録 |
| `src/handlers/survey/render_ui1_test.rs` | 新規 | UI-1 コントラクトテスト 11 件 |

UI-3 領域との競合回避: `report_html/helpers.rs` には触っていない。`render_read_hint_html` は UI-3 が追加済みであることをビルド時に確認。

---

## 2. 追加/変更した UI コンポーネント一覧

### 2.1 アップロード前 UI (`render_upload_form`)

| コンポーネント | DOM ID / セレクタ | 効果 |
|---|---|---|
| 使い方ステップガイド | `#survey-howto-steps` | 1→2→3→4 の番号付き図でフローを可視化 |
| ソース媒体ラジオカード | `#source-type-cards` (role=radiogroup) | Indeed/求人ボックス/その他を色マーカー＋説明付きカードで識別 |
| 給与単位ラジオカード | `#wage-mode-cards` (role=radiogroup) | 月給/時給/自動を想定雇用形態の説明付きで選択 |
| 強化ドロップゾーン | `#drop-zone` (role=button, p-10, animate-pulse) | 大きいパディング、アイコンアニメーション、a11y タッチ 44x44 |
| 進捗 aria-live | `#upload-status` (aria-live=polite) | スクリーンリーダー対応のアップロード進捗通知 |
| サンプル CSV 列 | `#survey-csv-samples` (`<details>`) | Indeed 主要列 (英字) と求人ボックス主要列 (日本語) のテーブルを折畳展開 |

**HTML 抜粋（ソース媒体カード部分）**:
```html
<label class="source-card ..." data-source="indeed">
  <input type="radio" name="source_type" value="indeed" checked>
  <div>
    <div class="text-sm font-bold text-white flex items-center gap-1.5">
      <span class="inline-block w-3 h-3 rounded-full bg-blue-500"></span>
      Indeed
    </div>
    <div class="text-[10px] text-slate-400">広域求人サイト・列名は英字混在</div>
  </div>
</label>
```

選択時に `border-blue-500 / bg-blue-500/10 / ring-1 ring-blue-500` をJSで動的付与（`syncCards()`）。

### 2.2 アップロード後 UI (`render_analysis_result`)

| セクション | DOM ID | 強化内容 |
|---|---|---|
| エグゼクティブサマリ KPI | `#survey-executive-summary` (data-total) | 4 KPI (主要地域/中央値/期待値/ギャップ) を icon+大数値+補助文。期待値ギャップ算出+色判定+読み方吹き出し |
| アクションバー | `#survey-action-bar`, `#btn-hw-integrate` | プライマリ「HW統合分析」を gradient+shadow で強調。セカンダリは role=group 化 |
| 給与統計 KPI | `#survey-salary-stats` | 中央値・平均に絵文字 (🟢/🟡)、ⓘ ツールチップ追加 |
| 給与帯チャート | `[data-chart="salary-range"]` | 中央値/平均 markLine + IQR markArea (緑シェード) + 外れ値除外バー (`#outlier-removal-bar`) + 読み方吹き出し |
| 雇用形態チャート | `[data-chart="employment-type"]` | 円グラフ + 100%帯 (`[data-stack="employment-100"]`, role=img) + 最多比率の読み方 |
| 都道府県別ヒートマップ (新規) | `#survey-prefecture-heatmap` | 47 県を 8x12 グリッド近似配置 + visualMap 凡例 + Top 5 表 + 読み方 |

**HTML 抜粋（KPI ギャップカード）**:
```html
<div class="p-4 bg-slate-800/60 rounded border border-slate-700/50" data-kpi="gap">
  <div class="flex items-center justify-between mb-2">
    <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
      <svg ...>...</svg> 期待値ギャップ
    </div>
    <span class="kpi-info" tabindex="0" role="button" aria-label="ギャップの説明"
      title="(中央値 − 期待値) ÷ 期待値 × 100。プラスは求職者期待を上回る訴求力">ⓘ</span>
  </div>
  <div class="text-2xl font-bold text-emerald-400 leading-tight">+14.3%</div>
  <div class="text-[10px] text-slate-500 mt-1">中央値 − 期待値 の相対差</div>
</div>
```

### 2.3 説明能力強化（用語・凡例）

- 各 KPI/グラフに `kpi-info` クラスの ⓘ アイコン (role=button + aria-label + title)
- IQR / Bootstrap CI / Tukey 法 / 月給換算 167h などの方法論を tooltip で説明
- グラフ凡例に絵文字併用: 🟢 中央値 / 🟡 平均 / 🔴/🟢/⚠️ シグナル
- グラフ下に「読み方:」吹き出し（border-blue-500/40 で視覚化）。例:
  > 中央値 280,000円が「ボリュームゾーン」。IQR (Q1〜Q3) 範囲は 240,000円〜310,000円で、求人の中央50%がこの帯に集中しています。

### 2.4 アクション動線

- HW 統合分析ボタン: `bg-gradient-to-r from-blue-600 to-blue-500 shadow-lg shadow-blue-500/20 text-base font-bold` で目立たせる
- ボタングループ化: 印刷用レポート / HTML ダウンロード / 別CSV を `role="group"` でまとめる
- ボタン hover 時に説明テキスト表示（HW統合分析ボタン）
- 全ボタンに `min-h-[44px]` (タッチターゲット 44x44 a11y)
- 全ボタンに `title` 属性（hover 時の説明）

---

## 3. memory feedback 遵守

| ルール | 遵守状況 |
|---|---|
| `feedback_correlation_not_causation` | ギャップ判定は「傾向」「シグナル」表現に統一。因果断定なし |
| `feedback_hw_data_scope` | サマリ/データ品質セクションで「アップロードCSVのみに基づく」「HW掲載求人のみ」を明示 |
| `feedback_test_data_validation` | 11 件のテストは全て具体値検証 (例: `+14.3%`, `280,000円`, `data-pref-count="5"`) |
| `feedback_e2e_chart_verification` | テストで ECharts config 内の県名/数値/visualMap を実質検証 |

---

## 4. テスト結果

### 4.1 新規追加: UI-1 コントラクトテスト 11 件

| # | テスト名 | 検証内容 |
|---|---|---|
| 1 | `ui1_upload_form_contains_step_guide_with_4_numbered_items` | 4ステップ番号 + 具体ラベル |
| 2 | `ui1_upload_form_source_type_visualized_as_3_radio_cards` | 3カード+role=radiogroup+色マーカー |
| 3 | `ui1_upload_form_wage_mode_visualized_as_3_radio_cards` | 3 wage モード+雇用形態説明 |
| 4 | `ui1_upload_form_drop_zone_has_a11y_attributes` | role=button, animate, p-10, min-h-[44px], aria-live |
| 5 | `ui1_upload_form_csv_samples_collapsible_with_indeed_and_jobbox_columns` | `<details>`+Job Title+求人タイトル等 |
| 6 | `ui1_analysis_executive_summary_kpi_cards_with_4_kpis_and_gap` | 4 KPI + 280,000円/245,000円/+14.3% 計算検証 |
| 7 | `ui1_analysis_action_bar_primary_hw_integrate_emphasized` | gradient+shadow+session-id+44px×4 |
| 8 | `ui1_analysis_prefecture_heatmap_47_data_points_in_chart_config` | 5県+47県表示+visualMap+Top5 |
| 9 | `ui1_analysis_salary_range_chart_iqr_shading_and_outlier_bar` | markLine 280000+markArea Q1/Q3+12件除外+Tukey用語 |
| 10 | `ui1_analysis_employment_type_chart_100_percent_stacked_bar` | 100%帯+正社員 72.0%+role=img |
| 11 | `ui1_analysis_kpi_info_tooltips_explain_methodology` | ⓘ×4+50パーセンタイル+中央50%+role=button |

実行結果:
```
running 11 tests
test ui1_upload_form_contains_step_guide_with_4_numbered_items ... ok
test ui1_upload_form_csv_samples_collapsible_with_indeed_and_jobbox_columns ... ok
test ui1_upload_form_source_type_visualized_as_3_radio_cards ... ok
test ui1_upload_form_drop_zone_has_a11y_attributes ... ok
test ui1_upload_form_wage_mode_visualized_as_3_radio_cards ... ok
test ui1_analysis_kpi_info_tooltips_explain_methodology ... ok
test ui1_analysis_executive_summary_kpi_cards_with_4_kpis_and_gap ... ok
test ui1_analysis_action_bar_primary_hw_integrate_emphasized ... ok
test ui1_analysis_employment_type_chart_100_percent_stacked_bar ... ok
test ui1_analysis_salary_range_chart_iqr_shading_and_outlier_bar ... ok
test ui1_analysis_prefecture_heatmap_47_data_points_in_chart_config ... ok

test result: ok. 11 passed; 0 failed
```

### 4.2 既存テスト全件

**ベースライン: 781 passed → 実行後: 801 passed; 0 failed; 1 ignored**

```
test result: ok. 801 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

UI-1 (+11) + UI-2/UI-3 並行作業 (+9) 含む。**既存テスト破壊ゼロ。**

---

## 5. 親セッションへの統合チェックリスト

- [x] `cargo build --lib` パス（警告 3 件: 既存 dead_code/unused、UI-1 由来なし）
- [x] `cargo test --lib` 全件 pass (801/801, ignored 1)
- [x] 公開 API シグネチャ不変（`render_upload_form`, `render_analysis_result` 戻り値・引数とも未変更）
- [x] memory ルール 4 件遵守（correlation_not_causation / hw_data_scope / test_data_validation / e2e_chart_verification）
- [x] DOM ID / data 属性で UI-2 / UI-3 とのテスト ID 衝突なし
  - UI-1 用: `#survey-howto-steps`, `#survey-csv-samples`, `#survey-executive-summary`, `#survey-action-bar`, `#survey-prefecture-heatmap`, `#outlier-removal-bar`, `[data-chart="salary-range"]`, `[data-chart="employment-type"]`, `[data-stack="employment-100"]`, `[data-kpi="region|median|expected|gap"]`, `[data-source="..."]`, `[data-wage="..."]`
- [x] アクセシビリティ: role=radiogroup / role=button / role=img / role=group / aria-label / aria-live / tabindex / min-h-[44px]
- [x] レスポンシブ: `sm:` `md:` `lg:` ブレイクポイント使用
- [x] 既存 ECharts CDN (`echarts@5.5.1`) で動作（heatmap/markArea/markLine/visualMap は 5.x 標準機能）

### 残課題（別 sprint）

- E2E (Playwright) で実ブラウザでのドラッグ＆ドロップ動作検証
- 都道府県ヒートマップのクリック→詳細 modal 表示（現状は静的表示のみ）
- 給与帯チャート markLine の信頼区間 (CI) シェード追加（bootstrap_ci データがある場合）
- 散布図への回帰直線+CI 追加は UI-2 の領域

---

## 6. 物語性向上のポイント（仮説駆動）

`feedback_hypothesis_driven` に従い「このKPIを見るとユーザーは何を判断するか?」を意識:

| KPI | ユーザーの判断 | UI 設計 |
|---|---|---|
| 主要地域 | 自社ターゲット地域とのマッチ | 最多掲載エリアを大文字+truncateで明示 |
| 給与中央値 | 提示給与の妥当性 | 緑色+大数値+「月給換算」明記 |
| 求職者期待値 | 応募集まりやすさ | 橙色で対比、推定モデル説明 |
| **期待値ギャップ** | **自社給与の競争力** | **+14.3% を緑/赤/黄で即判定+読み方吹き出し** |

ギャップ KPI が本UI 強化の「物語の主役」。ユーザーはこの一つを見るだけで「給与訴求 OK か NG か」を 5 秒で判断できる。

---

## 7. ファイルパス（絶対パス）

- 変更: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\render.rs`
- 変更: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\mod.rs`
- 新規: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\render_ui1_test.rs`
- 新規: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_ui1_results.md` (本ファイル)
