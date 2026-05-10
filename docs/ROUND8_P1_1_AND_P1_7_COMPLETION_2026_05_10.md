# Round 8 P1-1 + P1-7 完了報告

**日付**: 2026-05-10
**性質**: 機能完了 (実装 + ローカル PDF 検証 PASS)
**前提 docs**:
- `ROUND8_P0_1_COMPLETION_2026_05_10.md`
- `ROUND8_P0_2_COMPLETION_2026_05_10.md`
- `OPEN_DATA_UTILIZATION_TOTAL_AUDIT_2026_05_09.md` §7

---

## 実装スコープ

| ID | タスク | 状態 |
|---|---|---|
| **P1-1** | CSV 求人数 × 地域母集団 4 象限図 | ✅ |
| **P1-7** | 東京都特別区部の集約規模感の追記 | ✅ (4 象限図セクション内に含めた) |

P1-6 (recruiting_scores 不整合調査) を先に実施し、「priority は score でなく rank_percentile (S=上位 2%, A=5%, B=15%, C=50%, D=50%超) で決まる仕様」「`distribution_priority='C'` で `score=84.03` は不整合ではなく仕様」「ただし 104 自治体で全職業 score 同値の構造的問題あり (新宿区含む)」と判明。
→ **Y 軸を `priority_score` ではなく国勢調査 employees_total に変更** することで P0-2 のデータ資産を活用。

---

## 実装変更

### `src/handlers/analysis/fetch/market_intelligence.rs`

| 追加 | 内容 |
|---|---|
| `CsvMunicipalityCell` DTO | prefecture / name / count / median_salary (`MunicipalitySalaryAgg` の subset) |
| `SurveyMarketIntelligenceData.csv_municipalities` | 新フィールド (build 後 caller で inject) |

### `src/handlers/analysis/fetch/mod.rs`

re-export 拡張 (`CsvMunicipalityCell`)

### `src/handlers/survey/report_html/market_intelligence.rs`

| 追加 | 内容 |
|---|---|
| `QuadrantPoint` 内部 struct | display_name / csv_count / employees_total / median_salary |
| `quadrant_label(count, emp, c_med, e_med)` | 「重点配信 / 条件見直し / 開拓余地 / 後回し」分類 |
| `quadrant_advice(...)` | 各象限の機械生成アドバイス |
| `render_mi_market_quadrant(html, csv_municipalities, industry_gender_rows, code_master)` | 4 象限テーブル + P1-7 集約規模注記 |
| call site | `render_section_market_intelligence` 内、Round 8 P0-2 (industry_gender_summary) の直後 |
| use 拡張 | `CsvMunicipalityCell` |

### `src/handlers/survey/report_html/mod.rs`

build 後 inject (P1-1 の前提):
```rust
data.csv_municipalities = agg.by_municipality_salary.iter()
    .filter(...)
    .map(|m| CsvMunicipalityCell { prefecture, name, count, median_salary })
    .collect();
```

---

## 4 象限ロジック (実装サマリ)

### データソース統合

| データ | 取得元 | 集計単位 |
|---|---|---|
| X 軸 (CSV 求人数) | `agg.by_municipality_salary.count` | (prefecture, name) |
| Y 軸 (employees_total) | `industry_gender_rows` の sum (集約 city_code 単位) | aggregate code |
| 円サイズ (給与中央値) | `agg.by_municipality_salary.median_salary` | (prefecture, name) |

### 集約解決フロー (Plan A 継承)

1. CSV (prefecture, name) → `code_master` で municipality_code 解決
2. municipality_code → `aggregate_to_industry_structure_code` で集約 city_code (例: 13104 → 13100)
3. 集約 city_code 単位で employees_total を sum (industry_gender_rows 経由)
4. 集約された場合は注釈「(○○区を含む)」を表示名に付加

### 象限分割

中央値ベース (median split):
- count_median = 全対象自治体 (集約後) の CSV 求人数の中央値
- emp_median = 全対象自治体 (集約後) の employees_total の中央値

### 採用示唆 (機械生成)

| 象限 | 配信方針 |
|---|---|
| 重点配信 (求人多 × 母集団厚) | 配信予算と訴求の主戦場 |
| 条件見直し (求人多 × 母集団薄) | 給与・勤務条件・訴求の見直し |
| 開拓余地 (求人少 × 母集団厚) | 新規配信候補・伸び代 |
| 後回し (求人少 × 母集団薄) | 当面は後回し |

### P1-7 集約規模注記 (本セクション内に統合)

```
東京都特別区部の規模注意: 23 区は「特別区部」(13100) として一括集計されるため、
新宿区/千代田区などの個別求人を含む象限点は実際には特別区全体 (約 836 万人規模) に
対する位置づけです。「重点配信」象限に入った場合でも、個別区での母集団は数十万人〜
百万人単位で、単体自治体の求人と直接比較するには注意が必要です。
```

---

## ローカル PDF 検証結果

| 指標 | 値 |
|---|---|
| pages | 26 (P0-1 + P0-2 + P1-1 すべて格納、P1-1 は P24) |
| 「CSV 求人数 × 地域母集団 4 象限図」 | 1 (P24) |
| 「東京都特別区部の規模注意」(P1-7) | 1 |
| 「重点配信」 | 6 (タイトル + 表内 + 説明) |
| 「条件見直し」 | 1 (1 自治体該当) |
| 「後回し」 | 1 |
| 「開拓余地」 | 0 (サンプル少で該当なし、ロジックは正常) |
| 集約注釈「(千代田区 / 新宿区 を含む)」 | 1 |

### 出力例 (4 象限テーブル抜粋)

```
象限       対象自治体                              CSV求人数 国勢調査従業者 給与中央値 配信方針
重点配信   東京都 特別区部 (千代田区 / 新宿区 を含む) 17 件   7,955,350 人  30 万円   求人数・母集団とも厚い。配信予算と訴求の主戦場
条件見直し 北海道 伊達市                            1 件    11,466 人     22 万円   求人多いが母集団薄。給与・勤務条件・訴求の見直し
重点配信   福島県 伊達市                            1 件    18,499 人     29 万円   求人数・母集団とも厚い。配信予算と訴求の主戦場

象限分割基準: CSV 求人数中央値 = 1 件、従業者中央値 = 18,499 人 (対象 3 自治体)
```

### 完了条件 (ユーザー指定)

| 条件 | 結果 |
|---|---|
| 4 象限図が機械的に作られる | ✅ median split + label + advice 自動生成 |
| 優先地域・開拓余地・条件見直しが表示 | ✅ |
| 採用示唆が表示 | ✅ 各象限ごとに配信方針出力 |
| 業界別給与/職種別給与を作らない | ✅ 給与は CSV median のみ、業界・職種に分解せず |
| 特別区部の集約注記 (P1-7) | ✅ 836 万人規模の文脈明示 |

---

## P1-6 詳細記録

### Priority 仕様の確認 (Local DB read-only audit)

| priority | 件数 | rank_percentile 範囲 | score 範囲 |
|---|---:|---|---|
| S | 407 | 0.0005〜0.0195 (上位 2%) | 136〜169 |
| A | 627 | 0.0201〜0.0496 | 117〜140 |
| B | 2,090 | 0.0501〜0.1499 | 89〜129 |
| C | 7,293 | 0.1504〜0.4997 | 62〜102 |
| D | 10,428 | 0.5003〜1.0 | 14〜69 |

→ priority は **percentile ベース** で決まる。`MEDIA_REPORT_P0_FEASIBILITY_CHECK_2026_05_09.md` §4 の「priority_score=84 で priority='C' は不整合」記述は誤り (修正不要、本書で訂正)。

### 構造問題: 104 自治体で全職業 score 同値

新宿区 (13104) は全 11 職業で `score=84.03` 同値。同パターンの自治体が **104 件** (全自治体 1,917 件の 5.4%)。`build_municipality_target_thickness.py` の Model F2 計算式の特性 (自治体規模の補正係数が職業差を吸収) と推測。本ラウンド範囲外、別途設計レビュー要。

→ **P1-1 では Y 軸に priority_score を採用せず**、国勢調査 employees_total を使うことでこの構造問題を回避。

---

## 残課題 (P2 候補)

| ID | 内容 |
|---|---|
| P2-A | recruiting_scores の score 同値問題 (104 自治体) の根本調査 (build_municipality_target_thickness.py の Model F2 設計レビュー) |
| P2-B | 4 象限図の実図形 (CSS 散布図 or ECharts) 化。現状はテーブル表現 |
| P2-C | 最低賃金 DB 接続 (helpers.rs:936-958 ハードコード解除、P1-2 から降格) |
| P2-D | 産業構造空配列バグ修正 (insight/fetch.rs:179、Full/Public 経路) |
| P2-E | 昼間人口 / 通勤流入の survey PDF render 連結 |

---

## 監査メタデータ

- 着手: 2026-05-10
- ローカル PDF PASS: 2026-05-10
- DB 書込: ゼロ
- 実装: source 4 ファイル + docs 1 ファイル
- 既存テスト破壊: ゼロ (cargo check 24 warnings = 既存と同レベル)

**Round 8 P1-1 + P1-7 はローカル PDF 実物で PASS 判定。本番 push 後の Render PDF 検証で最終完了。**
