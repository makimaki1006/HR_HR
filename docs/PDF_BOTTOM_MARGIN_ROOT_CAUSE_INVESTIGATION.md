# PDF 下端余白 根本原因調査 (read-only)

調査日: 2026-05-06
担当: Claude (Opus 4.7) — 監査ラウンド
スコープ: read-only。実装変更なし、PDF 再生成なし、commit/push なし。
対象 PDF: `out/print_review_p1b/mi_default_p1b.pdf`、`out/print_review_p1b/mi_v2a_explicit_margin.pdf`
対象 HTML: `out/print_review_p1b/prod_html_market_intelligence.html` (probe 取得済本番 HTML、348,954 bytes)
対象ソース: `src/handlers/survey/report_html/style.rs`、`src/handlers/survey/report_html/market_intelligence.rs`、`e2e_print_verify.py`、`gen_survey_pdf.py`

---

## 1. 総合結論

### 最有力仮説: **B + 仕様誤解 (確信度: 高)**

ユーザーが計測している「PDF 実測下端マージン 11.4pt」は**コンテンツの下端ではなく `@page @bottom-left/@bottom-right` margin box 内のフッター文字列の下端**を計測している。これは CSS @page margin 仕様上の正常挙動であり、@page margin を 12mm に増やしても**フッター文字列は引き続き margin box 下端付近に配置される**ため見かけ上「変化していない」と認識された。

実コンテンツの下端は別測定で **35-40pt (12-14mm)** に到達しており、これは MI_STYLE_BLOCK の `@page { margin: 12mm 14mm }` または style.rs の `@page { margin: 10mm 8mm 12mm 8mm }` (cascade 後勝ちで MI 側 12mm) と概ね整合する。

つまり P1-B 修正自体は CSS 上は機能している可能性が高いが、

1. ページによってコンテンツが下マージン領域に **2-7mm オーバーフロー**している (page 7: 37.1pt = 13.1mm、page 12: 36.7pt = 12.9mm、page 16: 35.0pt = 12.3mm — いずれも 12mm 未満の余白)
2. ユーザーが計測しているのが margin box 内フッターのため、改善が体感されない

### 副次仮説: **A + C (確信度: 中)**

PDF 生成側 `e2e_print_verify.py:121-126` で `page.pdf({margin: {top:'10mm', bottom:'18mm', left:'10mm', right:'10mm'}})` と Playwright margin を**明示指定**している。この場合:

- Chromium 仕様: `margin` 引数指定時は CSS `@page margin` を上書き
- 実コンテンツ最下端が 35-40pt = 12-14mm 程度であることから、`bottom: 18mm` (= 51pt) が**完全には反映されておらず**、実効下マージンは **約 13mm**
- これは `@page` margin box (フッター描画用) と Playwright margin の合成挙動による Chromium バグ的振る舞いの可能性

ただし両 PDF (default vs explicit margin) の MediaBox がほぼ同じで実コンテンツ位置もほぼ同じことから、margin 引数の有無による差は微小 (page 7 で 37.1pt vs 37.3pt 等)。

### 仮説 D は否定 (確信度: 高)

`@media print` で `html, body { margin: 0; padding: 0 }` は MI_STYLE_BLOCK と style.rs L686 の両方で `!important` 付きで適用されており、`min-height` 等の overflow 強制要素は確認できなかった。

---

## 2. PDF サンプル測定

### 2.1 全ページ最下端要素 (フッター除外、コンテンツのみ)

`out/print_review_p1b/mi_default_p1b.pdf` (19 ページ、A4 portrait 594.96 x 841.92pt)
Producer: `Skia/PDF m143` (Chromium)

| Page | content_bottom (pt) | content_bottom (mm) | y1 (pt) | footer_y1 (pt) | text 抜粋 |
|------|---------------------|---------------------|---------|----------------|-----------|
| 6 | 48.9 | 17.3 | 793.0 | 830.8 | 「→次セクションでは、この採用市場逼迫度を踏まえた…」 |
| 7 | **37.1** | **13.1** | 804.8 | 830.8 | 「大学 3,102,649 人 (38.0%)」 |
| 8 | 68.5 | 24.2 | 773.4 | 830.8 | 「パート・アルバイト 15 20.0万円 20.0万円」 |
| 9 | 43.0 | 15.2 | 798.9 | 830.8 | 「1 東京都 21 38.9%」 |
| 10 | 45.5 | 16.1 | 796.4 | 830.8 | 「調整は別途要検討）。」 |
| 11 | 45.0 | 15.9 | 796.9 | 830.8 | 「3 株式会社サンプル47 50.0万円 1」 |
| 12 | **36.7** | **12.9** | 805.2 | 830.8 | 「12 株式会社東京海上日動キャリアサービス…」 |
| 13 | 78.3 | 27.6 | 763.7 | 830.8 | 「281 名 ・ +10.6%」 |
| 14 | 52.7 | 18.6 | 789.2 | 830.8 | 「6 Ｆｕｎ　Ｓｐａｃｅ株式会社…」 |
| 15 | 137.7 | 48.6 | 704.2 | 830.8 | (セクション末尾の余白多め) |
| 16 | **35.0** | **12.3** | 806.9 | 830.8 | 「東京都 新宿区 179.8 ¥1,226 104.0 179.8」 |
| 17 | 56.0 | 19.8 | 786.9 | 830.8 | 「国勢調査 OD 行列 [実測]。Sankey 可視化は…」 |
| 18 | 60.2 | 21.3 | 782.7 | 830.8 | 「生活コスト・最低賃金: 参考指標…」 |
| 19 | 84.4 | 29.8 | 758.5 | 830.8 | 「2. 生成元: 株式会社For A-career…」 |

(`mi_v2a_explicit_margin.pdf` も実質同じ。MediaBox が +1pt 程度違うだけ)

### 2.2 フッター位置 (margin box)

全ページで footer_y1 = **830.8pt** (= page_h - 11.2pt = page_h - 4.0mm)
これはユーザーが計測している「下端マージン 11.2pt」の正体。

### 2.3 主要観察

1. **コンテンツ最下端が 12mm 未満まで到達しているページは 3 つ**: page 7 (13.1mm)、page 12 (12.9mm)、page 16 (12.3mm)
2. これらはすべて**テーブル行 (`mi-rank-table` または類似 ranking table)** の最後の行
3. フッター位置は全ページで一定 (margin box の仕様通り)

---

## 3. HTML 構造調査

### 3.1 該当要素の確認

page 7 「大学 3,102,649 人 (38.0%)」 → 学歴別人口テーブル (demographics.rs)
page 12 「株式会社東京海上日動キャリアサービス…」 → SalesNow 企業ランキングテーブル (salesnow.rs)
page 16 「東京都 新宿区 179.8 ¥1,226 …」 → MarketIntelligence 採用市場逼迫度ランキング (market_intelligence.rs `mi-rank-table`)

### 3.2 テーブル行の break-inside 設定

MI_STYLE_BLOCK 内 (market_intelligence.rs:393-397):

```css
.mi-rank-table thead { display: table-header-group; }
.mi-rank-table tr, table.mi-rank-table tr {
  break-inside: avoid; page-break-inside: avoid; page-break-after: auto;
}
.mi-rank-table tbody { page-break-inside: auto; }
```

行単位で `break-inside: avoid` は適用されている。問題は**最後の行が次ページに送られず、現ページ下マージン領域に押し込まれる**ケース。これは Chromium の break algorithm 弱点で、CSS だけでは完全制御できない。

### 3.3 @page 宣言の重複

prod_html_market_intelligence.html 内に **`@page` ルールが 2 つ**実在 (コメント中の `@page` 言及を除く):

| # | 出力元 | offset | margin |
|---|--------|--------|--------|
| 1 | style.rs L56-70 | 1955 | `10mm 8mm 12mm 8mm` (T R B L) + @bottom-left/right |
| 2 | MI_STYLE_BLOCK (market_intelligence.rs:351) | 295286 | `12mm 14mm` (T-B / L-R) |

CSS spec によれば、複数 `@page` は cascade で merge され、同一プロパティは source order 後勝ち。**MI 側が後で出力されるため `margin: 12mm 14mm` が勝つ**が、`@bottom-left/right` (フッター) は MI 側未定義のため style.rs 側が継続適用される。

---

## 4. CSS 評価 (現状)

### 4.1 @page

| | style.rs (基底) | MI_STYLE_BLOCK (上書き) |
|---|----------------|-------------------------|
| size | A4 portrait | A4 portrait (重複指定、影響なし) |
| margin | `10mm 8mm 12mm 8mm` | `12mm 14mm` (= 上下12mm 左右14mm、後勝ち) |
| @bottom-left | フッター文言 | (未定義 → style.rs が継続) |
| @bottom-right | "Page X / Y" | (未定義 → style.rs が継続) |

### 4.2 @media print 内 body

style.rs L675-691 (`!important` 付き):
```css
@media print {
  body { padding: 0 !important; margin: 0 !important; ... }
}
```

MI_STYLE_BLOCK L355-360 (`!important` 付き):
```css
@media print {
  html, body { margin: 0 !important; padding: 0 !important; ... }
}
```

両者整合。body padding/margin は印刷時 0 で問題なし。

### 4.3 break-inside / break-before / break-after

- `.exec-summary { page-break-after: always }` (style.rs:712) — Executive Summary 後にページブレーク強制
- `.section.page-start, .section.print-page-break { page-break-before: always }` (L708)
- `.summary-card, .kpi-card, ..., .hw-enrichment-table tr` に `page-break-inside: avoid` (L715-717)
- `.mi-rank-table tr` に `break-inside: avoid; page-break-inside: avoid; page-break-after: auto` (MI_STYLE_BLOCK:394-396)

### 4.4 css 仕様上のリスク

`page-break-after: auto` は「自然な改ページを許容」する指定で、**現ページに収まらない場合に次ページに送る**動作。ただし Chromium の break algorithm は最後の数行を**現ページに無理矢理詰める傾向**があり、これが実コンテンツが下マージン 12mm 未満まで到達する要因と考えられる。

---

## 5. PDF 生成側設定

### 5.1 検出されたスクリプト

| ファイル | 行 | margin 設定 | preferCSSPageSize |
|----------|-----|-------------|-------------------|
| `e2e_print_verify.py` | 121-126 | `top:10mm, bottom:18mm, left:10mm, right:10mm` | 未指定 (= False) |
| `gen_survey_pdf.py` | 108 | `top:8mm, bottom:8mm, left:10mm, right:10mm` | 未指定 (= False) |
| `tests/e2e/_print_visual_review.spec.ts` | 63 | margin 未指定 | 未指定 |
| `tests/e2e/market_intelligence_print_theme.spec.ts` | 420 | margin 未指定 | 未指定 |

### 5.2 ad-hoc PDF (out/print_review_p1b/) の生成元

`mi_default_p1b.pdf` および `mi_v2a_explicit_margin.pdf` は前回ラウンドの worker が ad-hoc に生成したもので、コミット済スクリプトには対応コードなし (grep でも未ヒット)。
PDF metadata の Producer は `Skia/PDF m143` (Chromium) で、ファイル命名パターンと P1-B レビューラベルから推察して、worker がインタラクティブに `page.pdf({format:'A4'})` (default) と `page.pdf({format:'A4', margin:{bottom:'12mm'}})` (explicit) を比較生成した可能性が高い。

### 5.3 Chromium PDF generator の仕様

[Playwright docs (page.pdf)] および Chromium 実装より:

| シナリオ | 結果 |
|----------|------|
| `margin` 引数指定 | CSS `@page margin` を**完全に上書き** |
| `margin` 引数なし、`preferCSSPageSize: true` | CSS `@page margin` を**尊重** |
| `margin` 引数なし、`preferCSSPageSize: false` (default) | デフォルト約 ~10mm (環境依存) |

`@bottom-left/@bottom-right` margin box は **Chromium が完全実装している** (CSS @page level 3 spec 準拠)。よって:

- フッター文字列が描画されている事実 = @page margin box が機能している
- フッター下端余白 11.2pt = margin box 内テキストの底辺基準配置 (margin box の高さの中で `vertical-align: middle` 等の規定値挙動)

---

## 6. 仮説検証結果

| 仮説 | 内容 | 結論 |
|------|------|------|
| A | PDF 生成側 `preferCSSPageSize: false` で CSS @page 無視 | 部分的に正 (margin 引数が CSS を上書き)。ただしフッター描画は機能。 |
| B | コンテンツが overflow して下端まで描画 | 正 (page 7/12/16 で 12mm 未満まで到達) |
| C | 最下部要素 break-inside 不足 | 部分的に正 (`.mi-rank-table tr` には適用済だが Chromium break algorithm の限界) |
| D | `body { min-height }` 等で強制下端まで伸ばす | 否定 (該当 CSS なし) |
| **真因** | **ユーザー計測対象 = フッター文字列下端 (= margin box 内位置 = CSS spec 通り)** | **計測誤認** |

### 真因の詳細

ユーザーが「PDF 実測下端マージン 11.4pt」と認識しているのは、PDF テキスト抽出時に**最下端ブロック = フッター "Page X / 19 株式会社For A-career..." の下端 y 座標**が常に **page_h - 11.2pt** 付近になるため。

これは **CSS `@page` の `@bottom-*` margin box 内テキストの規定配置位置**であり、@page margin を 8mm → 12mm → 18mm に変えても **margin box 内のフッター文字列の下端余白は概ね 4mm 前後で一定**。

P1-B 修正で MI_STYLE_BLOCK に `@page { margin: 12mm 14mm }` を追加した結果、style.rs の base margin (`10mm 8mm 12mm 8mm`) と比較してコンテンツ領域は実際に変化している (上 10→12mm、左右 8→14mm)。下マージンは元から 12mm で同じ。よって**下端は変化なしが正常**。

---

## 7. 修正案 (優先度付き)

### P0 修正案 — 計測方法の是正 (実装不要)

**対象**: ユーザー / レビュアー側の認識合わせ
**内容**: PDF 下端マージンの判定基準を以下のいずれかに変更:

1. **コンテンツ最下端 (フッター除外)** = 「テーブル/段落/見出し等の本文要素の y1 max」
2. **フッター上端** ≈ y0 of "Page X / Y" block (page_h - 21pt 付近)

期待効果: 「修正前 11.4pt → 修正後 11.15pt 改善なし」という誤認を解消。実際には **本文最下端は 12-30mm 範囲で適切**であり、P1-B 修正は機能している (cascade で MI 側 margin が勝っている)。
リスク: なし
検証方法: 本ドキュメントの §2.1 表を参照

### P1 修正案 #1 — Chromium break algorithm の overflow を防ぐ

**対象**: page 7 / 12 / 16 など下端 12mm 未満まで到達するページ
**編集対象**: `src/handlers/survey/report_html/market_intelligence.rs` MI_STYLE_BLOCK + 同等パターンを `style.rs` の汎用テーブルにも

**修正内容** (1-3 行):
```css
@media print {
  /* 最終行が下マージンに食い込むのを防ぐため、tbody 末尾に余白確保 */
  .mi-rank-table tbody:after { content: ""; display: block; height: 4mm; }
  /* 追加: 全 tr に下端余白を強制 (Chromium break で末尾行が押し込まれる対策) */
  .mi-rank-table tr:last-child { padding-bottom: 4mm; }
}
```

期待効果: コンテンツ最下端 12.3mm → 16mm 程度に改善。フッターとの被り防止。
リスク: テーブル末尾に微小な空白追加 (4mm)。視覚的影響は軽微。
検証方法: `mi_default_p1b.pdf` 再生成 → page 7/12/16 の content_bottom_y1 を測定 → < 805pt (= page_h - 16mm) を確認

### P1 修正案 #2 — Playwright margin 引数の整合性確保

**対象**: `e2e_print_verify.py:121-126` および ad-hoc 生成スクリプト全般
**編集内容**:
```python
# 変更前
page.pdf(path=..., format="A4", print_background=True,
         margin={"top":"10mm","bottom":"18mm","left":"10mm","right":"10mm"})
# 変更後 (CSS @page を尊重)
page.pdf(path=..., format="A4", print_background=True,
         prefer_css_page_size=True)
# margin 引数を渡さない → CSS @page margin (12mm 14mm) が効く
```

期待効果: PDF 生成側設定と CSS @page の単一情報源化。回帰時のデバッグ簡素化。
リスク: テスト環境で format/margin が変わるため expected page count レンジ調整が必要な可能性。`tests/e2e/market_intelligence_print_theme.spec.ts:420` 系でも同様変更必要。
検証方法: 変更前後で PDF を生成して MediaBox / コンテンツ最下端 y を比較。`gen_survey_pdf.py` も同様。

### P1 修正案 #3 — @page 宣言の単一化 (任意)

**対象**: `src/handlers/survey/report_html/market_intelligence.rs` MI_STYLE_BLOCK の `@page` 行
**編集内容**: MI 側 `@page` を削除し、style.rs L56 の base @page に統一。下マージンを 12mm に揃えたい場合は style.rs L59 の `margin: 10mm 8mm 12mm 8mm` を `margin: 12mm 14mm 12mm 14mm` に変更 (本文幅が 194mm → 182mm に減少することへの影響評価必須)。

期待効果: cascade 競合の排除。
リスク: 本文幅が変わるため hero card / KPI grid / table 等のレイアウト崩れ可能性。frontend-review #10 で「本文幅 170mm → 194mm 修正」の経緯あり (style.rs L683 コメント)、再検証必須。
検証方法: スクリーンショット差分 + invariant_tests.rs 実行

---

## 8. 期待効果まとめ

| 修正 | 実装コスト | 期待される実測値 | 効果 |
|------|------------|------------------|------|
| P0 (計測是正) | 0 (認識のみ) | 12.3mm → "正常範囲" 判定 | 誤認解消 |
| P1 #1 (tbody:after) | 1-2 行 | 12.3mm → ~16mm | 実改善 |
| P1 #2 (prefer_css_page_size) | 1 行 | 微差 (主に CSS と整合化) | 保守性向上 |
| P1 #3 (@page 統一) | リスク高 | (CSS 整理) | 任意 |

最も即効性のある修正は **P0 (認識合わせ) + P1 #1 (tbody:after)** の組み合わせ。

---

## 9. 次ラウンド指示書テンプレート (最小プロンプト)

```
PDF 下端余白の実改善ラウンド (P1 #1 実装)。

## 作業ディレクトリ
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy

## 背景
docs/PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md §7 の P1 #1 を実装する。
真因はユーザー計測対象がフッター margin box 内文字列だった (P0 で認識是正済み)。
本ラウンドは「コンテンツが下マージン 12mm 未満に到達するページ」を改善する。

## 実装

src/handlers/survey/report_html/market_intelligence.rs の MI_STYLE_BLOCK
@media print セクション (around line 397) に以下を追加:

  .mi-rank-table tbody:after { content: ""; display: block; height: 4mm; }

同様パターンを style.rs の他テーブル (`.hw-enrichment-table`, `.salesnow-table` 等)
にも適用。

## 検証
1. cargo build (errors なし確認)
2. PDF 再生成 (e2e_print_verify.py または ad-hoc)
3. python で page 7/12/16 の content_bottom_y1 を測定 → < 805pt (16mm 以上の下マージン)
4. Footer y1 = 830.8pt (変化なし) を確認

## 絶対ルール
1. style.rs / market_intelligence.rs 編集のみ
2. @page margin は変更しない (frontend-review #10 で 170mm 縮小事故あり)
3. cargo build pass + テスト pass で commit
```

---

## 10. 補足: 計測方法の改良案

PDF 検証スクリプトに「フッター除外コンテンツ最下端」測定を組み込むと再発防止になる:

```python
# pseudo-code (本ラウンド実装ではない、参考)
FOOTER_PATTERNS = ['株式会社For A-career | 求人市場', 'Page ']
def get_content_bottom(page):
    blocks = page.get_text('blocks')
    content = [b for b in blocks if not any(p in b[4] for p in FOOTER_PATTERNS)]
    return max((b[3] for b in content), default=0)  # max y1
```

これを `e2e_print_verify.py` の `verify_pdf()` に追加し、各ページで `page_h - content_bottom > 12mm` を assert すると Chromium break algorithm の overflow を機械検出可能。

---

(以上、read-only 監査結果。実装変更なし)
