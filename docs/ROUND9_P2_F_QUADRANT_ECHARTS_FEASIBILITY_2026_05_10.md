# Round 9 P2-F: 4 象限図 ECharts 化 実装可否判断

**日付**: 2026-05-10
**判断**: **実装しない (CSS 維持)**

---

## 結論

CSS 散布図 (Round 8 P2-B / `e5bab26` で実装、本番 PDF PASS 済) を維持し、ECharts 化は **実装しない**。

---

## 理由 (3 行)

1. **PDF 印刷で既に PASS**、ECharts 化は Round 2.7-2.11 の viewport 問題 (5 ラウンド工数 / 2026-05-06〜09 の 4 日間) を再発させるリスクあり、便益と釣り合わない
2. **tooltip 相当は既に実装済** (`market_intelligence.rs:1760` の `title="{full}"` 属性で Web hover ネイティブ tooltip が出る)
3. 4 象限分割 (median split × 2 + 4 隅ラベル + 対数軸 + 円サイズ) を ECharts で再現するには `markLine` / `graphic` / `type:"log"` / `symbolSize` を組み合わせる必要があり、既存 PASS を壊すリスクが高い

---

## 比較表 (Agent F 監査結果)

| 観点 | CSS (現状 P2-B) | ECharts | コメント |
|---|---|---|---|
| PDF 印刷安定性 | ✅ 高 (`position:absolute` で固定) | △ viewport 補正必須 | Round 8 P2-B で「CSS が適切」既決 |
| インタラクション | ❌ `title` 属性 tooltip のみ | ✅ tooltip / zoom / pan | PDF では無効、Web のみ |
| 実装コスト | ✅ 既存 80 行 | ✗ scatter + visualMap + markLine + graphic 約 100-150 行 + JS 検証 | log + median split + 4 隅 ラベル + 円サイズ の組合せは ECharts でも非自明 |
| viewport 問題リスク | ✅ ゼロ | ⚠️ 高 (Round 2.7-2.11 再発リスク) | 既存 8 chart は preparePdfRender で守られているが、9 個目追加で regression risk |
| 集約注釈表現 | ✅ `display_name` 直接ラベル | △ rich text / formatter 再実装 | |
| log スケール | ✅ Rust 側で `f64::ln` 計算済 | ✅ `xAxis.type:"log"` だが markLine も log 軸座標で要再指定 | |

---

## Round 2 chart 問題の歴史 (Agent F 抽出)

**問題サマリ**: ECharts SSR 完全一致なのに PDF 生成時に chart が本文域 555pt を超過、見切れ。

**真因** (`docs/PDF_CHART_RENDERING_ROOT_CAUSE_2026_05_09.md:6-9, 36-44`): Playwright `page.pdf()` (Chromium DevTools `Page.printToPDF`) は viewport を A4 に縮小しない仕様で、default 1280×720 のまま `<body>`/`<.echart>` に 1248-1280px 伝搬。`emulateMedia('print')` も `beforeprint`/`afterprint` を発火させず、Rust 側 resize hook が動かず。

**解決策** (`tests/e2e/helpers/pdf_helper.ts:32-77`): `preparePdfRender()` で viewport を 794×1123 (A4 portrait @96dpi) に強制縮小、`[_echarts_instance_]` 全件に `instance.resize()` 発火、800ms 待機、bbox.width ≤ 800px を `waitForFunction` で保証。

**工数**: Round 2.7-AC → 2.8-D → 2.9-A/B → 2.10-A/B/C/D → 2.11 と **5 ラウンド以上**、`PDF_*_2026_05_0[8|9].md` に **15+ 件の監査ドキュメント**。

---

## 別ラウンドで PoC するなら

以下条件すべて満たす場合のみ本採用:

1. 4 象限図以外の chart で `preparePdfRender` viewport ガードが Round 9 以降も連続 PASS していること (regression 尽き)
2. 実顧客 CSV で点数が 30+ になり CSS 配置が密集して読めなくなる事象が発生
3. ローカル別ブランチで CSS 版併存のまま ECharts 版を `if cfg!(feature = "echarts_quadrant")` で隔離 PoC
4. PDF 検証で 1 ラウンド (1 日) 以内に bbox/viewport を 800pt 以下で安定させられること

これらが揃うまでは現状維持。

---

## 監査メタデータ

- 実装変更: ゼロ
- DB 書込: ゼロ
- docs のみ作成
- 影響範囲: なし

**Round 9 P2-F は判断 docs のみ作成、実装なし。**
