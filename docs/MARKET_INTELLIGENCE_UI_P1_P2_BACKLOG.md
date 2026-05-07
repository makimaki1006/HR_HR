# Market Intelligence UI P1/P2 バックログ

**最終更新**: 2026-05-08
**前提**:
- UI P0 (`5d0d86d`) 本番反映済 (配信ヒーローバー / parent_rank 主表示 / data label badge / 印刷 CSS)
- Round 6 (`ad6488d`) 本番反映済 (Print/PDF 納品品質 P0/P1 改善 13 commit)
- 設計参照: `docs/MARKET_INTELLIGENCE_UI_IMPROVEMENT_PLAN.md`
- 印刷 / PDF P1 設計: `docs/MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md`
- Round 6 運用記録: `docs/ROUND6_PRINT_PDF_DELIVERY_QUALITY.md`
- variant 隔離 (Full / Public / default) を絶対に変えない
- Hard NG 用語 (target_count / estimated_population / 推定人数 / 想定人数 / 母集団人数 等) を docs / コードに書かない
- resident estimated_beta を人数化しない (Hard NG 維持)
- 中立表現を維持 (劣位 / 集中 / 縮小 などの評価語禁止)

---

## 進捗サマリ (2026-05-08)

| 項目 | 状態 | commit |
|---|---|---|
| P1-1 mi-badge-insufficient 4 種統一 | ✅ 実装済 | `b9cc610` |
| P1-NEW Print/PDF P1 (印刷読み順 / 改ページ / 注釈再配置) | ✅ 実装済 | `f5e41c2` |
| P1-B PDF 下端余白 12mm 反映 | ✅ 完了 (調査の結果、計測対象誤りと確定) | `74d07fa` |
| P1-Round6-A hero polish (empty-state 文言調整) | ✅ 完了 | `7fff0fb` |
| P1-Round6-B print annotations 統合 (空白 page 抑制) | ✅ 完了 (紙面効率削減効果は別ターゲット、副作用なし) | `9a616f9` |
| P1-Round6-C 配信地域ランキング自治体集約 | ✅ 完了 | `89ca8f8` |
| P0-Round6-D 通常 PDF 導線への MI variant 含有 + グラフ印刷見切れ修正 | ✅ 完了 | `556d960` |
| P1-Round6-E 生活コスト比較表 自治体集約 | ✅ 完了 | `ad6488d` |
| P1-2 解釈ストリップ (so what 1 行) | ⏳ 未着手 | — |
| P1-3 政令市区ランキング見せ方改善 | ⏳ 未着手 | — |
| P1-4 配信ヒーローバー文言微調整 | ⏳ 未着手 | — |
| P1-5 印刷時の余白 / 改ページ微調整 | ⏳ 未着手 (Print P1 で部分対応済 / 余白微調整は別) | — |
| P2-1〜P2-4 | ⏳ 未着手 | — |
| P2-Round6-A page 25 情報密度改善 (3 表同居解消) | ⏳ 未着手 | — |
| P2-Round6-B spec selector 監査 (`miParentWardCount=0` / `miSectionCount=0`) | ⏳ 未着手 (本番影響なし、HTML probe 命名規約再確認) | — |
| P2-Round6-C ヒストグラム軸ラベル重なり (page 5/6) | ⏳ 未着手 (中央値 / 平均 / 最頻値 近接時のオーバーラップ) | — |

---

## P1: 次にやる小改善

### P1-1. `mi-badge-insufficient` 4 種バッジの統一

- **目的**: データ品質ラベルの視覚的整合を取り、読み手が「実測 / 推定 β / 参考 / データ不足」を一目で識別できるようにする
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (バッジ HTML 生成箇所)
  - `assets/css/report_print.css` または該当 CSS (バッジクラス定義)
- **変更範囲**: HTML (class 名統一) / CSS (4 variant の色・余白・font-weight 統一) / docs (本ファイル)
- **E2E 追加 / 修正要否**: 既存 spec を拡張 (`tests/e2e/market_intelligence_*.spec.ts` 等で 4 バッジが描画されているか class セレクタで確認)。新規 spec 不要。
- **リスク**:
  - variant 隔離崩しに注意 (Full / Public / default で同一 class を共有する場合のみ)
  - 印刷 CSS 側で色がコントラスト不足にならないこと
  - Hard NG 用語をバッジ文言に混入させない
- **完了条件**:
  - `cargo test` PASS
  - Hard NG grep (`target_count|estimated_population|推定人数|想定人数|母集団人数`) が docs / 生成 HTML で 0 件
  - E2E で 4 バッジ class が検出される
  - 印刷プレビューで色が判別可能

### P1-NEW. 印刷 / PDF P1 (実装済 ✅)

- **目的**: A4 縦印刷 / PDF を「営業に持ち出して即読める紙資料」として成立させる (読み順 8 ステップ + 改ページヒント + 印刷専用ブロック)
- **設計**: `docs/MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md`
- **対象ファイル**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (印刷向け要約 / 注釈ブロック追加、`@media print` CSS 拡張)
- **状態**: ✅ 実装済 (commit hash は orchestrator が確定後に追記)
- **テスト追加**:
  - `tests/no_forbidden_terms.rs::no_forbidden_terms_near_mi_print_blocks` (印刷ブロック近傍の Hard NG 用語ガード)
- **完了確認**:
  - 印刷向けクラス (`mi-print-summary` / `mi-print-annotations` / `mi-print-only`) 周辺で Hard NG 用語混入なし
  - 既存 `no_forbidden_identifiers_in_src` / `no_forbidden_ja_phrases_in_codebase` PASS
  - resident estimated_beta を紙でも人数換算しない

### P1-B. PDF 下端余白 12mm 反映 (実装済 ✅ / 計測誤認と確定)

- **目的**: A4 印刷時の下マージンを 12mm に揃え、フッターと本文の重なりを防ぐ
- **commit**: `74d07fa` (`fix(market_intelligence): apply 12mm @page margin in print media`)
- **状態**: ✅ 完了。`74d07fa` の CSS 修正は維持。
- **調査結論**: 当初「下端余白 11.4pt のままで改善なし」と認識されたが、これは PDF 計測スクリプトが `@page @bottom-left` / `@bottom-right` margin box 内のフッター文字列 (`Page X / 19 株式会社...`) を本文最下端として拾っていた**計測対象誤認**。実コンテンツ最下端は 12-30mm 範囲で `@page { margin: 12mm 14mm }` が概ね効いている。詳細は `docs/PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md` 参照。
- **追加対応 (任意)**: page 7 / 12 / 16 のテーブル末尾行が 12mm 未満まで到達するケースは Chromium break algorithm の限界に起因。実改善が必要なら同調査ドキュメント §7 P1 #1 (`tbody:after` で 4mm 余白確保) を別タスクとして切る。
- **再発防止**: PDF 余白の自動検証時はフッター margin box を除外して本文 block の y1 を計測すること (`docs/POST_RELEASE_MONITORING_CHECKLIST.md` §3 補足参照)。

### P1-2. 解釈ストリップ (so what 1 行) 追加

- **目的**: 生活コストカード / 配信優先度カードに 1 行の中立的な解釈を加え、読み手の意思決定を支援する
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (生活コスト / 配信優先度ブロック)
- **変更範囲**: Rust ロジック (1 行サマリ生成) / HTML (`<p class="mi-interpretation">`) / CSS (簡易スタイル) / docs
- **E2E 追加 / 修正要否**: 既存 spec を拡張。`mi-interpretation` セレクタの存在と、文言が中立表現か (評価語混入なし) を確認。
- **リスク**:
  - 中立表現違反 (劣位 / 集中 / 縮小 などを書かない)
  - データ不足時に空文字や「不明」を出さず、`mi-badge-insufficient` 経由で代替表示にフォールバック
  - Full / Public 出し分けを変更しない
- **完了条件**:
  - `cargo test` PASS
  - 中立表現 grep で評価語ゼロ
  - 解釈文に Hard NG 用語ゼロ
  - E2E PASS

### P1-3. 政令市区ランキングの見せ方改善

- **目的**: parent_rank 主表示の前提下で、順序入替・列幅・行間を整え可読性を上げる
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (ランキングテーブル生成)
  - 該当 CSS (テーブル列幅 / 行間)
- **変更範囲**: HTML (列順) / CSS (width, padding, line-height) / docs
- **E2E 追加 / 修正要否**: 既存 ranking spec の列順アサーションを更新 (新規 spec 不要)
- **リスク**:
  - parent_rank の意味を崩さない (主表示維持)
  - 印刷時の改ページで行が割れないこと
  - 中立表現維持
- **完了条件**:
  - `cargo test` PASS
  - 印刷プレビューで列が枠内に収まる
  - E2E ranking spec PASS

### P1-4. 配信ヒーローバー文言微調整

- **目的**: P0 で導入したヒーローバー文言を読み手の理解しやすさ重視で微調整する
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (ヒーローバー生成箇所)
- **変更範囲**: 文言定数 (Rust) / docs
- **E2E 追加 / 修正要否**: 既存 spec の文言アサーションを更新
- **リスク**:
  - Hard NG 用語の混入禁止
  - 中立表現維持 (規模差を「劣位」と書かない)
  - Full / Public で文言を取り違えない
- **完了条件**:
  - `cargo test` PASS
  - Hard NG grep 0 件
  - E2E PASS

### P1-5. 印刷時の余白 / 改ページ微調整

- **目的**: P0 印刷 CSS のうえに、余白とセクション境界の改ページを調整しレポート完成度を上げる
- **対象ファイル候補**:
  - `assets/css/report_print.css` 等の印刷専用 CSS
- **変更範囲**: CSS (`@page` margin / `page-break-*` / body padding) のみ
- **E2E 追加 / 修正要否**: 既存印刷 E2E (PDF or screenshot) があれば差分確認。新規不要。
- **リスク**:
  - `@page` 重複定義 + body padding の二重インデントによる本文幅縮小 (2026-04-30 事故再発防止)
  - 評価語混入の心配なし (CSS のみ)
- **完了条件**:
  - `cargo test` PASS
  - 印刷プレビューで本文幅が縮まない
  - 主要セクションが想定通りの位置で改ページされる

---

## P2: その後の改善

### P2-1. 比較カードの強化 (前年比など)

- **目的**: 既存比較カードに前年比などの時間軸視点を加える
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs`
  - 比較ロジックを担う module (時系列ソース取得層)
- **変更範囲**: Rust ロジック (前年データ参照) / HTML (前年比セル追加) / CSS / docs
- **E2E 追加 / 修正要否**: 新規 spec 1 本 (前年比セルの存在 + 数値整合)
- **リスク**:
  - 前年データ不足時のフォールバック (`mi-badge-insufficient` で表示)
  - 単位の一貫性 (% vs 比率) を全コードベースで統一 (2026-04-30 employee_delta_1y 事故再発防止)
  - 中立表現維持 (「縮小」と書かない)
- **完了条件**:
  - `cargo test` PASS
  - Hard NG grep 0 件
  - 単位整合 grep 確認
  - 新規 E2E PASS

### P2-2. 生活コスト説明の改善

- **目的**: 生活コスト指標の根拠と読み方を簡潔に補足し、誤読を防ぐ
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (生活コストカード)
- **変更範囲**: HTML (補足テキスト) / docs
- **E2E 追加 / 修正要否**: 既存 spec を拡張
- **リスク**:
  - 中立表現維持
  - 出典明記 (e-Stat 等) の文言ミスなし
  - Full / Public 文言の取り違え禁止
- **完了条件**:
  - `cargo test` PASS
  - E2E PASS

### P2-3. データ不足時の代替表示

- **目的**: データ不足時に空欄や 0 を出さず、`mi-badge-insufficient` と短いガイダンスで読み手の混乱を防ぐ
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (各カード描画分岐)
- **変更範囲**: Rust 分岐ロジック / HTML (代替ブロック) / docs
- **E2E 追加 / 修正要否**: 新規 spec 1 本 (データ不足ケースの代替表示確認)。既存 chart 検証ロジック (canvas 存在のみ) ではなく、ECharts 初期化 + 代替表示 class の両立を確認 (2026-04-08 事故再発防止)
- **リスク**:
  - resident estimated_beta を人数化しない (Hard NG)
  - 「データ不足」表現が評価語にならないように維持
  - variant 隔離崩しなし
- **完了条件**:
  - `cargo test` PASS
  - Hard NG grep 0 件
  - 新規 E2E PASS
  - ドメイン不変条件 (失業率 < 100% 等) を逆証明テストで確認

### P2-4. ユーザー向け注釈の整理

- **目的**: 散在する脚注 / 補足を末尾に集約し、レポートの導線を整える
- **対象ファイル候補**:
  - `src/handlers/survey/report_html/market_intelligence.rs` (注釈ブロック)
- **変更範囲**: HTML (注釈集約セクション追加) / CSS (注釈用 typography) / docs
- **E2E 追加 / 修正要否**: 既存 spec を拡張 (注釈セクションの存在確認)
- **リスク**:
  - 印刷時の改ページで注釈が分断されない
  - Hard NG 用語 / 評価語混入なし
  - 出典記載漏れなし
- **完了条件**:
  - `cargo test` PASS
  - Hard NG grep 0 件
  - 印刷プレビューで注釈が一塊で出力される
  - E2E PASS

---

## 共通完了基準 (各項目で必ず確認)

- `cargo test` 全 PASS
- Hard NG grep 0 件 (`target_count|estimated_population|推定人数|想定人数|母集団人数`)
- 中立表現 grep 確認 (`劣位|集中|縮小` 等の評価語が新規追加されていない)
- variant 隔離 (Full / Public / default) 不変
- 印刷プレビュー目視確認 (本文幅 / 改ページ)
- 該当 E2E PASS
