# Round 2.10-D: render_echart_div 出力監査

監査日: 2026-05-09
スコープ: `src/handlers/survey/report_html/` 配下の chart wrapper helper 出力 HTML と `style.rs` selector の対応

## 1. chart wrapper 関数別出力

| 関数 | file:line | 出力 class | data 属性 | inline style | 備考 |
|---|---|---|---|---|---|
| `render_echart_div` | helpers.rs:132-139 | `class="echart"` | `data-chart-config='<json>'` | `height:{N}px;width:100%;` | survey/report_html 配下の主たる chart 生成器 |
| `balance.rs` 直接生成 | balance.rs:499 | `class="echart"` + `role="img"` + `aria-label` | `data-chart-config='...'` | `height:{N}px;` (width 指定なし) | survey 外。同じ class 名で互換 |
| `balance.rs` 直接生成 | balance.rs:573 | `class="echart"` + `role="img"` + `aria-label` | `data-chart-config='...'` | `height:400px;` | 同上 |
| `demographics.rs` 直接生成 | demographics.rs:409 | `class="echart"` | `data-chart-config='...'` | `height:350px;` | survey 外 |
| `render_figure_caption` | helpers.rs:472-478 | `class="figure-caption"` + 内側 `class="fig-no"` | なし | なし | chart の上部に配置 |

注: helpers.rs に `render_chart_wrapper` 関数は **存在しない**。chart wrapper は `render_echart_div` 単独で、外側に `.echart-wrap` `.echart-container` `.chart-container` `.chart-wrapper` 等のラッパーを **生成していない**。

## 2. chart 親 wrapper (section / main / body)

| 階層 | 要素 | class / 設定 | width 設定 | print 時 width | 備考 |
|---|---|---|---|---|---|
| ルート | `<body>` | mod.rs:649 で `<body>` 開始 | `padding:8px 16px` (style.rs:86) | `padding:0 !important; margin:0 !important` (style.rs:686-687) | 固定 width なし。`<main>` ラッパーは存在しない |
| 親 | `<section class="section">` | style.rs:390-395 | `margin-bottom:16px` のみ | 同左 | 固定 width なし。本文幅は @page margin で決まる (A4 194mm) |
| chart 直親 | `<div class="echart">` | style.rs:656 | `max-width:100%` | `width:100% !important; max-width:100% !important; overflow:visible !important` (733-738) | inline で `width:100%` も付与済 |
| chart 子 | ECharts 自動生成 `<div>` | (ECharts runtime) | (動的) | `max-width:100% !important` (749-752) | 1280px 等の固定値は CSS では設定なし |

ECharts runtime が DOM に `_echarts_instance_` 属性を付与する想定で style.rs:785, 809 が selector として記述されている。

## 3. 1280px / 100vw / max-width 固定箇所 grep 結果

`style.rs` (survey report) 内で `1280px` の文字列は **0 件** (style.rs L1280 は別件 `.report-sev-critical`)。
`100vw` の文字列も **0 件**。
固定 width 値は `width:1px !important` (L439, 視覚的ヘルパー), `width:64px` (L1749) 等の小要素のみ。**body/main/section に固定 width は設定されていない**。

`@page` margin: 10mm 8mm 12mm 8mm (style.rs:59) → 本文幅は A4 210mm − 8mm×2 = **194mm**。

## 4. selector マッチ状況 (CSS との対応)

| CSS selector | 実 DOM 上の対応 | マッチするか | 影響 |
|---|---|---|---|
| `.echart` | `<div class="echart">` (helpers.rs:136) | ✅ マッチ | 印刷時の width:100%, overflow:visible 適用 |
| `.echart-wrap` | 存在しない | ❌ 対象なし | 死に rule |
| `.echart-container` | 存在しない (style.rs:670 で page-break-inside だけ書かれているが、emit 側に該当 wrapper なし) | ❌ 対象なし | 死に rule |
| `.chart-container` | 存在しない | ❌ 対象なし | 死に rule |
| `.chart-wrapper` | 存在しない | ❌ 対象なし | 死に rule |
| `[data-chart]` (style.rs:784, 808) | 実 DOM は `data-chart-config` 属性 → セレクタ `[data-chart]` は **属性名完全一致のみ**、 `[data-chart-config]` には **マッチしない** | ❌ ミスマッチ | 死に rule (ただし `.echart` rule は別途効くため致命的ではない) |
| `[_echarts_instance_]` | ECharts runtime が DOM 初期化後に付与 | ⚠️ runtime 依存 | page.pdf() 経路で ECharts init 完了後ならマッチ。init 前なら未付与 |

### 仮説判定

- **selector 不一致**: `[data-chart]` が `[data-chart-config]` と一致しない問題は確認できたが、`.echart` class が同じ要素に付いているため、width 制約は実質適用されている (致命傷ではない)。
- **親 wrapper 固定**: `<body>` / `<section>` / `<main>` には固定 width が **設定されていない**。1280px の screen 幅が PDF に持ち込まれている原因は CSS の 1280px 固定ではなく、page.pdf() 呼び出し時の **viewport (≈960pt → CDP デフォルト ~816px / 1280px screen-emulation)** の影響。CSS 側で本文幅 100% を強制するのは現状 OK。
- **chart wrapper の二重ラップ欠落**: `render_echart_div` は `.echart` 単独 div のみ emit。CSS は `.echart-wrap` / `.chart-container` 等の存在を仮定しているが emit 側で生成していない (歴史的経緯による selector 過剰指定)。

## 5. Round 2.11 修正対象 (file:line)

優先度順:

1. **helpers.rs:132-139 `render_echart_div`** — `data-chart-config` のままで OK (JS が依存)。ただし PDF print 経路向けに `data-chart="echart"` (値は識別不要) も冗長付与すれば、style.rs:784/808 の `[data-chart]` selector が活きる。代替策として **CSS 側を `[data-chart-config]` に変更**するほうが無侵襲。
   - 推奨: `style.rs:784, 791, 808, 820` の `[data-chart]` → `[data-chart-config]` に置換。

2. **style.rs:670, 722, 733, 740-743, 749-750, 804-807, 816-819, 825-828** — `.echart-wrap` `.echart-container` `.chart-container` `.chart-wrapper` の **死に selector**。削除するか、`render_echart_div` の出力に外側 wrapper `<div class="echart-container">` を追加して図表番号と一体化する整理案。
   - 最小修正: 死に selector を残しても害なし → 触らない判断も可。
   - 整理案 (推奨): helpers.rs:132-139 を `<div class="echart-container"><div class="echart" ...></div></div>` に変更し、`page-break-inside: avoid` (style.rs:670) を chart 単位で確実に効かせる。

3. **mod.rs:649 `<body>` 直下** — `<main class="report-main">` ラッパーが無いため、印刷時の本文幅制御は `<body>` の `padding:0 !important` (style.rs:686) だけで賄っている。固定 width が CSS に存在しないため追加修正不要。

## 補足

- `render_echart_div` は survey/report_html 配下の **唯一の chart 生成 helper**。`render_chart_wrapper` という名前の関数は **存在しない** (タスク指示の想定と異なる)。
- balance.rs / demographics.rs は survey 配下ではなく、別タブで `<div class="echart" data-chart-config>` を直接 format! している。同じ class/属性なので CSS は共通で効く。
- 1280px screen 幅持ち込み問題は CSS 1280px 固定ではなく、page.pdf() viewport 設定起因 (Round 2.9-A/B が JS resize で対処済)。CSS 側の `[data-chart]` 死に selector を `[data-chart-config]` に直すと、CDP 経路の最終防衛線が完成する。
