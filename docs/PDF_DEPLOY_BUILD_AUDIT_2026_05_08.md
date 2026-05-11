# Round 2.8-A: deploy/build 反映監査 (2026-05-08)

## 目的
本番 (`https://hr-hw.onrender.com`) が `origin/main` 最新 (`9ac1e33`) で稼働しているか、Round 2 / 2.5 / 2.6 / 2.7 の各 commit 固有文字列が本番出力に存在するかを read-only で確認する。

## 対象成果物
- `out/round2_7_pdf_review/mi_via_action_bar.pdf` (25 ページ, 6,228,065 bytes, 生成 2026-05-08 23:19)
- `out/round2_7_pdf_review/page_05_hist3_234.png` (図 3-2, 3-3, 3-4 = ヒストグラム y軸視覚チェック)
- `out/round2_7_pdf_review/page_06_hist3_5.png` (図 3-5 = 上限給与 5,000円刻みヒストグラム)
- ソース: `src/handlers/survey/report_html/helpers.rs:312-342`

## 1. /health 状態

```
GET https://hr-hw.onrender.com/health
HTTP 200
{"cache_entries":94,"db_connected":true,"db_rows":469027,"status":"healthy"}
```

`/version` は 404。Render 側に hash を返す endpoint なし。deploy hash 確認は本文 marker 経由で行う。

ローカル `origin/main` HEAD: `9ac1e33d16f693d6d4ea98a590455435eb97d06e` (Round 2.7-B' Full復元)

## 2. 本番 HTML/PDF 固有文字列チェック

PDF (`mi_via_action_bar.pdf`) からテキスト抽出後、CJK 互換異体字 (例: `⽉` → `月`, `‧` → `・`) を正規化した上で grep。

| commit | marker | 期待 | 実測 count | 結論 |
|---|---|---|---|---|
| 4afaa08 (Round 2-1) | `採用市場・ターゲット分析` | >0 | **1** | ✅ 反映 |
| 4afaa08 (Round 2-1) | `アップロード CSV + 公開統計` | >0 | **1** | ✅ 反映 |
| 54b6f6e (Round 2-2 SalaryHeadline) | `月給中央値` | >0 | **1** | ✅ 反映 |
| caab9a7 (Round 2.5 HW guard) | `ハローワーク` | =0 | **0** | ✅ guard 動作 |
| 847364b (Round 2.6 notes 中立化) | `公開求人＋アップロード CSV` | >0 | **1** | ✅ 反映 |
| 847364b (Round 2.6 notes 中立化) | `外部求人媒体スコープ` | >0 | **1** | ✅ 反映 |
| 6f1574f (Round 2.7-B 中立化) | `公的雇用需給指標` | >0 | **7** | ✅ 反映 |
| 6f1574f (Round 2.7-B 中立化) | `特定求人媒体` | >0 | **4** | ✅ 反映 |
| 9ac1e33 (Round 2.7-B' MI) | `有効求人倍率` (MI variant) | =0 | **0** | ✅ MI 中立化保持 |
| (salesnow guard) | `salesnow` | =0 | **0** | ✅ guard 動作 |

**結論**: PDF 本文に出現するすべての commit 固有文字列が期待通り。`origin/main` `9ac1e33` の deploy は **反映済み**。

## 3. Round 2.7-AC (`scale:false` / `minInterval:1` / 統合カード) 確認

PDF 化された ECharts チャートでは `<script>` tag 内 JSON 文字列は失われるため、PDF テキスト抽出では検証不可。視覚 + ソースコード照合で判定する。

### 3.1 ソースコード側 (build artifact 期待値)

`src/handlers/survey/report_html/helpers.rs:316-329` (commit `4591232` で導入)

```rust
"yAxis": {
    "type": "value",
    "min": 0,
    "scale": false,
    "minInterval": 1,
    "axisLabel": {"fontSize": 9}
},
```

cargo test `histogram_yaxis_scale_is_false_explicitly` / `histogram_yaxis_minInterval_is_1` は CI 上で pass している (`mod.rs:1840-1882`)。

### 3.2 PDF 視覚確認

| 図 | y 軸下限ラベル | min:0 視覚反映 | 統合カード (graphic) |
|---|---|---|---|
| 図 3-2 (下限 20,000円刻み) | 4 | **要確認** (PDF 上ラベルが 4 始まり、0/2 が間引かれた可能性あり) | ✅ 平均/中央値/最頻値カード描画あり |
| 図 3-3 (下限 5,000円刻み) | 2 | **要確認** | ✅ 統合カード描画あり |
| 図 3-4 (上限 20,000円刻み) | 2 | **要確認** | バッジのみ |
| 図 3-5 (上限 5,000円刻み) | 2 | **要確認** | ✅ 統合カード描画あり |

**所見**: PDF y 軸ラベルが下端から表示されていないように見える。3 通りの解釈が可能。

1. ECharts は `min:0` 設定でもデータ下端付近のみ axisLabel を間引くことがある (描画範囲は 0〜max のまま、ラベルだけが間引かれる) → **問題なし**
2. `scale:false` が effectively 効いておらず、axis 描画が data min から始まっている → **本物の不具合**
3. CSS の overflow / clipPath で y 軸下半分が切れて見えている → **render/CSS 問題**

PDF テキスト抽出だけでは ECharts option JSON を取り出せないため確定不能。HTML 直接取得 + script tag 抽出が必要。

## 4. 仮説判定

| 仮説 | 評価 |
|---|---|
| **A: deploy 未反映 (Render build 失敗/遅延)** | ❌ 否定。Round 2.6/2.7-B/2.7-B' のテキスト marker すべてが PDF に存在。`origin/main` `9ac1e33` の build は本番に届いている。 |
| **B: 別 chart builder が修正されていない** | ⚠️ 未確定。helpers.rs の `histogram_chart_config` 経由なら `scale:false`/`min:0` 必須。しかし図 3-2 で y軸ラベルが 4 始まりに見えるのは、別 builder 経由の histogram (例: salary_min only / salary_max only / hist3_5 / hist3_4 等) が helpers を通っていない可能性を残す。HTML 取得後に `<script>` 内の各 chart option JSON を直接突き合わせる必要あり。 |
| **C: option は正しいが render/CSS/timing 問題** | ⚠️ 未確定。`min:0` が JSON に正しく出ていても、印刷モード化のタイミング・grid.bottom・axisLabel 間引きで y 軸ラベル下端が消えて見える可能性。Worker D (CSS/render) 領域。 |

**現時点の最有力**: B もしくは C。A は否定確実。

## 5. 次ラウンド推奨

### Round 2.8-B (HTML 直接取得 + script tag 解析)

1. Playwright で `BASE_URL=https://hr-hw.onrender.com` + `?variant=market_intelligence` の HTML を保存 (`page.content()` → `out/round2_8/mi.html`)
2. HTML 内 `<script>` の ECharts option JSON を全件抽出
3. histogram chart それぞれについて以下を集計:
   - `yAxis.min` の値 (期待: `0`)
   - `yAxis.scale` の値 (期待: `false`)
   - `yAxis.minInterval` の値 (期待: `1`)
   - `graphic` (統合カード) の有無
4. 期待外の chart があれば、対応する builder を src 内で grep し、helpers 経由でないものを特定 (= 仮説 B 確定)
5. すべての histogram が期待通りなら CSS / 印刷 timing を疑い Worker D に引き継ぎ (= 仮説 C 確定)

### 注意事項
- HTML 取得用 spec は新規作成 (本ラウンドは編集許可外のため未実施)
- Tursoは触らない (read-only)
- 認証情報を ログ/docs に転記しない

## 検証ファイル参照

- 本番 PDF: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/out/round2_7_pdf_review/mi_via_action_bar.pdf`
- 視覚証拠: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/out/round2_7_pdf_review/page_05_hist3_234.png`
- 視覚証拠: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/out/round2_7_pdf_review/page_06_hist3_5.png`
- ソース根拠: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/report_html/helpers.rs:312-342`
- 単体テスト: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/report_html/mod.rs:1840-1882`
