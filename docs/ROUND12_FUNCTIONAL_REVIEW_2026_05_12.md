# Round 12 媒体分析レポート機能 機能単位検証レビュー

**日付**: 2026-05-12
**HEAD (Round 12 監査開始時点)**: f6ec14b (Round 11 完了時)
**HEAD (Round 12 修正・振り返り追記後)**: 3e74ee5 およびそれ以降
**ブランチ**: main
**性質**: 機能単位検証 + ロジック深掘り (ultrathinking 5 step + 逆証明) + 既知バグ確定 + 修正計画 + 振り返り (§6)

---

## 1. 経緯

Round 11 で「ヒストグラム改善」を実装した際、ユーザーから以下の指摘を受けた:

- 「他のグラフ配置も完全に不備、終わってる、ずっと改善されて無い」
- 「20 回お願いしているけど 1 つも前に進まない」
- 視覚レビュー軽視 + chart として機能していない実装が放置されていた

ユーザー指示で:
1. 機能を最小単位に分けて確認
2. 修正はこの時点では行わない (個別最適リスクのため)
3. 機能テスト + ユニットテスト (テストスクリプトの修正/追加は可)
4. 拡張スコープで機能テスト + ユニットテスト
5. ultrathinking 5 step + 逆証明

---

## 2. 体制

| Phase | Agent | 領域 | 対象ファイル |
|---|---|---|---|
| 1 | A 集計 | 純粋計算 (histogram/median/percentile) | helpers.rs |
| 1 | B マスタ | 都道府県・市区町村 JOIN | region.rs / aggregator.rs / municipality_code_master |
| 1 | C 判定 | 最賃判定 / score 閾値 / R²/HHI/失業率 | wage.rs / market_intelligence.rs / market_tightness.rs |
| 1 | D chart | ECharts options / chart 種別判定 | demographics / scatter / salary_stats / employment / seeker / etc. (12 ファイル) |
| 2 | E 統合 | HTML render end-to-end + K1-K17 再現 | mod.rs (round12_integration_tests) |

**5 step 深掘り**: L1 表面 / L2 論理 / L3 ドメイン不変 / L4 逆証明 / L5 因果トレース

---

## 3. 結果

### 3.1 テスト追加件数

| Agent | 件数 | 場所 |
|---|---:|---|
| A 集計 | 49 | helpers.rs::round12_aggregation_tests |
| B マスタ | 14 | region.rs::round12_master_tests |
| C 判定 (wage) | 26 | wage.rs::round12_judgement_tests |
| C 判定 (market_intelligence) | 23 | market_intelligence.rs::round12_judgement_tests |
| C 判定 (market_tightness) | 23 | market_tightness.rs::round12_judgement_tests |
| E 統合 | 34 | mod.rs::round12_integration_tests |
| **合計** | **169 件** | **全 PASS** |

### 3.2 アンチパターン test 設計

P0 確定バグ (K9/K10/K11/K12/K17) の test は「**現状の問題挙動を assert (PASS)**」する設計。修正実装後はこの test が **FAIL** に転じ、修正完了 indicator となる。

### 3.3 アプリ側コード変更ゼロ

D agent の事故 (PowerShell `Add-Content` で Shift-JIS / UTF-16 書込み → ファイル破壊) があったが、最終的にアプリ側コード変更ゼロで完了。教訓を A2 / C2 / E agent に展開し Edit/Write tool のみ使用。

---

## 4. K1-K17 確定的判定

### 🔴 P0 確定バグ (5 件、即時修正推奨)

| # | バグ | 発生源 | 修正方針 |
|---|---|---|---|
| **K9** | 人口ピラミッド X 軸負値表記 (`-1,000,000`) | demographics.rs:359-364 | `xAxis.axisLabel.formatter` で `Math.abs` 適用 |
| **K10** | 図 11-1/11-2 が chart ではなく数字 3-4 個表示 (求職者心理章) | seeker.rs:33-130 | `render_echart_div` で給与レンジ箱ひげ / 経験者比較棒グラフ追加 |
| **K11** | IQR 図 3-1 が CSS div、ECharts boxplot 未使用 | salary_stats.rs:133-156 | ECharts boxplot type series に置換 |
| **K12** | ヒートマップ 図 6-1 縦サイズ指定なしで thumbnail 化 | style.rs:1090-1098 | `.heatmap-cell` に `min-height: 36px;` 追加 |
| **K17** | 10 歳階級データ vs caption「25-44 ターゲット」の粒度ズレ | demographics.rs:102-107 | `is_target_age` を 5 歳階級専用 or 10 歳階級時 KPI 非表示 |

### 🟡 P1 UX/構造改善 (2 件)

| # | バグ | 発生源 | 修正方針 |
|---|---|---|---|
| **K1** | 「東京都 川崎市」マスタ誤組合せ (dominant_pref と dominant_muni 独立決定) | aggregator.rs:230-248 | 整合性チェック + 警告マーカー or `(pref, muni)` ペア化 |
| **K3** | 最賃割れアラート文言「差 50 円未満: 0 県」が矛盾と読まれる (ロジック健全) | wage.rs:270-276 | 文言を「最賃割れ: X 県 / ぎりぎり (0-49 円): Y 県」と並列明示 |

### 🟢 P2 改善余地 (3 件)

| # | バグ | 場所 | 修正方針 |
|---|---|---|---|
| K2 | 表 7-1 列順「市区町村→都道府県」 (UX、ロジック健全) | region.rs:596-598 | 列順入替 (任意) |
| K6 | 母集団レンジ重複行 | aggregator/parser_aggregator | SQL DISTINCT/GROUP BY 追加 |
| K16 | 散布図 X 軸スケール改善余地 | scatter.rs:116-128 | splitNumber / axisPointer 追加 |

### 🔍 DB 調査必要 (2 件)

| # | バグ | 対象 |
|---|---|---|
| K7 | 人口ピラミッド 女性 bar 欠落 | `v2_external_population_pyramid` データ欠損確認 |
| K8 | 人口ピラミッド 0-9, 10-19 欠落 | 同上 |

### ✅ 誤報 / 実装健全 (3 件)

| # | バグ | 判定 |
|---|---|---|
| K5 | 4 象限軸逆転 | **誤報** (実装は正しい、説明文との読み間違いの可能性) |
| K13 | ヒストグラム ラベル密集 | 実装健全、レンダリング側 |
| K14 | 92% ラベル欠落 | 実装健全、labelLine.length 調整 |

### 横展開新規発見 (D + E)

- **N1**: 全ヒストグラムで xAxis.formatter なし
- **N2**: 図 6-1「ヒートマップ」は ECharts でなく CSS grid (名称不一致)
- **N3**: industry_mismatch.rs に「4 象限図」がなく表のみ
- **N4**: 全 chart で xAxis.name vs caption 軸定義の自動クロスチェック test 不在
- **L5-4**: `age_group_sort_key` で `_ => 9999` フォールバックが filter を通過
- **L5-7**: tooltip 側 formatter 不在 (K9 関連)

---

## 5. Round 12 修正計画 (実装フェーズ)

| Phase | 修正内容 | 件数 |
|---|---|---:|
| 12-A | P0 確定バグ 5 件 (K9/K10/K11/K12/K17) | 5 |
| 12-B | P1 UX/構造 2 件 (K1/K3) | 2 |
| 12-C | P2 改善余地 (K2/K6/K16) | 3 |
| 12-D | DB 調査 (K7/K8) | 2 |

各修正後:
1. 該当 round12_integration_tests が FAIL に転じることを確認 (修正完了 indicator)
2. test を「修正後の正しい挙動」に書き換え (= 再度 PASS に転じる)
3. cargo test --lib 全件 PASS で完了

---

## 6. 修正後の振り返り (2026-05-12 完了)

### 6.1 実装した修正

| # | 修正 | ファイル | commit |
|---|---|---|---|
| K9 ✅ | 人口ピラミッド xAxis に `Math.abs` formatter 追加 (負値 -1,000,000 表記の解消) | demographics.rs:359-364 | 4678a1d |
| K10 ✅ | 求職者心理章 (図 11-1/11-2) に ECharts chart 実装 (給与レンジ横棒 + 経験者比較棒 + レンジ幅ドーナツ) | seeker.rs | 4678a1d |
| K11 ✅ | 給与統計 IQR に ECharts boxplot 追加 (5 数要約 min/Q1/中央値/Q3/max)、iqr-bar は補助残置 | salary_stats.rs | 4678a1d |
| K12 ✅ | `.heatmap-cell` に `min-height: 36px` + flex 中央寄せ | style.rs:1090-1098 | 4678a1d |
| K17 ⚠️ | `is_target_age` を 5 歳階級専用に厳格化 (関数のみ修正、caller 側は別 Round) | demographics.rs:102-107 | 4678a1d |
| K3 ✅ | 最賃割れアラート文言を「**最賃割れ: X 県** / 余裕 50 円未満 (時給ベース): Y 県」並列明示化 | wage.rs:270-276 | b4c58c0 |
| 図番号 ✅ | 求職者心理 4-1/4-2/4-3 → **11-1/11-2/11-3** リナンバ (雇用形態 4-1/4-2 との重複解消) | seeker.rs | 4678a1d |
| Phase 1 test ✅ | 集計・マスタ・判定・統合の unit test 169 件追加 (全 PASS) | helpers.rs / region.rs / wage.rs / market_intelligence.rs / market_tightness.rs / mod.rs::round12_integration_tests | b4c58c0 + 4678a1d |

### 6.2 視覚レビュー結果 (本番 PDF mtime 2026-05-12 16:09, deploy 反映後)

PNG 化 13 page を 1 枚ずつ Read で実物確認:

| Page | 確認項目 | 結果 |
|---|---|---|
| p4 | K11 boxplot 表示 | ✅ 「図 3-1 給与分布 boxplot」横向き、min~max + Q1-Q3 box + 中央値線、補助 IQR シェード併設 |
| p10 | K9 X 軸絶対値、K7/K8 データ完全性 | ✅ 男性 (青) と女性 (桃) 両 bar 描画、0-9〜80+ 全年齢階級表示 |
| p15 | K12 ヒートマップ縦サイズ | ✅ セル縦サイズ改善 (min-height 36px) |
| p16 | K3 最賃文言 | ✅ 「**最賃割れ: 5 県**...」「**最賃以上だが余裕 50 円未満 (時給ベース): 該当なし**」並列明示 |
| p19 | K10 求職者心理 chart | ✅ **完全実装**: 図 11-1 横棒 + ドーナツ、図 11-2 縦棒、全数値が chart 内に表示 |

PDF page 数: **27 → 28** (+1、K10 chart 追加で求職者心理章が拡張)。

cargo test --lib: **1478 passed / 0 failed / 2 ignored**。

### 6.3 残課題 / 次 Round 持ち越し

| # | 内容 | 重大度 |
|---|---|---|
| K17 caller | 10 歳階級データで「25-44 ターゲット層」KPI ラベルが依然表示 (関数は厳格化済、caller 側で「データ粒度不足」を表示する追加修正が必要) | P1 |
| K1 (aggregator 整合性) | `dominant_prefecture` と `dominant_municipality` を独立に最頻値決定 → 「東京都 川崎市」等の不整合発生可能性。`location_parser::designated_city_pref` でガードを追加する設計が必要 | P1 |
| ヒートマップ Top 10 → 47 県全表示 | 縦サイズは改善したが Top 10 のみ。47 県完全マップは P2 改善 | P2 |
| 未経験 0 件の chart 表示 | 経験者比較で未経験 0 件の場合、棒が見えない (「データなし」注記推奨) | P2 (軽微) |
| K2 表 7-1 列順 | 「市区町村→都道府県」を「都道府県→市区町村」に入替検討 (UX) | P2 |
| K6 母集団レンジ重複行 | SQL 層で DISTINCT/GROUP BY 追加 (上位 layer 担当) | P2 |
| K16 散布図 X 軸 | splitNumber / axisPointer 追加 | P2 |
| K7/K8 データソース調査 | `v2_external_population_pyramid` の女性 + 0-9/10-19 データ完全性 | DB 調査 |
| N1-N4 横展開発見 | xAxis.formatter / ヒートマップ ECharts 化 / 4 象限実図 / 軸名 cross-check test | P2 |

### 6.4 学んだ教訓

| 教訓 | 詳細 |
|---|---|
| **視覚レビューを工程に組み込む** | 「テスト pass」と「chart として機能している」を取り違えない。chart 修正タスクは必ず PNG 視覚レビューを工程に含める (memory `feedback_llm_visual_review` の hook 化検討) |
| **データ critical の評価基準** | 「描画されている」≠「データが完全に表示されている」。ユーザー指摘 5 基準 (データ完全性 / 軸表示形式 / 中央軸 / 粒度整合 / 業界標準フォーマット) を chart 評価の標準項目に |
| **PowerShell `Add-Content` で .rs ファイル禁止** | Shift-JIS / UTF-16 で書き込まれ Rust UTF-8 違反 → ファイル破壊。Agent への制約として常に明記 |
| **agent 並列の test 追加は別ファイル分離が安全** | 同一ファイルへの並列 edit は競合・破壊リスク。`tests/round*_*.rs` 形式の独立ファイル or 末尾の独立 `#[cfg(test)] mod` で分離 |
| **アンチパターン test 設計** | 「現状の問題挙動を assert (PASS)」→ 修正後 FAIL に転じる indicator は有用、ただし**修正完了時に assert を反転** して再度 PASS に戻すことを忘れない |
| **agent 報告の批判的 review** | E 監査の K1-K17 確定診断に対し、Phase 1 で複数 agent が「helpers 層では K4/K6 のバグ無し、真因は上位 layer」と逆証明 → agent 報告も鵜呑みにせず複数 agent の cross-check が必要 |
| **scope の狭さ警戒** | 「ヒストグラム改善」を「chart 改善」と取り違えるな。ユーザー指摘箇所以外の同種問題を横展開で発見する責任が私にある |

---

## 7. 関連 commit

- `4678a1d` fix(survey-pdf): restore chart functionality (K9/K10/K11/K12/K17, Round 12 P0)
- `b4c58c0` test+fix(survey): Round 12 Phase 1 unit tests + K3 alert phrasing + minor cleanup
- `c442990` docs(round12): functional review of media report (visual + logic deep audit)
- (本 commit) docs(round12): add post-implementation retrospective + visual review results
