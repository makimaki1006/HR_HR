# Round 12 媒体分析レポート機能 機能単位検証レビュー

**日付**: 2026-05-12
**HEAD**: f6ec14b (Round 11 完了時点)
**ブランチ**: main
**性質**: 機能単位検証 + ロジック深掘り (ultrathinking 5 step + 逆証明) + 既知バグ確定 + 修正計画

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

## 6. 修正後の振り返り (TODO: 修正完了時点で記入)

### 6.1 実装した修正

(修正完了後に記入)

### 6.2 視覚レビュー結果

(PDF 再生成 + PNG 比較で問題解消を確認)

### 6.3 残課題 / 次 Round 持ち越し

(修正未完項目、新発見バグ等)

### 6.4 学んだ教訓

(視覚レビュー軽視の根本対策、agent 事故防止策、test 設計、etc.)

---

## 7. 関連 commit

- (Round 12 修正コミット群、commit 後に追記)
