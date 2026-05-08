# PDF chart rendering 真因確定 (Round 2.10 統合)

**作成日**: 2026-05-09  
**対象**: ECharts chart が PDF で screen viewport (1280px) のまま描画され、本文域 555pt 内に押し込まれて見切れる問題

## 要約

Round 2.7-AC / 2.9 が「修正したのに PDF で効かない」を繰り返した真因は **option 層の問題ではなく**、**Playwright viewport 1280px のまま `page.pdf()` を呼んでいる** ことにある。

## 監査結果サマリ (4 worker)

### 2.10-A 実 DOM 抽出
- 1280px の出所 = `<body>` (Playwright viewport 由来)
- BODY padding 16×2 → `.section` / `.echart` に 1248px 伝搬
- `<svg>` 1248 は ECharts が root bbox を書き戻した結果 (副次)
- canvas 不在 (SVG renderer 確定)

### 2.10-B selector マッチ
- ✅ `.echart` count=8、`[_echarts_instance_]` count=8 (実マッチ)
- ❌ `.chart-container` / `.chart-wrapper` / `.echart-wrap` / `.echart-container` / `[data-chart]` 全て **count=0** (Round 2.9-B 死に rule)
- pdf-rendering class 付与で width 1248 → **1280** (むしろ +32px)

### 2.10-C option 差分
- source = SSR `data-chart-config` = runtime `getOption()` **完全一致**
- yAxis.min=0 / scale=false / minInterval=1 / graphic / radar.center / scatter.xAxis.show / pie.minAngle / markLine label backgroundColor すべて期待通り
- SVG viewBox=null (固定 px width/height) = resize() 必須

### 2.10-D render_echart_div
- 実出力: `<div class="echart" style="height:Npx;width:100%" data-chart-config='...'>`
- `[data-chart]` ❌ 実属性は `data-chart-config` (Round 2.9-B selector 誤り)
- 1280px / 100vw / max-width 固定設定は src のどこにもない (= viewport 由来確定)
- `<main>` ラッパー不在

## 真因確定

### 主因 (B)
**Playwright viewport 1280×720** で page.pdf() が走るため、`<body>`/`<section>`/`<.echart>` が 1280-1248px 伝搬。`emulateMedia('print')` も `page.pdf()` も viewport を A4 に縮小しない仕様。

### 副因 (A)
Round 2.9-B / Round 2-3 で追加した CSS selector の多くが**実 DOM に存在しない**:
- `.chart-container` / `.chart-wrapper` / `.echart-wrap` / `.echart-container` / `[data-chart]` → 全部 0 マッチ

### 副因 (C)
SVG viewBox 不在 (固定 px width/height) → ECharts resize() を明示呼出しないと SVG width 更新されない。

## 修正案 (Round 2.11)

### 案 X (推奨、最小修正、確実): Playwright viewport を A4 portrait に切替

`tests/e2e/helpers/pdf_helper.ts` の `preparePdfRender()` で:

```typescript
export async function preparePdfRender(page: Page) {
  // viewport を A4 portrait に縮小 (本因)
  await page.setViewportSize({ width: 794, height: 1123 });
  
  // 全 ECharts instance を再 resize (副因 C 対策)
  await page.evaluate(() => {
    document.querySelectorAll('[_echarts_instance_]').forEach(el => {
      const inst = (window as any).echarts?.getInstanceByDom?.(el);
      if (inst) inst.resize();
    });
  });
  
  // resize 完了待ち
  await page.waitForTimeout(800);
  
  // bbox.width <= 760pt ガード (副因 C のフェイルセーフ)
  await page.waitForFunction(() => {
    const charts = Array.from(document.querySelectorAll('[_echarts_instance_]'));
    return charts.length === 0 || charts.every(el => {
      const w = el.getBoundingClientRect().width;
      return w > 0 && w <= 760;
    });
  }, { timeout: 10_000 });
}
```

`e2e_print_verify.py` も同様。

### 案 Y (副次、整理): 死に rule 削除

`style.rs` から以下の selector 削除:
- `.chart-container` / `.chart-wrapper` / `.echart-wrap` / `.echart-container` / `[data-chart]` (全て count=0)
- `html.pdf-rendering .chart-container` / `html.pdf-rendering [_echarts_instance_]` (Round 2.9-B、選択肢 X で不要に)
- 残すのは `.echart` / `[_echarts_instance_]` のみ

このラウンドでは案 X のみ実施 → PDF 実物確認 → 効果あれば案 Y を別 commit で整理。

## Round 2.11 修正対象ファイル (具体的に)

| 修正 | ファイル | 行 | 内容 |
|---|---|---|---|
| 案 X | `tests/e2e/helpers/pdf_helper.ts` | 全体 | `setViewportSize({width:794,height:1123})` を `preparePdfRender` 冒頭に追加 |
| 案 X | `e2e_print_verify.py` | `page.pdf()` 直前 | Python 版 `page.set_viewport_size({"width":794,"height":1123})` 追加 |

## 通常導線 PDF (window.print() 経路) について

ユーザーがブラウザで「印刷」ボタンから生成する PDF (Chrome 印刷ダイアログ経由) は **Chrome 自身が A4 縮小印刷する**ため viewport 問題の影響を受けない見込み。ただし PDF 実機検証は Round 2.11 後に実顧客 CSV で再確認が必要。

## 検証方法

Round 2.11 修正後:
1. `out/round2_11_pdf_review/` に PDF 再生成
2. bbox 実測: 全 chart drawing rect width <= 540pt
3. PDF PNG 目視:
   - 図 3-2〜3-5: yAxis 0 始まり / バッジ / 統合カード視認
   - 図 5-1: プロット点複数 / X 軸目盛 / 回帰線全域
   - 図 MT-2: 中央適正サイズ
4. 全 PASS で完了扱い、1 件でも未達なら追加調査

## 学び

- **CSS selector を追加する前に、実 DOM の class / 属性を grep する** (Round 2.9-B の `.chart-container` 死に rule)
- **Playwright viewport は print media を発火しても変わらない** (Round 2.7-AC / 2.9 はこの前提を見落とした)
- **SVG viewBox 不在の場合 resize() 必須** (副因 C)
- **option 層の修正は確実に届く** (Round 2.7-AC は機能していた、見え方の問題は別層)

memory 候補: `feedback_pdf_chart_viewport_trap.md` (Playwright `page.pdf()` の viewport 1280px 罠)
