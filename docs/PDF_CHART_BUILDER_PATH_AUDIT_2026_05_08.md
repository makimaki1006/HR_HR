# Round 2.8-B: PDF chart builder 経路監査

**日付**: 2026-05-08
**目的**: PDF (採用コンサルレポート, `?variant=market_intelligence`) に出ている各図がどの Rust 関数 / option builder で生成されているかを特定し、Round 2.7-AC の修正が PDF 経路に反映されているかを確認する。
**スコープ**: read-only audit。コード変更なし。

---

## 1. PDF 通常導線 (採用コンサルレポート PDF)

```
HTTP /survey/report?variant=market_intelligence
  → handlers/survey/handlers.rs:706
    → report_html::render_survey_report_page_with_variant_v3_themed()
      → report_html/mod.rs (variant=MarketIntelligence)
        → salary_stats::render_section_salary_stats()         [図 3-2/3-3/3-4/3-5]
        → scatter::render_section_scatter()                    [図 5-1]
        → market_tightness::render_section_market_tightness_with_variant()
            ↳ MarketIntelligence は Full 分岐 (line 81)
              → render_radar_chart() (line 1070)               [図 MT-2 (4 軸)]
        → regional_compare::render_radar_chart() (line 728)    [図 RC-1 / 図 4-16]
```

**重要**: `?variant=market_intelligence` は内部で **Full** 分岐に流れる (market_tightness.rs:81)。
したがって PDF が呼ぶレーダーは `render_radar_chart` (4 軸版, line 1070) であり、
`render_radar_chart_public` (3 軸版, line 1661) **ではない**。

---

## 2. 図番号 → 生成関数マップ

| 図番号 | PDF 表示名 | render 関数 | option builder | ファイル:行 | Round 2.7-AC 修正反映 |
|---|---|---|---|---|---|
| 図 3-2 | 下限月給ヒスト 20,000円 bin | `render_section_salary_stats` | `build_histogram_echart_config` | salary_stats.rs:188-202 / helpers.rs:146 | ✅ 反映 |
| 図 3-3 | 下限月給ヒスト 5,000円 bin | `render_section_salary_stats` | `build_histogram_echart_config` | salary_stats.rs:208-222 / helpers.rs:146 | ✅ 反映 |
| 図 3-4 | 上限月給ヒスト 20,000円 bin | `render_section_salary_stats` | `build_histogram_echart_config` | salary_stats.rs:233-245 / helpers.rs:146 | ✅ 反映 |
| 図 3-5 | 上限月給ヒスト 5,000円 bin | `render_section_salary_stats` | `build_histogram_echart_config` | salary_stats.rs:251-265 / helpers.rs:146 | ✅ 反映 |
| 図 5-1 | 散布図 給与×件数 | `render_section_scatter` | inline JSON (builder なし) | scatter.rs:111-144 | N/A (Round 2-3 で `xAxis.show:true` 等を直接 inline 化済) |
| 図 MT-2 | レーダー 採用市場逼迫度 (4 軸) | `render_radar_chart` (Full 経路) | inline JSON (builder なし) | market_tightness.rs:1070-1148 | N/A (Round 2-3 で `radar.center: ["50%","55%"]`, `radius:65%` 設定済) |
| 図 RC-1 / 図 4-16 | 5 軸 統合レーダー (地域比較) | `render_radar_chart` | inline JSON (builder なし) | regional_compare.rs:728-783 | N/A (Round 2-3 で `radar.center` 設定済) |

---

## 3. 全 chart option builder の列挙

`fn build_*_echart_config|_chart_config` を grep した結果、**専用の builder 関数は 1 つのみ**:

| builder | 用途 | yAxis.min=0 適用 | scale: false | markLine バッジ | xAxis.show | radar.center |
|---|---|---|---|---|---|---|
| `build_histogram_echart_config` (helpers.rs:146) | 給与ヒストグラム (図 3-2/3-3/3-4/3-5) | ✅ (line 325) | ✅ (line 326) | ✅ (graphic 統合カード, line 246-310) | category 軸のため不適用 | N/A |

**他のチャートは inline JSON 構築**。それぞれの inline 設定:

| 関数 | 用途 | 設定の特徴 |
|---|---|---|
| `render_section_scatter` (scatter.rs:111) | 図 5-1 散布図 | `xAxis.show:true`, `axisLine.show:true`, `axisTick.show:true`, percentile axis range |
| `render_radar_chart` (market_tightness.rs:1106) | 図 MT-2 (Full/MI 4軸) | `radar.center:["50%","55%"]`, `radius:"65%"` |
| `render_radar_chart_public` (market_tightness.rs:1694) | 図 MT-2 (Public 3軸) | `radar.center:["50%","55%"]`, `radius:"65%"` |
| `render_radar_chart` (regional_compare.rs:754) | 図 RC-1 (5軸地域比較) | `radar.center:["50%","55%"]`, `radius:"65%"` |
| `render_salary_range_chart` (render.rs:755) | **HTML preview live (PDF外)** | yAxis.min なし、scale なし、minInterval なし — bar chart |
| `render_distribution_charts` (render.rs:731) | **HTML preview live (PDF外)** | inline 構築 |

---

## 4. Round 2.7-AC 修正と実 PDF 経路の一致

| 修正対象 | 修正内容 | PDF 経路 | 一致 |
|---|---|---|---|
| `build_histogram_echart_config` | `yAxis.min:0`, `scale:false`, `minInterval:1`, `graphic` 統合ラベルカード | 図 3-2/3-3/3-4/3-5 で使用される | ✅ 一致 (4 件すべて) |

**結論**: Round 2.7-AC で修正した builder は、PDF 通常導線の図 3-2〜3-5 でそのまま使われている。
修正がレポートに**反映されない経路差分は histogram に関しては存在しない**。

---

## 5. 仮説 B 判定 (「別 builder が PDF 経路で使われている」)

**判定: 否 (NO)**

- `build_*_echart_config` 系の chart option builder は histogram の 1 つだけ。
- 散布図・レーダーは builder を介さず inline JSON 構築。それぞれ Round 2-3 で直接書き換え済み。
- HTML preview 側 (`render.rs::render_salary_range_chart`, `render_distribution_charts`) は別経路だが PDF (variant=market_intelligence) は通らない。

ただし注意点:
1. `render.rs::render_salary_range_chart` は PDF には載らないが、ライブ画面の給与レンジ棒グラフで使われており、Round 2.7-AC 互換の `min:0` / `minInterval` を持っていない。**PDF とライブで描画ポリシーが乖離している**。
2. `render_radar_chart` (Full, line 1070) と `render_radar_chart_public` (line 1661) はほぼ同一構造の inline JSON だが、Public だけ「3 軸」「公的雇用需給指標」表記を持つ。PDF (MarketIntelligence) は **Full 側の `render_radar_chart` を呼ぶ** ため、軸ラベルは「有効求人倍率」「欠員補充率」「採用余力」「離職率」の 4 軸。

---

## 6. 次ラウンド修正候補

PDF で残っている疑い (今回 read-only audit のみで動作未確認):

| 候補 | 該当ファイル:行 | 仮説 |
|---|---|---|
| **(A) 図 5-1 散布図のラベル衝突** | scatter.rs:111-144 | builder 未経由の inline JSON。回帰線 markLine label 設定未確認 |
| **(B) 図 MT-2 (Full/MI) の 4 軸ラベル長文** | market_tightness.rs:1078-1093 | `欠員補充率\n(N%)` 等の改行ラベルがレーダー外周で切れる懸念 |
| **(C) 図 RC-1 5 軸ラベルの軸名重なり** | regional_compare.rs:746-752 | `axisName.padding:[3,5]` のみ。5 軸 (失業率水準/流動性/若年層/学歴/デジタル適合) がレーダー上で詰まる懸念 |
| **(D) HTML preview 側 `render_salary_range_chart`** | render.rs:769-773 | `yAxis.min` 未設定、PDF 側との整合性が取れていない (PDF 影響なしだが UX 一貫性問題) |
| **(E) `render_distribution_charts`** | render.rs:731 | inline JSON 構築、未監査 |

**推奨**: 次ラウンドは **(B)/(C) のレーダー軸ラベル切れ** を実 PDF で目視検証 (Round 2.7-AC で histogram は確定したため、残るブランクは radar/scatter 周辺の可能性が高い)。

---

## 7. 監査メソッド (再現可能性)

```bash
# 図番号の出現箇所
rg -n '"図 3-2"|"図 3-3"|"図 3-4"|"図 3-5"|"図 5-1"|"図 MT-2"|"図 RC-1"' src --type rs

# builder 関数の列挙
rg -n "fn build_.*_echart_config|fn build_.*_chart_config" src --type rs

# render_radar_chart 系全列挙
rg -n "fn render_radar_chart|fn render_radar_chart_public" src --type rs

# variant 分岐
rg -n "MarketIntelligence|ReportVariant::Full" src/handlers/survey/report_html/market_tightness.rs
```
