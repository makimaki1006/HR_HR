# V2 ハローワークダッシュボード 詳細実装計画書

**策定日**: 2026-03-13
**3専門チーム調査の統合版**
**前提ドキュメント**: `IMPROVEMENT_ROADMAP_V2.md`（コンセプト版）

---

## 調査結果サマリー（事実に基づく）

### 確認済みDBスキーマ（postingsテーブル主要カラム）

| カラム | 型 | 用途 | 充填率（推定） |
|--------|-----|------|--------------|
| `salary_min` | INTEGER | 月給最低額（円） | 中〜高（0値あり、NULLIF使用） |
| `salary_max` | INTEGER | 月給最高額（円） | 中〜高 |
| `salary_type` | TEXT | 月給/日給/時給 | 高（インデックス作成済み） |
| `bonus_months` | REAL | 賞与月数 | 中（30-50%） |
| `annual_holidays` | INTEGER | 年間休日数 | 中（40-60%） |
| `overtime_monthly` | REAL | 月間残業時間 | 低〜中（20-40%） |
| `employee_count` | INTEGER | 従業員数 | 中（40-60%） |
| `employment_type` | TEXT | 雇用形態 | 高（95%+） |
| `industry_raw` | TEXT | 産業分類（原文） | 高（インデックス作成済み） |
| `job_type` | TEXT | 事業所形態 | 高 |
| `facility_name` | TEXT | 施設名 | 高 |
| `recruitment_reason_code` | INTEGER | 求人理由（1=欠員,2=増員,3=新設） | 高（80%+） |
| `job_description` | TEXT | 仕事内容 | 高 |
| `requirements` | TEXT | 応募要件 | 高 |
| `benefits` | TEXT | 福利厚生 | 高 |
| `company_features` | TEXT | 企業特徴 | 中 |
| `latitude` / `longitude` | REAL | 座標 | 中（要確認） |
| `education_required` | TEXT | 学歴要件 | 中（60-80%） |
| `capital` | INTEGER | 資本金 | 低〜中 |
| `founding_year` | INTEGER | 設立年 | 低〜中 |

### 既存v2テーブル（7個）

| テーブル | Phase | 内容 |
|---------|-------|------|
| `v2_vacancy_rate` | Phase 1 | 欠員補充率 |
| `v2_regional_resilience` | Phase 1 | Shannon多様性/HHI |
| `v2_transparency_score` | Phase 1 | 透明性スコア |
| `v2_text_temperature` | Phase 2 | テキスト温度計 |
| `v2_cross_industry_competition` | Phase 2 | 異業種競合 |
| `v2_anomaly_stats` | Phase 2 | 異常値検出 |
| `v2_cascade_summary` | Phase 2 | カスケード集計 |

### 重要な発見

| 発見 | 影響 |
|------|------|
| **RESAS APIは2025年3月24日に提供終了** | Phase 4-3の人口データ取得は国土交通DPF GraphQL APIに変更 |
| **v2_vacancy_rateはrecruitment_reason_codeから計算** | Phase 5-1の充足予測でデータリーケージ発生 → 学習時は除外必須 |
| **デバッグログにemail/pw_len出力中** | Phase 0で即時修正 |
| **/healthはDB確認なし** | `"OK"`文字列のみ → JSON+DB接続チェックに拡張 |
| **キャッシュデフォルト2000エントリ** | 新タブ追加で不足の可能性 → 3000に引上げ |

---

## 実装前必須確認事項（Bash実行が必要）

以下はチーム調査時にDB直接アクセスできず未確認。**実装着手前に必ず確認**:

```sql
-- 1. 主要カラム充填率
SELECT
  COUNT(*) as total,
  COUNT(salary_min) as salary_min_filled,
  COUNT(annual_holidays) as holidays_filled,
  COUNT(bonus_months) as bonus_filled,
  COUNT(latitude) as lat_filled,
  COUNT(longitude) as lng_filled,
  COUNT(overtime_monthly) as overtime_filled
FROM postings;

-- 2. recruitment_reason_code分布（Phase 5教師データ品質）
SELECT recruitment_reason_code, COUNT(*)
FROM postings GROUP BY recruitment_reason_code;

-- 3. salary_type分布
SELECT salary_type, COUNT(*) FROM postings GROUP BY salary_type;

-- 4. geocodeカバー率（Phase 3-4, 5-2に影響）
SELECT COUNT(*) FROM postings WHERE latitude IS NOT NULL AND latitude > 0;
```

---

## Phase 0: 基盤整備

### 0-1. デバッグログのマスキング

**現状**: `src/lib.rs` L223-228 でemail全文、pw_len、外部パスワード長+期限をログ出力中

```rust
// 修正後: ドメインのみ出力、パスワード情報なし
tracing::info!(
    "Login attempt: domain={}, external_count={}",
    form.email.split('@').nth(1).unwrap_or("?"),
    state.config.external_passwords.len(),
);
tracing::info!("Login result: ok={}", pw_ok);
```

**ファイル**: `src/lib.rs` 2行変更

### 0-2. /health エンドポイント拡張

**現状**: `src/lib.rs` L466-468 で `"OK"` 文字列のみ

```rust
// 修正後: DB接続+キャッシュ状態をJSON返却
async fn health_check(State(state): State<Arc<AppState>>) -> Json<Value> {
    let db_ok = state.hw_db.is_some();
    let db_rows = if let Some(db) = &state.hw_db {
        db.query_scalar::<i64>("SELECT COUNT(*) FROM postings", &[]).unwrap_or(-1)
    } else { -1 };
    Json(json!({
        "status": if db_ok { "healthy" } else { "degraded" },
        "db_connected": db_ok, "db_rows": db_rows,
        "cache_entries": state.cache.len(),
    }))
}
```

**ファイル**: `src/lib.rs` ~15行変更（ルート定義のstate注入も必要）

### 0-3. キャッシュ設定最適化

**現状**: `src/config.rs` L87 デフォルト2000エントリ

```rust
// 修正: 2000 → 3000
.unwrap_or(3000),
```

**ファイル**: `src/config.rs` 1行変更

### Phase 0 合計: ~20行変更、3ファイル

---

## Phase 1: 給与分析

**新規ファイル**: `scripts/compute_v2_salary.py`
**追記**: `src/handlers/analysis.rs`

### 1-1. 給与構造分析（v2_salary_structure）

```sql
CREATE TABLE v2_salary_structure (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    salary_type TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    avg_salary_min REAL, avg_salary_max REAL,
    median_salary_min REAL, median_salary_max REAL,
    p25_salary_min REAL, p75_salary_min REAL,
    salary_spread REAL,
    avg_bonus_months REAL, bonus_disclosure_rate REAL,
    estimated_annual_min REAL, estimated_annual_max REAL,
    PRIMARY KEY (prefecture, municipality, industry_raw, emp_group, salary_type)
);
```

**アルゴリズム**: 地域×産業×雇用形態×給与種類でグループ化 → numpy.percentile([10,25,50,75,90]) → 年収推定(月給×(12+賞与月数))
**工数**: Python ~200行 + Rust ~250行

### 1-2. 給与競争力指数（v2_salary_competitiveness）

```sql
CREATE TABLE v2_salary_competitiveness (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    local_avg_salary REAL NOT NULL,
    national_avg_salary REAL NOT NULL,
    competitiveness_index REAL NOT NULL,  -- (local-national)/national*100
    percentile_rank REAL NOT NULL,         -- 全国内順位(0-100)
    sample_count INTEGER NOT NULL,
    PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
);
```

**アルゴリズム**: 全国平均をemp_group×industryで計算 → 地域別と比較 → パーセンタイルランキング
**工数**: Python ~120行 + Rust ~150行

### 1-3. 報酬パッケージ総合スコア（v2_compensation_package）

```sql
CREATE TABLE v2_compensation_package (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    avg_salary_min REAL, avg_annual_holidays REAL,
    avg_bonus_months REAL, avg_overtime REAL,
    salary_pctile REAL, holidays_pctile REAL,
    bonus_pctile REAL, overtime_pctile REAL,
    composite_score REAL NOT NULL,  -- 給与40%+休日25%+賞与20%+残業15%
    rank_label TEXT NOT NULL,        -- S/A/B/C/D
    PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
);
```

**アルゴリズム**: 4指標をパーセンタイル化 → 加重平均（給与40%+休日25%+賞与20%+残業15%） → S(≥80)/A(≥65)/B(≥50)/C(≥35)/D(<35)
**工数**: Python ~150行 + Rust ~120行

### Phase 1 合計: Python ~470行 + Rust ~520行、3テーブル

---

## Phase 2: テキスト分析

**新規ファイル**: `scripts/compute_v2_text.py`
**追記**: `src/handlers/analysis.rs`

### 2-1. 求人原稿品質スコア（v2_text_quality）

```sql
CREATE TABLE v2_text_quality (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    avg_total_chars INTEGER NOT NULL,
    avg_numeric_density REAL NOT NULL,
    avg_bullet_rate REAL NOT NULL,
    avg_section_count REAL NOT NULL,
    description_fill_rate REAL NOT NULL,
    requirements_fill_rate REAL NOT NULL,
    benefits_fill_rate REAL NOT NULL,
    features_fill_rate REAL NOT NULL,
    quality_score REAL NOT NULL,  -- 0-100
    PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
);
```

**アルゴリズム**: 文字数(0-30) + 数値含有率(0-25) + 構造化度(0-20) + 充填率(0-25) = quality_score
**工数**: Python ~180行 + Rust ~100行

### 2-2. キーワードプロファイル（v2_keyword_profile）

```sql
CREATE TABLE v2_keyword_profile (
    prefecture TEXT NOT NULL,
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    top_keywords TEXT NOT NULL,         -- "介護:1523,看護:890,..."
    distinctive_keywords TEXT NOT NULL,  -- TF-IDF上位
    avg_keyword_diversity REAL NOT NULL,
    PRIMARY KEY (prefecture, industry_raw, emp_group)
);
```

**アルゴリズム**: MeCabなし正規表現トークナイザ(`[一-龥ぁ-んァ-ヴー]{2,8}`) → TF-IDF → 頻度TOP10 + 特徴語TOP10
**工数**: Python ~150行 + Rust ~80行

### 2-3. テキスト類似度（v2_text_similarity）

```sql
CREATE TABLE v2_text_similarity (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    intra_facility_dup_rate REAL NOT NULL,
    template_rate REAL NOT NULL,
    avg_unique_ratio REAL NOT NULL,
    most_common_template TEXT,
    PRIMARY KEY (prefecture, municipality, emp_group)
);
```

**アルゴリズム**: テキスト正規化→MD5ハッシュ(先頭500文字) → 施設内重複率 + 地域内テンプレート率(同一ハッシュ3件以上)
**工数**: Python ~130行 + Rust ~80行

### Phase 2 合計: Python ~460行 + Rust ~260行、3テーブル

---

## Phase 3: 市場構造分析

**新規ファイル**: `scripts/compute_v2_market.py`
**追記**: `src/handlers/analysis.rs`

### 3-1. 企業採用戦略の類型化（v2_employer_strategy + v2_employer_strategy_summary）

**4象限分類**: salary_percentile × amenity_score（休日/賞与/福利厚生キーワード/低残業のスコア）
- プレミアム型（高給与+高福利）
- 給与一本勝負型（高給与+低福利）
- 福利厚生重視型（低給与+高福利）
- コスト優先型（低給与+低福利）

**工数**: Python ~200行 + Rust ~100行

### 3-2. 地域間ベンチマーク（新規テーブル不要）

**6軸レーダーチャート**: 求人活性度/給与競争力/人材定着率/産業多様性/情報透明性/テキスト温度
既存v2テーブルのクエリ組み合わせ → EChartsレーダーで表示

**工数**: Rust ~200行 + JS ~50行

### 3-3. 雇用者独占力指数（v2_monopsony_index）

```sql
CREATE TABLE v2_monopsony_index (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    total_postings INTEGER NOT NULL,
    unique_facilities INTEGER NOT NULL,
    hhi REAL NOT NULL,
    concentration_level TEXT NOT NULL,  -- '分散'/'やや集中'/'高集中'
    top1_name TEXT, top1_share REAL,
    top3_share REAL, top5_share REAL,
    gini REAL,
    PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
);
```

**アルゴリズム**: V1 compute_layer_a.py A-2パターン踏襲 → HHI = Σ(share_i²) + Gini係数
**閾値**: HHI < 0.15 = 分散, 0.15-0.25 = やや集中, ≥ 0.25 = 高集中
**工数**: Python ~150行 + Rust ~80行

### 3-4. 空間的ミスマッチ検出（v2_spatial_mismatch）

```sql
CREATE TABLE v2_spatial_mismatch (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL,
    emp_group TEXT NOT NULL,
    posting_count INTEGER NOT NULL,
    avg_salary_min REAL,
    accessible_postings_30km INTEGER NOT NULL DEFAULT 0,
    accessible_avg_salary_30km REAL,
    accessible_postings_60km INTEGER NOT NULL DEFAULT 0,
    accessible_avg_salary_60km REAL,
    salary_gap_vs_accessible REAL,
    isolation_score REAL NOT NULL DEFAULT 0,  -- 0-1
    PRIMARY KEY (prefecture, municipality, emp_group)
);
```

**アルゴリズム**: municipality_geocode座標 → haversine距離 → 30km/60km圏内集計 → isolation_score = 1 - min(accessible/median, 1.0)
**最適化**: 緯度差2度以内でフィルタ（3.6Mペア → ~360Kペアに削減）
**工数**: Python ~250行 + Rust ~120行

### Phase 3 合計: Python ~600行 + Rust ~500行 + JS ~50行、3テーブル

---

## Phase 4: 外部データ統合

**新規ファイル**: `scripts/compute_v2_external.py`
**追記**: `src/handlers/analysis.rs` + `src/handlers/overview.rs`
**環境変数**: `ESTAT_APP_ID`（e-Stat APIキー、Phase 4-1/4-2/4-3実行時のみ）

### 4-0. 市区町村コードマッピング（municipality_code_map）

**データソース**: 総務省「全国地方公共団体コード」CSV
**マッチ戦略**: 完全一致 → 部分一致 → 政令市区名変換
**工数**: Python ~100行

### 4-1. 有効求人倍率（v2_external_job_ratio）

**API**: e-Stat v3.0 (`statsCode=00450222`)
**更新頻度**: 月次
**工数**: Python ~120行 + Rust ~80行

### 4-2. 賃金構造基本統計（v2_external_wage_structure + v2_industry_mapping）

**API**: e-Stat v3.0 (`statsCode=00450091`)
**注意**: HW DBのindustry_raw（中分類）→ 賃金統計（大分類）の多対一マッピングが必要
**更新頻度**: 年次
**工数**: Python ~180行 + Rust ~80行

### 4-3. 人口データ（v2_external_population）

**API**: e-Stat国勢調査 (`statsCode=00200521`) + **国土交通DPF GraphQL**
**重要**: ~~RESAS API~~ → **2025年3月24日提供終了** → 国土交通DPF代替
**DPFエンドポイント**: `https://www.mlit-data.jp/api/v1/graphql`（APIキー不要）
**更新頻度**: 国勢調査5年ごと（最新2020年、次回2025年版は2026年秋公開）
**工数**: Python ~200行 + Rust ~60行

### 4-4. 最低賃金マスタ（v2_external_minimum_wage）

**データソース**: 厚労省PDF → 手動CSV化（47行のみ）
**更新頻度**: 年次（10月施行）
**2025年度**: 全県1,000円超え達成（東京1,226円〜秋田1,023円）
**工数**: Python ~70行

### 4-5. 介護施設データ（v2_external_care_facilities + v2_care_coverage_summary）

**データソース**: 介護サービス情報公表システム オープンデータ（厚労省CSV）
**更新頻度**: 半年（6月末/12月末）
**マッチ**: 法人格除去 + Jaccard係数(2-gram) > 0.5
**工数**: Python ~250行 + Rust ~80行

### Phase 4 合計: Python ~920行 + Rust ~300行、7テーブル

---

## Phase 5: 予測・推定モデル

**新規ファイル**: `scripts/compute_v2_prediction.py`
**追記**: `src/handlers/analysis.rs`
**依存**: Phase 1-4のテーブル（特徴量として使用）

### 5-1. 充足困難度予測（v2_fulfillment_score + v2_fulfillment_summary）

**モデル**: LightGBM (n_estimators=200, max_depth=5, learning_rate=0.05)
**教師データ**: `recruitment_reason_code == 1`（欠員補充）= 充足困難の代理変数
**特徴量**: salary_min, annual_holidays, bonus_months, employee_count, overtime_monthly, education_required, employment_type, v2_transparency_score, v2_text_temperature

**データリーケージ警告**: v2_vacancy_rateはrecruitment_reason_codeから計算されているため、学習時は除外。スコアリング時のみ使用。

**評価**: 5-fold StratifiedKFold CV, AUC ≥ 0.65 で公開、< 0.60 は延期
**出力変換**: predict_proba * 100 → A(0-25)/B(25-50)/C(50-75)/D(75-100)
**工数**: Python ~400行 + Rust ~120行

### 5-2. 地域間流動性推定（v2_mobility_estimate）

**モデル**: 重力モデル（Gravity Model）
**計算**: `gravity_score = (avg_salary * n_postings) / avg_distance^1.5`
**前提条件**: geocodeカバー率50%以上（未確認、要Bash検証）→ 50%未満なら延期

**工数**: Python ~180行 + Rust ~100行

### 5-3. 給与分位テーブル（v2_shadow_wage）

```sql
CREATE TABLE v2_shadow_wage (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL DEFAULT '',
    industry_raw TEXT NOT NULL DEFAULT '',
    emp_group TEXT NOT NULL,
    salary_type TEXT NOT NULL DEFAULT '月給',
    total_count INTEGER NOT NULL,
    p10 REAL, p25 REAL, p50 REAL, p75 REAL, p90 REAL,
    mean REAL, stddev REAL, iqr REAL,
    PRIMARY KEY (prefecture, municipality, industry_raw, emp_group, salary_type)
);
```

**最小サンプル**: 10件
**フィルタ**: salary_min ≥ 50000（月給）で外れ値除去
**工数**: Python ~150行 + Rust ~100行

### Phase 5 合計: Python ~730行 + Rust ~320行、5テーブル

---

## Phase 6: プロダクト化

**新規ファイル**: `src/handlers/diagnostic.rs`, `src/handlers/export.rs`, `static/js/diagnostic.js`
**追記**: `src/lib.rs`, `src/handlers/mod.rs`, `src/handlers/competitive/render.rs`, `templates/dashboard_inline.html`

### 6-1. 条件入力→市場診断 UI（新規タブ: 7番目）

**エンドポイント**: GET `/tab/diagnostic` + GET `/api/diagnostic/evaluate`
**パターン**: competitive/comp_filterと同じHTMXフォーム → パーシャルHTML返却
**入力**: 月給、年間休日、賞与月数、雇用形態
**出力**: パーセンタイルバー + 充足困難度 + 改善提案

**工数**: Rust ~250行 + JS ~120行

### 6-2. 印刷レポート強化

**既存**: comp_reportハンドラーに分析セクション追加
**追加**: 欠員補充率 + 透明性 + 充足困難度 + 給与パーセンタイル
**印刷CSS**: `@media print` 既存あり → 拡張のみ

**工数**: Rust ~150行 + CSS ~30行

### 6-3. EChartsインタラクティブ化

**新規チャート**: パーセンタイルバー、充足困難度ヒートマップ
**パターン**: charts.jsのChartHelpers名前空間 + htmx:afterSettleイベント

**工数**: Rust ~80行 + JS含む(6-1に統合)

### 6-4. CSVエクスポート

**エンドポイント**: GET `/api/export/{table_name}`
**レスポンス**: UTF-8 BOM付きCSV + Content-Disposition
**対象**: Phase 1-5の全v2テーブル

**工数**: Rust ~180行 + JS ~20行

### Phase 6 合計: Python ~200行 + Rust ~660行 + JS ~140行 + CSS ~30行

---

## 全Phase工数サマリー

| Phase | Python | Rust | JS/CSS | 合計 | 新テーブル |
|-------|--------|------|--------|------|-----------|
| 0: 基盤 | 0 | ~20行 | - | ~20行 | 0 |
| 1: 給与 | ~470行 | ~520行 | - | ~990行 | 3 |
| 2: テキスト | ~460行 | ~260行 | - | ~720行 | 3 |
| 3: 市場構造 | ~600行 | ~500行 | JS ~50行 | ~1,150行 | 3 |
| 4: 外部データ | ~920行 | ~300行 | - | ~1,220行 | 7 |
| 5: 予測 | ~730行 | ~320行 | - | ~1,050行 | 5 |
| 6: プロダクト | ~200行 | ~660行 | JS ~140, CSS ~30 | ~1,030行 | 0 |
| **合計** | **~3,380行** | **~2,580行** | **~220行** | **~6,180行** | **21テーブル** |

---

## 推奨実装順序

### Sprint 1: 基盤 + データ確認（1日）
1. Phase 0（3施策すべて）
2. DB充填率の確認（上記SQL実行）
3. cargo build + cargo test

### Sprint 2: 給与分析（3日）
1. `scripts/compute_v2_salary.py` 作成
2. ローカル実行 → テーブル確認
3. `analysis.rs` 追記（fetch + render）
4. cargo build → デプロイ

### Sprint 3: テキスト分析（2日）
1. `scripts/compute_v2_text.py` 作成
2. ローカル実行 → テーブル確認
3. `analysis.rs` 追記
4. cargo build → デプロイ

### Sprint 4: 市場構造（3日）
1. `scripts/compute_v2_market.py` 作成
2. 3-1, 3-3, 3-4 のPython事前計算
3. 3-2（ベンチマーク）のRust+ECharts実装
4. cargo build → デプロイ

### Sprint 5: 外部データ（3日）
1. e-Stat APIキー取得（ユーザー作業）
2. `scripts/compute_v2_external.py` 作成
3. 4-0（コードマッピング）→ 4-4（最低賃金）→ 4-1（求人倍率）→ 4-2, 4-3 の順
4. 4-5（介護施設）は手動検証が必要なため最後

### Sprint 6: 予測モデル（3日）
1. `scripts/compute_v2_prediction.py` 作成
2. 5-3（Shadow Wage、依存なし）→ 5-1（充足予測）→ 5-2（流動性、条件付き）
3. `analysis.rs` 追記
4. AUC評価 → 基準未達の場合は延期判断

### Sprint 7: プロダクト化（4日）
1. 6-1（条件診断UI）: diagnostic.rs + diagnostic.js
2. 6-4（CSVエクスポート）: export.rs
3. 6-2（レポート強化）: render.rs拡張
4. 6-3（ECharts）: 統合テスト

---

## 依存関係グラフ（再掲）

```
Phase 0 (基盤)
  ├── Phase 1 (給与) ──────────┐
  ├── Phase 2 (テキスト) ──────┤
  ├── Phase 3 (市場構造) ──────┼── Phase 5 (予測) ── Phase 6 (プロダクト)
  └── Phase 4 (外部データ) ────┘
       ↑
       4-0: 市区町村コードマッピング
```

Phase 1-4は並列実装可能。Phase 5はPhase 1-4の結果を特徴量として使用。Phase 6はPhase 1-5の表示層。

---

## Pythonスクリプト命名規則

| ファイル名 | Phase | 内容 |
|-----------|-------|------|
| `scripts/compute_v2_analysis.py` | 既存Phase 1 | vacancy_rate, resilience, transparency |
| `scripts/compute_v2_phase2.py` | 既存Phase 2 | text_temperature, cross_industry, anomaly |
| `scripts/compute_v2_salary.py` | Phase 1 | salary_structure, competitiveness, compensation |
| `scripts/compute_v2_text.py` | Phase 2 | text_quality, keyword_profile, similarity |
| `scripts/compute_v2_market.py` | Phase 3 | employer_strategy, monopsony, spatial_mismatch |
| `scripts/compute_v2_external.py` | Phase 4 | 全外部データ取得+格納 |
| `scripts/compute_v2_prediction.py` | Phase 5 | fulfillment, mobility, shadow_wage |

全スクリプトは `cd hellowork-deploy && python3 scripts/SCRIPT_NAME.py` で実行。
DB書き込みはユーザーが手動実行（Claude実行禁止）。
