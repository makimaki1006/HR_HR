# Round 6: Print/PDF 納品品質 P0/P1 改善ラウンド 運用記録

**ラウンド期間**: 2026-05-06 〜 2026-05-08
**最終更新**: 2026-05-08
**最終 commit**: `ad6488d`
**対象 variant**: `market_intelligence` (Full / Public / default は不変)
**作業ディレクトリ**: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`

---

## 1. 概要

### 1.1 ラウンド起点

Print/PDF P1 客観レビューで **C 判定** を受けた状態。具体的には PDF 本文に内部 fallback 文言 (「データ不足のため特定できませんでした (要件再確認)」「データ準備中」「Sample 法人」等) が出力されており、営業に持ち出して読める紙資料として成立していない問題があった。加えて、

- ヒーロー第 2 枠で「該当なし」が二重表示されるケース
- 配信地域ランキングで同一自治体が職種別に最大 11 行連続表示される問題
- グラフ ECharts/canvas/svg が印刷時に本文幅を超えて見切れる問題
- 通常導線 (媒体分析タブ) の PDF 出力に MarketIntelligence variant が含まれていない問題
- 生活コスト比較表で同一自治体が複数行重複する問題

を解消する必要があった。

### 1.2 ラウンド終点

- PDF 30 pages / 7.5 MB 出力
- MarketIntelligence セクションの 5 マーカー全揃い
- 自治体重複: 配信地域ランキング 12 行 → 1 行 / 生活コスト比較 5+ 行 → 1 行
- グラフ印刷見切れなし (right_margin 36.9-39.2pt 維持)
- Hard NG 13 用語 0 件 (全 variant で確認)
- Full / Public / default の variant 隔離維持
- 通常導線 PDF からも MarketIntelligence variant が起動可能

---

## 2. 反映 commit 13 件 (2026-05-06 〜 2026-05-08)

| # | SHA | 日時 | 内容 |
|---|---|---|---|
| 1 | `c7a4b34` | 2026-05-06 21:07 | post-release cleanup と monitoring notes (docs 4 件追加) |
| 2 | `b9cc610` | 2026-05-06 21:16 | UI P1-1 - data-label badge 4 種統一 (insufficient 追加) |
| 3 | `f5e41c2` | 2026-05-06 21:37 | Print/PDF P1 layout for consulting report (印刷向け要約 / 注釈 / 印刷 CSS 強化 / E2E 13 spec) |
| 4 | `d3cacbe` | 2026-05-06 23:03 | print PDF から内部 fallback 文言を削除 (「データ不足」/「データ準備中」/ KPI ラベル別名化) |
| 5 | `86b882c` | 2026-05-07 00:48 | print report 内部 empty-state を非表示 |
| 6 | `7fff0fb` | 2026-05-07 02:37 | hero empty-state 文言を polish |
| 7 | `1a409d9` | 2026-05-07 04:03 | E2E navigationTimeout を 180s に延長 (Render free-tier cold start 対応) |
| 8 | `74d07fa` | 2026-05-07 04:06 | print media に @page 12mm margin を反映 (html/body margin/padding を 0 にリセット) |
| 9 | `689c618` | 2026-05-07 14:28 | PDF margin 計測の誤認 (フッター margin box 拾い) を docs 化 |
| 10 | `9a616f9` | 2026-05-07 14:47 | print annotations を統合し空白 page を回避 (`break-before: avoid`) |
| 11 | `89ca8f8` | 2026-05-07 17:01 | 配信地域ランキングの自治体重複を集約 (max score 行を代表 + 「ほか N 職種」) |
| 12 | `556d960` | 2026-05-07 17:48 | 通常 PDF 導線で variant=market_intelligence を選択可能に + グラフ印刷見切れ修正 |
| 13 | `ad6488d` | 2026-05-07 23:47 | 生活コスト比較表の自治体行を集約 |

注: `c7a4b34` は当ラウンドの起点として位置付け、後続 12 commit の docs 基盤を提供している。

---

## 3. 検証サマリ (最終時点 / `ad6488d` 直後)

| 検証項目 | 結果 |
|---|---|
| `cargo test --lib` | 1197 PASS / 0 FAIL / 2 ignored |
| `cargo test --lib market_intelligence` | 118 PASS / 0 FAIL |
| `cargo test --test no_forbidden_terms` | 5 PASS / 0 FAIL |
| 本番 E2E (`BASE_URL=https://hr-hw.onrender.com`) | 21 passed / 0 failed / 2 skipped |
| 通常導線 PDF | 30 pages / 7.5 MB |
| MI 5 マーカー (`mi-print-summary` / `mi-print-annotations` / `mi-parent-ward-ranking` / `mi-rank-table` / hero bar) | 全存在 |
| 給与・生活コスト比較 自治体重複 | 前回 5+ 行 → 1 行 |
| 配信地域ランキング 自治体重複 | 前回 12 行 → 1 行 |
| グラフ見切れ (page 4/5/6/8/10) | right_margin 36.9-39.2pt |
| Hard NG 13 用語 | 0 件 (全 variant) |
| Full / Public / default ↔ MI variant 隔離 | 維持 |

---

## 4. 解消した問題

### 4.1 Print/PDF C 判定 4 件の解消 (commit 4-6)

- 「データ不足のため特定できませんでした (要件再確認)」を `render_mi_print_summary` から削除し、S/A 0 件のときは「該当なし」+ 配信地域ランキング案内に置換。
- 「データ準備中」を `render_mi_hero_bar` 第 2 枠から削除し、`mi-badge-insufficient`「該当なし」+ 「該当なし」値表示に統一。
- `INSUFFICIENT_LABEL` 定数を「データ不足」→「該当なし」に変更 (バッジ class は `mi-badge-insufficient` 維持)。
- KPI ラベル「重点配信候補」→「配信検証候補」にリネーム (ヒーロー Card 1 = S/A 件数 / KPI = スコア 80+ 件数で別概念であることを示す)。

### 4.2 ヒーロー第 2 枠の二重表示 (commit 6)

「該当なし」が二重表示されるケースを単一表示に修正し、政令市区ランキング対象外の自治体である旨を明示。

### 4.3 印刷ページ余白 (commit 8-10)

- `@media print` 内で `html` / `body` の `margin` / `padding` を `0 !important` にリセット。
- `print-color-adjust: exact !important` を全要素に適用。
- `@page { size: A4 portrait; margin: 12mm 14mm; }` を `MI_STYLE_BLOCK` 末尾側に出力 (cascade 後勝ちで効かせる)。
- 注釈ブロックに `break-before: avoid !important` を追加し空白 page 19 の発生を抑制 (font-size 9.5pt、margin/padding/line-height 圧縮)。なお紙面効率削減効果そのものは別ターゲットだったため未実現。副作用なし。

### 4.4 配信地域ランキングの自治体集約 (commit 11)

- `municipality_recruiting_scores` のキー (`municipality_code`, `basis`, `occupation_code`, `source_year`) により同一自治体が職業数だけ行が増えていた問題を、表示側で `municipality_code` 単位に集約。
- `distribution_priority_score` 最大の行を代表として 1 行に集約。「ほか N 職種」サブテキストを併記。
- 列ヘッダに「代表職種」を追加 (順位 / 市区町村 / 代表職種 / 配信優先度 / 厚み指数 / 競合求人数 / 区分)。
- DB スキーマ / SQL 集約は一切変更せず、表示側集約のみ。

### 4.5 通常導線 PDF への MI variant 含有 (commit 12)

- 媒体分析タブのアクションバーに「採用コンサルレポート PDF」ボタンを追加 (`data-variant="market_intelligence"`)。
- `openVariantReport()` 経由で既存 pref / muni / industry filter を保持しつつ新タブ起動。
- レポート画面の `variant_indicator` には MI 切替リンクを置かない設計 (Full / Public への variant_isolation 違反を回避するため、媒体分析タブから流入する経路に集約)。

### 4.6 グラフ印刷見切れの解消 (commit 12)

- `@media print` 内で `.echart` / `.echart-wrap` / `.echart-container` / `.chart-container` / `.chart-wrapper` の `width:100%` / `max-width:100%` / `overflow:visible` を `!important` で強制。
- 内部 `canvas` / `svg` も `max-width:100%` / `height:auto` で本文幅 (A4 = 194mm) に収める。
- `helpers.rs` の ECharts 初期化スクリプトに `beforeprint` / `afterprint` / `matchMedia('print')` listener を追加し double resize で SVG renderer の反映遅延を吸収。
- 印刷ボタンの `onclick` で `window.print()` 直前に `echarts.getInstanceByDom` 経由で全 chart を `resize()` する fallback を追加。

### 4.7 生活コスト比較表の自治体集約 (commit 13)

同一自治体が複数行重複する問題を、表示側集約で 1 自治体 1 行に解消。

---

## 5. 残課題 (P2)

| # | 項目 | 概要 |
|---|---|---|
| P2-A | page 25 情報密度 | 3 表同居 (配信地域ランキング / 給与・生活コスト比較 / 母集団レンジ) → 章割り再考が必要 |
| P2-B | spec selector 監査 | `miParentWardCount=0` / `miSectionCount=0` (本番影響なし、HTML probe 命名規約変更の可能性) |
| P2-C | ヒストグラム軸ラベル重なり (page 5/6) | 「中央値」「平均」「最頻値」が近接時にオーバーラップ |
| P2-D | 実顧客 CSV で 1 本 PDF 確認 | fixture ではなく本番データで説得力検証 |
| P2-E | fixture 法人名差替え | Sample → 実在風匿名 (優先度低) |

---

## 6. 学習教訓

### 6.1 既知パターンの再発: print CSS cascade trap

`feedback_print_css_cascade_trap.md` で記録済みの「`@page` 重複定義 + body padding/margin の二重インデントにより本文幅が想定より狭まる」パターンが今回も発生。`74d07fa` で `html` / `body` の `margin` / `padding` を `0 !important` にリセットし解消。

### 6.2 PDF margin 計測の罠 (新規)

PyMuPDF の `blocks` には `@page` margin box (フッター / ヘッダー) が含まれるため、フッター文字列 (`Page X / 19 株式会社...`) を本文最下端として拾うと CSS `@page` margin の変更が誤って「反映なし」と判定される。本文 block の y1 を計測対象とする必要がある。詳細は `docs/PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md`。

### 6.3 ローカル成功 ≠ 本番成功

`feedback_partial_commit_verify.md` の教訓を本ラウンドでも適用。Render deploy hash が想定 commit と一致しているかを各反映後に確認するフローを徹底。

### 6.4 改善対象を snippet/chars 数だけで判断しない

注釈統合 (commit 10) では当初「ページ削減効果」を期待したが、実体は別ターゲットであり削減効果は未実現 (副作用はなし)。改善対象を文字数や snippet 数だけで判断せず、実 PDF を目視で確認する必要がある。

---

## 7. 関連ドキュメント

- `docs/MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md` — Print/PDF P1 設計仕様
- `docs/MARKET_INTELLIGENCE_UI_P1_P2_BACKLOG.md` — UI P1/P2 バックログ (本ラウンド完了反映)
- `docs/POST_RELEASE_MONITORING_CHECKLIST.md` — 本番監視チェックリスト (本ラウンド記録追記)
- `docs/PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md` — PDF 余白計測誤認の調査記録
- `docs/PRINT_PDF_P1_ROOT_CAUSE_AUDIT.md` — P1 客観レビュー (C 判定根拠)
