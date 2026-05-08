# PDF データソース混入監査 (Round 1-L)

**生成日**: 2026-05-08
**監査対象**: `out/real_csv_pdf_review_20260508/` 配下の Indeed 経路 PDF 4 本
**監査主旨**: 通常導線 PDF (Indeed CSV) で出力される「採用マーケットインテリジェンス版」(`variant=market_intelligence`) に、HW 併載版 (`variant=full`) でのみ表示すべき HW 由来文言・セクション・指標が混入していないかを評価。

---

## 1. 入力 PDF と想定 variant

variant 判定根拠: 各 PDF の HTML 版 line 2189 にある `<span class="variant-current">...現在: <strong>採用マーケットインテリジェンス版</strong></span>` および line 2191 の `<a href="?variant=full" ... HW併載版 に切替>`。

| PDF | pages | 想定 variant (確定) | 切替リンク先 |
|---|---|---|---|
| `indeed-2026-04-27.pdf` | 33 | **MarketIntelligence** (`?variant=market_intelligence`) | `?variant=full` (HW併載版) |
| `indeed-2026-04-27_1_.pdf` | 27 | MarketIntelligence | 同上 |
| `indeed-2026-04-28.pdf` | 32 | MarketIntelligence | 同上 |
| `indeed-2026-04-30.pdf` | 31 | MarketIntelligence | 同上 |

参考: `out/print_review_p1h/mi_via_action_bar.pdf` および `prod_html_market_intelligence.html` (action bar 経由 MI 生成、line 2189 同一) と完全同一の variant。

---

## 2. PDF 本文 hit 件数 (fitz `get_text()` ベース)

```
indeed-2026-04-27.pdf  : 87 hits (33 pages)
indeed-2026-04-27_1_.pdf: 64 hits (27 pages)
indeed-2026-04-28.pdf  : 88 hits (32 pages)
indeed-2026-04-30.pdf  : 89 hits (31 pages)
```

代表 PDF (04-27) の語句別内訳:
| 語句 | hits |
|---|---|
| HW (略語) | 58 |
| ハローワーク | 8 |
| 公共職業安定 | 6 |
| 職業安定 | 6 |
| 職業紹介 | 6 |
| 厚生労働省 | 2 |
| 職業安定業務統計 (= 職業安定 親文字列) | 2 |
| 有効求人倍率 (4 軸 KPI 全ページ集計) | 多数 (本文+表+KPI) |

参考 (HTML grep / variant 比較):
| variant | HTML 内 hit 総数 |
|---|---|
| Full (`prod_html_full.html`) | 71 |
| MarketIntelligence (`prod_html_market_intelligence.html`) | 70 |
| Public (`prod_html_public.html`) | 55 |

→ 実装上 `MarketIntelligence` は `Full` とほぼ同等の HW 言及量。`Public` は HW 欠員補充率 KPI とソース注記分の差分のみ削減 (~15 件)。

---

## 3. variant 仕様照合 (実装根拠)

`src/handlers/survey/report_html/mod.rs` (read のみ):

| 項目 | line | 内容 |
|---|---|---|
| `enum ReportVariant` 定義 | 93-105 | `Full` / `Public` / `MarketIntelligence` の 3 種 |
| `show_hw_sections()` | 139-143 | `Full \| MarketIntelligence` で `true`、`Public` のみ `false` |
| Section H (HW データ連携) guard | 786-789 | `if variant.show_hw_sections() { ... render_section_hw_enrichment() }` |
| Section 4MT (採用市場逼迫度) | 812 | `render_section_market_tightness_with_variant(...)` で variant 渡し |
| Section 4B (産業ミスマッチ) | 819-836 | `Full \| MarketIntelligence` は HW 求人構成比、`Public` は CSV 媒体掲載比 |

`src/handlers/survey/report_html/market_tightness.rs` (read のみ):
- line 70-86: `Full / MarketIntelligence` → 4 軸 (有効求人倍率 / HW 欠員補充率 / 失業率 / 離職率)、`Public` → 3 軸 (HW 欠員補充率を除外)。

**結論 (仕様レベル)**: 実装上、`MarketIntelligence` variant は `Full` と同じ HW セクション・HW 欠員補充率 KPI・HW 求人構成比表を **意図的に出力する設計**。Round 1-L のタスク前提「MI variant では HW 文言混入を NG とする」は、現行実装の設計と乖離している。

---

## 4. 全 hit の判定 (代表 PDF 04-27、46 サンプル抽出)

判定基準:
- **P0**: 通常版 (Indeed/MI/Public 想定) で HW 比較・HW 給与・HW 市場指標が**本文セクションとして** 出ているもの (商品仕様外、即時修正対象)
- **P1**: 出典注記 / 凡例として軽く登場 (混入可能性、要仕様確認)
- **P2**: フッター / メタ / 全体免責 / グレー (許容範囲)

### 4.1 P0 候補 (本文セクションとして HW を主体に表示)

| # | page | 文脈 | 判定理由 |
|---|---|---|---|
| 1 | 1 | 表紙サブタイトル: 「ハローワーク掲載求人 + アップロード CSV クロス分析」 | カバーページで HW を主軸として宣言。MI variant は本来「採用マーケットインテリジェンス」を主軸とすべきだが HW 併載版相当の表現が露出 |
| 2 | 2 | Executive Summary 優先アクション: 「当サンプル 99.6% / HW 市場 66.1% で 33.5pt 差。(Section 4 参照)」 | 本文 KPI として HW 市場値との直接比較。HW 併載でしか出すべきでない |
| 3 | 3 | 第 3 章「地域 × HW データ連携」全体: 表 3-1「市区町村別 CSV-HW 求人件数 対応表 (CSV件数の多い 15 地域)」、概念図「HW ハローワーク 掲載 公的職業紹介 欠員補充率」 | 章まるごと HW 主体のセクション。`render_section_hw_enrichment` が `show_hw_sections() == true` で出力されている (mod.rs:786-789) |
| 4 | 8-9 | 第 4 章 採用市場逼迫度: 「📈有効求人倍率 1.33倍」「👥HW 欠員補充率 中 30%」「業界を問わない地域全体値: 有効求人倍率 / 失業率 / HW 欠員補充率 / 開廃業動態」 | 4 軸レーダー / KPI カードに HW 欠員補充率 (HW 由来) が含まれる。Public の 3 軸版に切替えれば除外可能 |
| 5 | 9 | 第 4B 章 産業ミスマッチ: 表「産業別 就業者構成比 vs HW 求人構成比 (大分類)」 | 本表は `render_section_industry_mismatch()` で `Full \| MarketIntelligence` 経路。Public 経路では `render_section_industry_mismatch_csv` (CSV vs 国勢調査) に置換される |
| 6 | 19 | 第 9 章 企業分析: 「HW 求人件数が多い法人は採用活動が活発な可能性」「観測指標: HW 求人数 × 1 年人員推移 を合成した参考値」 | 企業ベンチマーク表のソート/評価軸が HW 求人数。MI variant 仕様で意図的か再確認要 |
| 7 | 22-25 | 第 12B 章 SalesNow セグメント: 規模帯別「HW 求人継続率」、各表ヘッダー「HW 求人」列、「🎯 求人積極期 (HW 5 件以上) — ハローワークで 5 件以上の求人を継続している 10 社」 | HW 件数を継続率/閾値の評価軸として使用。「ハローワーク」「HW」を本文ヘッドラインで多用 |

### 4.2 P1 候補 (出典注記 / 凡例の軽い登場)

| # | page | 文脈 | 判定 |
|---|---|---|---|
| 8 | 9 | KPI 出典注記: 「出典: 厚生労働省 職業安定業務統計 (一般職業紹介状況) / 計算: 有効求人数 / 有効求職者数」 | **許容**: 有効求人倍率の正式出典は厚労省。ただし MI variant が HW 欠員補充率込みで出している点と整合 |
| 9 | 9 | 「出典: ハローワーク掲載求人 (自社集計)」(HW 欠員補充率の出典明示) | P1 (P0 の対指標 #4 が消えれば連動消失) |
| 10 | 9 | 「出典: 厚生労働省 雇用動向調査 (産業計)」 (離職率) | **許容**: 離職率の正式出典 |
| 11 | 10 | 「⚠ 就業者構成は国勢調査... HW 求人は HW 登録求人のみで全求人市場ではありません」 | P1: P0 #5 (4B 章) の caveat。表自体を Public 版に切り替えるなら同時に消える |

### 4.3 P2 候補 (許容範囲)

| # | page | 文脈 | 判定 |
|---|---|---|---|
| 12 | 32-33 | 注記 / 出典 / 免責 (本レポート全体): 「ハローワーク公開データ (hellowork.db / postings テーブル)」「非公開求人・職業紹介事業者経由の求人 ... は本レポートに含まれない」「データ源 - アップロード CSV / ハローワーク公開データ / 地域注目企業データベース / e-Stat」 | **許容**: 全レポート共通の出典・免責欄。データソース透明性確保のため必要 |
| 13 | 33 | 推奨アクション: 「給与水準... HW 市場との適合度を比較する」 | P2: 注記内の運用ガイダンス。本文指標が消えれば文言調整推奨 |

---

## 5. グレー判定 (厚労省関連)

| 出典記述 | 該当 page | 判定 |
|---|---|---|
| 厚生労働省 職業安定業務統計 (一般職業紹介状況) — 有効求人倍率の出典 | 9 | **許容**: 厚労省の正式公表値で外部統計として一般的。MI でも保持可 |
| 厚生労働省 雇用動向調査 (産業計) — 離職率の出典 | 9 | **許容**: 同上 |
| ハローワーク掲載求人 (自社集計、e-Stat 由来ではない) — HW 欠員補充率の出典 | 9 | **要修正**: HW 自社集計値であり、他の厚労省公表値と並列に置くと出典の性質を誤認させる。MI variant の Public 化方針なら消去対象 |

ラベル整合性: KPI カード / データソース表 / 出典注記の 3 箇所すべてで「厚生労働省 ○○」「ハローワーク ○○」のラベルが付与されており、ラベル不整合は確認されず。

---

## 6. 通常導線 PDF への HW 混入サマリ

| 重大度 | 件数 (代表 PDF 04-27 ベース) | 主な箇所 |
|---|---|---|
| **P0** | 7 系統 | 表紙サブタイトル / Exec Summary HW 比較 / 第 3 章 HW 連携 / 第 4 章 HW 欠員補充率 4 軸 / 第 4B 章 HW 求人構成比 / 第 9 章 HW 観測指標 / 第 12B 章 HW 求人継続率 |
| **P1** | 4 系統 | KPI 出典注記 (HW 自社集計) / Public 化に連動して消える caveat 群 |
| **P2** | 2 系統 | 全体注記・出典欄 / 推奨アクション内の HW 言及 |

4 PDF の hit 件数の差 (87 / 64 / 88 / 89) は対象 CSV の規模 (人気企業数等) によるテーブル行数差で、構造的な P0/P1/P2 の **章・KPI 種類は 4 PDF とも同一** (同じ MI variant、同じ section 構成)。

---

## 7. 仕様レベルの発見

1. **`show_hw_sections()` の判定**:
   - mod.rs:139-143 で `MarketIntelligence` は `Full` と同じく HW セクションを表示する設計。
   - line 142: `matches!(self, Self::Full | Self::MarketIntelligence)`
2. **設計コメント (mod.rs:175-180)**:
   - `Full`: 「ハローワーク掲載求人と統合分析を含む完全版（社内分析向け）」
   - `Public`: 「e-Stat 等の公開データを主軸とした版（対外提案向け）」
   - `MarketIntelligence`: 「採用ターゲット分析を含む拡張版（媒体分析・配信地域提案向け）」
3. **タスク主旨との乖離**:
   - Round 1-L は「通常導線 = MI variant、HW 文言混入は NG」を前提としているが、実装は「MI variant = Full + 追加 5 セクション (mi_data)」として HW 主体セクションを温存する設計。
   - もし MI variant を「対外提案向け / HW 言及最小化」と再定義するなら、`show_hw_sections()` を `matches!(self, Self::Full)` に絞り、`market_tightness_with_variant` の Public 経路を MI でも適用する必要がある。

---

## 8. 修正方針案 (実装変更は本ラウンドの責務外)

優先度高い順:

1. **`show_hw_sections()` の MI 除外** (mod.rs:139-143):
   `matches!(self, Self::Full | Self::MarketIntelligence)` → `matches!(self, Self::Full)`
   - 影響: Section H (HW データ連携、第 3 章) が MI で非表示
2. **`render_section_market_tightness_with_variant` の MI 経路を Public と同じ 3 軸版に変更** (market_tightness.rs:80-86):
   - 影響: HW 欠員補充率 KPI / 4 軸レーダーが除外、3 軸版に
3. **第 4B 章産業ミスマッチを MI でも CSV 経路に切替** (mod.rs:819-836):
   - 影響: 「HW 求人構成比」→「CSV 媒体掲載構成比」に置換
4. **企業分析・SalesNow セグメント表のヘッダー / 凡例の HW 言及を一般化**: 「HW 求人」→「公開求人件数」等の中立表現
5. **Cover subtitle の HW 言及を MI variant では別文言に切替** (P1H 等で既に検討済みの可能性)
6. **全体注記・出典欄 (P2)**: variant 連動でデータソース表記を出し分け

ただし上記のうち **どれを採用するかは商品仕様の意思決定であり、実装乖離か仕様乖離かはユーザー側で確定要**。

---

## 9. 重大度サマリ

| 区分 | 4 PDF 共通件数 |
|---|---|
| **P0 系統** | 7 (章 / KPI / 表のレベルで HW 由来主体) |
| **P1 系統** | 4 (出典注記 / 仕様連動の caveat) |
| **P2 系統** | 2 (全体注記 / 推奨アクション軽言及) |
| **グレー (厚労省)** | 2 種許容 (有効求人倍率 / 離職率) + 1 種要再評価 (HW 欠員補充率の自社集計出典) |

---

## 10. 次ラウンド推奨修正

1. **意思決定タスク**: 「MarketIntelligence variant の商品仕様は Full 系 (HW 併載) なのか Public 系 (HW 言及最小化) なのか」をユーザー確認。
2. **Public 系と決まれば**: §8 の 1〜4 番を実装変更。MI variant 専用の追加 5 セクション (`render_section_market_intelligence`) は維持しつつ、HW セクションを除外。
3. **Full 系と決まれば**: 本ラウンドで P0 と判定した 7 系統はすべて意図動作。本 audit を「現状維持の根拠ドキュメント」として活用し、ユーザー向けには「MarketIntelligence は HW 併載 + 採用ターゲット分析の上位版」と説明。
4. **グレー指標 (HW 欠員補充率 / HW 自社集計の出典)** は、いずれの経路でもラベルを「ハローワーク掲載求人 (自社集計、e-Stat 由来ではない)」と維持し、厚労省公表値と並列で誤認されないよう注記強化を継続。
5. PDF 視覚レビュー (Round 1-A 系) と本データソース監査の結果を統合し、`PDF_REVIEW_GATE_CRITERIA_2026_05_08.md` のゲート条項に「variant ごとに本文 HW 言及件数の上限」を追加することを検討。

---

## 参考実装ソース

- `src/handlers/survey/report_html/mod.rs` (lines 85-180, 760-1003)
- `src/handlers/survey/report_html/market_tightness.rs` (lines 70-95, 1325-1470, 1655-1740)
- `src/handlers/survey/report_html/hw_enrichment.rs` (Section H 本体)
- `src/handlers/survey/report_html/market_intelligence.rs` (MI 専用 5 セクション)
- `src/handlers/survey/render.rs` (lines 510-600: action bar での variant 動線)
- `src/handlers/survey/handlers.rs` (line 702-706: クエリから variant 解決)
