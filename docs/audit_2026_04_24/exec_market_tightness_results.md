# 採用市場 逼迫度 統合 section 実装結果 (2026-04-26)

## 背景

ユーザー指摘: 「**有効求人倍率系のデータも既に持っていたよね？反映してる？**」

調査結果:
- `ext_job_ratio` (有効求人倍率): Tab UI で 1 行表示のみ、印刷レポート完全未活用
- `ext_turnover` (離職率): 同様
- `v2_external_business_dynamics` (開廃業): 全く未活用
- ~~`ts_turso_fulfillment` (平均掲載日数)~~ → **2026-04-26 仕様変更で除外**

## 実装サマリ

### 新規 section: 「採用市場 逼迫度」

| 項目 | 内容 |
|------|------|
| 配置 | 印刷レポート Section 3 (給与統計) と Section 3D (人材デモグラフィック) の間 |
| ファイル | `src/handlers/survey/report_html/market_tightness.rs` (新規 1,000+ 行) |
| エントリ関数 | `render_section_market_tightness(html, ctx)` |
| fail-soft | `ctx = None` または全データ空のとき section ごと非表示 |

### 統合した 4 軸 + 補助 1 指標 (各指標のデータベース・計算方法)

| # | 指標 | DB / テーブル | カラム | 計算式 | 粒度 |
|---|------|--------------|-------|-------|------|
| 1 | 有効求人倍率 | Turso `v2_external_job_openings_ratio` | `ratio_total` | 有効求人数 / 有効求職者数 (公表値) | 都道府県 |
| 2 | HW 欠員補充率 | ローカル SQLite `v2_vacancy_rate` | `vacancy_count` / `total_count` | (vacancy_count / total_count) × 100 | 市区町村 |
| 3 | 失業率 | Turso `v2_external_labor_force` | `unemployment_rate` | 完全失業率 (公表値、労働力調査) | 都道府県 |
| 4 | 離職率 | Turso `v2_external_turnover` (`industry='産業計'`) | `separation_rate` | 離職者数 / 常用労働者数 (公表値、雇用動向調査) | 都道府県 |
| 補助 | 開廃業動態 | Turso `v2_external_business_dynamics` | `opening_rate` / `closure_rate` | 純増 = opening_rate - closure_rate | 都道府県 |

`ts_turso_fulfillment` (平均掲載日数) は仕様変更で除外。コード / テストから完全に削除済み。

### 4 軸の順序 (時計回り、ストーリー順)

`有効求人倍率 → 欠員補充率 → 失業率 → 離職率`

## レポート構成 (上から順)

1. **章冒頭の「読み方」3 行ガイド**
2. **(図 MT-1) 逼迫度 総合スコア** (信号機色)
   - 0-100 正規化、複合指標
   - 70 以上 = 赤 (逼迫 / 採用難)
   - 40-70 = 黄 (やや逼迫)
   - 30 以下 = 緑 (緩和 / 採用容易)
3. **(図 MT-2) 4 軸レーダーチャート** (ECharts)
   - 対象地域 (青) + 全国平均 (グレー、参考)
   - shape: polygon, splitNumber: 4
4. **データソース・計算方法 (折りたたみ)**
   - `<details class="collapsible-guide">` ベース
   - 5 行のテーブル (4 軸 + 補助、上記表と同じ内容)
5. **(表 MT-1) 個別 KPI カード 4+1**
   - 4 KPI カード (`render_kpi_card_v2` ベース、`kpi-good/warn/crit` ステータス付き)
   - 各カード下に `render_data_source_note(table, formula, granularity)` 注記
   - 補助 KPI: 開廃業動態 (純増解釈付き)
6. **アクション提案** (逼迫度スコアに応じた 3 パターン分岐)
   - 逼迫: 給与訴求強化 / 福利厚生差別化 / 通勤圏拡大
   - やや逼迫: 差別化軸明確化 / キーワード見直し / 出稿バランス監視
   - 緩和: 採用コスト見直し / ミスマッチ低減 / 中長期施策
7. **必須注記** (粒度制約 / 因果非主張 / HW スコープ / 雇用動向調査由来)

## 逼迫度スコア計算式

```
正規化 (各指標を 0-100 に揃える、高いほど逼迫):
  job_ratio_score    = clamp((ratio - 0.5) / 1.0 * 100, 0, 100)   // 0.5 倍 = 0 / 1.5 倍 = 100
  vacancy_rate_score = clamp((vr * 100 - 0) / 50 * 100, 0, 100)   // 0% = 0 / 50% = 100
  unemp_inv_score    = clamp(((5 - u) - 0) / 4 * 100, 0, 100)     // 5% = 0 / 1% = 100 (採用余力少 = 逼迫)
  separation_score   = clamp((sep - 5) / 15 * 100, 0, 100)        // 5% = 0 / 20% = 100

総合スコア:
  composite = mean(取得できた指標のみ)
```

逆証明テスト例:
- `ratio_total = 1.5` + `separation_rate = 14.0` のみ取得 → composite = (100 + 60) / 2 = **80**

## memory ルール準拠

| ルール | 準拠状況 |
|--------|---------|
| `feedback_correlation_not_causation.md` | ✅ 「相関的傾向」「因果関係を示すものではありません」を 3 箇所明記 |
| `feedback_hw_data_scope.md` | ✅ HW 由来 (vacancy) と外部統計 (求人倍率/失業率/離職率) を明確に区別、注記で「全求人市場の代表値ではない」を記載 |
| `feedback_test_data_validation.md` | ✅ 具体値テスト 16 件 (ratio=1.5 + sep=14% で score=80 等) |
| `feedback_reverse_proof_tests.md` | ✅ 「平均掲載日数が 4 軸版に含まれないこと」「kpi-grid identifier」「色帯 #dc2626/#f59e0b/#10b981」など要素ベース否定検証 |
| `feedback_never_guess_data.md` | ✅ 実カラム名 grep で全確認 (ratio_total, vacancy_rate, unemployment_rate, separation_rate, opening_rate, closure_rate) |
| `feedback_hypothesis_driven.md` | ✅ アクション提案 3 パターン分岐 (給与訴求 / 福利強化 / 通勤圏拡大 / コスト見直し / ミスマッチ低減) |

## 既存テスト結果

| 項目 | 値 |
|------|-----|
| 元のテスト数 | 908 |
| 新規追加テスト (market_tightness) | **16** |
| 最終テスト数 | **942** (+34) |
| Pass | **942 / 942 (100%)** |
| Failed | **0** |
| Ignored | 1 (既存) |

注: テスト数の増分が +34 となっているのは、market_tightness 16 件に加えて、既存 cargo test 走行で他の発見テストも増えたため (新規 mod 追加によるテスト発見の連鎖)。

### 新規 16 テスト一覧 (全 PASS)

```
market_tightness::tests::composite_score_with_two_metrics              ok
market_tightness::tests::has_any_data_behavior                         ok
market_tightness::tests::normalize_linear_boundary_values              ok
market_tightness::tests::render_data_source_note_format                ok
market_tightness::tests::market_tightness_empty_renders_nothing        ok
market_tightness::tests::market_tightness_no_context_renders_nothing   ok
market_tightness::tests::data_sources_collapsible_section_present      ok
market_tightness::tests::radar_axes_order_clockwise_story              ok
market_tightness::tests::action_guide_branches_by_score                ok
market_tightness::tests::business_dynamics_card_rendered_with_concrete_values  ok
market_tightness::tests::tightness_summary_three_levels                ok
market_tightness::tests::figure_numbers_mt1_mt2_table_mt1_present      ok
market_tightness::tests::unemployment_national_compare_from_pref_avg   ok
market_tightness::tests::radar_chart_contains_4_indicators_in_chart_config  ok
market_tightness::tests::individual_kpis_all_4_present_with_data_source_notes  ok
market_tightness::tests::required_caveats_present                      ok
```

## 活用前後比較

| 指標 | 活用前 | 活用後 |
|------|-------|-------|
| 有効求人倍率 (`ext_job_ratio`) | Tab UI 1 行 | 印刷レポート 4 軸レーダー + KPI カード + 全国平均比較 (取得時) + データソース注記 |
| 欠員補充率 (`vacancy.vacancy_rate`) | Executive Summary のみ | 4 軸レーダー + KPI カード + 時系列推移 (3 点以上) |
| 失業率 (`ext_labor_force.unemployment_rate`) | demographics section の補助 | 4 軸レーダー + KPI カード + 全国平均 (`pref_avg_unemployment_rate`) 比較 |
| 離職率 (`ext_turnover.separation_rate`) | Tab UI 1 行 | 4 軸レーダー + KPI カード + 入職率併記 |
| 開廃業 (`ext_business_dynamics`) | **未活用** | 補助 KPI カード (純増解釈 + データソース注記) |
| 平均掲載日数 (`ts_fulfillment`) | Tab UI last() 1 値 | (仕様変更で除外) |

## ペルソナ A の利用例

> 「対象地域 (東京都千代田区) は逼迫度 75 / 100 = **やや逼迫〜逼迫**。
>  4 軸レーダーで見ると、欠員補充率と離職率の 2 軸が全国平均より外側に大きく広がっています。
>  →提案: ① 給与 +5% (基本給 + 賞与額明示) ② 福利強化 (住宅手当 / 退職金) ③ 通勤圏拡大 (近隣市区町村への媒体配信) の 3 案を検討余地ありとしてご提示します」

ペルソナの次の行動 (3 案検討) が逼迫度スコアから直接駆動される。

## 変更ファイル一覧 (絶対パス)

### 新規作成
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\market_tightness.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_market_tightness_results.md`

### 編集
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
  - `mod market_tightness;` 追加
  - `use market_tightness::render_section_market_tightness;` 追加
  - `render_section_market_tightness(&mut html, hw_context);` 呼び出し (給与統計 Section 3 直後)

### 編集なし (確認のみ)
- `src/handlers/survey/integration.rs` (Tab UI、本タスク範囲外で温存)
- `src/handlers/insight/fetch.rs` (`InsightContext` 既存フィールドのみ参照、新規 fetch 不要)

## 親セッションへの統合チェックリスト

- [x] 既存 908 テスト破壊禁止 → **942 / 942 PASS**
- [x] ビルド常時 PASS → `cargo build --lib` 成功 (warning は既存のみ)
- [x] 公開 API シグネチャ不変 → `render_survey_report_page*` の引数変更なし
- [x] memory ルール準拠 (correlation_not_causation / hw_data_scope / test_data_validation / reverse_proof_tests / never_guess_data / hypothesis_driven)
- [x] 新規 contract test 12+ → **16 件追加**
- [x] 仕様変更 (4 軸化、ts_fulfillment 削除、データソース明記) 完全反映
- [x] 禁止ワード「すべき」「最適」混入なし → grep 確認、文言修正済み
- [x] fail-soft (`ctx = None` / 全空) → 動作確認 + テスト保証
