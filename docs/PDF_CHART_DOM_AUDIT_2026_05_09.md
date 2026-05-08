# Round 2.10-A: ECharts chart 実 DOM 監査

**取得日**: 2026-05-09
**対象**: 本番 `https://hr-hw.onrender.com` `/report/survey?variant=market_intelligence`
**取得方法**: Playwright `page.evaluate` による live DOM 抽出 (read-only)
**生 JSON**: `out/round2_10_dom_audit/dom_tree.json`
**spec**: `tests/e2e/_round2_10_dom_audit.spec.ts`

## 取得サマリ

- 抽出 chart 件数: **8 件** (期待 6 件、実際は MI variant に追加 chart 含む 8 件)
- viewport: Playwright デフォルト (1280x720)
- ECharts レンダラ: **SVG** (canvas は全 chart で `null`)

## 必須表

| # | chart (title) | root selector | root class | parent class | grandparent class | root width | parent width | grand width | svg width | canvas width | overflow (root) | 備考 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 図 3-2 下限月給ヒストグラム (20,000円刻み) | `div.echart` | `echart` | `section page-start` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 1 | 図 3-3 下限月給 (5,000円刻み) | `div.echart` | `echart` | `section page-start` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 2 | 図 3-4 上限月給 (20,000円刻み) | `div.echart` | `echart` | `section page-start` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 3 | 図 3-5 上限月給 (5,000円刻み) | `div.echart` | `echart` | `section page-start` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 4 | 図 MT-2 採用市場 3 軸レーダー | `div.echart` | `echart` | `section page-start` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 5 | 男女年齢ピラミッド (採用市場 逼迫度) | `div.echart` | `echart` | `section` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 6 | 図 4-1 雇用形態構成ドーナツ | `div.echart` | `echart` | `section` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |
| 7 | 図 5-1 月給下限×上限 散布図 | `div.echart` | `echart` | `section` | (BODY) | **1248px** | **1248px** | **1280px** | `width="1248"` | — | visible | inline `width: 100%` |

備考:
- **全 8 chart で完全に同じ寸法プロファイル**。例外なし。
- root 要素 `div.echart` の inline style は全件 `height: NNNpx; width: 100%; position: relative;` (height のみ chart 種別で 220/250/280/320/360 に変動)。
- root の computed `maxWidth: 100%` だが、`100%` は親 `<div class="section">` の 1248px に解決される。
- 親 `<div class="section">` (`page-start` 修飾あり/なし) は **inline style 空・class CSS で 1248px 固定**。
- BODY computed width = **1280px** (= viewport 幅)。`padding: 8px 16px` を box-sizing: border-box で内包し、内側コンテナへ 1280 - 32 = **1248px** が伝搬している。
- HTML computed width も 1280px。
- SVG `width` 属性は ECharts が root の bbox (1248) を直接書き戻している。viewBox は `null` (= ECharts デフォルト、resize 時にも幅依存で再計算される)。
- canvas は全件 `null` (SVGRenderer 採用)。

## 1280px の出所

階層別の幅伝搬チェーン (上→下):

| 階層 | 要素 | computed width | 出所 |
|---|---|---|---|
| L0 | `<html>` | 1280px | viewport (Playwright 1280x720; print 時は @page A3 横で別値になり得る) |
| L1 | `<body>` | **1280px** | viewport を継承。**ここに固定値依存 (padding 16px x2) があるため媒体に追従しない** |
| L2 | `<div class="section">` | **1248px** | 1280 - body padding 32 = 1248。class CSS で `width: ...` 明示があるかは追加確認推奨だが、computed 上は親から自動継承され `maxWidth: none` |
| L3 | `<div class="echart">` (root) | **1248px** | inline `width: 100%` が L2 の 1248px に解決 |
| L4 | `<svg>` | **1248** (attr) | ECharts が root bbox を直接書き戻し |

**結論: 1280px の出所は `<body>` (= viewport 幅)**。Round 2.9 で root に `width: 100%` を当てても、親 `.section` も BODY もすべて 1280/1248px の固定鎖になっているため、root 単独修正では効かない。print CSS が効かなかったのも、`@page` で paper size を変えても **BODY が viewport 由来の 1280px に固定されている**ため、@media print で BODY または上位の幅指定を上書きしていない限り変わらない。

## 仮説判定

提示された 3 仮説のうち:

- **B (親 wrapper が固定幅)** ✅ 採択
  - 親 `<div class="section">` が 1248px、BODY が 1280px。root の inline `width: 100%` は機能しているが、親自体が固定相当のため意味がない。
  - 真因は **BODY の 1280px**。これは viewport 由来で、screen 表示では正常だが PDF 出力時に @page サイズへ追従していない。
- **C (SVG が固定 attr)** ⚠️ 部分的に該当
  - SVG `width="1248"` は ECharts が描画時の root bbox から書いた **結果**。SVG 単体での原因ではないが、resize event を発火しないと再計算されない。
- **D (対象要素の取り違え)** ❌ 否定
  - `[_echarts_instance_]` で取得した root は `div.echart`、これは正しい対象。問題は対象要素ではなく**親の幅指定**。

## 次ラウンド推奨修正対象

優先順 (cost 低 → 高):

1. **🟢 第一候補: `@media print` で BODY とコンテナを A3-横相当に強制**
   - 例:
     ```css
     @media print {
       @page { size: A3 landscape; margin: 12mm; }
       html, body { width: 100% !important; max-width: none !important; padding: 0 !important; }
       body > .section, .section { width: 100% !important; max-width: 100% !important; }
       .echart { width: 100% !important; }
     }
     ```
   - 加えて **PDF 生成時に ECharts `resize()` を全インスタンスで明示呼出し** (SVG width attr は自動更新されないので)。
2. **🟡 第二候補: PDF 生成側 (Playwright `page.pdf({ width, height })`) の `width` を 1248px ではなく A3 横の物理幅と一致させる**
   - 現状 `page.pdf` の幅指定が 1280px (viewport そのまま) と推測。print 用 viewport を `page.setViewportSize({ width: <A3pxs>, height: ... })` で先に切り替えてから ECharts resize → pdf 出力する 3 段フロー。
3. **🔴 第三候補 (root 単独修正は不可)**
   - Round 2.9 の `el.style.width='100%'` だけでは BODY と `.section` が固定鎖のため**永久に効かない**。再試行禁止。

### 検証で次に確認すべき点 (Round 2.10-B 候補)

- `<div class="section">` に CSS で明示 `width:` が当たっているか (computed で 1248px だが inline 空、class からの set があるか stylesheet を grep)。
- BODY の computed が `1280px` 固定になる原因 (`html { width: 1280px }` のような明示指定があるか、それとも viewport 由来の自動値か)。テンプレート HTML / print stylesheet の grep で確定。
- PDF 出力フロー (Playwright の `page.pdf()` 呼出箇所と width/height/printBackground 設定) と print CSS の合致状況。

## 関連ファイル (絶対パス)

- spec: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\tests\e2e\_round2_10_dom_audit.spec.ts`
- 生 JSON: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\out\round2_10_dom_audit\dom_tree.json`
- meta: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\out\round2_10_dom_audit\meta.json`
- 本 docs: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\PDF_CHART_DOM_AUDIT_2026_05_09.md`
