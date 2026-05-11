# オープンデータ活用 媒体分析レポート 成果監査 (Round 7)

**作成日**: 2026-05-09
**作業ディレクトリ**: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`
**モード**: read-only (実装変更なし、commit/push なし、cleanup なし)
**対象 variant**: `market_intelligence` (採用コンサルレポート PDF)
**判定根拠 PDF**: `out/round2_11_pdf_review/mi_via_action_bar.pdf` (30 pages / 6.0MB / 2026-05-09 09:00 生成)
**判定根拠 PDF (実顧客)**: `out/real_csv_pdf_review_20260508/indeed-2026-04-{27,27_1_,28,30}.pdf` (4 PDF / 25-33 pages)

---

## 1. Executive Summary

- オープンデータ起点の採用コンサル PDF は **Round 4-6 で営業携行可能水準に到達** (Hard NG 13 用語 0、印刷崩れ解消、自治体重複解消、5 マーカー揃存)。
- 「データを取得・投入する」基盤面は強い (10+ 外部統計テーブル、729,949 行の人口×職種×年齢×性別、20,845 行の採用スコア・厚み指数)。
- 一方で **「投入したが PDF に出ていない」未活用資産が依然として 5 件**残る (P0-1/-3 由来の age/gender 列、産業×就業者構成 region.rs:269、業界×給与、業界×職種、推定β飽和)。
- 実現度は **62/100**。投入データ充足度 (≒90) と PDF 露出度 (≒55) の乖離が支配要因。
- 次に投資すべき領域は **既存データの PDF 露出最大化** (新規データ収集ではなく `render_*` 関数 4 件追加)。

---

## 2. できるようになったこと

### 2.1 追加された外部データ (Turso/SQLite 投入済)

| データ名 | 行数 | 利用状況 |
|---|---:|---|
| `municipality_occupation_population` | 729,949 | PDF 採用 (人材供給ヒートマップ) |
| `municipality_recruiting_scores` | 20,845 | PDF 採用 (配信地域ランキング) |
| `v2_municipality_target_thickness` | 20,845 | PDF 採用 (厚み指数列) |
| `municipality_living_cost_proxy` | 1,917 | PDF 採用 (給与・生活コスト比較) |
| `v2_external_population_pyramid` | 17,235 | PDF 採用 (Section 6) |
| `v2_external_population` | 1,917 | PDF 採用 (KPI / 高齢化率) |
| `v2_external_daytime_population` | 1,740 | PDF 採用 (Section 7) |
| `v2_external_migration` | 1,741 | PDF 一部採用 |
| `v2_external_foreign_residents` | 1,742 | PDF 一部採用 |
| `v2_external_minimum_wage` | 47 | PDF 採用 (Section 14) |
| `v2_external_prefecture_stats` | 47 | PDF 採用 (Section 4) |
| `v2_external_job_opening_ratio` | 47 | PDF 採用 (Section 4) |
| `v2_external_industry_structure` | (county×industry) | PDF 採用 (Section 5 産業ミスマッチ) |
| `v2_external_commute_od` / `_with_codes` | 86,762 ×2 | PDF 一部採用 (通勤流入 KPI) |
| `commute_flow_summary` | 27,879 | PDF 一部採用 |
| `v2_commute_flow_summary` | 3,786 | PDF 一部採用 |
| `municipality_geocode` | 2,626 | (地図用、PDF 未使用) |

### 2.2 追加された分析セクション (PDF 出現)

| セクション | データ起源 | 状態 |
|---|---|---|
| Section 3 給与分布統計 (3-1〜3-5) | indeed CSV | A |
| Section 4 採用市場逼迫度 4 軸レーダー | prefecture_stats + job_opening_ratio + minimum_wage | A |
| Section 5 産業ミスマッチ表 4B-1 | industry_structure × postings | A |
| Section 6 人材デモグラフィック (人口ピラミッド) | population_pyramid | A |
| Section 7 主要市区町村別人材デモ | external_population + daytime + migration | A |
| Section 8 雇用形態構成 | postings | A |
| Section 11 都道府県分析 (ヒートマップ) | postings + external_population | A |
| Section 13 市区町村分析 Top15 | postings + external_population | A |
| Section 14 最低賃金比較 | minimum_wage | A |
| MI 配信判断ヒーロー (66/100 等) | recruiting_scores + thickness | A |
| MI 配信地域ランキング (12行→1行集約済) | recruiting_scores | A |
| MI 給与・生活コスト比較 (重複5+→1行集約済) | living_cost_proxy + minimum_wage | A |
| MI 人材供給ヒートマップ (age/gender 列追加済) | municipality_occupation_population | A |

### 2.3 追加された PDF 表現

| 要素 | 状態 |
|---|---|
| 表紙・サマリー・章立て (第3-6章) | A |
| 配信判断ヒーロー (採用市場逼迫度 66/100) | A |
| MI 5 マーカー (`mi-print-summary` / `mi-print-annotations` / `mi-parent-ward-ranking` / `mi-rank-table` / hero bar) | A 全揃存 |
| 出典バッジ ([実測] / [推定 β]) | A |
| 注記・出典・免責 (第6章) | A |
| 印刷時グラフ見切れ防止 (right_margin 36.9-39.2pt) | A |
| Hard NG 用語 (target_count / 推定人数 / 想定人数 / 母集団人数) 検出 | A 0 件 |
| 自治体重複集約 (配信地域 12→1, 生活コスト 5+→1) | A |

### 2.4 追加された検証

| 検証 | 件数 | 状態 |
|---|---|---|
| `cargo test --lib` | 1197 PASS / 0 FAIL / 2 ignored | A |
| `cargo test --lib market_intelligence` | 118 PASS | A |
| `cargo test --test no_forbidden_terms` | 5 PASS | A |
| 本番 E2E (Render) | 21 passed / 0 failed / 2 skipped | A |
| 実顧客 CSV PDF 4 本目視 | indeed-2026-04-{27,27_1_,28,30} | A |
| MI variant PDF PNG 目視 | 4 PNG (page 5/6/7/13) | A |

---

## 3. 実現確認マトリクス (主要 22 機能)

実コード + 実 PDF 両方で確認。docs だけの主張は B 以下に格下げ。

| # | 機能 | 実装 | DB | fetch | render | PDF表示 | 実顧客CSVで出る | 検証済 | 判定 | 根拠 |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 地域 × 人口 (KPI) | Y | Y | Y | Y | Y | Y | Y | A | round2_11 page 13 / Round6 §3 |
| 2 | 地域 × 職種 (配信地域ランキング) | Y | Y | Y | Y | Y | Y | Y | A | Round6 commit 11 集約済 |
| 3 | 地域 × 年齢 (人口ピラミッド) | Y | Y | Y | Y | Y | Y | Y | A | Section 6 |
| 4 | 地域 × 性別 (人口ピラミッド男女別) | Y | Y | Y | Y | Y | Y | Y | A | Section 6 図 D-1 |
| 5 | 職種 × 年齢 (人材供給テーブルに列追加) | Y | Y | Y | Y | Y | Y | Y | A | market_intelligence.rs:1664-1682 列存在 |
| 6 | 職種 × 性別 (同上) | Y | Y | Y | Y | Y | Y | Y | A | 同上 |
| 7 | 産業ミスマッチ (業界別求人構成) | Y | Y | Y | Y | Y | Y | Y | A | Section 5 表 4B-1 |
| 8 | 採用市場逼迫度 4 軸レーダー | Y | Y | Y | Y | Y | Y | Y | A | Section 4 / page 7 |
| 9 | 給与×地域 (市区町村平均月給・中央値) | Y | Y | Y | Y | Y | Y | Y | A | Section 13 |
| 10 | 給与×雇用形態 | Y | Y | Y | Y | Y | Y | Y | A | Section 8 表 4-1 |
| 11 | 最低賃金比較 (47県) | Y | Y | Y | Y | Y | Y | Y | A | Section 14 |
| 12 | 配信検証候補 KPI (スコア80+件数) | Y | Y | Y | Y | Y | Y | Y | A | real_csv page 26 |
| 13 | 給与・生活コスト比較 (1自治体1行) | Y | Y | Y | Y | Y | Y | Y | A | Round6 commit 13 |
| 14 | 通勤流入 KPI (取得数) | Y | Y | Y | Y | Y | Y | Y | A | real_csv page 26 |
| 15 | 産業×就業者構成 Top10 (region.rs:269) | Y | Y | Y | Y(未接続) | (重複機能 integration.rs 経由で出る) | 部分 | Y | B | Round 2-4 §P0-3 / mod.rs から `render_section_industry_structure` 未呼出 |
| 16 | 推定 β 厚み指数 200.0 飽和 | Y | Y | Y | Y | Y (飽和) | Y | 部分 | B | Round 2-4 #9 #懸念 1 |
| 17 | 業界 × 給与クロス | N | Y(postings.salary) | N | N | N | N | N | D | Round 2-4 #8 / Round 1-E §3 推奨 2 |
| 18 | 業界 × 職種クロス | N | 部分 | N | N | N | N | N | D | Round 2-4 #7 |
| 19 | 業界 × 雇用形態クロス | N | Y(postings) | N | N | N | N | N | D | Round 1-E §2 |
| 20 | 企業規模 × 給与 × 業界 (3軸) | N | 部分 (SalesNow) | N | N | N | N | N | D | Round 1-E §3 推奨 3 |
| 21 | 業界別 求職者人口 (industry×population) | N/A | N | - | - | - | - | - | E | mop に industry 軸なし (Round 1-F §4) |
| 22 | 職種別 通勤OD | N/A | N | - | - | - | - | - | E | commute_flow_summary.occupation='all' のみ |

**集計**: A=14 / B=2 / C=0 / D=4 / E=2

---

## 4. 会社目的への貢献

### 4.1 採用コンサル資料としての価値

- 「indeed CSV 単体では出せない情報 (国勢調査人口ピラミッド、最低賃金、有効求人倍率、産業構成、市区町村別生活コスト指標、採用スコア)」が PDF 一本に統合された。
- 配信判断の数値根拠 (66/100 / スコア80+件数 / 厚み指数 / 通勤流入数) が紙面で示せる状態。営業の口頭説明に依存していた論点をレポートに移植できた。
- ただし「業界×給与」「業界×職種」「企業規模×給与」など **採用コンサル価値の高いクロス 3 件が依然欠落** (Round 1-E §2 完全欠落 Top 3)。

### 4.2 オープンデータ活用の価値

- e-Stat 系 14 テーブル + Agoop メッシュの一部 + 国勢調査 mop が PDF 文脈に組み込まれた。
- CSV 単体では「給与の分布」「雇用形態構成」しか言えなかった水準から、「地域人口厚み・産業構成・最低賃金・通勤流入」まで言える水準に到達。
- ただし投入済 14 テーブルのうち「PDF 採用 13、未活用 (PDF 内では地図表示なし) 1 (`municipality_geocode` 2,626 行)」+ Agoop 1km メッシュ (49,370 行/月) は PDF 未活用。

### 4.3 PDF 納品物としての価値

- Round 6 で `Hard NG 13 用語 0 件 / 印刷崩れ解消 / 自治体重複解消 / variant 隔離維持` を達成。**営業携行可能水準**。
- 残る品質課題は P2-A〜E (page 25 情報密度、ヒストグラム軸ラベル重なり、fixture 法人名差替え)。致命的問題ではない。
- 自己説明性は中程度 (注記・出典・免責は揃存、ただし「業界/業種/産業」3 用語並存などの用語整理余地あり = Round 1-E §4)。

---

## 5. 未接続・未活用の資産 (Round 2-4 b/c/d 突合)

| 資産 | 棚卸し由来分類 | 残存度 | 即修正コスト |
|---|---|---|---|
| `render_section_industry_structure` (region.rs:269) を mod.rs から未呼出 | Round 2-4 P0-3 / b 補足 | 残存 (integration.rs に重複機能あるため致命的ではない) | 1 行 |
| `OccupationCellDto.age_class/gender` の表列出力 | Round 2-4 P0-1 / b | **解消済** | (済) |
| 推定β 厚み指数の 200.0 飽和 | Round 2-4 #9 / d | 残存 | データ層調査要 |
| 配信地域ランキング 自治体重複 | Round 2-4 #2 / d | **解消済 (Round6 commit 11)** | (済) |
| 業界×給与クロス | Round 2-4 P0-2 / f | 残存 | 1 関数追加 (postings 既存) |
| 業界×職種クロス | Round 2-4 #7 / f | 残存 | マッピング表 + 1 関数 |
| `municipality_geocode` 2,626 行 (地図) | Round 5 §1 | PDF 未使用 | 別スコープ (PDF に地図画像化は重い) |
| Agoop `posting_mesh1km_*` 49,370 行 | Round 5 §2 | PDF 未使用 | 1km メッシュ→市区町村集計が要 |

---

## 6. 既存 DB から追加できる改善 (10+ 件)

新規巨大データ収集なし。投入済データのみで実現可能なもの。

| 案 | 使う既存データ | 実装コスト | 期待価値 | 優先度 | 分類 |
|---|---|---|---|---|---|
| I-1: 業界×給与クロス表追加 | `postings.salary_min/max` + `job_category_name` (16) | 中 (1関数) | 高 (Round 1-E 完全欠落 Top 2) | P0 | 2 |
| I-2: 職種×給与クロス表追加 | `postings.salary_min/max` + `occupation_major` (14) | 中 | 高 (Round 1-E 完全欠落 Top 1) | P0 | 2 |
| I-3: `render_section_industry_structure` を mod.rs に 1 行接続 | region.rs:269 既存 | 低 | 中 (重複機能あるため微) | P1 | 1 |
| I-4: 業界×雇用形態クロス | `postings.industry_raw` + `employment_type` | 低 | 中 | P1 | 1 |
| I-5: 雇用形態×地域 (市区町村Top15正社員比率) | `postings` のみ | 低 | 中 (本サンプル正社員99.6%で差出ず) | P2 | 1 |
| I-6: 業界×地域ヒートマップ (47×16) | `postings` + `job_category_name` | 中 | 中 | P1 | 2 |
| I-7: 求職者-求人ギャップ (mop × postings 職種粒度) | `mop` + `postings` + occ コード対応表 | 高 (対応表設計要) | 高 | P1 | 3 |
| I-8: 推定β 厚み指数 飽和の根本対応 | `v2_municipality_target_thickness` データ層調査 | 中 | 中 (誤読防止) | P1 | 3 |
| I-9: 通勤流入 Top10 元自治体表 | `v2_commute_flow_summary.top10_json` | 低 | 中 | P2 | 1 |
| I-10: 昼夜間人口比 ラベル化 (働く人が流入する街か) | `v2_external_daytime_population` | 低 | 中 | P2 | 1 |
| I-11: 外国人比率 自治体スライス | `v2_external_foreign_residents` | 低 | 低 | P3 | 1 |
| I-12: 企業規模×業界×給与 (3軸) | SalesNow `salesnow_aggregate_for_f6.csv` (11,072) + `postings.salary` | 高 | 高 (Round 1-E 完全欠落 Top 3) | P1 | 3 |
| I-13: 業界/業種/産業 用語整理 (用語集) | (実装ではなく文言整理) | 低 | 中 (Round 1-E §4) | P2 | 1 |

**分類凡例**: 1=既存 fetch で render 接続のみ / 2=SQL/DTO 追加 / 3=設計が必要 / 4=データ不足

---

## 7. 優先順位付き Next Action (Top 5)

| # | 案 | 理由 | コスト |
|---|---|---|---|
| 1 | I-1 業界×給与クロス表追加 | postings 単独・NULL ゼロで即可、Round 1-E 完全欠落 Top 2、コンサル価値高 | 中 |
| 2 | I-2 職種×給与クロス表追加 | Round 1-E 完全欠落 Top 1、求人原稿の競争力評価に直結 | 中 |
| 3 | I-3 region.rs:269 を mod.rs に 1 行接続 | 1 行修正で即時、Round 2-4 P0-3 由来の懸案消化 | 低 |
| 4 | I-12 企業規模×業界×給与 (3軸) | Round 1-E 完全欠落 Top 3、SalesNow 既存集約版で実装可 | 高 |
| 5 | I-8 推定β 厚み指数 200.0 飽和のデータ層調査 | 数値の信頼性を損なう既知問題、解釈ラベル化での暫定対応も含む | 中 |

---

## 8. 総合評価

### 8.1 成果サマリ

- **Before (Round 3 以前)**: indeed CSV 単体ベースで給与/雇用形態しか語れない PDF。
- **After (Round 6 完了時点)**: 国勢調査・最低賃金・有効求人倍率・産業構成・通勤流入・採用スコア・生活コストを統合した 30-page PDF。Hard NG 0、印刷品質クリア、自治体重複集約済。
- **未踏領域**: 業界×給与・職種×給与・規模×業界×給与の 3 大欠落クロス。

### 8.2 実現度: 62/100

- 投入データ充足度: 90 (e-Stat 14 + Agoop + SalesNow 集約版)
- PDF 露出度: 55 (投入済の 14 テーブル中 13 が PDF 出現するが、最重要クロス 3 件が欠落)
- 検証充足度: 80 (cargo 1197 + 本番 E2E 21 + 実顧客 CSV 4 PDF 目視)
- UI/印刷品質: 75 (Round 6 で改善、P2 残課題あり)
- 全体: 62

### 8.3 事業上の意味

- **適合顧客**: 都道府県・市区町村粒度の採用相談、indeed 求人 CSV を持参する人材派遣・人材紹介の顧客、地域 (47 県+主要 1,696 市区町村) を跨いだ配信戦略を検討する顧客。
- **強み**: HW 単独では言えない indeed CSV 統合 / 採用スコア・生活コスト・通勤流入を市区町村粒度で単一 PDF にまとめる。
- **弱み**: 業界別給与・職種別給与の数値根拠が現状欠落 (営業説明で「資料外」になる)。本サンプル (群馬中心 252 件・正社員 99.6%) のように地域が偏ると雇用形態×地域などのクロスが意味を成さない。

### 8.4 出すべきでない顧客ケース

- 「業界別の給与水準を出してほしい」が主目的の顧客 → 現状 PDF に該当表なし。I-1 実装後に再検討。
- 「自社規模 (中小/大企業) と同層の競合給与」を主目的とする顧客 → 規模×給与クロス未実装。
- 全国レベルの統計分析を求める顧客 (本 PDF は indeed CSV 持参前提のサンプル特性ベース。CSV が単一県中心の場合、地域分析の説明力が落ちる)。
- HW 求人 = 求人市場全体と誤認しそうな顧客 → 注記はあるが、口頭補足が必要 (`feedback_hw_data_scope.md` の制約)。

### 8.5 次に投資すべき 1 領域

**「PDF 露出最大化」**。新規データ収集ではなく、既に Turso/SQLite に投入済かつ PDF 文脈にハマる業界×給与・職種×給与・規模×業界×給与の 3 クロス追加で、実現度を 62 → 80 まで引き上げられる見込み。投資効率が最大。

---

## 9. 編集ファイル

- 絶対パス: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\OPEN_DATA_MEDIA_REPORT_VALUE_AUDIT_2026_05_09.md`
