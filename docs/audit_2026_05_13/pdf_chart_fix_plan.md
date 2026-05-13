# PDF Chart 描画問題 修正計画 (2026-05-13)

scope: `src/handlers/survey/report_html/` (read-only audit)
関連: `pdf_visual_review.md`, `G_rendering.md`, `B_data_integrity.md`

---

## #1 Page 4 図 3-1 boxplot 本体空白

- **原因**: `salary_stats.rs:141` の boxplot tooltip `formatter` が JS 関数文字列 (`"function(p){return p.name+...}"`) になっており、`helpers.rs:1076` の inline init では JSON.parse 後そのまま `chart.setOption(config)` するため、formatter は string のまま渡る。setOption は formatter が string 関数文字列の場合エラー扱いせず受理するが、ECharts の boxplot は描画前に tooltip option validate を走らせるブラウザ環境があり、ここで silent fail。さらに `series.data` は単一行 `[[min,Q1,med,Q3,max]]` で yAxis category `["給与レンジ"]` のみ。box の幅は category 軸幅依存だが高さ 160px + IQR 単一行で画面に潰れる。G-P0 #2 と同型。
- **該当**: `salary_stats.rs:138-169`, `helpers.rs:1076-1082`
- **修正案**: (a) formatter を ECharts テンプレ string `"{b}<br/>min/Q1/中央値/Q3/max: {c}"` に置換、または (b) inline init script (helpers.rs:1076) に function-string 復元処理 (`/^function\s*\(/` で eval) を移植 (static/js/app.js 同等)。container 高さは `render_echart_div(..., 200)` まで増やすこと推奨。
- **工数目安**: 0.5h

## #2 Page 8 採用難易度 "ゲージ" 空白

- **原因**: そもそも **gauge chart は実装されていない**。`market_tightness.rs:1013-1068` の `render_tightness_summary` は ECharts gauge を使わず CSS `<div>` で「58/100 採用やや困難」を表示するだけ。視覚レビュアは大きな数値+色付き枠を「ゲージが描画失敗」と誤認した可能性が高い。直下の図 MT-2 4 軸レーダー (`market_tightness.rs:1114-1156`) は ECharts radar で描画されているが page 8 末尾で見切れ改ページしている可能性も。
- **該当**: `market_tightness.rs:1044` (figure caption "図 MT-1") + `1046-1060` (CSS div), `1156` (図 MT-2 render_echart_div)
- **修正案**: 仕様確認が先。本当に gauge を出すなら `series.type:"gauge"` (axisLine + pointer + detail) を追加し figure_caption を MT-1 にぶら下げる。現状の信号機 div で意図通りなら "ゲージ" という用語を rendering 説明文から削除 (review 誤認防止)。レーダー (MT-2) が改ページで見切れる場合は `page-break-inside:avoid` を style.rs に追加。
- **工数目安**: 1h (仕様確認 + 実装 or 文言修正)

## #3 Page 13 図 4-1 雇用形態ドーナツ空白

- **原因**: `employment.rs:66-90` のドーナツ config は正常 (pie, radius `["35%","65%"]`)。container は `render_echart_div(&config.to_string(), 250)` で 250px。問題は `center: ["35%", "50%"]` と `legend.right: "5%", orient: vertical` の組み合わせで legend が広い場合 (雇用形態 6 ラベル × fontSize 10)、pie center 35% 位置でも実 chart area が狭く描画失敗する可能性。加えて helpers.rs:1074 の `if (el.offsetHeight === 0) return;` ガードは print media で section の親要素に `display:none` が一時的に効いていると skip される (G-P2 @page cascade 関連)。Round 12 で render_section_employment が出力直後に section に挿入されておりレイアウト確定前に init が走るタイミング問題は無し。最有力は legend が pie を圧迫しての null 表示。
- **該当**: `employment.rs:66-90`
- **修正案**: `legend.orient` を `"horizontal"` + `"bottom":0` に変更、`series.center` を `["50%","45%"]` に戻す。container 高さも 280 へ。または `legend.right: 10` を `legend.left: "65%"` 明示 + `legend.top:"middle"` で legend 領域固定化。
- **工数目安**: 0.5h

## #4 Page 14 図 5-1 散布図 回帰線急峻 (slope=1267 異常)

- **原因**: `aggregator.rs:501-513` で `scatter_min_max` を構築する際、`r.salary_parsed.min_value`/`max_value` の単位 (時給 vs 月給) を区別せず収集。`linear_regression_points` (aggregator.rs:736-) は yen 単位の混在 (時給=数百〜2000 円 + 月給=10万〜80万円) を含む点群で OLS を計算するため、極小 x (時給) と巨大 y (誤分類で月給) 等の outlier が slope を 1267 に膨張させる。一方 scatter.rs:50-58 では 5-200 万円フィルタを掛けるが、これは **描画用フィルタ後**で、`reg.slope/intercept` は無フィルタの値を使い続けるため (scatter.rs:97 で `reg.slope * x_min_yen + reg.intercept` を採用)、回帰線だけ全データ汚染スケール、点群だけクリーン → 視覚不整合。R²=0.0001 は OLS 自体が異常を吐いている強い証拠。
- **該当**: `aggregator.rs:501-513`, `scatter.rs:92-108`
- **修正案**: aggregator.rs:501 のフィルタ段で `5_0000..=200_0000` (yen) かつ y>=x の条件を `scatter_min_max` 構築時点に適用、または scatter.rs で `filtered_points` から再度 `linear_regression_points` を実行し描画と表 5-1 両方で同一 slope を使う。後者推奨 (集計層に表示制約を逆流させない)。
- **工数目安**: 1h

## #5 Page 20 図 11-2 経験者 vs 未経験可 バー消失

- **原因**: `seeker.rs:152-160` の series.data は `[{value: 経験者}, {value: 未経験可}]` 両方を出力しており、コード上は両バー描画されるはず。視覚で「未経験可 (13件)」のバー消失は **データ側 `inexp.inexperience_avg_salary` が `None`** のケースで `to_man_opt` (line 133) が 0.0 を返した結果、bar 高さ 0 = 不可視となった可能性が最有力。card 表示の "35.4万円" は別経路 (`format_man_yen` line 188-191) で stale or 不整合データを表示している疑い。job_seeker.rs:115-160 の `analyze_inexperience_tag` が `inexp_avg` を返すロジックを再確認すれば判明する。
- **該当**: `seeker.rs:133` (to_man_opt の None→0.0), `seeker.rs:155-157`, `job_seeker.rs:115-160`
- **修正案**: (a) `to_man_opt` が None の場合 series.data から該当 entry を除外し xAxis.data も連動して 1 カテゴリにする (両方欠落しない設計)、(b) card と chart を同一 data source から描画する helper を切り出し整合保証、(c) None 時に `value: null` + `label.formatter` で「データなし」明示。
- **工数目安**: 0.5h

## #6 Page 31 図 19-1 4 象限散布図 空白

- **原因**: `market_intelligence.rs:1675-1758` の 4 象限図は **CSS 絶対配置で実装** (ECharts ではない)。`to_x_pct` (line 1686) は log スケール `10 + 80 * (lc - log_count_min) / (log_count_max - log_count_min)` で、対象自治体が少数 (n<5) かつ count 範囲が狭い (例: 1〜3 件) と全点が x 軸ほぼ同位置に集まる。Y 軸も従業者数 (令和3年経済センサス) が特別区部 836万 vs 他 数万 のレンジで log でも極端、median split (line 1647-1648) も小サンプルで偏る。図そのものは描画されているが点が 1-2 個重なって視覚上「ほぼ空白」となる構造的問題。
- **該当**: `market_intelligence.rs:1640-1758`
- **修正案**: (a) `points.len() < 5` 時は警告文を表示し図を省略 (現状 `is_empty()` のみ check, line 1626)、(b) x/y 軸の log 計算で sample 少時はリニアスケールに fallback、(c) 点サイズを `to_size_px` (line 1707) で最低 14px に引き上げ、(d) 重なり点の jitter (±2-3%) を追加して視認性確保。本図の閲覧文脈 (顧客 CSV 自治体数依存) も note に既に明記済 (line 1663-1667) なので、極小サンプル時の見え方ガード追加が最小コスト。
- **工数目安**: 1h

---

## 横断的所見

- **P0 共通**: `helpers.rs:1076-1082` の inline init は function-string formatter を復元しない (G-P0 #2)。salary_stats #1 とピラミッド (報告外) で再発。app.js 同等の関数復元処理を inline 化することで #1/#3 含む将来再発を一括防止可能。
- **データ単位の一貫性**: #4 は `MEMORY: feedback_unit_consistency_audit` の典型再発パターン (DB カラム/集計単位の混在)。scatter_min_max に時給混入は集計段の単位 guard 不足。
- **CSS chart vs ECharts**: #2 #6 は CSS 実装で ECharts ではない。視覚レビューアの誤認も含むが、図番号がついている以上「描画チャート」として期待されるため、CSS 実装の表現力限界を明示する figure_caption suffix (例: 「(CSS 簡易図)」) で誤認を防げる。
