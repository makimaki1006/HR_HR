# Survey Section 07.5 / 07.6 + Indeed (SP) 対応 実装記録 (2026-06-24 〜 2026-07-01)

## 1. 対象範囲

媒体分析タブ (Survey) のレポート出力に以下を新設・拡張:

- **Section 07.5「年間休日 × 給与 詳細」** — 求人ボックス + Indeed (SP) の description から
  年間休日数を抽出し、給与・企業別に集計・可視化。§07.5-1 サマリー / §07.5-2 分布 /
  §07.5-3 散布図 / §07.5-4 個別求人 / §07.5-5 セグメント別給与統計 の 5 ブロック構成。
- **Section 07.6「人気度シグナル」** — Indeed (SP) 固有の「人気」「超人気」タグを集計。
  §07.6-1 サマリー / §07.6-2 月給・年間休日 中央値比較 / §07.6-3 人気タグ別 給与統計。
- **Indeed (SP) 対応** — UI ラジオ「Indeed (SP)」新設、CSV パーサ (css-u74ql7 等)、
  雇用形態フォールバック、CsvSource::IndeedSp / UserSourceHint::IndeedSp 拡張。

## 2. コミット履歴 (時系列)

### Phase 1: Section 07.5 初期実装 + リファクタリング (2026-06-24 〜 06-30)

| Commit | 内容 | agents |
|---|---|---|
| `79cf0f4` | fix: 相関ラベル順序 / 契約社員誤混入 / 「年」単独過広マッチ / 単一値表示 / dedup_key 拡張 / Q3 R-7 hybrid | Opus×1 + Sonnet×2 (3 並列) |
| `d85d065` | chore: dead code 削除 / コメント乖離修正 / SCATTER_X_MIN 等の magic number 定数化 | Sonnet |
| `dd7ef65` | refactor: `JobboxAnalysis` sub-struct 抽出 + `infer_employment_type_for_jobbox` 関数分離 + `classify_employment_for_scatter` 一元化 | Opus |
| `928f5a3` | perf: scatter+scatter_emp+Pearson 4-pass → 1-pass 統合 / HashMap::with_capacity / `median_of` 統合 | Opus |
| `ba15670` | test: Pearson / dedup / extract 境界 / salary_text 分岐 / 空データ非対称 ユニットテスト 24 件追加 | Sonnet |

### Phase 2: Indeed (SP) + Section 07.6 新設 (2026-06-30)

| Commit | 内容 | agents |
|---|---|---|
| `0243622` | feat: Indeed (SP) UI + CsvSource::IndeedSp + build_column_map + PopularityAnalysis + section_07_6_popularity.rs 新設 | Opus×2 + Sonnet×1 (3 並列) |
| `a09b662` | fix: 人気タグ (css-u74ql7) を tags_raw に明示 append (jobsearch-JobCard-tag が先に確定する接合面問題) | 手動 |

### Phase 3: セルフレビュー由来の修正 (2026-07-01)

10 reviewer (Opus×5 + Sonnet×5) 並列レビュー + Opus 統合。Overall score: **7/10** (前回コミットで
重大バグは解消済み、残るのは Section 07.6 popularity の母集団設計欠陥 3 件 + 雇用フォールバック誤分類)。

| Commit | 内容 | Findings | agents |
|---|---|---|---|
| `093bb6a` | fix: popularity 集計を IndeedSp 限定 (`matches!` ガード) + tags_raw split 厳密一致 + popular_ratio 分母是正 + 雇用フォールバック廃止 | 1,2,3,4 | Opus |
| `e179d4c` | fix: 小サンプル n threshold guard (両群 n≥5 / Pearson n<10 で「傾向判定なし」/ n<10 で回帰直線非描画) | 5,6 | Sonnet |
| `18e9ad9` | fix-ui: `push_kpi_card_simple` を common.rs に統一 (.kpi クラス) + 月給差を万円表示 + @media print break-inside avoid | 7,8,9 | Sonnet |
| `cfae2b3` | refactor: `classify_employment_for_scatter` の冗長 if 統合 + `MIN_MONTHLY_SALARY` 定数化 + jobcard tag 列インデックスをヘッダー解析時キャッシュ + //! コメント修正 | 12,13 | Sonnet |
| `449c75e` | test: popularity 集計 6 件 + IndeedSp parse 統合 3 件 = +12 件追加 | 10,11 | Sonnet |

### Phase 4: カテゴリ別/人気タグ別 給与統計追加 (2026-07-01)

新要件:
1. 年間休日カテゴリごとに給与下限・上限の平均/中央値/最頻値を可視化
2. 人気/超人気タグごとに給与下限・上限の平均/中央値/最頻値を可視化
3. Indeed (SP) CSV 列変更 (css-1hwmqh1 消失) 対応 → 雇用フォールバック復活

| Commit | 内容 | agents |
|---|---|---|
| `7c9efcb` | feat: `SalaryStats` struct + `compute_salary_stats` + `JobboxAnalysis.salary_stats_by_holiday_category` + PopularityAnalysis の 3 SalaryStats + 表 7.5-A 給与中央値列 + §07.5-5 セグメント別給与 + §07.6-3 人気タグ別給与 + 雇用フォールバック復活 | Opus×2 + Sonnet×2 (4 並列) + Opus verify |

### Phase 5: 要件誤解訂正 + 本番動作 debug (2026-07-01)

| Commit | 内容 |
|---|---|
| `f1f3b40` | (誤解) §07.5-1 補助 全体統計テーブル (年間休日/月給下限/月給上限 の全体 平均/中央値/最頻値) 追加 |
| `10c3e35` | revert: 上記削除 (要件はカテゴリ別の統計であり §07.5-5 が本来の要件) |
| `33fb0a6` | **fix: jobbox_records に IndeedSp も含める** (§07.5-4 個別求人 と §07.5-5 セグメント別給与が本番で消えていた根本原因) |

## 3. 実装仕様

### 3.1 CsvSource / UserSourceHint 拡張

```rust
pub enum CsvSource {
    Indeed,
    IndeedSp,  // 2026-06-30 追加
    JobBox,
    Unknown,
}
```

`detect_csv_source` は `css-u74ql7` (人気タグ列、Indeed SP 固有) または
`css-bxyec3 + css-1vlebyu` の組み合わせで IndeedSp を判定 (Indeed PC より優先)。

`UserSourceHint::from_str("indeed_sp")` → `IndeedSp`。UI ラジオ 4 種
(Indeed / Indeed SP / 求人ボックス / その他)。

### 3.2 Indeed (SP) CSV ヘッダマッピング

| CSS class | フィールド |
|---|---|
| `css-bxyec3 href` | url |
| `css-bxyec3` | job_title |
| `css-14qk2ra` | company_name |
| `css-18rxko3` | location |
| `css-18rxko3 (2)` | salary |
| `css-1hwmqh1` | employment_type (2026-07-01 以降の CSV では消失、フォールバックで対応) |
| `css-1vlebyu` | description (年間休日抽出元) |
| `css-u74ql7` | **人気/超人気タグ** (tags_raw に明示 append) |
| `jobsearch-JobCard-tag (n)` | tags (jobbox と同じ扱い、副次) |

### 3.3 雇用形態フォールバック

- Commit `093bb6a` (Phase 3) で「Indeed(SP) は css-1hwmqh1 で取得できる前提」として一旦廃止
- Commit `7c9efcb` (Phase 4) で復活: ユーザー方針「月給→正社員、時給→パート・アルバイト、
  契約社員は正社員雇用でも最初 6 ヶ月は契約社員なので分けるのはナンセンス」
- 適用: `matches!(source, CsvSource::JobBox | CsvSource::IndeedSp)` + employment_type 空欄
  → `infer_employment_type_for_jobbox(salary_type)` (Monthly/Annual→正社員、Hourly→パート・アルバイト)

### 3.4 jobbox_records の集計対象 (Commit `33fb0a6` の根本修正)

`§07.5-4 個別求人` と `§07.5-5 セグメント別給与` は `jobbox_records` を集計対象とする。
Commit `33fb0a6` 以前は `matches!(r.source, CsvSource::JobBox)` のみで、Indeed (SP) の
225 レコードは 1 件も入らず全カテゴリ n=0 で skip されていた。

修正後: `matches!(r.source, CsvSource::JobBox | CsvSource::IndeedSp)`。
Indeed (PC) は description が短いため対象外のまま (年間休日抽出が期待できない)。

### 3.5 集計フィールド一覧

`SurveyAggregation`:
- `.jobbox: JobboxAnalysis` (Commit `dd7ef65` で sub-struct 抽出)
  - `annual_holidays_values / category_distribution` (全 source 対象)
  - `jobbox_records` (JobBox + IndeedSp のみ、Monthly + 給与記載 + 会社名記載でフィルタ)
  - `salary_vs_holidays_scatter / scatter_emp` (Commit `928f5a3` で 1-pass 化)
  - `salary_holidays_correlation / regression` (Pearson n<10 で None)
  - `holiday_pct_ge_120 / ge_125 / holiday_stddev / holiday_q3` (R-7 hybrid, n≥20 で線形補間)
  - `salary_stats_by_holiday_category: Vec<(String, SalaryStats)>` (Commit `7c9efcb`)
- `.popularity: PopularityAnalysis` (Commit `0243622`, IndeedSp 限定に修正 `093bb6a`)
  - `popular_count / super_popular_count / none_count`
  - `popular_ratio` (分母は IndeedSp 由来件数のみ)
  - `popular_salary_median / non_popular_salary_median / *_holidays_median` (両群 n≥5 で表示)
  - `popular_salary_stats / super_popular_salary_stats / non_popular_salary_stats: SalaryStats`

`SalaryStats` (Commit `7c9efcb` で新設):
```rust
pub struct SalaryStats {
    pub n: usize,
    pub min_mean: Option<i64>, pub min_median: Option<i64>, pub min_mode: Option<i64>,
    pub max_mean: Option<i64>, pub max_median: Option<i64>, pub max_mode: Option<i64>,
}
```
最頻値は 5 万円刻みビン。タイの場合は最小ビンを返す。

### 3.6 描画構造 (最終)

**Section 07.5** (`render_navy_section_jobbox_detail`):
1. §07.5-1 サマリー — KPI 5 枚 (平均年間休日 / Q3 / 標準偏差 / 120日以上比率 / 125日以上比率)
2. §07.5-2 分布 — SVG 横棒グラフ + カテゴリ別 月給下限/上限 中央値テーブル (万円)
3. §07.5-3 散布図 — 給与×年間休日、雇用形態色分け、Pearson r + 回帰直線 (n≥10 で描画、n<30 で「参考値」注記)
4. §07.5-4 個別求人 — 月給制で会社名+給与記載ある求人の一覧テーブル (最大 100 件、年間休日降順)
5. §07.5-5 セグメント別給与 — 6 カテゴリ × (月給下限, 月給上限) × (平均, 中央値, 最頻値) の 8 列テーブル

**Section 07.6** (`render_navy_section_popularity`):
1. §07.6-1 サマリー — KPI 5 枚 (人気/超人気件数 / 人気タグ比率 / 月給差 万円 / 年間休日差)
2. §07.6-2 中央値比較 — 人気タグあり vs なしの月給・年間休日中央値
3. §07.6-3 人気タグ別給与 — 超人気/人気/タグなし × (月給下限, 月給上限) × (平均, 中央値, 最頻値)

skip 条件:
- Section 07.5: `annual_holidays_values` と `jobbox_records` が両方空
- §07.5-5: 全カテゴリで n=0
- Section 07.6: `popular_count == 0 && super_popular_count == 0`
- §07.6-3: 3 グループ全て n=0

## 4. multi-agent workflow の運用実績

| Phase | 並列度 | model 内訳 | 所要時間 | 成功率 |
|---|---|---|---|---|
| Phase 1 Commit 79cf0f4 | 3 並列 | Opus×1 (aggregator) + Sonnet×2 | ~6分 | 3/3 |
| Phase 2 Commit 0243622 | 3 並列 | Opus×2 + Sonnet×1 | ~10分 | 3/3 |
| Phase 3 セルフレビュー | 10 並列 | Opus×5 + Sonnet×5 | ~9分 | 9/10 (Opus overloaded 1 件) |
| Phase 3 Commit 1-5 | 順次 5 phase | Opus×1 + Sonnet×4 | ~29分 | 5/5 |
| Phase 4 Commit 7c9efcb | 4 並列 | Opus×2 + Sonnet×2 + Opus verify | ~12分 | 3/4 実装完了 (Agent C connection closed だが実装完了後に切断、verify agent が commit まで実施) |

Opus/Sonnet の使い分け方針:
- **Opus**: 数式の正確性 (correlation, percentile)、母集団設計、CSV パース分岐、アーキ変更 (JobboxAnalysis sub-struct 抽出等)
- **Sonnet**: UI テンプレート、テスト追加、命名・コメント整形、既定パターンの適用

## 5. テスト実績

| 累計 test 数 | 変化 |
|---|---|
| セッション開始時 | 1130 |
| Commit `ba15670` 後 | 1154 (+24) |
| Commit `0243622` 後 | 1165 (+11) |
| Commit `449c75e` 後 | 1177 (+12) |
| Commit `7c9efcb` 後 | **1191 (+14)** |

全期間で 0 failed。Commit `928f5a3` (1-pass Pearson) は Pearson 恒等式
`var_x = sum_xx - sum_x*mean_x`, `cov = sum_xy - sum_x*mean_y` に基づき計算結果不変。

## 6. E2E 本番動作確認

`indeed-2026-07-01 (1).csv` (225 行, 人気 24 件 + 超人気 6 件 + タグなし 195 件、年間休日抽出 75 件):

- §07.5-1〜§07.5-3 描画 OK
- §07.5-4 個別求人 描画 OK (Commit `33fb0a6` 後)
- §07.5-5 セグメント別給与 描画 OK (Commit `33fb0a6` 後)
- §07.6-1〜§07.6-3 描画 OK

E2E スクリプト: `e2e_section_075_075_5.py` (scratchpad)。

実データ例 (§07.6-3):
| グループ | n | 下限 平均 | 下限 中央値 | 下限 最頻値 | 上限 平均 | 上限 中央値 | 上限 最頻値 |
|---|---|---|---|---|---|---|---|
| 超人気 | 6 | 22.0 万円 | 20.3 万円 | 20.0 万円 | 28.2 万円 | 29.7 万円 | 25.0 万円 |
| 人気 | 23 | 25.2 万円 | 24.6 万円 | 20.0 万円 | 32.3 万円 | 30.0 万円 | 20.0 万円 |
| タグなし | 153 | 22.1 万円 | 21.9 万円 | 20.0 万円 | — | — | — |

## 7. 意思決定ログ

| 日 | 決定 | 理由 |
|---|---|---|
| 06-24 | Section 07.5 を GAS プロジェクトから移植 | GAS 版の集計 (年間休日分布、給与×年間休日散布図) を活用しつつ、GAS にない「企業名×年間休日×給与」個別求人一覧を独自追加 |
| 06-25 | dedup_key に salary_raw + employment_type 追加 | 同一施設の経験別/雇用形態別求人を別レコードとして残す (V2 dedup ルール準拠) |
| 06-26 | §07.5-4 の表示対象を「月給制 + 給与記載」のみに限定 | 年俸を月給換算 (÷12) すると大企業数値が違和感、mini bar スケール歪み |
| 06-26 | §07.5-1 KPI から「抽出件数 N件 全 M件中 (X%)」を削除 | 信頼性低下の印象を回避 (ユーザー指示) |
| 06-30 | Indeed (SP) を独立 CsvSource として分割 | Indeed (PC) と CSS クラス構造が全く別、また人気タグは SP 固有機能 |
| 07-01 | 雇用フォールバック復活 | Indeed (SP) CSV 列変更 (css-1hwmqh1 消失) 対応。「契約社員は正社員雇用でも最初 6 ヶ月は契約社員なので分けるのはナンセンス」 |
| 07-01 | jobbox_records の集計対象に IndeedSp を追加 | §07.5-4 / §07.5-5 の本番非表示バグ根本修正 |

## 8. 残課題 (defer)

セルフレビュー Phase 3 で defer と判定:

- **Commit 6: PopularSignal enum 化 + `aggregate_records_core` 分割** — risk=high、Phase 5 のテスト基盤を活用してから
- SectionData コンテナ化 — Section 07.x が 4 個超になるまで over-engineering
- DefaultHasher 衝突対策 — 理論上の懸念のみ
- inline style への動的値埋め込み — 現状全て静的リテラル
- `decode_csv_bytes` の Cow 化 — Render starter 512MB プランでは実害なし
- `render_distribution_block` / `render_scatter_svg_emp` の 90/170 行分割 — Section 07.7 で SVG が増える段階で再評価
- §07.5-4 個別求人テーブル 印刷時 100→30 件絞り込み — UI 仕様変更でユーザー確認必要

## 9. 参考ファイル

- `src/handlers/survey/aggregator.rs` (JobboxAnalysis, PopularityAnalysis, SalaryStats, compute_salary_stats)
- `src/handlers/survey/upload.rs` (CsvSource, UserSourceHint, detect_csv_source, build_column_map, extract_annual_holidays, infer_employment_type_for_jobbox)
- `src/handlers/survey/render.rs` (ソース媒体ラジオ 4 枚)
- `src/handlers/survey/handlers.rs` (source_type valid リスト)
- `src/handlers/survey/report_html/navy_report/section_07_5_jobbox_detail.rs`
- `src/handlers/survey/report_html/navy_report/section_07_6_popularity.rs`
- `src/handlers/survey/report_html/navy_report/common.rs` (push_kpi_card_simple)
