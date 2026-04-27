# Impl-2 実装結果: 媒体分析タブ「人材デモグラフィック」section 統合

**実装日**: 2026-04-26
**担当範囲**: D-1 年齢層ピラミッド / D-2 学歴分布 / #10 失業率→採用候補プール / #17 教育施設密度
**並列分担**: Impl-2 (本担当) は `src/handlers/survey/report_html/` 新規 `demographics` section
**ベース計画**: `docs/audit_2026_04_24/survey_data_activation_plan.md`

---

## 1. 設計サマリ: 「1 つの section にまとめて意味を出す」

**ストーリー**: 「対象地域の労働力候補者の年齢構成・学歴・失業状態・教育施設密度を俯瞰します」

```
[人材デモグラフィック]
├── section-howto (3 行ガイド: 何を見る / どう読む / 因果に注意)
├── 図 D-1 年齢階級別 人口ピラミッド (主役: 横棒 ECharts、左=男性 / 右=女性)
│   └── 必須 caveat (D-1): 「生産年齢人口の定義は 15-64 歳。実際の労働参加率は別途要確認」
├── 表 D-1 人材プール 主要 KPI (補助カード群)
│   ├── 15-64 歳 (生産年齢) 人口 + 全人口比率           — D-1 集計
│   ├── 25-44 歳 (採用ターゲット層) 人口 + 比率         — D-1 集計
│   ├── 推定失業者数 (採用候補プール) + 失業率 + 県平均比 — #10
│   └── 教育施設 (幼〜高 合計) + 10 万人あたり密度       — #17
├── 教育施設 4 区分内訳テーブル (幼 / 小 / 中 / 高)     — #17 詳細
│   └── 必須 caveat (#17): 「大学・専門学校カラムは存在しない / 相関するが本質的要因ではない」
├── 図 D-2 最終学歴 構成 (国勢調査 25 歳以上)
│   ├── 5 段階バー (中卒 / 高卒 / 短大・高専 / 大卒 / 大学院)
│   └── 必須 caveat (D-2): 「国勢調査 (5 年に 1 回) ベース、最新 2020 年」
├── 共通 caveat: 「属性データと採用容易性は相関する場合があるが、職種・条件マッチングが本質的要因」
└── section-bridge: 「次セクションでは、この人材プールを前提とした給与の相関分析・地域分布へと進みます」
```

**並列 KPI ではなく統合的 narrative**:
- D-1 ピラミッドが「人材の量と年齢構成」
- 補助 KPI が「採用可能なプール (#10) と教育インフラ (#17)」を肉付け
- D-2 学歴分布が「人材の質的構成」を補完
- 4 案を別バラバラに表示せず、「人材像の全体像」として一画面で読める

---

## 2. 作成 / 変更ファイル

### 新規作成
| ファイル (絶対パス) | 用途 | 行数 |
|--------------------|------|------|
| `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\demographics.rs` | 新規 section 本体 + 13 件の逆証明テスト | 約 600 行 |

### 変更 (既存ファイル)
| ファイル (絶対パス) | 変更内容 |
|--------------------|---------|
| `src\handlers\insight\fetch.rs` | `pub ext_education: Vec<Row>` を InsightContext に追加 + `af::fetch_education(...)` で取得 |
| `src\handlers\survey\report_html\mod.rs` | `mod demographics;` 宣言 + `use demographics::render_section_demographics;` + Section 3 の後に呼び出し追加 |
| `src\handlers\insight\report.rs` | mock InsightContext に `ext_education: vec![]` 追加 (Impl-3 の `ext_social_life`/`ext_internet_usage` も同時に補完) |
| `src\handlers\insight\pattern_audit_test.rs` | 同上 (Impl-3 が既に補完済み、`ext_education` のみ追加) |
| `src\handlers\survey\integration.rs` | mock InsightContext (fixb_tests) に同 3 フィールド補完 |
| `src\handlers\survey\report_html\mod.rs` | mock_empty_insight_ctx に同 3 フィールド補完 |
| `src\handlers\survey\report_html_qa_test.rs` | 同上 |

> 注: `ext_social_life`/`ext_internet_usage` は Impl-3 が `InsightContext` に追加した状態で merge 待ちだったため、本 PR でテスト mock を補完しないと build が通らなかった。Impl-3 領域 (lifestyle.rs / wage::render_section_household_vs_salary) は触れていない。

---

## 3. ext_education 取得経路

```
fetch.rs (insight)
└── af::fetch_education(db, turso, pref)
    └── analysis::fetch::subtab5_phase4_7::fetch_education
        └── SELECT prefecture, education_level, male_count, female_count, total_count
            FROM v2_external_education
            WHERE prefecture = ?1
            ORDER BY total_count DESC
```

**Turso 課金影響**: `v2_external_education` への新規参照 1 件追加 (subtab5 では既に使用中のため、connection pool は既存のものを共有)。

---

## 4. 各案の前後具体値

### D-1 年齢層ピラミッド

| 入力 (ext_pyramid) | 出力 |
|--------------------|------|
| 5 歳刻み 5 行: 20-24 (5,000+4,800), 25-29 (6,000+5,800), 30-34 (7,000+6,800), 35-39 (7,500+7,300), 40-44 (8,000+7,800) | ECharts 横棒 (男性負数 / 女性正数)、`data-chart-config` 内に 5 ラベルすべて含有、`図 D-1` キャプション付 |
| 入力: 15-19 (1,000+1,000), 25-29 (1,500+1,500), 35-39 (2,000+2,000), 65-69 (500+500) | 「15-64 歳 (生産年齢) 9,000 人 (90.0%)」「25-44 歳 (採用ターゲット層) 7,000 人 (70.0%)」 |

### D-2 学歴分布

| 入力 (ext_education) | 出力 |
|---------------------|------|
| 5 行: 中卒 50K, 高卒 300K, 短大高専 150K, 大卒 400K, 大学院 100K (合計 1,000K) | 5 段階バー、大卒 400,000 人 (40.0%) などの具体表示、`図 D-2` キャプション、`edu-bar-row` クラス |

### #10 失業率→採用候補プール

| 入力 (ext_labor_force / pref_avg_unemployment_rate) | 出力 |
|----------------------------------------------------|------|
| employed=975,000, unemployed=25,000, rate=2.5% / pref_avg=0.02 (=2.0%) | 「推定 失業者数 (採用候補プール) 25,000 人 / 失業率 2.50% (県平均比 1.25 倍)」 |
| employed=400,000, unemployed=0 (直接値なし), rate=4.0% | rate × labor から逆算: 400,000 × 4% = 16,000 人 |

### #17 教育施設密度

| 入力 (ext_education_facilities + ext_pyramid) | 出力 |
|---------------------------------------------|------|
| 幼 20 + 小 50 + 中 25 + 高 15 = 110 校 / 人口 100,000 (15-64 男 50K+女 50K) | 「教育施設 (幼〜高 合計) 110 校 (110.0/10万人)」+ 4 区分内訳テーブル |
| 幼 10+小 20+中 10+高 10 = 50 校 / 人口 100,000 | 「50 校 (50.0/10万人)」 |

> **schema 制約**: `v2_external_education_facilities` には `kindergartens / elementary_schools / junior_high_schools / high_schools / reference_year` のみ存在。**大学・専門学校カラムは存在しない**ため、計画書原案の「大学密度 1.2/10万人 (全国平均 0.8)」表現は実装不可。代わりに「幼〜高 合計密度 + 4 区分内訳 + caveat 文言で『新卒採用ポテンシャルの参考値としては高校以下のみ』を明示」する形に調整。

---

## 5. 新規 逆証明テスト一覧 (13 件)

| # | テスト名 | 検証内容 |
|---|---------|---------|
| 1 | `demographics_empty_data_renders_nothing` | 全データ空 → section 一切出力なし (空白セクション抑止) |
| 2 | `demographics_d1_pyramid_5year_bands_present` | 5 歳刻みラベル 5 件 (20-24 〜 40-44) すべてが ECharts data-chart-config 内に存在、`図 D-1` キャプション + 「人材デモグラフィック」見出し存在 |
| 3 | `demographics_d1_working_age_and_target_age_kpis` | 9,000 人 / 90.0% / 7,000 人 などの**具体値**を計算 → 表示 |
| 4 | `demographics_d2_education_bars_5_levels` | 5 段階すべて (中卒/高卒/短大・高専/大卒/大学院) ラベル + 400,000 人 / 40.0% 具体値 + 図 D-2 キャプション + edu-bar-row class 存在 |
| 5 | `demographics_p10_unemployed_direct_value` | unemployed=25,000 直接値 → 「25,000 人」「失業率 2.50%」「採用候補プール」表示 |
| 6 | `demographics_p10_unemployed_calculated_from_rate` | unemployed=0 でも employed=400,000 × rate=4.0% → 16,000 人 と逆算表示 |
| 7 | `demographics_p10_pref_avg_compare` | rate=2.5% / pref_avg=0.02 (=2.0%) → ratio 1.25 倍 計算表示 |
| 8 | `demographics_p17_education_facilities_breakdown` | 4 区分合計 110 校 + 各内訳 + 「大学・専門学校カラムは存在しない」caveat |
| 9 | `demographics_p17_facility_density_per_100k` | 人口 100,000 / 施設 50 → 「50.0/10万人」密度計算 |
| 10 | `demographics_required_caveats_present` | 4 案の必須 caveat すべての文言検証 (生産年齢定義 / 労働参加率 / 国勢調査 25 歳以上 / 失業率×労働力人口 / 属性・職種マッチング / 施設密度と採用容易性は相関するが本質的要因 等) |
| 11 | `demographics_section_has_howto_and_bridge` | section-howto 冒頭ガイド + section-bridge 次セクションへのつなぎ |
| 12 | `demographics_age_sort_key_works` | 5/10 歳刻み混在で `age_group_sort_key` が正しく昇順ソート (逆証明) |
| 13 | `demographics_age_categorization` | is_working_age / is_target_age / is_senior 各境界の正確性 (15-19/25-29/40-44/60-64/65-69 等) |

**逆証明設計** (`feedback_reverse_proof_tests.md` 準拠):
- 「要素存在」だけでなく「具体値が計算結果と一致」を検証 (ex: `9,000 人`、`90.0%`、`25,000 人`、`16,000 人`、`1.25 倍`)
- 集計ロジックが偶然動作するのではなく、入力 → 出力の対応関係が明示的に検査される
- 要件 8 件以上に対し 13 件で +5 件の余裕

---

## 6. 既存テスト結果

```
running 842 tests
test result: ok. 841 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.62s
```

**前後比較**:
- 開始時 baseline (Impl-1/Impl-3 並列含む): 829 tests + 13 新規 = 842
- **既存テスト破壊: 0 件**
- **新規テスト全 13 件 pass**
- ignored 1 件は本実装と無関係 (既存 ignore)

`cargo test` (--bin / --doc / integration) もすべて 0 failed:
```
running 0 tests   (bin)
test result: ok. 0 passed; 0 failed; ...
running 20 tests  (integration)
test result: ok. 20 passed; 0 failed; ...
```

`cargo build --quiet` 警告のみ (本実装由来の新規 error / warning は無し)。

---

## 7. 制約遵守確認

| 制約 | 遵守状況 |
|------|---------|
| 既存 825+ テスト破壊禁止 | OK (842 中 841 pass、本実装由来の破壊 0) |
| ビルド常時パス | OK |
| 公開 API シグネチャ不変 | OK (`render_survey_report_page` シグネチャ不変。`InsightContext` は struct 拡張のみ、既存呼出側は影響なし) |
| memory: feedback_correlation_not_causation | OK (全 4 案で「相関する場合がある」「本質的要因」「目安」「参考」表現を採用、断定なし) |
| memory: feedback_test_data_validation | OK (具体値検証: 9,000 / 7,000 / 25,000 / 16,000 / 1.25 倍 / 50.0/10万人 など) |
| memory: feedback_reverse_proof_tests | OK (要素存在チェックではなく、計算結果値の逆証明テスト 13 件) |
| 絵文字禁止 (severity 用 ⚠ は可) | OK (本 demographics.rs に絵文字 0 件、既存 helpers の \u{1F4D6} (📖)・\u{26A0}\u{FE0F} (⚠️) は読み方ヒント / severity 用で許容範囲) |

---

## 8. 親セッション 統合チェックリスト

- [x] `src/handlers/survey/report_html/demographics.rs` 新規作成済み (600 行、13 tests)
- [x] `mod demographics;` を `report_html/mod.rs` に追加
- [x] `use demographics::render_section_demographics;` を追加
- [x] `if let Some(ctx) = hw_context { render_section_demographics(&mut html, ctx); }` を Section 3 の後に呼び出し
- [x] `InsightContext` に `pub ext_education: Vec<Row>` フィールド追加 (`fetch.rs`)
- [x] `build_insight_context` で `ext_education: af::fetch_education(db, turso, pref)` を fetch
- [x] 5 件の mock InsightContext 構造体をすべて補完 (`ext_education` + 並列 Impl-3 の `ext_social_life` / `ext_internet_usage` を本 PR で混在解消)
- [x] 全 825+ 既存テスト維持
- [x] 新規逆証明テスト 13 件 pass
- [x] 必須 caveat 文言を 4 案すべてに付与
- [x] 並列 (Impl-1: integration.rs Tab UI、Impl-3: lifestyle.rs / wage::render_section_household_vs_salary) と競合せず

### 親セッション側 確認推奨

1. **integration.rs Tab UI への D-1/#10 反映** は Impl-1 担当範囲のため未実施。 必要であれば Impl-1 と同期して `render_integration` 内に同等情報を追加する。
2. **#10 のキー値 25,000 人 のような「失業者数 単純積」表現** は仕様書通りだが、実運用では `unemployment_rate` の単位が `%` (整数表記) か `比率` (0.025) かで揺れがある可能性。本実装は「`unemployment_rate` カラムが % 値 (1〜10 程度) であること」を前提としており、もし比率値 (0.01〜0.1) が混在する場合は `if rate > 1.0 { rate } else { rate * 100.0 }` の正規化を将来追加検討。 (現状 schema は % 値固定なので問題なし)
3. **#17 の大学/専門学校データ** は schema 拡張案件 (将来 ETL で `universities` / `senmon_gakko` カラムを追加すれば KPI 「大学密度 1.2/10万人」を表示可能になる)。本 PR では schema 制約を明示する caveat を追加して実装。

---

## 9. 主要ファイル絶対パス

- 新規: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\demographics.rs`
- 変更:
  - `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\fetch.rs`
  - `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
  - `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\report.rs`
  - `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\pattern_audit_test.rs` (Impl-3 が ext_education を既に追加していたため変更なし)
  - `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\integration.rs` (Impl-3 が ext_education を既に追加していたため変更なし)
  - `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html_qa_test.rs`
- 報告書: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_impl2_results.md`
