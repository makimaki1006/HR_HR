# V2 HW Dashboard PDF 競合調査レポート 設計仕様 (2026-04-24)

**対象ファイル**: `src/handlers/survey/report_html.rs`（HEAD 2530 行、全面再構成前提）
**対象エントリポイント**: `render_survey_report_page(agg, seeker, by_company, by_emp_type_salary, salary_min_values, salary_max_values, hw_context, salesnow_companies) -> String`
**成果物**: A4 縦 PDF 出力を前提とした HTML
**設計者**: Agent P1（設計リード）
**実装者**: Agent P2（実装）
**QA**: Agent P3（検証）
**最終更新**: 2026-04-24

---

## 目次

1. ペルソナ・ユースケース
2. 情報アーキテクチャ（セクション一覧と配置論理）
3. Executive Summary 詳細設計
4. 各セクション詳細（データ源・視覚表現・So What テンプレ）
5. ビジュアルデザインシステム
6. 印刷 CSS 必須ルール
7. ブランディングガイド
8. 実装エージェント (P2) への厳密指示
9. QA チェックリスト (P3)

---

## 1. ペルソナ・ユースケース

### 1.1 プライマリーペルソナ

| 属性 | 値 |
|------|-----|
| 役割 | 採用コンサル / 人事部長 |
| 知識レベル | 採用市場を把握している中級以上。統計用語（中央値、IQR）は理解できる |
| 目的 | 顧客企業の経営層・採用担当に「このエリア×職種の採用戦略」を提案する |
| 典型タスク | レポートをチームで読み合わせ、優先アクションを決める |

### 1.2 セカンダリーペルソナ（被渡人）

| 属性 | 値 |
|------|-----|
| 役割 | 顧客企業の経営層・採用担当 |
| 知識レベル | 採用実務はあるが統計は非専門 |
| 目的 | 自社の採用上の立ち位置と、次に打つべき一手を知る |
| 典型タスク | 資料をめくり、赤字（重大）セクションから順に読む |

### 1.3 使用シチュエーション

- **媒体**: A4 カラー印刷 または PC / タブレット PDF 閲覧
- **場面**: 採用戦略会議での配布、社内共有、提案の根拠資料
- **所要時間**: 全体把握 3 分 / 深掘り 15 分
- **環境**: 白黒印刷も発生（モノクロ耐性が必要）

### 1.4 成功状態 / 失敗状態

| 状態 | 内容 |
|------|------|
| 成功 | 1 ページ目（Executive Summary）だけで「このエリアの採用優先度」「次に打つべき 3 手」が分かる |
| 成功 | セクションごとに「数値 → So What（だから何）→ 次の行動」が並び、読み手が迷わない |
| 失敗（前回発生） | 数値羅列で「どこを見るべきか」不明 |
| 失敗（前回発生） | モノクロ印刷でチャートが潰れる |
| 失敗（前回発生） | 見出しがページ末尾に孤立し、本文が次ページへ分離 |
| 失敗（前回発生） | ランキング語彙（例: 「上位」「ランキング」）を使い誤認を誘発 |

### 1.5 禁止ワード（ユーザー明示指示 + MEMORY 由来）

| 禁止語 | 理由 | 代替表現 |
|--------|------|----------|
| 「ランキング」「順位」「1位」「上位」 | 評価的断定の誘発 | 「件数が多い順に整理」「件数の多い 5 件」 |
| 「おすすめ」「ベスト」「最適」 | 評価的断定 | 「件数・給与から見た候補」「該当条件を満たす」 |
| 「優良」「質が高い」 | 評価的断定 | 「掲載情報の記載が豊富」「該当条件を満たす」 |
| 「求人件数」（サンプル件数の意味で使う場合） | 混同 | 「本レポートのサンプル件数」（対象求人を説明した上で使い分け） |
| 「この地域は採用が有利/不利」 | 因果断定 | 「この地域では XX の傾向がある（相関、因果は別途検討）」 |
| 「XX すべき」「XX しなければならない」 | 根拠の薄い指示 | 「XX の選択肢がある。判断は現場の文脈に依存」 |
| 「確実に」「必ず」「100%」 | 断定 | 「今回のサンプルでは」「本データ上は」 |

---

## 2. 情報アーキテクチャ（セクション一覧と配置論理）

### 2.1 配置方針

- **逆ピラミッド**: 結論 → 根拠 → 詳細データ
- **ページ単位完結**: 主要セクションは 1 ページ（A4 縦）内に収める（後述 print CSS で制御）
- **視線誘導**: 各ページ左上に「セクション番号 + 見出し + So What 要約」を必ず置く
- **モノクロ耐性**: 色のみに依存する図表を作らない（パターン・数値ラベル併記）

### 2.2 セクション一覧（ページ順）

| # | セクション名 | 配置 | 目的 | 1 ページ完結 |
|---|-------------|------|------|--------------|
| 0 | 表紙 (Cover) | p.1 | 案件名・対象地域・生成日・機密表記 | 必須 |
| 1 | Executive Summary | p.2 | 5 KPI + 推奨優先アクション 3 + スコープ注意 | **必須** |
| 2 | HW 市場比較 (Market Context) | p.3 | アップロードデータと HW 公開市場との差分 | 望ましい |
| 3 | 給与分布 統計 | p.4 | 中央値/IQR/信頼区間での給与の散らばりを示す | 望ましい |
| 4 | 雇用形態分布 | p.5 | 構成比 + 形態別平均給与 | 必須 |
| 5 | 給与の相関分析（散布図） | p.6 | 下限-上限の対応 + 回帰 | 望ましい |
| 6 | 地域分析（都道府県） | p.7 | 地域別件数 + 平均下限給与 | 必須 |
| 7 | 地域分析（市区町村） | p.8 | 上位市区町村の件数・給与（ランキング表現を避ける） | 望ましい |
| 8 | 最低賃金比較 | p.9 | 平均下限給与の 160h 換算 vs 都道府県最低賃金 | 必須 |
| 9 | 企業分析 | p.10 | 掲載件数が多い法人 + 平均給与 | 必須 |
| 10 | タグ × 給与相関 | p.11 | 特定ワード（資格、待遇）と給与差 | 望ましい |
| 11 | 求職者心理分析 | p.12 | 期待給与の範囲推定、未経験者ギャップ | 望ましい |
| 12 | SalesNow 地域注目企業 | p.13 | 該当地域の SalesNow 企業データ（hw_posting_count 等） | データ有時のみ |
| 13 | 注記・出典・免責 | 末尾 | スコープ制約、相関≠因果、データ限界の明示 | 必須 |

### 2.3 配置理由（セクション間の論理）

- p.1 (表紙) → p.2 (Executive Summary): 「何の資料か」→「結論 5 行」と即座に結論提示
- p.3 (HW 市場比較): 結論の後に「アップロード = 自社サンプル」と「HW 市場全体」の位置関係を最初に示し、以降の数値がどの母集団なのかを読み手に固定
- p.4-5 (給与・雇用形態): 提案の最重要変数（給与と雇用形態）を先
- p.6-8 (地域 → 市区町村 → 最低賃金): 地域軸を段階的に深堀り
- p.9-11 (企業・タグ・求職者心理): 「競合」「訴求ワード」「求職者側期待」の三角形で提案材料を補強
- p.12 (SalesNow): 営業ターゲットの具体的な法人リスト
- p.13 (注記): スコープと限界を明示して読み手の誤解を防ぐ

### 2.4 セクション間の削除可能性

- **必須（削除不可）**: 0, 1, 4, 6, 8, 9, 13
- **データ依存（省略可）**: 2 (`hw_context = None` 時)、12 (`salesnow_companies.is_empty()` 時)
- **低サンプル時省略**: 3, 5, 10, 11（`agg.salary_values.len() < 30` 等の低サンプル条件下では「本セクションはサンプル不足のため省略しました」のプレースホルダーを 1 ブロック出すのみ）

---

## 3. Executive Summary 詳細設計

### 3.1 目的

**3 分間でレポート全体の要旨を把握させる**。1 ページ内に必ず収める。

### 3.2 レイアウト（A4 縦、マージン 12mm）

擬似図（markdown）:

```
+----------------------------------------------------------+
| [F-A-C株式会社ロゴ or 文字]         生成日: 2026-04-24   |
|                                                          |
|  Executive Summary                                       |
|  対象: 東京都 千代田区 ｜ 介護職                         |
+----------------------------------------------------------+
|                                                          |
|  ▼ 5 KPI（カード 5 枚を横 5 列 or 2x3 グリッド）         |
|  +----------+ +----------+ +----------+                  |
|  | サンプル | | 主要地域 | | 主要業種 |                  |
|  | 1,234 件 | | 千代田区 | | 介護職員 |                  |
|  +----------+ +----------+ +----------+                  |
|  +----------+ +----------+                               |
|  | 給与中央 | | 新着比率 |                               |
|  | 月給25万 | |  18.3%   |                               |
|  +----------+ +----------+                               |
|                                                          |
+----------------------------------------------------------+
|  ▼ 推奨優先アクション（3 件、上から順）                  |
|                                                          |
|  [重大] 1. 給与下限を月 +2 万円引き上げる候補            |
|         根拠: 当サンプル中央値 25 万 / 該当市区町村      |
|         HW 中央値 27 万で 2 万差                         |
|         (Section 8 参照)                                 |
|                                                          |
|  [注意] 2. 雇用形態「正社員」の構成比を見直す候補        |
|         根拠: 当サンプル 45% / HW 市場 62%               |
|         (Section 4 参照)                                 |
|                                                          |
|  [情報] 3. 訴求タグ「賞与あり」の給与プレミアム          |
|         根拠: 該当タグ平均が全体比 +1.2 万円             |
|         (Section 10 参照)                                |
|                                                          |
+----------------------------------------------------------+
|  ※ 本レポートはハローワーク掲載求人のみが対象。          |
|  全求人市場の代表ではない。給与バイアスに留意。          |
|  相関と因果は別概念で、示唆は仮説であり実施判断は        |
|  現場文脈に依存します。                                  |
+----------------------------------------------------------+
```

### 3.3 5 KPI 定義

| # | 指標名 | 計算式 | データ源 | 表示書式 | 注意 |
|---|--------|--------|----------|----------|------|
| 1 | サンプル件数 | `agg.total_count` | SurveyAggregation.total_count | `{:,} 件` | 「求人件数」と呼ばない。「本レポートのサンプル件数」と呼ぶ |
| 2 | 主要地域 | `agg.dominant_prefecture` + `agg.dominant_municipality` | SurveyAggregation | `東京都 千代田区` / 空なら `全国` | `None` 時は「-」表示 |
| 3 | 主要雇用形態 | `agg.by_employment_type` の件数最多要素 | SurveyAggregation.by_employment_type (Vec<(String, usize)>) | `正社員 (62%)` | ランキングと呼ばない、「件数が最も多い形態」と呼ぶ |
| 4 | 給与中央値 | `agg.enhanced_stats.as_ref().map(\|s\| s.median)` | EnhancedStats.median (i64) | 月給: `月給 {:,} 円` / 時給: `時給 {:,} 円`（`agg.is_hourly` 参照）| Null 時は「算出不能（サンプル不足）」 |
| 5 | 新着比率 | `agg.new_count as f64 / agg.total_count as f64 * 100.0` | SurveyAggregation | `{:.1}%` | `total_count == 0` は「-」 |

### 3.4 推奨優先アクション 3 件の生成ルール

**データ源**: Section 1-12 のうち以下 3 指標を算出し、severity の高い順に上から 3 件表示。

| 優先度判定 | 指標 | 閾値 | Severity | アクション文言テンプレート |
|-----------|------|------|----------|--------------------------|
| A | 給与ギャップ: `当サンプル salary_min 中央値` vs `HW 市場中央値` (hw_context) | 差分 >= 2 万円 | Critical | 給与下限を月 +X 万円引き上げる候補。根拠: 当サンプル中央値 Y 万 / 該当市区町村 HW 中央値 Z 万で X 万差 (Section 6/8 参照) |
| A | 同上 | 1 万 <= 差分 < 2 万円 | Warning | 同上（Warning 文言） |
| B | 雇用形態構成: 「正社員」構成比 vs HW 市場同比率 | 差分 >= 15pt | Warning | 雇用形態「正社員」の構成比を見直す候補。根拠: 当サンプル A% / HW 市場 B% (Section 4 参照) |
| C | タグプレミアム: by_tag_salary で `diff_percent > 5%` かつ `count >= 10` のタグ | 条件成立 | Info | 訴求タグ「X」の給与プレミアム。根拠: 該当タグ平均が全体比 +Y 万円 (Section 10 参照) |

**生成順**: Critical > Warning > Info の順に並べ、3 件未満の場合は「現時点では該当なし」プレースホルダーを表示。**ランキング語彙は使わず**、「該当条件を満たすアクション候補」と呼ぶ。

### 3.5 スコープ注意書き（必須）

Executive Summary 下部に以下 2 行を**必ず**表示する（変更不可）:

```
※ 本レポートはハローワーク掲載求人のみが対象。全求人市場の代表ではない。
  給与バイアス（HW は中小企業・地方案件比率が高い）に留意。
※ 示唆は相関に基づく仮説であり、因果を証明するものではない。
  実施判断は現場文脈に依存します。
```

---

## 4. 各セクション詳細

各セクションは以下の統一構造で記述:

- **Header**: セクション番号・タイトル・要約（So What 1 行）
- **Body**: チャート or テーブル
- **Footer**: So What 本文 + 「参考セクション」クロスリファレンス

### 4.1 Section 2: HW 市場比較

| 項目 | 内容 |
|------|------|
| 目的 | アップロードされた自社サンプルが、HW 公開市場全体とどう違うかを 1 ページで可視化 |
| 配置理由 | 以降の数値がどの母集団のものか読み手に固定させる |
| データ源 | `hw_context: Option<&InsightContext>` - `ts_salary`, `ts_counts`, `ext_min_wage` Vec<Row> |
| 既存関数 | `render_section_hw_comparison(html, agg, by_emp_type_salary, ctx)` |
| 視覚表現 | 2 列比較カード（サンプル側 vs HW 市場側）× 4 項目（平均月給 / 構成比 / 件数推移 / 最低賃金） |
| So What テンプレ | 「当サンプルの平均月給は HW 市場より X 円（Y%）{高い/低い}。該当地域の求人全体と比べた自社サンプルの位置関係がこれ」 |
| ページ分割 | 1 ページ完結。`page-break-inside: avoid` |
| データ欠損時 | `hw_context = None` ならセクション自体を**出力しない** |

### 4.2 Section 3: 給与分布 統計

| 項目 | 内容 |
|------|------|
| 目的 | 平均 / 中央値 / IQR / 信頼区間で給与の散らばりを多面的に示す |
| データ源 | `agg.enhanced_stats: Option<EnhancedStats>`、`agg.salary_values: Vec<i64>`、`salary_min_values`、`salary_max_values` |
| 既存関数 | `render_section_salary_stats(html, agg, salary_min_values, salary_max_values)` |
| 視覚表現 | ヒストグラム (ECharts) + 統計 KPI 6 枚（mean, median, std_dev, 95% CI lower/upper, trimmed_mean） + reliability バッジ |
| So What テンプレ | 「中央値 X 円、IQR は Y 円幅。外れ値を除いた trimmed mean は Z 円。信頼性評価: {reliability}。サンプル数 {count} 件（30 件未満は参考値）」 |
| ページ分割 | 1 ページ完結 |
| データ欠損時 | `enhanced_stats = None` or `count < 30` 時、「サンプル不足のため統計的分析は省略」プレースホルダー |

### 4.3 Section 4: 雇用形態分布

| 項目 | 内容 |
|------|------|
| 目的 | 構成比 + 形態別平均給与の 2 軸同時表示 |
| データ源 | `agg.by_employment_type: Vec<(String, usize)>`、`by_emp_type_salary: &[EmpTypeSalary]` |
| 既存関数 | `render_section_employment(html, agg, by_emp_type_salary)` |
| 視覚表現 | 横棒グラフ（件数）+ テーブル（形態 / 件数 / 構成比 / 平均月給 / 中央値） |
| So What テンプレ | 「{最多形態}が{構成比}% を占め、平均月給は{金額}円。{2 位形態}との平均差は{差}円」 |
| ページ分割 | 1 ページ完結 |
| 禁止表現 | 「上位」「ランキング」→「件数が最も多い形態」等 |

### 4.4 Section 5: 給与の相関分析（散布図）

| 項目 | 内容 |
|------|------|
| 目的 | 下限給与と上限給与の関係から給与設定の一貫性を示す |
| データ源 | `agg.scatter_min_max: Vec<ScatterPoint>`、`agg.regression_min_max: Option<RegressionResult>` |
| 既存関数 | `render_section_scatter(html, agg)` |
| 視覚表現 | 散布図 + 回帰直線 + R² 表示 |
| So What テンプレ | 「下限と上限の相関係数 R² = {値}。回帰式 y = {slope}x + {intercept}。R² > 0.7 で一貫、< 0.3 で散らばり大」 |
| 注意 | 「相関は因果ではない」注記を必ず入れる |
| データ欠損時 | `scatter_min_max.len() < 10` で省略 |

### 4.5 Section 6: 地域分析（都道府県）

| 項目 | 内容 |
|------|------|
| 目的 | 地域別の件数と平均下限給与を比較 |
| データ源 | `agg.by_prefecture: Vec<(String, usize)>`、`agg.by_prefecture_salary: Vec<PrefectureSalaryAgg>` |
| 既存関数 | `render_section_region(html, agg)` |
| 視覚表現 | 棒グラフ（件数）+ テーブル（県名 / 件数 / 平均月給 / 平均下限月給） |
| So What テンプレ | 「{最多件数県}が全体の {%} を占める。平均下限給与は県間で {max}-{min} 円のレンジ」 |
| 禁止表現 | 「ランキング」「1 位」→「件数の多い順に整理」 |

### 4.6 Section 7: 地域分析（市区町村）

| 項目 | 内容 |
|------|------|
| 目的 | 市区町村粒度での件数・給与差を示す |
| データ源 | `agg.by_municipality_salary: Vec<MunicipalitySalaryAgg>` |
| 既存関数 | `render_section_municipality_salary(html, agg)` |
| 視覚表現 | テーブル（県 / 市区町村 / 件数 / 平均月給 / 中央値）上位 15 件 |
| So What テンプレ | 「件数の多い上位 15 市区町村のうち、平均給与が地域内で最も高いのは {市区町村名} の {金額} 円」 |
| 注意 | 伊達市・府中市等の同名異県を区別（`prefecture` を必ず併記） |

### 4.7 Section 8: 最低賃金比較

| 項目 | 内容 |
|------|------|
| 目的 | 平均下限給与の 160h 換算が都道府県最低賃金を下回っていないか確認 |
| データ源 | `agg.salary_min_values`（平均下限算出元）+ `hw_context.ext_min_wage` |
| 既存関数 | `render_section_min_wage(html, agg)` |
| 視覚表現 | テーブル（県 / 平均下限月給 / 160h 換算時給 / 最低賃金 / 差額） + 差額が負値のセルはアラート色 |
| So What テンプレ | 「{該当県数} 県で平均下限給与の 160h 換算が最低賃金を下回る傾向。該当求人群は労基上要確認」 |
| Severity 適用 | 差額 < 0 円 → Critical (red #ef4444), 差額 < 50 円 → Warning (amber #f59e0b) |

### 4.8 Section 9: 企業分析

| 項目 | 内容 |
|------|------|
| 目的 | 掲載件数が多い法人の平均給与・件数を示す |
| データ源 | `by_company: &[CompanyAgg]`（name, count, avg_salary, median_salary） |
| 既存関数 | `render_section_company(html, by_company)` |
| 視覚表現 | テーブル（法人名 / 件数 / 平均月給 / 中央値月給）上位 15 件 |
| So What テンプレ | 「件数の多い上位 15 社のうち、平均給与が最も高い法人は {name} の {金額} 円」 |
| 禁止表現 | 「採用力ランキング」「ベスト法人」→「掲載件数の多い法人」 |

### 4.9 Section 10: タグ × 給与相関

| 項目 | 内容 |
|------|------|
| 目的 | 訴求ワード（資格・待遇等）と給与差の相関を示す |
| データ源 | `agg.by_tag_salary: Vec<TagSalaryAgg>`（tag, count, avg_salary, diff_from_avg, diff_percent） |
| 既存関数 | `render_section_tag_salary(html, agg)` |
| 視覚表現 | テーブル（タグ / 件数 / 平均月給 / 全体平均との差 / 差分率%）、差分率で降順ソート（ただし「ランキング」と呼ばない） |
| So What テンプレ | 「タグ『{tag}』を含む求人は全体平均比 +{Y} 円 ({Z}%)。当該タグがプレミアム要因の可能性（相関、因果は別途検討）」 |
| フィルタ条件 | `count >= 10` のタグのみ表示（少数サンプル除外） |

### 4.10 Section 11: 求職者心理分析

| 項目 | 内容 |
|------|------|
| 目的 | 期待給与の範囲推定、未経験-経験者ギャップを示す |
| データ源 | `seeker: &JobSeekerAnalysis` - expected_salary, salary_range_perception, inexperience_analysis, new_listings_premium |
| 既存関数 | `render_section_job_seeker(html, seeker)` |
| 視覚表現 | KPI カード 4 枚（期待給与ポイント / 範囲幅 / 未経験-経験者ギャップ / 新着プレミアム）+ 範囲幅ヒストグラム（narrow/medium/wide の件数棒） |
| So What テンプレ | 「求職者の期待給与ポイントは {expected_point} 円。レンジ幅平均 {avg_range_width} 円。未経験求人は経験者求人より平均 {gap} 円低い傾向」 |
| データ欠損時 | 該当 Option が None の項目は「算出不能」と表示（非表示化しない、読み手が欠損の存在を認識できるように） |

### 4.11 Section 12: SalesNow 地域注目企業

| 項目 | 内容 |
|------|------|
| 目的 | 営業ターゲット候補として該当地域の SalesNow 登録企業を示す |
| データ源 | `salesnow_companies: &[NearbyCompany]` - company_name, prefecture, sn_industry, employee_count, credit_score, hw_posting_count |
| 既存関数 | `render_section_salesnow_companies(html, companies)` |
| 視覚表現 | テーブル（法人名 / 都道府県 / 業種 / 従業員数 / 信用スコア / HW 掲載件数）、HW 掲載件数で降順 |
| So What テンプレ | 「該当地域で SalesNow データ登録企業は {N} 社。HW 掲載件数が多い法人は採用活発な傾向（相関、因果は別途検討）」 |
| 出力条件 | `salesnow_companies.is_empty()` なら**セクション非出力** |

### 4.12 Section 13: 注記・出典・免責

| 項目 | 内容 |
|------|------|
| 目的 | スコープ制約、相関≠因果、データ限界を明示 |
| データ源 | 静的テキスト |
| 視覚表現 | 小文字の段落 5 項目 |
| 必須記載（変更不可） |  |

必須記載項目（全てそのまま書く）:

1. **データスコープ**: 本レポートはハローワーク掲載求人のみが対象。職業紹介事業者の求人・非公開求人は含まれない。全求人市場の代表ではない。
2. **給与バイアス**: ハローワーク掲載求人は中小企業・地方案件の比率が高く、給与水準は民間媒体より低く出る傾向がある。
3. **相関と因果**: 本レポートに記載する「傾向」「相関」は因果関係を証明するものではない。
4. **サンプル件数と求人件数**: 本レポートの「サンプル件数」は分析対象求人数であり、地域全体の求人件数ではない。
5. **出典**: データ源 - アップロード CSV / ハローワーク公開データ / SalesNow 登録データ / e-Stat。
6. **生成元**: F-A-C 株式会社 / 生成日時: {now}。

---

## 5. ビジュアルデザインシステム

### 5.1 タイポグラフィ

| 用途 | font-family | size | weight | line-height | letter-spacing |
|------|-------------|------|--------|-------------|----------------|
| 表紙タイトル | "Hiragino Kaku Gothic ProN", "Meiryo", "Noto Sans JP", sans-serif | 28pt | 700 | 1.2 | 0.05em |
| セクション見出し (H2) | 同上 | 18pt | 700 | 1.3 | 0.02em |
| サブ見出し (H3) | 同上 | 14pt | 700 | 1.4 | 0 |
| 本文 | 同上 | 11pt | 400 | 1.6 | 0 |
| キャプション / 表セル | 同上 | 10pt | 400 | 1.4 | 0 |
| KPI 数値 | 同上 | 24pt | 700 | 1.1 | 0 |
| 注記 / 免責 | 同上 | 9pt | 400 | 1.5 | 0 |
| ページ番号 / ヘッダー | 同上 | 8pt | 400 | 1 | 0 |

### 5.2 カラーパレット

印刷時に意味が保たれる（モノクロ耐性あり）パレット:

| トークン | Light カラー | Dark カラー | 用途 | モノクロ代替 |
|---------|------------|-----------|------|------------|
| --c-primary | #1e3a8a (blue-900) | #60a5fa (blue-400) | セクション見出し・表紙 | 濃グレー #1a1a1a |
| --c-text | #0f172a | #f1f5f9 | 本文 | 黒 |
| --c-text-muted | #64748b | #94a3b8 | 注記 | 濃グレー #555 |
| --c-border | #e2e8f0 | #334155 | 枠線 | #ccc |
| --c-bg | #ffffff | #0f172a | 背景 | 白 |
| --c-bg-card | #f8fafc | #1e293b | カード背景 | #f5f5f5 |
| --c-critical | #ef4444 (red-500) | 同 | 重大（警告） | 濃グレー + `▲▲` 記号 |
| --c-warning | #f59e0b (amber-500) | 同 | 注意 | 中グレー + `▲` 記号 |
| --c-info | #3b82f6 (blue-500) | 同 | 情報 | 薄グレー + `●` 記号 |
| --c-positive | #10b981 (emerald-500) | 同 | 良好 | 白抜き + `◯` 記号 |

**Severity と色の対応は `Insight helpers.rs` の定義（Severity::Critical=#ef4444 等）に厳密に従うこと**。勝手に変更しない。

### 5.3 スペーシング

| 要素 | 値 |
|------|-----|
| セクション間マージン | 16mm (紙面)、24px (画面) |
| セクション内 H2 下 | 8mm |
| カード内パディング | 8mm |
| テーブル行高 | 最低 7mm |
| 表紙上下中央配置余白 | 上 40mm / 下 30mm |
| 本文段落間 | 4mm |

### 5.4 チャート style ガイド（ECharts）

| 項目 | 値 |
|------|-----|
| フォント | `"Hiragino Kaku Gothic ProN", "Meiryo", "Noto Sans JP", sans-serif` |
| グリッド左余白 | 最低 60px（日本語ラベル対応） |
| 棒グラフ色 | --c-primary 単色 + ラベル数値を必ず併記（モノクロでも読める） |
| 散布図マーカー | 半透明 (opacity: 0.5) + 回帰線は太 2px |
| 凡例位置 | top-right |
| toolbox | 非表示（印刷レポートでは不要） |
| animation | false（印刷時ちらつき防止） |
| ヒストグラム | 棒間隔 0、数値ラベル棒上に常時表示 |

### 5.5 テーブル style

| 項目 | 値 |
|------|-----|
| ヘッダー背景 | --c-primary（白文字） |
| 奇数行背景 | 白 |
| 偶数行背景 | --c-bg-card |
| セル padding | 4mm 水平 / 2mm 垂直 |
| 罫線 | 下線のみ 1px solid --c-border（縦線なし） |
| 数値セル | 右寄せ、等幅数字 (font-feature-settings: 'tnum') |
| 文字セル | 左寄せ |
| ソート可能テーブル | `.sortable-table` クラス、role="grid" |

---

## 6. 印刷 CSS 必須ルール

### 6.1 @page 宣言（必須、変更禁止）

```css
@page {
  size: A4 portrait;
  margin: 12mm;
  @bottom-left {
    content: "F-A-C株式会社 | ハローワーク求人データ分析レポート";
    font-size: 8pt;
    color: #999;
  }
  @bottom-right {
    content: "Page " counter(page) " / " counter(pages);
    font-size: 8pt;
    color: #999;
  }
}

@page :first {
  /* 表紙にはページ番号を出さない */
  @bottom-left { content: ""; }
  @bottom-right { content: ""; }
}
```

### 6.2 page-break-inside: avoid 対象（必須リスト）

以下の class を持つ要素に**必ず** `page-break-inside: avoid` を適用:

- `.section` （セクション全体）
- `.kpi-card` （KPI カード単位）
- `.exec-summary-action` （Executive Summary のアクション 1 件）
- `.echart-container` （ECharts 描画コンテナ）
- `.comparison-card` （2 列比較カード）
- `.stat-box` （統計 KPI 枠）
- `table` （テーブル全体、ただし行数多い場合は `thead { display: table-header-group; }` で次ページにヘッダー再表示）

### 6.3 page-break-before / after

- `.section.page-start` → `page-break-before: always`（主要セクションの開始）
- `.cover-page` → `page-break-after: always`
- `.exec-summary` → `page-break-after: always`（必ず次ページから本編）

### 6.4 色・背景の印刷保持

```css
@media print {
  * {
    -webkit-print-color-adjust: exact;
    print-color-adjust: exact;
  }
  .no-print { display: none !important; }
  body { font-size: 11pt; }
  .cover-page { background: #fff !important; }
}
```

### 6.5 モノクロ印刷耐性（必須）

severity バッジは色のみでなくアイコン文字を併記:

- Critical → 背景 #ef4444 + `▲▲ 重大`
- Warning → 背景 #f59e0b + `▲ 注意`
- Info → 背景 #3b82f6 + `● 情報`
- Positive → 背景 #10b981 + `◯ 良好`

棒グラフ・散布図の色は 1 系統のみ使用し、複数系列の比較が必要な場合はパターン（斜線・ドット）または数値ラベルで区別する。

### 6.6 widow / orphan 制御

```css
p, li { orphans: 3; widows: 3; }
h2, h3 { page-break-after: avoid; break-after: avoid; } /* 見出しだけが先ページ末尾に孤立しないように */
```

### 6.7 テーブルのページ跨ぎ

```css
table { border-collapse: collapse; }
thead { display: table-header-group; } /* 次ページにヘッダー再表示 */
tr { page-break-inside: avoid; }
```

---

## 7. ブランディングガイド

### 7.1 F-A-C 株式会社 identity

- **社名表記**: 「F-A-C株式会社」（半角ハイフン、株式会社は全角、間スペースなし）
- **プライマリーカラー**: --c-primary (#1e3a8a)
- **ロゴ**: 現状は `.cover-logo` に CSS `display: none`（ロゴ未提供）。ロゴファイル提供後は `<img>` or `<svg>` を挿入し、180x60px で表紙に配置。実装時点ではテキストロゴ（社名 14pt bold）で代用。

### 7.2 表紙レイアウト指定

```
[垂直中央揃え、水平中央]
  ┌─────────────────────────┐
  │  [ロゴ枠 180x60px]      │  ← 上から 60mm
  │                         │
  │  ハローワーク求人市場   │  ← 28pt bold
  │  総合診断レポート       │
  │                         │
  │  競合調査分析           │  ← 16pt regular
  │  | 2026年04月           │
  │                         │
  │  対象: 東京都 千代田区  │  ← 12pt
  │                         │
  │  [機密情報表記]         │  ← 10pt、下から 40mm
  │                         │
  │  F-A-C株式会社          │  ← 10pt、下から 20mm
  │  | 生成日時: ...        │
  └─────────────────────────┘
```

### 7.3 フッター定型文（必須、変更不可）

- 全ページ @bottom-left: `F-A-C株式会社 | ハローワーク求人データ分析レポート`
- 全ページ @bottom-right: `Page {n} / {total}`
- 本文末尾フッター: `生成日時: {now} | データソース: CSVアップロード分析結果 | ※本レポートはアップロードされたCSVデータに基づく分析です。ハローワーク掲載求人のみが対象であり、全求人市場を反映するものではありません。`

### 7.4 機密情報表記（表紙）

```
この資料は機密情報です。外部への持ち出しは社内規定に従ってください。
```

---

## 8. 実装エージェント (P2) への厳密指示

### 8.1 絶対に守ること

1. **既存 struct のフィールドを変更しない**。`SurveyAggregation`, `JobSeekerAnalysis`, `CompanyAgg`, `EmpTypeSalary`, `SalaryRangePerception`, `InexperienceAnalysis`, `EnhancedStats`, `BootstrapCI`, `TrimmedMeanResult`, `QuartileStats`, `InsightContext`, `Insight`, `Severity`, `NearbyCompany` 全ての field 名・型は既存のまま使う。
2. **関数シグネチャ `render_survey_report_page(agg, seeker, by_company, by_emp_type_salary, salary_min_values, salary_max_values, hw_context, salesnow_companies) -> String` は変更不可**。
3. **`Severity::Critical/Warning/Info/Positive` の色は `helpers.rs::Severity::color()` の値と厳密一致させる**（Critical=#ef4444, Warning=#f59e0b, Info=#3b82f6, Positive=#10b981）。
4. **禁止ワード（1.5 節）を HTML に 1 箇所も出力しない**。
5. **MEMORY 注意書き（「HW 掲載求人のみ」「相関≠因果」）を表紙直後のサマリーと末尾注記の 2 箇所に必ず出す**。

### 8.2 契約検証手順（実装前に必ず実行）

実装開始前に以下を grep で再確認（2026-04-23 事故再発防止）:

```bash
grep -n "pub struct SurveyAggregation" src/handlers/survey/aggregator.rs
grep -n "pub struct JobSeekerAnalysis" src/handlers/survey/job_seeker.rs
grep -n "pub struct EnhancedStats" src/handlers/survey/statistics.rs
grep -n "pub struct InsightContext" src/handlers/insight/fetch.rs
grep -n "pub enum Severity" src/handlers/insight/helpers.rs
grep -n "pub struct NearbyCompany" src/handlers/company/fetch.rs
```

期待フィールド（抜粋、仕様書 4 章参照）と完全一致するか確認。不一致なら**実装を中断して P1 に戻す**。

### 8.3 実装順序（推奨）

1. `render_css()` 全面書き換え（5 章のカラー・タイポ + 6 章の @page 含む）
2. 表紙 (Cover) - 既存の構造を踏襲しつつ 7.2 節の仕様に沿う
3. **新規**: Executive Summary セクション（3 章）を `render_section_executive_summary(html, agg, seeker, by_company, by_emp_type_salary, hw_context)` として実装。既存 `render_section_summary` の後ではなく**表紙直後**に配置
4. 既存 `render_section_summary` は Section 1 の KPI テーブルとして縮小統合（Executive Summary と重複させない）
5. 各 Section の見出し・So What 行・クロスリファレンスを統一フォーマットで追加
6. モノクロ耐性（severity アイコン併記）を既存の全 badge に適用
7. print CSS 全面書き換え（6 章）

### 8.4 テスト要求

実装後、以下が通ること:

```bash
cargo test --lib survey
cargo test --lib insight
```

既存テストケース（`parser_aggregator_audit_test.rs`, `pattern_audit_test.rs`, `global_contract_audit_test.rs`）を破壊しない。

### 8.5 変更範囲

- **編集対象**: `src/handlers/survey/report_html.rs` のみ
- **参照 ONLY**: 8.1 の全 struct 定義ファイル（編集禁止）
- **新規ファイル作成禁止**: 1 ファイル内で完結させる

### 8.6 Feature Flag

本改修は直接 `render_survey_report_page` を書き換える。ロールバックは git revert で対応（feature flag 不要）。

---

## 9. QA チェックリスト (P3)

### 9.1 構造チェック

- [ ] HTML に `<section class="cover-page">` が 1 箇所存在
- [ ] HTML に Executive Summary 相当の section（KPI 5 + アクション 3）が存在
- [ ] 必須セクション 0, 1, 4, 6, 8, 9, 13 が全て HTML 出力に含まれる
- [ ] `hw_context = Some(...)` で HTML を生成した場合、Section 2 (HW 市場比較) が含まれる
- [ ] `hw_context = None` で HTML を生成した場合、Section 2 が**含まれない**
- [ ] `salesnow_companies.is_empty() == true` で Section 12 が**含まれない**
- [ ] Section 13（注記）が本文末尾に含まれる

### 9.2 CSS / 印刷チェック

- [ ] `@page { size: A4 portrait; margin: 12mm; ... }` が含まれる
- [ ] `@page { @bottom-left ...F-A-C株式会社 ... }` が含まれる
- [ ] `@page { @bottom-right ... counter(page) ... }` が含まれる
- [ ] `@media print` ブロックに `-webkit-print-color-adjust: exact;` または `print-color-adjust: exact;` が含まれる
- [ ] `.section { page-break-inside: avoid; }` 相当のルールが含まれる
- [ ] `thead { display: table-header-group; }` が含まれる

### 9.3 コンテンツ必須項目チェック

- [ ] MEMORY 注意書き「ハローワーク掲載求人のみが対象」が**表紙直後 Executive Summary**と**末尾注記**の **2 箇所**に含まれる
- [ ] 「相関」「因果」に関する注意書きが**少なくとも 1 箇所**に含まれる
- [ ] 「F-A-C株式会社」が**3 箇所以上**（表紙、@page footer、本文末尾）に含まれる
- [ ] 生成日時が表紙と本文末尾に含まれる

### 9.4 禁止ワードチェック（1.5 節）

HTML 出力全体に対して以下を grep:

- [ ] `ランキング`, `順位`, `1位`, `上位`（文脈なしで使われていないこと。「件数の多い順」のような安全表現は可）
- [ ] `おすすめ`, `ベスト`, `最適`
- [ ] `優良`, `質が高い`
- [ ] `すべき`, `しなければならない`
- [ ] `確実に`, `必ず`（注意書きの「必ず」以外）

※ 完全一致で出現させない。安全な代替表現になっているか目視確認。

### 9.5 Severity / 色チェック

- [ ] Critical 相当の要素で `#ef4444` または `.bg-red-500/10` が使われている
- [ ] Warning 相当で `#f59e0b` または `.bg-amber-500/10`
- [ ] Info 相当で `#3b82f6` または `.bg-blue-500/10`
- [ ] Positive 相当で `#10b981` または `.bg-emerald-500/10`
- [ ] Severity バッジ全てに文字アイコン（▲▲ / ▲ / ● / ◯）が併記されている（モノクロ耐性）

### 9.6 Executive Summary チェック

- [ ] KPI 5 枚（サンプル件数、主要地域、主要雇用形態、給与中央値、新着比率）が含まれる
- [ ] 推奨優先アクション 3 件（または該当なしプレースホルダー）が含まれる
- [ ] 各アクションに severity バッジ・根拠数値・参照セクション番号が含まれる
- [ ] スコープ注意書き 2 行が Executive Summary 下部に含まれる

### 9.7 データ欠損ハンドリング

以下の欠損ケースでレンダリングがパニックしないこと:

- [ ] `agg.total_count == 0`
- [ ] `agg.enhanced_stats == None`
- [ ] `agg.salary_values.is_empty()`
- [ ] `seeker.expected_salary == None`
- [ ] `seeker.salary_range_perception == None`
- [ ] `seeker.inexperience_analysis == None`
- [ ] `hw_context == None`
- [ ] `salesnow_companies.is_empty()`

### 9.8 既存テスト非破壊

- [ ] `cargo test --lib survey` 全合格
- [ ] `cargo test --lib insight` 全合格
- [ ] `global_contract_audit_test` 全合格

### 9.9 視覚スナップショット（手動）

実装後に HTML を Chrome で開き、印刷プレビュー（Ctrl+P → Save as PDF）で:

- [ ] 表紙が 1 ページ目に完結し次ページ本編が始まる
- [ ] Executive Summary が 2 ページ目に完結する
- [ ] 見出しがページ末尾に孤立しない
- [ ] テーブルがページ跨ぎしてもヘッダーが次ページ冒頭に再出現する
- [ ] モノクロ印刷プレビュー（Chrome: Color → Black and White）で severity が判別可能

---

## 付録 A: 確認済み既存 struct（grep 結果、2026-04-24 時点）

### SurveyAggregation（aggregator.rs:73-96）

- total_count: usize
- new_count: usize
- salary_parse_rate: f64
- location_parse_rate: f64
- dominant_prefecture: Option<String>
- dominant_municipality: Option<String>
- by_prefecture: Vec<(String, usize)>
- by_salary_range: Vec<(String, usize)>
- by_employment_type: Vec<(String, usize)>
- by_tags: Vec<(String, usize)>
- salary_values: Vec<i64>
- enhanced_stats: Option<EnhancedStats>
- by_company: Vec<CompanyAgg>
- by_emp_type_salary: Vec<EmpTypeSalary>
- salary_min_values: Vec<i64>
- salary_max_values: Vec<i64>
- by_tag_salary: Vec<TagSalaryAgg>
- by_municipality_salary: Vec<MunicipalitySalaryAgg>
- scatter_min_max: Vec<ScatterPoint>
- regression_min_max: Option<RegressionResult>
- by_prefecture_salary: Vec<PrefectureSalaryAgg>
- is_hourly: bool

### JobSeekerAnalysis（job_seeker.rs:8-13）

- expected_salary: Option<i64>
- salary_range_perception: Option<SalaryRangePerception>
- inexperience_analysis: Option<InexperienceAnalysis>
- new_listings_premium: Option<i64>
- total_analyzed: usize

### SalaryRangePerception（job_seeker.rs:17-25）

- avg_range_width: i64
- avg_lower: i64
- avg_upper: i64
- expected_point: i64
- narrow_count: usize
- medium_count: usize
- wide_count: usize

### InexperienceAnalysis（job_seeker.rs:28-34）

- inexperience_count: usize
- experience_count: usize
- inexperience_avg_salary: Option<i64>
- experience_avg_salary: Option<i64>
- salary_gap: Option<i64>

### EnhancedStats（statistics.rs:178-189）

- count: usize
- mean: i64
- median: i64
- min: i64
- max: i64
- std_dev: i64
- bootstrap_ci: Option<BootstrapCI>
- trimmed_mean: Option<TrimmedMeanResult>
- quartiles: Option<QuartileStats>
- reliability: String

### BootstrapCI（statistics.rs:10-17）

- lower, upper, bootstrap_mean, sample_mean: i64
- sample_size, iterations: usize
- confidence_level: f64

### TrimmedMeanResult（statistics.rs:74-79）

- trimmed_mean, original_mean: i64
- trimmed_count, removed_count: usize
- trim_percent: f64

### QuartileStats（statistics.rs:119-127）

- iqr, lower_bound, upper_bound: i64
- outlier_count, inlier_count: usize

### CompanyAgg / EmpTypeSalary / TagSalaryAgg / MunicipalitySalaryAgg / PrefectureSalaryAgg / ScatterPoint / RegressionResult

- aggregator.rs:12-60 参照（各フィールドは 4 章で参照済み）

### InsightContext（fetch.rs:12-77）

- vacancy, resilience, transparency, temperature, competition, cascade, salary_comp, monopsony, spatial_mismatch, wage_compliance, region_benchmark, text_quality: Vec<Row>
- ts_counts, ts_vacancy, ts_salary, ts_fulfillment, ts_tracking: Vec<Row>
- ext_job_ratio, ext_labor_stats, ext_min_wage, ext_turnover: Vec<Row>
- ext_population, ext_pyramid, ext_migration, ext_daytime_pop, ext_establishments, ext_business_dynamics: Vec<Row>
- ext_care_demand, ext_household_spending, ext_climate: Vec<Row>
- ext_households, ext_vital, ext_labor_force, ext_medical_welfare, ext_education_facilities, ext_geography: Vec<Row>
- pref_avg_unemployment_rate, pref_avg_single_rate, pref_avg_habitable_density: Option<f64>
- flow: Option<FlowIndicators>
- commute_zone_count, commute_zone_pref_count: usize
- commute_zone_total_pop, commute_zone_working_age, commute_zone_elderly: i64
- commute_inflow_total, commute_outflow_total: i64
- commute_self_rate: f64
- pref, muni: String

### Insight（helpers.rs:117-125）

- id: String
- category: InsightCategory
- severity: Severity
- title, body: String
- evidence: Vec<Evidence>
- related_tabs: Vec<&'static str>

### Severity（helpers.rs:60-66）

- Critical = 0, Warning = 1, Info = 2, Positive = 3
- label() -> 重大/注意/情報/良好
- color() -> #ef4444/#f59e0b/#3b82f6/#10b981

### NearbyCompany（company/fetch.rs:6-14）

- corporate_number, company_name, prefecture, sn_industry, postal_code: String
- employee_count, hw_posting_count: i64
- credit_score: f64

---

## 付録 B: 前回失敗要因と本仕様での対策

| 前回失敗 | 本仕様での対策 |
|----------|--------------|
| 視覚デザインの意図が言語化されず場当たり改修 | 5 章で全タイポ・カラー・スペーシングを数値指定 |
| ペルソナ/ユースケース定義不在 | 1 章で 2 ペルソナ + 成功/失敗状態を明示 |
| 各セクションの存在理由が不明瞭 | 2.3 節で配置論理を明示、4 章で各セクションの目的・配置理由を記述 |
| ブランディング統一原則なし | 7 章で社名表記・表紙レイアウト・フッター文言を固定 |
| モノクロ印刷でチャート潰れ | 5.4, 6.5 でモノクロ耐性ルールを規定（アイコン併記、パターン利用） |
| ランキング語彙の誤用 | 1.5 節で禁止ワードリスト、QA 9.4 で grep チェック |
| 見出し孤立 | 6.6 節で widows / orphans / h2 break-after: avoid |
| サンプル件数と求人件数混同 | 1.5 節で用語を区別、3.3 節で「サンプル件数」と表記 |

---

**仕様書終了**

Agent P2（実装）は本仕様書の 8 章に従って `src/handlers/survey/report_html.rs` を書き換えること。
Agent P3（QA）は本仕様書の 9 章チェックリストで検証すること。
疑義は P1（設計リード）に戻して確認すること。勝手に逸脱しないこと。
