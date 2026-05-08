# PDF セグメント表示・接続監査 (Round 2-4)

**作成日**: 2026-05-08  
**目的**: 既存実装済みのセグメント分析が、なぜ採用コンサル PDF に出ていない / 弱く見えるのかを切り分け  
**read-only 監査**、コード変更なし、commit 1 件のみ。

## 入力 PDF

`out/real_csv_pdf_review_20260508/indeed-2026-04-{27,27_1_,28,30}.pdf` (variant=`market_intelligence` で生成済)

## 9 クロス分類結果

| # | 分析軸 | 実装状態 (DTO/fetch/render) | DB データ | variant 接続 | 実 PDF 表示 | 問題分類 (a-f) | 修正要否 |
|---|---|---|---|---|---|---|---|
| 1 | 地域 × 人口 | ✅ 完備 | ✅ | ✅ MI | 表示 | OK | - |
| 2 | 地域 × 職種 | ✅ 完備 | ✅ | ✅ MI | 行重複 (前橋市×管理 46 行) | **d** (印刷崩れ) | 集約 |
| 3 | 地域 × 性別 | ✅ 完備 (人口ピラミッド) | ✅ | ✅ MI | 表示 | OK | - |
| 4 | 地域 × 年齢 | ✅ 完備 | ✅ | ✅ MI | 表示 | OK | - |
| 5 | 職種 × 性別 | ✅ DTO/fetch | ✅ | ✅ MI | 列が捨てられる | **b** (render 未接続) | render 修正 |
| 6 | 職種 × 年齢 | ✅ DTO/fetch | ✅ | ✅ MI | 列が捨てられる | **b** (render 未接続) | render 修正 |
| 7 | 業界 × 職種 | ❌ 未実装 | △ (postings) | - | 不在 | **f** (真に未実装) | 新規実装 |
| 8 | 業界 × 給与 | ❌ 未実装 | ✅ (postings) | - | 不在 | **f** (真に未実装) | 新規実装 |
| 9 | 職種 × 地域 × 推定β | ✅ 完備 | ✅ | ✅ MI | 指数 200.0 飽和 | **d** (clamp 上限ヒット) | データ調査 |

補足: `render_section_industry_structure` (region.rs:269) は impl + テスト完備だが mod.rs から呼び出されていない (**b** 補足)。

## 分類別件数

- **a** (variant 非表示): 0 件
- **b** (DTO/fetch あり、render が捨てる/未接続): **3 件** (#5, #6, 補足)
- **c** (データ空): 0 件
- **d** (印刷崩れ・行重複): **2 件** (#2, #9)
- **e** (fixture 限定): 0 件
- **f** (真に未実装): **2 件** (#7, #8)

「実装済みだが PDF 未表示/弱表示」(a/b/c/d/e 合計): **5 件**  
「真に未実装」(f): **2 件**

## 即修正可能な接続不備 Top 3

### P0-1: `render_mi_occupation_cells` に age_class/gender 列追加

- 場所: `src/handlers/survey/report_html/market_intelligence.rs:1105`
- 現状: `OccupationCellDto.age_class` (1087) と `gender` (1088) は完備、SQL SELECT (331) も完備、render のみが捨てている
- 修正: 1 関数の表ヘッダ + セル出力に 2 列追加
- 効果: #2 / #5 / #6 / #9 の 4 クロス解消、PDF page 数 5-6 ページ短縮見込み
- 推奨: Round 3 P0 最優先

### P0-2: 業界×給与クロス表新設

- 場所: 新規 `render_section_industry_salary_cross` 関数
- 必要データ: `postings.salary_min/max` (NULL ゼロ) × `job_category_name` (16 種で集約済)
- Round 1-E §3 推奨 2 と整合
- 推奨: Round 3 P1

### P0-3: `render_section_industry_structure` を mod.rs に 1 行接続

- 場所: `src/handlers/survey/report_html/region.rs:269` impl + テスト完備
- 修正: mod.rs から該当関数を呼ぶ 1 行追加
- 効果: 表 6-2「産業別就業者 Top10」が有効化
- 推奨: Round 3 P0

## 既知の懸念

1. **推定 β 行が 11 職種すべて「指数 200.0」固定**: `v2_municipality_target_thickness` の clamp 上限ヒット可能性。データ層調査必要 (#9 の根本原因)
2. **workplace × estimated_beta / resident × measured が両方未収集**: Round 4 完了報告で「将来課題」記載と整合
3. **`fetch_occupation_cells_measured_returns_xor_consistent` テスト**: 既存テストはあるが、render が age/gender を捨てているため XOR 不変条件の意義が弱まっている
4. **#2 行重複 46 行**: Round 1-K でも検出。集約ロジック適用必要 (前橋市 × 管理的職業従事者で全 11 職種 × 諸条件 が並ぶ)

## 真に未実装 2 件 (#7, #8)

### 業界 × 職種 (#7)
- マッピング表整備が前提 (業界カテゴリ ↔ 職種コード)
- Round 3 着手前にユーザー判断 (業界軸の定義 / 表示形式)

### 業界 × 給与 (#8)
- DB データは充足 (postings.salary_min/max + job_category_name)
- 1 関数追加で実装可能
- Round 3 P1 候補

## 次ラウンド推奨

**Round 3 P0**:
1. P0-1 render_mi_occupation_cells に age_class/gender 列追加
2. P0-3 render_section_industry_structure 接続

**Round 3 P1**:
1. P0-2 業界×給与クロス表新設
2. #2 / #9 の data 層調査 (行重複 / 指数飽和)

**Round 3 着手前ユーザー判断**:
- #7 業界×職種マッピング定義
- #9 推定β飽和の根本対応 (clamp 上限引き上げ / 別指標切替 / そのまま維持)
