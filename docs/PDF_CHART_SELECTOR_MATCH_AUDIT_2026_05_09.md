# Round 2.10-B: CSS selector マッチ監査 (2026-05-09)

## 概要

Round 2.9-B (commit e1c2370) で追加した print CSS のセレクタが、本番 (https://hr-hw.onrender.com) の MarketIntelligence ページの実 DOM にマッチしているかを read-only で検証した。

## 検証手法

- Playwright (Chromium) で本番にログイン → CSV upload → MI variant ページ取得
- ECharts 描画完了 (4s 待機) 後、9 個の CSS selector の `querySelectorAll().length` と最大 6 要素の `getBoundingClientRect().width` を測定
- `emulateMedia({ media: 'print' })` 後、`document.documentElement.classList.add('pdf-rendering')` を JS で付与し、再測定
- spec: `tests/e2e/_round2_10_selector_audit.spec.ts`
- raw: `out/round2_10_selector_audit/match.json`

## 必須表

| selector | match count | bbox width (初期 / pdf-rendering 後) | target chart に当たるか | width 変化 | 判定 |
|---|---:|---|---|---|---|
| `.echart` | **8** | 1248 / 1280 | YES | +32px (悪化) | マッチ・効かず |
| `.echart-wrap` | 0 | - / - | NO | - | 不一致 |
| `.echart-container` | 0 | - / - | NO | - | 不一致 |
| `.chart-container` | 0 | - / - | NO | - | 不一致 |
| `.chart-wrapper` | 0 | - / - | NO | - | 不一致 |
| `[data-chart]` | 0 | - / - | NO | - | 不一致 |
| `[_echarts_instance_]` | **8** | 1248 / 1280 | YES | +32px (悪化) | マッチ・効かず |
| `html.pdf-rendering .chart-container` | 0 | - | NO | - | 不一致 |
| `html.pdf-rendering [_echarts_instance_]` | **8** | - / 1280 | YES (class 付与後) | width 同上 | マッチ・効かず |

## 追加観測 (CHART_DETAILS, pdf-rendering 適用後)

| 項目 | 値 |
|---|---|
| viewport.innerWidth | 1280 |
| `html` width | 1280 (`html.pdf-rendering`) |
| `body` width | 1280 |
| 親要素 `div.section.page-start` width | 1280 |
| `.echart` `computedWidth` | **1280px** |
| `.echart` `computedMaxWidth` | 100% |
| `.echart` `style.width` (inline, helper が付けた) | 100% |
| `.echart` height | 220 / 320 / 360 |

## 仮説判定

候補:
- **A: selector 不一致**: `.chart-container` `.chart-wrapper` `.echart-wrap` `.echart-container` `[data-chart]` の 5 種は実 DOM に存在せず、Round 2.9-B で追加した CSS は完全に空振り。
- **B: selector はマッチするが CSS override 失敗**: `.echart` と `[_echarts_instance_]` は 8 件マッチするが、bbox.width が `1248px → 1280px` (むしろ増加)。`html.pdf-rendering [_echarts_instance_]` も 8 件マッチするが効果なし。
- **C: html.pdf-rendering class が DOM に付いていない**: helper で明示的に付与した結果 root に付いた (確認済) ので **false**。

**結論: A + B の複合**。

具体的に:
1. **ECharts のクラス名は `.echart` (単数形・短い)**。Round 2.9-B が追加した `.chart-container` 系は本番 DOM に存在しない (Rust テンプレート `helpers.rs` が出力する div は `class="echart"`)。
2. `.echart` / `[_echarts_instance_]` は 8 件正しくマッチしているが、**親 (`div.section.page-start`, `body`, `html`) がすべて 1280px のまま**で、印刷メディアでも viewport が A4 幅 (~595pt = 約 794px) に収縮していない。`emulateMedia('print')` も `page.pdf()` も Chromium DevTools 上で「viewport を A4 にリサイズ」しないため、container を `width:100%` にしてもその親が 1280px なら 100% は 1280px。
3. `html.pdf-rendering [_echarts_instance_]` のセレクタ自体は specificity も特殊属性セレクタも問題ないが、**根本原因は親幅が 1280px のまま**なので CSS の効きようがない。

## 次ラウンド推奨修正対象

### 優先度 P0 (selector 不一致を解消)

不要な空振りセレクタを削除し、実 DOM に当たる **`.echart`** に集約:

```css
/* 削除 (空振り) */
html.pdf-rendering .chart-container { ... }
html.pdf-rendering [data-chart] { ... }
.echart-wrap, .echart-container, .chart-wrapper { ... }

/* 追加・統合 */
html.pdf-rendering .echart,
html.pdf-rendering [_echarts_instance_] { ... }
```

### 優先度 P0 (真因 = 親幅 1280px の固定)

`.echart` に幅制約を効かせるには、**親チェーンを A4 本文域 (≤ 760pt ≒ 1013px) に縛る**必要がある。候補:

```css
@media print, html.pdf-rendering {
  html, body { width: 794px !important; max-width: 794px !important; }
  div.section, div.section.page-start { max-width: 760px !important; width: 100% !important; }
  .echart, [_echarts_instance_] { width: 100% !important; max-width: 760px !important; }
}
```

または、helper 側で `viewport` を `{ width: 794, height: 1123 }` に setViewportSize してから `page.pdf()` を呼ぶ (CSS より確実)。

### 優先度 P1 (検証フック)

次ラウンドで Round 2.10-B spec を再実行し、以下を確認:
- `.echart` の bbox.width ≤ 760pt (≒ 1013px)
- 親 `body` width ≤ 794px

## 制約遵守

- read-only (実装変更・commit/push なし)
- 編集ファイル: `tests/e2e/_round2_10_selector_audit.spec.ts` (新規 spec) / `out/round2_10_selector_audit/match.json` / 本ファイルのみ
