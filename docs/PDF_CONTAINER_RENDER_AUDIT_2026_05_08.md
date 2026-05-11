# Round 2.8-D: container/render 監査

監査日: 2026-05-08
入力: `out/round2_7_pdf_review/mi_via_action_bar.pdf` + 既存 PNG
方針: read-only。実装変更なし。

---

## 1. chart container CSS

`render_echart_div(config_json, height)` (helpers.rs:131-139) が全 chart の生成元:

```html
<div class="echart" style="height:{H}px;width:100%;" data-chart-config='...'></div>
```

| 項目 | 値 | 評価 |
|------|----|----|
| width | inline `width:100%` + `.echart { max-width: 100% }` (style.rs:656) | OK |
| height | inline `height:{H}px` (220-360px) | OK (offsetHeight=0 は発生しない) |
| overflow | `@media print` で `overflow: visible !important` (style.rs:736) | OK |
| max-width | `@media print` で `max-width:100% !important` (style.rs:735) | OK |
| 子 svg | `@media print` で `max-width:100%; height:auto` (style.rs:743-745) | OK |

CSS 自体は妥当。print 用の見切れ対策が `@media print` 内に揃っている (style.rs:733-752, 2026-05-06 P0-2 で導入済み)。

---

## 2. ECharts resize listener

helpers.rs:1040-1076 にて全実装済み:

| listener | 状態 | 行 |
|----------|------|----|
| `beforeprint` | 有 | helpers.rs:1064 |
| `afterprint` | 有 | helpers.rs:1065 |
| `resize` | 有 | helpers.rs:1066 |
| `matchMedia('print').change` | 有 (Safari fallback) | helpers.rs:1068-1075 |
| renderer | `'svg'` 明示 | helpers.rs:1048 |
| ガード | `if (el.offsetHeight === 0) return;` | helpers.rs:1043 |

mod.rs:658 の「印刷/PDF保存」ボタンは `getInstanceByDom().resize() → setTimeout(window.print, 50)` を実装。

しかし **page.pdf() (Chromium DevTools Protocol Page.printToPDF) は beforeprint / afterprint / matchMedia('print') を発火させない**。これが核心。

---

## 3. PDF 内 chart bbox 測定

`mi_via_action_bar.pdf` (A4 portrait = 595x842pt) を PyMuPDF で測定:

| page | 想定 chart | 実 bbox (x0,y0)-(x1,y1) | width | A4 幅 595pt 超過 |
|------|-----------|--------------------------|-------|-----------------|
| 5 | 図 3- ヒスト | (40,76)-(1000,242) | **960pt** | **+405pt 超過** |
| 6 | 図 3- ヒスト/散布 | (40,76)-(1000,242) | **960pt** | **+405pt 超過** |
| 7 | 図 MT-2 レーダー | (40,472)-(1000,712) | **960pt** | **+405pt 超過** |
| 13 | 図 5-1 散布 | (40,76)-(1000,286) | **960pt** | **+405pt 超過** |
| 12 | (副散布 2 枚) | (40,361)-(291,517) / (304,361)-(555,517) | 251pt 各 | OK |
| 2 | (副ヒスト) | (40,149)-(556,288) | 516pt | OK |

→ **大形 chart 4 枚 (主要散布/レーダー/ヒスト) すべて x1=1000pt まで伸びて用紙右端 (555pt 安全域) を 445pt 超過**。これが PNG で見られた「右端見切れ」「グラフ消失」の物理的原因。

副 chart (page 2/12) は問題ない → DOM 構造の差ではなく、**chart 描画時点の親要素 width** がクラスター単位で異なる (grid 内の cell vs full-bleed) 可能性が高い。

---

## 4. canvas vs SVG renderer

`echarts.init(el, null, { renderer: 'svg' })` 明示 (helpers.rs:1048, insight/render.rs:1119)。**SVG 統一**で確定。canvas fallback は無い。

→ 仮説 D-3 (canvas が PDF で fallback して draw されない) は除外。drawings は実際 PDF 内に SVG path として描画されている (drawings=21〜114 検出)。

---

## 5. PDF 生成時の wait 条件

`e2e_print_verify.py` が今回の PDF 生成元:

```python
page.goto(url, timeout=90_000)
time.sleep(wait_sec)         # = 12秒 (固定)
page.emulate_media(media="print")
time.sleep(1)                # ← たった 1秒
page.pdf(path=..., format="A4", ...)
```

| wait | 状態 |
|------|------|
| `networkidle` | 無 (`goto` の `wait_until` 指定なし → デフォルト `load` のみ) |
| `waitForFunction(echarts.getInstanceByDom)` | 無 |
| `chart.on('finished')` の poll | 無 |
| `emulate_media('print')` 後の resize 完了待ち | 無 (固定 1 秒) |

`emulate_media('print')` は CSS の `@media print` を有効化するが、**ECharts SVG の attribute (width/height) は JS resize() を呼ばない限り再計算されない**。1 秒の固定 sleep では不十分かつ resize 自体トリガーされていない。

---

## 6. 仮説判定

| 仮説 | 判定 | 根拠 |
|------|------|------|
| **D-1**: container width が 0 / 極小 | ❌ 否定 | inline `height:Hpx; width:100%` 付与済み、PDF bbox は逆に大きすぎ |
| **D-2**: resize timing で PDF 生成時に未 resize | ✅ **本命** | bbox x1=1000pt → screen viewport (≈1280px) の SVG attribute がそのまま PDF に流入。beforeprint/matchMedia は page.pdf() で発火しない。e2e_print_verify は明示的 resize 呼び出しなし |
| **D-3**: canvas が PDF で fallback して draw されない | ❌ 否定 | renderer:'svg' 明示、PDF 内 drawings 確認済み |
| **D-4**: data が空 (Worker E 領域) | △ 別軸 | bbox は出力されているので data は存在。空欄パネルは別仮説 (option 不正 / フィルタ後 0 件) |

→ **D-2 確定**。Round 2.8-A〜C で「option は正しい」と判定された前提と整合: **option は正しいが、レンダリング時の親要素 width が screen viewport のままで、A4 印刷幅 (本文域 555pt) に再 resize されないまま PDF 化されている**。

副因として page.pdf API が beforeprint を発火させない仕様 (Chromium バグというより設計通り) を helpers.rs の resize hook 設計が想定していない。

---

## 7. 次ラウンド推奨

### 即効修正 (P0, 1ラウンド)

1. **e2e_print_verify.py に明示 resize トリガを追加**:
   ```python
   page.emulate_media(media="print")
   page.evaluate("""() => {
     document.querySelectorAll('[_echarts_instance_]').forEach(el => {
       const c = echarts.getInstanceByDom(el);
       if (c) c.resize();
     });
   }""")
   page.wait_for_function(
     "() => Array.from(document.querySelectorAll('.echart')).every(el => "
     "{ const c=echarts.getInstanceByDom(el); return c && c.getWidth()<=600; })",
     timeout=10_000
   )
   page.pdf(...)
   ```

2. **本番側 helpers.rs にも `chart.on('finished')` ベースの ready flag を追加**し、page.pdf() を直接叩く運用 (Render の PDF API 等) でも安全にする。

### 構造修正 (P1, 別ラウンド)

3. ECharts 設定で `media` クエリ的に **A4 width に固定した container** を print 専用に用意し、`@media print` で screen 用 div を `display:none`、印刷用 div だけ表示 → resize 不要にする (mi が採用している `mi-print-only / mi-screen-only` パターンを全 chart に拡張)。

### 検証ラウンド推奨

- **Round 2.8-E**: 修正後の PDF を再生成し、bbox x1 が 555pt 以下に収まることを確認
- **Round 2.8-F**: data 空欄問題 (Worker E 仮説) を独立に調査

---

## 報告サマリ

- container CSS: 妥当 (height inline + print 用 max-width 整備済)
- resize listener: beforeprint/afterprint/matchMedia/svg renderer 全揃い
- **PDF 内 chart bbox: x1=1000pt と A4 幅 595pt を 405pt 超過 (page 5/6/7/13)**
- renderer: SVG 確定
- PDF 生成 wait: `time.sleep(12)` + `emulate_media + sleep(1)` のみ。ECharts resize 明示呼び出し無し、`finished` event 待ちも無し
- **仮説判定: D-2 確定**。option は正しいが PDF 化時に screen viewport のまま resize されず本文域を超過
- **推奨**: e2e_print_verify に explicit resize + getWidth ガードを追加 → 即効改善
