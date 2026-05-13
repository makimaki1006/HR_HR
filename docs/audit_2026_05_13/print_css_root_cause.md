# 印刷 CSS 根本原因調査 (2026-05-13)

調査対象: `src/handlers/survey/report_html/style.rs` (2735 行)

読了済み @media print ブロック (16 箇所):
163, 276, 676, 921, 1159, 1423, 1447, 1488, 1528, 1568, 1598, 1616, 1621, 1632, 1637, 2015, 2214, 2459, 2709
読了済み html.pdf-rendering ブロック (799-819): 完了

## 結論 (200 word 以内)

**根本原因は 2 段で連鎖している。**

1. **直接原因 (#A / #B 共通)**: `style.rs:782-791` (@media print) と `style.rs:799-807` (html.pdf-rendering) の
   `[_echarts_instance_] { overflow: hidden !important; }`
   ECharts SVG renderer は root `<svg>` の外側に `<g class="echarts-graphic">` や axisLabel `<text>` を **viewBox を超える位置** に配置することがある (特に value 軸の負値ラベル、graphic.group の `top: 0` 配置)。親要素に `overflow: hidden` が掛かると SVG viewport 外の要素はクリップされ消える。
2. **増幅原因**: 同ブロック内
   `[_echarts_instance_] svg, ... { height: auto !important; }` (line 788-791, 811-813)
   ECharts は `<svg width="W" height="H">` を明示属性で出すが、CSS `height:auto !important` が prop を上書きするため SVG が **アスペクト比に従って縦に潰れる** (width 100% に対し height が 0 〜 数 px)。X 軸 tick label は SVG 下端の絶対 y 座標に出るため、潰れた SVG では描画領域外。

修正案: `overflow: hidden` → `overflow: visible`、`height: auto` の削除 (内部 svg/canvas には height 制約を掛けない)。

## 検査した @media print / html.pdf-rendering 全ブロック

| Line | スコープ | ECharts 関連 | 主要 rule (echart/svg 影響のみ) |
|------|----------|--------------|-------------------------------|
| 163  | contenteditable | ✗ | outline none のみ |
| 276  | no-print, table, h2/3 | △ | `.echart, figure, img { page-break-inside: avoid }` 害なし |
| 676  | **主要 chart block** | ✅ | line 734-750 `.echart { width:100%; max-width:100%; overflow:visible }` 健全 / line 740-745 `.echart svg { max-width:100%; height:auto !important }` **#A/#B 一因** / line 782-787 `[_echarts_instance_] { overflow:hidden !important }` **#A/#B 主因** / line 788-791 `[_echarts_instance_] svg { height:auto !important }` **#A/#B 増幅** |
| 921  | レイアウト系 (exec-kpi 等) | ✗ | grid 制約のみ |
| 1159 | section padding | ✗ | 害なし |
| 1423-1637 | テーマ別 (v6/v7/v8 装飾) | ✗ | 装飾色のみ。text 隠蔽なし |
| 2015 | KPI カード | ✗ | 害なし |
| 2214 | wp 系装飾 | ✗ | 害なし |
| 2459 | data-theme=v8 chart | △ | `break-inside: avoid` のみ (害なし) |
| 2709 | data-theme=v7a chart | △ | `break-inside: avoid` のみ (害なし) |
| **799-819** | **html.pdf-rendering scope** | ✅ | line 802-807 `overflow: hidden !important` **page.pdf() 経路で #A/#B 主因** / line 808-813 `svg { height:auto !important }` 増幅 |

font-size:0 / color:transparent / display:none on svg/text の隠蔽パターンは全 scope に **存在せず**。
`svg text {…}`, `g.graphic {…}`, `[_echarts_renderer]` などの ECharts 内部要素を直接 hide する rule も **存在しない**。

## 仮説検証手順

Playwright で次を実行し、root cause を実証する:

```js
// 1. 本番 HTML を取得して page.pdf() を撃つ
await page.goto(reportUrl);
await page.addStyleTag({ content: 'html.pdf-rendering { all: revert }' }); // null-effect 比較
// 2. classList.add('pdf-rendering') 後の SVG bounding を測定
await page.evaluate(() => document.documentElement.classList.add('pdf-rendering'));
const m = await page.evaluate(() => {
  const sv = document.querySelectorAll('.echart svg, [_echarts_instance_] svg');
  return Array.from(sv).map(s => ({
    bb: s.getBoundingClientRect(),
    h: s.getAttribute('height'),
    cssH: getComputedStyle(s).height,
    parentOverflow: getComputedStyle(s.parentElement).overflow,
  }));
});
console.log(m); // height が 0 / parentOverflow=hidden を確認
// 3. 該当 2 rule を打ち消すと axisLabel / graphic が見えることを確認
await page.addStyleTag({ content:
  '[_echarts_instance_]{overflow:visible !important} ' +
  '[_echarts_instance_] svg, .echart svg{height:revert !important}' });
await page.pdf({ path: 'fix_test.pdf' });
```

期待: 打ち消し後の PDF で page 11 X 軸 (-200/-100/0/100/200) と page 5/6 graphic chip が出現。

## 修正案 (優先順)

### P0 (即時): `overflow: hidden` を `visible` に

`style.rs:785` および `style.rs:805`

```css
/* 修正前 */
[_echarts_instance_] { overflow: hidden !important; ... }
html.pdf-rendering ... [_echarts_instance_] { overflow: hidden !important; ... }

/* 修正後 */
[_echarts_instance_] { overflow: visible !important; ... }
html.pdf-rendering ... [_echarts_instance_] { overflow: visible !important; ... }
```

リスク: 元々 P0-2 で `overflow:hidden` を入れた目的は **右端見切れ** だが、line 734-739 (`.echart { overflow:visible }`) と矛盾している。`max-width: 100%` で幅は既に制約済みのため、子の `overflow:hidden` は冗長。リスク低。

### P0 (即時): svg/canvas の `height: auto !important` 削除

`style.rs:740-745`, `788-791`, `808-813`

```css
/* 修正前 */
.echart canvas, .echart svg, ... { max-width: 100% !important; height: auto !important; }

/* 修正後: height を CSS で強制しない (ECharts が attribute で設定する height を honor) */
.echart canvas, .echart svg, ... { max-width: 100% !important; }
```

リスク: 一部 chart が aspect ratio で縮まなくなる可能性あるが、ECharts は resize hook で適切な height を attribute 設定するため実害なし。Round 2.9-A の JS resize と整合する。

### P1: ECharts 内部要素への overflow visible を明示

```css
.echart svg, [_echarts_instance_] svg { overflow: visible !important; }
.echart svg g, [_echarts_instance_] svg g { overflow: visible; }
```

SVG `overflow:visible` で viewBox 外要素 (axisLabel, graphic) を保護。

## 念のため確認すべき箇所

- `style.rs:442, 452, 1022, 1128, 1912, 1936` の `overflow:hidden` は echart 系セレクタではないが、もし chart が `.figure` 等の wrapper 内なら間接的にクリップする可能性。各セレクタの祖先構造を要確認。
- demographics.rs:399-408 で `grid: { containLabel: true }` または `grid.bottom` が axisLabel 用に十分か (CSS 修正後も label が出ない場合は JS 側で grid.bottom を 30 以上に)。
- `style.rs:316` `.echart { page-break-inside: avoid }` は害なし。
- Round 2.9-A の JS (onbeforeprint resize) が page.pdf() 経路で発火するかは別系統の疑い (CSS 修正で解消しない場合は JS hook 経路を再調査)。
