# Round 2.8-C: ECharts option 実出力監査

- 監査対象: `data/generated/debug_mi_variant_market_intelligence.html` (3,548 行)
  - 同 HTML は本番 MI variant と同じビルダーが生成 (Rust → Tera テンプレート 経由) する成果物
  - 取得済 `out/round2_7_pdf_review/` には PNG/PDF のみで HTML が無いため、上記ローカル debug 成果物を使用
- 認証情報: 本ファイルには記載しない
- 監査範囲: ECharts option JSON の **実値** vs Round 2 / 2.7 の期待値
- read-only: 実装変更・コミット・push なし
- 編集ファイル: 当ファイル新規作成のみ

---

## 抽出結果

| 項目 | 値 |
|------|----|
| `data-chart-config` 属性数 | 8 |
| パース成功 | 8 / 8 |
| ECharts 初期化スクリプト | `script[3]` 内 1 ヶ所 (`echarts.init(el, null, { renderer: 'svg' }); chart.setOption(config)`) |
| `animation` | `false` (init 時に強制上書き) |
| `backgroundColor` | `#fff` (init 時に強制上書き) |
| renderer | **`'svg'`** (canvas ではない) |

option は inline `setOption({...})` ではなく、`<div class="echart" data-chart-config="...">` 属性に**JSON エンコードされて格納**され、DOMContentLoaded 後に一括 init される設計。

### chart index → 図の対応 (近傍見出しから推定)

| idx | 近傍見出し (mojibake 前) | 想定図番 | type |
|---|---|---|---|
| 0 | 下限給与の分布 (20,000 円刻み) | 図3-2 | bar (histogram) |
| 1 | 下限給与の分布 (5,000 円刻み) - 詳細 | 図3-3 | bar (histogram) |
| 2 | 上限給与の分布 (20,000 円刻み) | 図3-4 | bar (histogram) |
| 3 | 上限給与の分布 (5,000 円刻み) - 詳細 | 図3-5 | bar (histogram) |
| 4 | (見出し検出失敗 / 採用市場) | 図MT-2 | radar |
| 5 | 人材デモグラフィック | 図D-1 | bar (horizontal, 2 series) |
| 6 | 雇用形態分布 | 図4-1 | pie (donut) |
| 7 | 相関分析 (散布図) 下限給与 vs 上限 | 図5-1 | scatter + line(回帰) |

---

## 各 chart の option 期待値 vs 実値

### 図3-2 / 3-3 / 3-4 / 3-5 給与ヒストグラム (chart 0-3)

| 項目 | 期待 (Round 2/2.7) | 実値 | 一致 |
|---|---|---|---|
| `yAxis.min` | `0` | **未指定 (`null`)** | NG |
| `yAxis.scale` | `false` | **未指定 (`null`)** | NG (auto) |
| `yAxis.minInterval` | `1` | **未指定 (`null`)** | NG |
| `series[0].markLine.label.distance` | `8` | **未指定 (`null`)** | NG |
| `series[0].markLine.label.position` | `'insideEndTop'` | **未指定 (`null`)** | NG |
| `series[0].markLine.label.backgroundColor` | 半透明背景指定 | **未指定** | NG |
| `series[0].markLine.label.rotate` | 削除 or 0 | **未指定 (default 0 と思われる)** | 部分一致 |
| `graphic` (中央値・平均・最頻値の近接統合カード) | 近接時に配列生成 | **`graphic: None` (全 4 ヒスト共通)** | NG |

実 markLine `label` は `{fontSize: 10, formatter: "..."}` のみ。距離/位置/背景色は **一切未指定**。Round 1-B で指摘された P0 修正 (yAxis.min=0, label.distance=8) は **builder に反映されていない**。

### 図MT-2 採用市場レーダー (chart 4)

| 項目 | 期待 | 実値 | 一致 |
|---|---|---|---|
| `radar.center` | `["50%","55%"]` | **未指定 (default `["50%","50%"]` と思われる)** | NG |
| `radar.radius` | `"65%"` (or `"60%"`) | **未指定 (default `"75%"`)** | NG |
| `radar.axisName.padding` | `8` | **未指定 (`color`/`fontSize` のみ)** | NG |
| `radar.indicator` | 4軸 | 4軸 (有効求人倍率/欠員補充率/採用評価/離職率) | OK |
| `radar.shape` | `polygon` | `polygon` | OK |

### 図4-1 雇用形態ドーナツ (chart 6)

| 項目 | 期待 | 実値 | 一致 |
|---|---|---|---|
| `series[0].minAngle` | `5` | **未指定 (`null`)** | NG |
| `series[0].radius` | `['35%','60%']` | `['35%','65%']` | 近似 |
| `series[0].center` | センタリング (`['50%','50%']`) | `['35%','50%']` | NG (左寄り) |
| `series[0].label.show` | `true` 全件 | **明示なし** (`fontSize:10, formatter:"{b}\n{d}%"` のみ) | NG (default true だが all-show 強制なし) |

`center: ['35%','50%']` で**左にオフセット**されている。Round 1-B 指摘の「中央が小さい」現象と整合。`minAngle` 未指定なので小スライス (パート以下) は label が他と重なって判読困難。

### 図5-1 散布図 (chart 7)

| 項目 | 期待 | 実値 | 一致 |
|---|---|---|---|
| `xAxis.show` | `true` (強制) | **未指定 (default `true`)** だが axisLine/axisTick は明示されず | 部分一致 |
| `xAxis.axisLine.show` | `true` | **未指定** | NG (default に依存) |
| `xAxis.name` | あり | `"下限給与(万円)"` (mojibake) | OK |
| `xAxis.min` / `max` | `dataMin..dataMax` 全域 | `min:19.0, max:41.0` | OK (明示) |
| `yAxis.min` / `max` | `0` または整備値 | `min:24.0, max:62.0` | NG (`0` 始まりではない) |
| 回帰 series.data | `[xMin..xMax]` 全域 (連続点) | **`[[19.0, 22.24], [41.0, 61.37]]` の 2 点のみ** | OK (線形回帰なら 2 点で全域 OK だが ECharts line series は端点間を直線で結ぶので**実は全域描画される**) |

Round 1-B で「回帰線が右端からのみ描画」と指摘されたが、option 上は xMin=19 から xMax=41 の 2 点で正しく定義されている。**option は正しい**。すなわち PDF で「右端のみ」に見える原因は**レンダリング側**(SVG renderer の clip / grid.left の不足 / printToPDF タイミング)の可能性が高い。

ただし xAxis の axisLine/axisTick の明示指定が無いため、**印刷 CSS で軸線が消えている**可能性は残る (Worker D 領域)。

### chart 5 (人材デモグラフィック横棒)

`barWidth` 未指定。Round 1-B 指摘の「棒がドット幅」と整合。`barCategoryGap` も未指定。

---

## graphic 統合カード

全 8 chart で `graphic` 属性は**完全に不在**。ヒストの中央値/平均/最頻値が近接した場合に統合カードを生成する設計があるはずだが、**builder からの出力に含まれていない**。

→ ヒスト4枚すべてで **graphic 配列が生成されていない**。近接時のみ生成する条件分岐すら呼ばれていないか、その実装自体が未着手。

---

## 仮説判定

**仮説 B: option が間違っている (= builder 修正未反映)** が**主因**。

具体的には:
- ヒスト 4 枚: yAxis.min / scale / minInterval / markLine.label.distance/position/backgroundColor / graphic 統合 すべて **未実装**
- レーダー: center / radius / axisName.padding すべて **未実装**
- ドーナツ: minAngle / center センタリング / label.show 強制 すべて **未実装**
- 散布図 (chart 7): yAxis.min=0 が **未実装**

**仮説 C (option 正しいが render で壊れる)** が成立するのは、散布図の xAxis 軸線消失と回帰線の見た目だけ。option 上は 2 点で全域指定済なので、line series の端点描画は SVG renderer 側で生じる可能性。ただしこれも `xAxis.axisLine.show: true` の **明示指定漏れ**で説明可能 → **仮説 B の延長**。

判定: **B 主、C 補助** (散布図軸線のみ render 起因の可能性残)

---

## 次ラウンド推奨修正対象

優先度順:

1. **Worker A/B 領域 (Rust builder option 生成側)**
   - `yAxis.min: 0`, `yAxis.scale: false`, `yAxis.minInterval: 1` をヒスト 4 枚に強制付与
   - `markLine.label.distance: 8`, `position: 'insideEndTop'`, `backgroundColor` (半透明) を全 markLine データに付与
   - `radar.center: ['50%','55%']`, `radius: '65%'`, `axisName.padding: 8` を MT-2 に付与
   - donut `series[0].minAngle: 5`, `center: ['50%','50%']` (左寄り `35%` を撤回), `label.show: true`
   - scatter `xAxis.axisLine.show: true`, `axisTick.show: true` 明示, `yAxis.min: 0`
   - `barWidth: 18` または `barCategoryGap: '40%'` 明示 (chart 5)
   - graphic 統合カード生成ロジックの**実装そのもの**(現状は未着手)

2. **Worker D 領域 (render/CSS/timing) — 補助**
   - 散布図の axisLine が SVG renderer で消える件は、上記 axisLine.show 明示後に**再観測**して切り分け
   - `printToPDF` 前の `chart.resize()` 完走待機 (現状 `beforeprint` で resize は呼ぶが `await` していない)

3. **Round 1-B レポートとの突合**
   - Round 1-B の P0 修正 5 項目はいずれも **builder 反映されていない**ことが本監査で確定
   - つまり Round 2 (修正実施)→ Round 2.7 (PDF レビュー) のサイクルで、修正が **commit / deploy されていない** か、別ブランチに留まっている疑い → git log / diff 確認推奨

---

## 監査メモ

- 本 HTML は debug 用ローカル生成成果物。本番 (`hr-hw.onrender.com/report/insight`) と同じテンプレートエンジン (Tera) + 同じ option ビルダー (Rust) からの出力前提で扱った。
- 本番 HTML の取得は本ラウンドでは実施せず (既存 `out/round2_7_pdf_review/` に PDF/PNG のみ、HTML 不在)。本番取得が必要なら次ラウンドで Playwright + `/report/insight` HTML スナップショット取得を別タスク化推奨。
- 文字列フィールドの mojibake は cp932/utf-8 取り違えによる表示問題のみで、JSON 構造解析には影響しない。
