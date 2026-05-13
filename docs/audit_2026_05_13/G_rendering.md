# G: Rendering (HTML / CSS / ECharts / PDF) Audit

監査日: 2026-05-13
scope: `src/handlers/survey/report_html/`, `src/handlers/{demographics,balance,workstyle,region/karte,trend}*`, `static/css/`, `static/js/`
read-only

## サマリ

過去事故 (人口ピラミッド `stack:"total"` + 負値、formatter 不在) と同種の **未修正パターンを複数検出**。

- **P0 #1**: `region/karte.rs:905-934` 人口ピラミッドが `stack:"total"` + 男性負値で正負キャンセル発生 (過去 10+ ラウンド見逃しと完全同型)。`report_html/demographics.rs` は `barGap:"-100%"` で修正済だが、`region/karte.rs` は未修正。
- **P0 #2**: `report_html/helpers.rs:1087-1099` の inline ECharts 初期化スクリプトは function-string → Function 復元処理を **行わない**。一方 `static/js/app.js` の汎用 init はそれを行う。`report_html` 系 (`survey/report` PDF レポート) でレンダされるチャートでは `axisLabel.formatter: "function(v){...}"` が runtime 評価されず、`demographics.rs:403` (ピラミッド abs ラベル) と `salary_stats.rs:141` (boxplot tooltip) が dead-code。
- **P1**: `render_echart_div` (helpers.rs:132) の attribute エスケープが `'` のみ。`<`/`&` がチャート label に混入すると HTML attribute parse 破綻可能性。
- **P1**: `invariant_tests.rs` に pyramid 系列の不変条件 (`stack` 同一 + 負値混在を禁ずる) test が無い。`region/karte.rs` 再発の構造的原因。
- **P2**: `@page` 二重定義 (`style.rs:56` と `market_intelligence.rs:387`)。cascade 後勝ちで動作するが MI 以外で余白統一できず。
- **P2**: `render_echart_div` helper の薄さ。pyramid 専用 helper 化すれば再発防止に有効。

## 詳細

### P0 #1 — region/karte.rs 人口ピラミッド (再発)

`src/handlers/region/karte.rs:905-906`
```rust
males.push(-get_i64(r, "male_count")); // 負数化して左側に表示
females.push(get_i64(r, "female_count"));
```
`src/handlers/region/karte.rs:922-937` で `"stack": "total"` のまま series 2 本。`stack:"total"` は同名 stack の正負を **加算**するため、男性 -1234 + 女性 +1234 → 累積 0 で**バーが消える** (過去事故と同症状)。

**証拠**:
- 同種事故修正済の `report_html/demographics.rs:411-427` は `barGap:"-100%"` (stack 無し) + `axisLabel formatter Math.abs` で対応。
- `report_html/round12_integration_tests.rs:410` に「K9: pyramid xAxis formatter なし → 男性側が負数のまま表示」と確定バグの記載あり。

**修正案**: `"stack": "total"` を除去し `"barGap": "-100%"` を両 series に追加。`xAxis.axisLabel.formatter` で abs 適用 (P0 #2 と併修)。`xAxis.min`/`max` を ±max で対称化。tooltip の `valueFormatter` も abs() で揃える。

### P0 #2 — formatter string が evaluate されない (report_html 限定)

`src/handlers/survey/report_html/helpers.rs:1087-1099` (report_html inline script):
```js
document.querySelectorAll('.echart[data-chart-config]').forEach(function(el) {
  var config = JSON.parse(el.getAttribute('data-chart-config'));
  config.animation = false;
  config.backgroundColor = '#fff';
  var chart = echarts.init(el, null, { renderer: 'svg' });
  chart.setOption(config);  // ← string formatter は string のまま渡る
```
対する `static/js/app.js` (ダッシュボード用) には `/^function\s*\(([^)]*)\)\s*\{([\s\S]*)\}$/` で関数復元する処理あり (`a(r)` 呼び出し)。report_html はこの処理を持たない。

**影響箇所**:
- `demographics.rs:403` — ピラミッド `xAxis.axisLabel.formatter` が `"function(v){return Math.abs(v).toLocaleString();}"` のまま ECharts に渡り、関数として展開できないため abs 化されない。負値そのまま表示。
- `salary_stats.rs:141` — boxplot tooltip が同型 function-string。

**修正案**: report_html inline script に app.js 同等の関数復元処理を移植するか、formatter を ECharts template string (`"{value}"` 形式) に書き換え。後者推奨 (安全)。pyramid の abs() は値生成側で `data` を絶対値化 + tooltip 別経路で対応可能。

### P1 — attribute escape 不完全

`src/handlers/survey/report_html/helpers.rs:134`
```rust
let escaped = config_json.replace('\'', "&#39;");
```
HTML attribute 値で危険なのは `'`, `"`, `&`, `<`。`&` は entity reference として解釈されるため、市区町村名・産業名に `&` 含むケースでは JSON が破損する可能性。

**修正案**: `&` → `&amp;`, `<` → `&lt;` を追加 (順序: `&` 先、その後 `<`/`'`)。

### P1 — invariant_tests.rs に pyramid pattern test 不在

`report_html/invariant_tests.rs` は失業率/有効求人倍率/HHI 等の数値ドメイン不変条件は検証するが、「ピラミッド series が同名 stack を持たないこと」「pyramid 系列に負値があれば対称 series 構成」等の ECharts 不変条件 test が無い。`region/karte.rs` の再発はこの test 不在が原因。

**修正案**: ECharts JSON を parse して `series` 中で `stack` 値が同一かつ `data` に正負混在がある場合 panic する invariant test を追加。`report_html` と `region/karte` の双方を対象にする。

### P1 — JSON NaN/Infinity

`trend/helpers.rs:138` は NaN/Inf を Null 置換 (OK)。survey report_html 側の `f64` 経路 (`market_intelligence.rs`, `market_tightness.rs`) は値域 validate (`!s.is_nan()` 等) ありで安全。problem 残らず。

### P1 — markLine label.show 修正済

`helpers.rs:207-218` でコメント明記、`helpers.rs:386` 周辺で `label.show=true` 固定。Round 13 で対応済。

### P2 — @page 二重定義 (mi だけ別余白)

`survey/report_html/style.rs:56` `@page { margin: 10mm 8mm 12mm 8mm }` がベース、`market_intelligence.rs:387` で `12mm 14mm` 上書き。MI 以外は 10mm/8mm 余白で確定。意図的ならコメントで明記推奨。`PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md` 参照可。

### P2 — render_echart_div helper 共通化

`report_html/` 配下で 17 箇所 `render_echart_div` 呼び出し。pyramid のように特殊設定 (barGap, abs formatter, 対称 axis) を持つ chart は別 helper (`render_pyramid_echart`) に切り出すと再発防止に有効。`region/karte.rs` も同 helper 経由にできる。

### 補足 — `renderer: 'svg'` 強制

`helpers.rs:1095` で `renderer: 'svg'`。印刷時 chart 見切れ対策と推測。`beforeprint`/`afterprint` で `chart.resize()` 二重呼び出しで stabilize 済。問題なし。

### 補足 — page.pdf() vs window.print()

`docs/PDF_CHART_DOM_AUDIT_2026_05_09.md` 等の存在を確認。詳細経路は未読 (read-only scope 内で時間切れ)。CDP 経由 `page.pdf()` は `@media print` を一部しか honor しない (HTML5 spec)、`window.print()` 経由は完全 honor。survey report PDF 生成経路がどちらかは契約系 (A 領域) の確認に委ねる。

## 優先度サマリ

| 優先度 | 件数 | 領域 |
|--------|------|------|
| P0 | 2 | region/karte.rs pyramid 再発、report_html formatter 評価不在 |
| P1 | 3 | attribute escape、invariant test 不在、(NaN/markLine は対応済) |
| P2 | 2 | @page 二重定義、helper 共通化 |
