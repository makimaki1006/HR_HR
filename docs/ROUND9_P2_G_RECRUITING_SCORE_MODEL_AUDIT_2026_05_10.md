# Round 9 P2-G: recruiting_scores Model F2 監査 + バグ修正

**日付**: 2026-05-10
**性質**: 監査 + 致命バグ修正 + 設計欠陥への注記追加

---

## 0. エグゼクティブ・サマリ

| 発見 | 重大度 | 修正 |
|---|---|---|
| **致命バグ**: build (0..200) と display (0..100) のスコア範囲不一致 → 11.2% (2,331行) が表示除外 | 🔴 高 | ✅ display 側を 0..200 に拡張 (Option B 採用) |
| **設計欠陥**: 4 score 成分中 3 つが職業を入力に取らない → 都市部 104 自治体で全 11 職業 score 同値 | 🟡 中 | 注記追加で対応 (中期改修) |
| KPI 閾値 80 → 160 への更新 (3 ヶ所) | 🟡 中 | ✅ 修正 + コメント説明 |

---

## 1. 監査結果サマリ

### 1-1. score 同値の実態 (DB 集計)

| 指標 | 同値自治体数 | 全自治体数 | 割合 |
|---|---|---|---|
| 同一 muni 内で 11 職業すべて `distribution_priority_score` 同値 | 104 | 1,895 | **5.49%** |
| `commute_access_score` 同値 | 1,872 | 1,872 | **100%** (occupation を入力に取らない) |
| `competition_score` 同値 | 1,895 | 1,895 | **100%** (同上) |
| `salary_living_score` 同値 | 1,895 | 1,895 | **100%** (同上) |
| `target_thickness_index` 同値 | 104 | 1,895 | 5.49% (cap 200 張り付き) |

**104 cap-saturated 自治体の内訳**: 東京都 24, 神奈川県 24, 大阪府 9, 愛知県 9, 兵庫県 8, 埼玉県 6, 千葉県 5, 京都府 5… すべて大都市圏。

### 1-2. Model F2 計算式 (build_municipality_recruiting_scores.py:228-244)

```python
weights = {"target": 50.0, "commute": 25.0, "competition": 15.0, "salary": 10.0}
positive_score = Σ component[k] * (weight[k] / Σweights)  # 重み合計 100
penalty_reduction_pct = (200.0 - competition_score) / 200.0 * 30.0  # 0..30%
raw_score = positive_score * (1 - penalty_reduction_pct/100.0)
distribution_priority_score = max(0.0, min(200.0, raw_score))   # ← clamp 0..200
```

build 側は **0..200 スケール**で投入するが、`is_priority_score_in_range` (display) が `0..=100.0` を要求していた → **不一致が致命バグ**。

### 1-3. cap saturation の構造

`scripts/build_municipality_target_thickness.py:1189-1203`:
```python
out[(pref, muni)][occ] = max(0.0, min(idx, 200.0))   # cap=200
```

都市部 (大都市圏 104 自治体) では「全国平均の 2 倍超」が常態 → 全 11 職業で thickness=200 cap 張り付き → 残り 3 score 成分も非職業差分 → 計算結果が必然的に同値化。

---

## 2. 修正内容

### 2-1. 致命バグ修正 (Step 1a)

#### `src/handlers/analysis/fetch/market_intelligence.rs:828-836`

```rust
// 旧
pub fn is_priority_score_in_range(&self) -> bool {
    match self.distribution_priority_score {
        Some(s) => (0.0..=100.0).contains(&s) && !s.is_nan(),
        None => true,
    }
}

// 新
pub fn is_priority_score_in_range(&self) -> bool {
    match self.distribution_priority_score {
        Some(s) => (0.0..=200.0).contains(&s) && !s.is_nan(),
        None => true,
    }
}
```

ドキュメンテーションコメントも更新: 「build 側 clamp(0, 200) と一致」「実データ max=169.38」を明記。

#### `src/handlers/survey/report_html/market_intelligence.rs` KPI 閾値 (3 ヶ所)

| 場所 | 旧 | 新 |
|---|---|---|
| L577 (重点配信候補 fallback) | `>= 80.0` | `>= 160.0` |
| L2165 (KPI high_priority_count) | `>= 80.0` | `>= 160.0` |
| L2406-2410 (4 区分判定) | `>= 80 / >= 65 / >= 50` | `>= 160 / >= 130 / >= 100` |

### 2-2. unit test 拡張

`tests/...market_intelligence.rs::test_priority_score_range_invariant`:

```rust
let cases = [
    (0.0, true, "下限 0"),
    (100.0, true, "中間 100 (旧上限、現在は範囲内)"),  // ← 追加
    (169.38, true, "実データ max"),                    // ← 追加
    (200.0, true, "新上限 200"),                      // ← 追加
    (50.5, true, "中間値"),
    (-0.1, false, "負値"),
    (200.001, false, "新上限超過"),                   // ← 100.001 から変更
    (201.0, false, "上限超過"),                        // ← 追加
    (f64::NAN, false, "NaN は不適合"),
];
```

実行結果:
```
test handlers::analysis::fetch::market_intelligence::tests::test_priority_score_range_invariant ... ok
```

### 2-3. Step 1b: 配信ランキング cap saturation 注記

`render_mi_distribution_ranking` (market_intelligence.rs:2370 周辺) に `<p class="mi-note">` 追加:

```
配信優先度スコアは 0〜200 のスケールで、重み合計 100 + ペナルティ調整後の値です
(build_municipality_recruiting_scores の clamp(0, 200))。
160 以上を「重点配信」、130 以上を「拡張候補」、100 以上を「維持/検証」、
それ未満を「優先度低」として分類します。

大都市圏の cap saturation について: 都市部 (特別区・政令市本市等の 104 自治体) では
母集団の集積により thickness 指数が上限 200 に張り付き、職業別 score が同値になる場合があります。
この場合、配信ランキング上位の代表職種は同点扱いで、職業選択の判断には別途産業構成
(前述 産業 × 性別セクション) と推奨アクション (4 象限図) を併用してください。
```

---

## 3. 影響範囲

### 影響あり (修正対象)

| 表示要素 | 影響 | 修正後 |
|---|---|---|
| 配信地域ランキング (page 25) | 上位 20 件中、score>100 の行が "invariant_violation" として除外されていた | display 範囲拡張で復活、注記追加 |
| KPI「配信検証候補 (>=80)」 | 旧スケールでカウント、過大計上 | 閾値 160 に更新 |
| KPI「配信優先度 平均」 | 計算自体は不変、外れ値除外閾値が変わる | 影響軽微 |

### 影響なし (priority_score 不参照)

| 表示要素 | 確認結果 |
|---|---|
| 4 象限図 (P2-B) | `csv_count` × `employees_total` × `median_salary` 参照、priority_score 不参照 |
| 推奨アクション (P2-A) | 同上 |
| 産業 × 性別 (P0-2) | priority_score 不参照 |
| 職業 × 性別 × 年齢 (P0-1) | priority_score 不参照 |

---

## 4. 残課題 (中期改修候補)

### 4-1. 設計欠陥: 3 成分が職業を入力に取らない

| 改修方針 | 影響 |
|---|---|
| commute_access_score を職業別 commute_flow_summary で再計算 | `commute_flow_summary.occupation_group_code` 列既存 (`build_municipality_recruiting_scores.py:122` で `'all'` 使用中) |
| competition_score を職業別 postings 件数で再計算 | postings に職業情報があれば可能 |
| salary_living_score は muni 定数で OK (生活コスト proxy のため) | 修正不要 |

### 4-2. cap saturation 自体への対応

| 案 | 評価 |
|---|---|
| cap を 200 から log scale 等に変える | 比較容易性が崩れる |
| **104 自治体に表示注記** | ✅ 本ラウンドで対応 |
| 都市部用に別指標 (例: 求人密度) を併記 | 中期改修案 |

---

## 5. 監査メタデータ

- 監査: 4 並列 agent (P2-G/D'/H/F) のうち P2-G 担当
- 修正ファイル: 2 件 (market_intelligence.rs × 2)
- unit test: 1 件追加 + 既存拡張
- DB 書込: ゼロ
- 既存テスト破壊: ゼロ (test_priority_score_range_invariant pass)

**Round 9 P2-G はバグ修正完了 + 注記追加完了。本番 push 後の Render PDF 検証で最終完了。**
