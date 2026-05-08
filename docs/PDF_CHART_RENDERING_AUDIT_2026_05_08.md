# PDF チャート描画品質監査レポート (Round 1-B)

- 監査対象: `out/real_csv_pdf_review_20260508/indeed-2026-04-27.pdf` (33ページ / 7.7MB)
- 補助入力: `histogram_crops_fullres/` 既存クロップ + 当監査で再レンダー (DPI200 / matrix 3-4x)
- 監査範囲: ヒストグラム / 散布図 / 棒グラフ / レーダー / ダンベル比較 / MarketIntelligence系チャート/表
- 監査担当: Round 1-B Visual Audit (read-only)
- 注記: 当ファイルは新規作成のみ。実装・コミット・push は一切行っていない。Round 1-A 出力 (`out/pdf_visual_audit_20260508/`) は不在のため独自レンダーで代替。

---

## サマリ

| 区分 | 件数 |
|------|------|
| 監査対象チャート (`charts: 9` + Sparkline + Donut + Treemap + Radar 含む) | 14 件 |
| 表 (`tables: 27`) | 27 件（参照のみ。表セルのスパークライン4種は P2 として計上） |
| **P0 (切れ・読めない)** | **5 件** |
| **P1 (読めるが見栄え悪い)** | **5 件** |
| **P2 (微調整)** | **4 件** |
| ハードNGヒット (`hardNgHits`) | 0 件（メトリクス側は無検出） |

---

## グラフ別表

| chart_id | page | type | issue | severity | 原因仮説 | 修正候補 |
|----------|------|------|-------|----------|----------|----------|
| 図3-1 (給与統計サマリ IQR シェードバー) | 5 | レンジバー | 異常なし。IQR / 中央値 / 平均位置がいずれも判読可能 | OK | — | — |
| 図3-2 (下限給与ヒスト 20K刻み) | 6 | bar histogram | Y軸が `30` 始まり (0始まりでない) → 棒の高さ差が誇張される。縦書き「最頻値」「平均」ラベルが reference markLine と重なり判読不能。右端に小さい青いアーティファクト | **P0** | yAxis `min` 未指定 (auto = dataMin) / markLine.label に `distance` (offset) 未設定で `rotate:90` ラベルが線の上に重なる | yAxis `min: 0`, markLine.label `distance: 8` + `position: 'insideEndTop'` または rotate 削除 |
| 図3-3 (下限給与ヒスト 5K刻み) | 6 | bar histogram | Y軸 `15` 始まり、縦書きラベル線重なり (図3-2 と同種) | **P0** | 同上 | 同上 |
| 図3-4 (上限給与ヒスト 20K刻み) | 6 | bar histogram | Y軸 `15` 始まり、縦書きラベル線重なり | **P0** | 同上 | 同上 |
| 図3-5 (上限給与ヒスト 5K刻み) | 7 | bar histogram | Y軸 `10` 始まり、最上位ビン (25) が上端でクリップの可能性、縦書きラベル線重なり | **P0** | 同上 + grid.top 不足 | yAxis `min: 0`, `max: 'dataMax'` 強制 + grid.top 余白増 |
| 図5-1 (月給下限×上限 散布図) | 14 | scatter | **X軸の目盛・タイトル両方が完全に消失**。回帰直線が右端 (~32万円〜) からしか描画されておらず全域に伸びていない。データ点も Y=33 付近で下端カット | **P0** | xAxis `show: false` または axisLabel/axisLine `fontSize: 0` の誤設定 / 回帰線 series の data 配列が右端2点のみ / xAxis range が dataMin 始まりで grid.bottom 不足 | xAxis 強制 show, regression series を [xMin..xMax] 全域で生成、yAxis `min: 0` または `min` を整備 |
| 図MT-2 (採用市場 4軸レーダー) | 8 | radar | レーダー中心が右にシフトし「有効求人倍率(1.33倍)」「欠員補充率(30%)」のみ表示。「離職率」「新着比率」軸ラベルが左カット | **P1** | radar `center` デフォルト or grid 未調整 / `name.padding` 不足 / コンテナ width が短い | radar `center: ['50%','55%']`, `radius: '60%'`, axisName `padding: 8` |
| 図D-1 (人口ピラミッド) | 10 | diverging bar | 凡例「女」が「女性」から欠ける可能性 (右端カット気味) | **P1** | legend.right padding 不足 | legend `right: 12`, `itemGap` 確保 |
| 図4-1 (雇用形態ドーナツ Top 6) | 12 | donut | 99.63% を占める「正社員」のラベルが描画されず、0.37% の「パート」のみ吹出ラベル。ドーナツ自体が空白の中央に小さく表示 | **P1** | label.show 条件が `value > threshold` で大スライスを抑制 / radius 設定が小さすぎ | label `show: true` 全件、radius `['35%','60%']`, center センタリング |
| 図4-2 (雇用形態別給与水準横棒) | 13 | horizontal bar | 棒の太さがほぼ 0 でドット表示にしか見えない。値ラベルとカテゴリ名のみ可読 | **P1** | barWidth 自動計算が 1 カテゴリあたり数px / yAxis category gap 過大 | series `barWidth: 18` または `barCategoryGap: '40%'` 明示 |
| 図10-1 (訴求タグツリーマップ) | 18 | treemap | 表10-1 には 12+ タグあるのに、ツリーマップは 3 セル (経験者歓迎 / 育成環境 / 駅近) のみ | **P1** | treemap `levels[0].itemStyle.gapWidth` 大 + visual min size threshold で小セル非表示 / data 配列を top-3 に絞っている | data 配列を top-12 に拡張、`leafDepth: 1`, label fontSize 動的調整 |
| 表内スパークライン (差額バー) | 16 | inline bar | バーが PDF zoom で 1-2px 幅、ほぼ視認不能 | P2 | 表セル幅に対する SVG height/width が小 | sparkline 高さ 12→16px, 太さ 4→6px |
| 表内スパークライン (件数バー) | 17 | inline bar | 同上 | P2 | 同上 | 同上 |
| 表内スパークライン (観測指標) | 19 | inline bar | 同上 | P2 | 同上 | 同上 |
| 表内スパークライン (構成比) | 20 | inline bar | 同上 | P2 | 同上 | 同上 |

---

## 共通原因仮説 (top 3 + 補助)

1. **yAxis `min` 未指定 (auto = dataMin)** — 図3-2〜3-5 が全て 0 始まりでなく、ヒストグラム本来の「分布の高さ比較」を歪める。`min: 0` を全ヒストに強制すべき。
2. **markLine `axisLabel.rotate: 90` のオフセット不足** — 縦書き「最頻値」「平均」が reference 線そのものに重なる。`distance` または `offset` 指定で線から数px 離す、もしくは rotate を 0 に戻し位置を `'insideEndTop'` に。
3. **`xAxis.show` 抑制 / regression series 部分描画** — 図5-1 散布図で X軸欠落 + 回帰線が右端のみ。axis show を強制 + 回帰線データを全域 [xMin..xMax] で生成する必要がある。

補助仮説:
- **コンテナ width / `grid.right` `grid.left` の余白不足** — レーダー中心オフセット、人口ピラミッド凡例カット、ドーナツ中央寄り全て同根の可能性。レポート用印刷 CSS で `@page` size と body padding が二重に効き、ECharts container width が想定より縮んでいる疑い (印刷CSS カスケード罠 — 過去事故記録あり)。
- **canvas vs SVG レンダラー差** — PDF 化 (puppeteer/printToPDF) で canvas のラスタが粗く、スパークラインが 1-2px に潰れる。SVG レンダラー (`renderer: 'svg'`) への切替で表内グラフは劇的に改善する見込み。
- **PDF 生成タイミング (ECharts 再描画未完了)** — 一部チャートで axis label が消える現象は `chart.resize()` 完走前に印刷ダイアログ発火している疑い。`waitForFunction(() => echartsInstance._zr.painter.refreshHover === undefined)` 等で完了待機を保証すべき。
- **donut / treemap の label 表示条件** — `value < threshold` で非表示にする実装が、メイン要素 (99.63%) を逆に隠す副作用を生む。閾値ロジックを再検討。

---

## 補足: 監査手順 (再現性確保)

1. PyMuPDF で `indeed-2026-04-27.pdf` を 1〜33 ページ全て DPI 200 で再レンダー → `Temp/pdf_audit_round1b/page_NN.png`
2. 図3系 / 図5-1 / 図MT-2 は matrix 3-4x で部分クロップ (`*_chart_top.png`, `*_radar_zoom.png`)
3. メトリクス (`indeed-2026-04-27.metrics.json`) と margin サマリ (`pdf_quality_summary.json`) を突合
4. P0/P1/P2 の分類は「PDF 等倍 (A4 縦) で営業先に提示できるか」基準で人間判断

監査者注: チャート描画の根本対策は ECharts オプション側 (yAxis.min, axisLabel.distance, xAxis.show, label.show, renderer: 'svg') に集約される。次ラウンド P0 修正の最小構成は5項目。
