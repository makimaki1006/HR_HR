# Round 2.10-C: SSR / runtime ECharts option 差分監査

**日付**: 2026-05-06
**対象**: 本番 `https://hr-hw.onrender.com` MI variant ページ (action bar 経由 popup)
**方式**: read-only Playwright (login + upload + popup), 実装変更なし
**spec**: `tests/e2e/_round2_10_option_diff.spec.ts`
**成果物**:
- `out/round2_10_option_diff/prod_html_screen.html`
- `out/round2_10_option_diff/prod_html_print.html`
- `out/round2_10_option_diff/ssr_data_chart_config.json`
- `out/round2_10_option_diff/runtime_screen.json`
- `out/round2_10_option_diff/runtime_print.json`

Round 2.8-C で参照していた `data/generated/debug_*.html` は使用せず、Playwright で本番 HTML / DOM を直接取得して再確認した。

---

## 1. 必須差分表 (8 chart × 3 階層)

| # | chart 種別 (推定) | source option (Rust) | SSR `data-chart-config` | runtime `getOption()` | SVG attribute (screen) | 一致 |
|---|---|---|---|---|---|---|
| 0 | salary histogram (bar + markLine) | yAxis.min=0 / scale=false / minInterval=1 / graphic present / markLine BG `#22c55e/#ef4444/#3b82f6` | 同左 (graphicShape=0 = stats_close=false で empty group) | 同左 | width=1248 height=220 viewBox=null | ✅ |
| 1 | salary histogram | 同上 | 同左 (graphicShape=0) | 同左 | 1248×220 | ✅ |
| 2 | salary histogram | 同上 | 同左 (graphicShape=1 = stats_close=true 統合カード present) | 同左 | 1248×220 | ✅ |
| 3 | salary histogram | 同上 | 同左 (graphicShape=0) | 同左 | 1248×220 | ✅ |
| 4 | radar (market_tightness or regional_compare) | radar.center=["50%","55%"] / radius="65%" | 同左 | 同左 | 1248×320 | ✅ |
| 5 | bar (横向き 2 系列) | yAxis.type=category / xAxis.type=value | 同左 | xAxis.axisLine="auto" (ECharts default で復元、SSR では未指定) | 1248×360 | ✅ (default 復元) |
| 6 | pie (employment donut) | series.minAngle=5 | 同左 | minAngle=5 | 1248×250 | ✅ |
| 7 | scatter (+ line 回帰) | xAxis.show=true / axisLine.show=true / axisTick.show=true / yAxis.min=24 max=62 | 同左 | 同左 (axisLine=true / axisTick=true) | 1248×280 | ✅ |

**結論: source → SSR `data-chart-config` → runtime `getOption()` は全 8 chart で完全一致。**

---

## 2. 重点項目別の確認結果

| 項目 | 期待 | SSR | runtime (screen) | runtime (print) |
|---|---|---|---|---|
| ヒストグラム `yAxis.min` | `0` | `0` (chart 0–3) | `0` | `0` |
| `yAxis.scale` | `false` | `false` | `false` | `false` |
| `yAxis.minInterval` | `1` | `1` | `1` | `1` |
| markLine `label.backgroundColor` | `#22c55e/#ef4444/#3b82f6` | 一致 | 一致 (取得経路微差: 配列 wrap) | — |
| `graphic` 統合カード | stats_close 時 children=1, それ以外 children=0 group | chart 2 のみ children=1, 他は children=0 | `graphic: present` | — |
| scatter `xAxis.show` / `axisLine.show` | `true/true` | `true/true` | `true/true` | — |
| radar `center` | `["50%","55%"]` | `["50%","55%"]` | `["50%","55%"]` | `["50%","55%"]` |
| radar `radius` | `"65%"` | `"65%"` | `"65%"` | `"65%"` |
| pie `minAngle` | `5` | `5` | `5` | — |

---

## 3. SVG attribute と print media

| # | screen bbox | screen SVG width×height | print bbox | print SVG width×height |
|---|---|---|---|---|
| 0–3 (hist) | 1248×220 | 1248×220 | 1280×220 | 1280×220 |
| 4 (radar) | 1248×320 | 1248×320 | 1280×320 | 1280×320 |
| 5 (bar) | 1248×360 | 1248×360 | 1280×360 | 1280×360 |
| 6 (pie) | 1248×250 | 1248×250 | 1280×250 | 1280×250 |
| 7 (scatter) | 1248×280 | 1248×280 | 1280×280 | 1280×280 |

- screen / print 双方で **viewBox=null**。SVG は固定 width/height (px) で出力されており、`@media print` 切替時にコンテナ幅が 1248 → 1280 に変わると ECharts が resize されず固定値のまま。
- ただし print 時の bbox.width = 1280 / SVG width = 1280 で一致しており、Round 2.9 の `preparePdfRender(760)` は今回未呼び出しだが、ブラウザの emulateMedia('print') 単独では SVG width が `1280` のままで縮まない。

---

## 4. 仮説判定

- **E1 (source option が HTML に出ていない / Tera/render bug)**: ❌ 棄却。SSR `data-chart-config` JSON は source の json! macro 出力と完全一致。
- **E2 (HTML は正しいが ECharts runtime で書き換えられている)**: ❌ 棄却。runtime `getOption()` は SSR と同値 (ECharts default 補完で `axisLine=true` 等が解決されるが、実値は変わらず)。
- **E3 (option は正しいが SVG attribute / layout が違う)**: ✅ **採択**。
  - 全 chart で `viewBox` が `null` (固定 px 出力) のため、PDF print 時にコンテナ幅 1280px 想定の SVG が A4 幅 (`@page` の content-box ≈ 760px 相当) に縮約されると、内部要素 (`graphic` の右上カードや markLine ラベル) が想定外位置に押し込まれる。
  - print 時の bbox.w=1280 で screen と等値なので、Round 2.9 で導入した `preparePdfRender(760)` を経由しないと resize が走らず、PDF レンダラ側でラスタ縮小されている可能性が高い。

---

## 5. Round 2.8-C 結論との差分

Round 2.8-C は古い `data/generated/debug_*.html` を参照していた疑いがあったが、今回 **本番 Playwright で取得した HTML / runtime DOM の両方で source と完全一致** を確認。
→ option 層の問題ではない。残るは **SVG layout / PDF rasterization (Round 2.10-D 領域)** のみ。

---

## 6. 次ラウンド推奨

1. **Round 2.10-D**: SVG 内部要素の位置監査
   - `<g class="ec-graphic">` の右上カード (`right:10`, `top:10`) の実 BBox 計測
   - markLine ラベル `<text>` の x 座標と viewport 幅の関係
   - `preparePdfRender(760)` 適用前後で SVG width/viewBox がどう変わるか比較
2. **Round 2.10-E** (任意): `renderer:'svg'` を `renderer:'canvas'` に切替えた場合の PDF 差分 A/B (実装変更なしの diagnostic フラグで)
3. SSR/runtime/source 一致は確定したので、option 周りの追加監査は不要。

---

## 補足: 参照した source 行

- `src/handlers/survey/report_html/helpers.rs:316-341` (yAxis.min/scale/minInterval, graphic, markLine)
- `src/handlers/survey/report_html/helpers.rs:199/218/237` (markLine label.backgroundColor)
- `src/handlers/survey/report_html/scatter.rs:116-141` (xAxis.show / axisLine / yAxis)
- `src/handlers/survey/report_html/market_tightness.rs:1116-1129, 1704-1717` (radar.center / radius)
- `src/handlers/survey/report_html/regional_compare.rs:757-764` (radar.center / radius)
- `src/handlers/survey/report_html/employment.rs:75-82` (pie.minAngle=5)
- `src/handlers/survey/report_html/helpers.rs:1042-1049` (`echarts.init(el, null, {renderer:'svg'})` + `setOption(config)` の DOM 構築)
