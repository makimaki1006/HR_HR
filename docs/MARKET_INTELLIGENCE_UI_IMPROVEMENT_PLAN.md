# MarketIntelligence UI 改善設計書

**対象 variant**: `MarketIntelligence` のみ（Full / Public / default は変更禁止）
**作成日**: 2026-05-06 / Worker D（設計のみ、実装は別チーム）
**前提**: Round 4 (`0fd7368`) 時点で `mi-*` class 27 個 + `MI_STYLE_BLOCK` 投入済み。本書はその上に積む差分設計。

---

## 1. 現状の課題（Round 4 後の残存問題）

E2E スクショ (`market_intelligence_smoke.spec.ts` / `market_intelligence_print_theme.spec.ts`) からの観察と DOM 構造レビューに基づく。

### 1.1 ファーストビューの優先順位が不明瞭
- `mi-root` 直下に「免責 → KPI サマリ → 結論サマリ → 配信ランキング → 人材供給 → 給与 → 生活コスト → シナリオ → 通勤 → 政令市区ランキング → 注記」が縦一列。スクロールせずに「どこに配信すべきか」が読めない。
- KPI 4 枚（配信優先度 A / 厚み平均 / 政令市区集積地 / 重点配信候補）の意味が並列で、**主従関係**が無い。営業現場では「**重点配信候補（S+A 件数）**」が核心 KPI。

### 1.2 政令市区ランキング（商品コア）が下位に埋没
- `render_mi_parent_ward_ranking` は section の最後付近。スクショ最下部まで PgDn しないと到達しない。
- 表内では「市内順位 (主)」「全国順位 (参考)」が両方 `<th>` に並ぶため視覚的同格に見える。`mi-ref` で fontsize は落ちているが、列幅・並び順での主従強調は弱い。

### 1.3 データラベル 4 種の使い分けが未整理
- 仕様書（`SURVEY_MARKET_INTELLIGENCE_METRICS.md` §1）は「実測 / 推定 / 参考 / データ不足」の 4 ラベル運用。現状 CSS は `mi-badge-measured / -estimated-beta / -reference` の 3 種のみ。**「データ不足」プレースホルダ用 badge** が未定義で、`render_mi_placeholder` が独自インライン色で出ている。

### 1.4 生活コスト・配信優先度カードの「解釈文」欠落
- `mi-living-cost-card` は label + 数値のみ。「高 / 中 / 低」帯と「この値が高いと…」の解釈が無く、営業が値を読み解けない。
- 配信優先度 KPI も同様（数字のみ、so what が無い）。

### 1.5 印刷崩れリスク
- `MI_STYLE_BLOCK` の `@media print` は KPI grid と living-cost grid の block fallback のみ。
- `mi-rank-table` は印刷時に列幅が広いと A4 横で 5 列が縮み、`thickness-bar-wrap` (80px 固定) が枠外。
- `mi-parent-group` の `border-radius` / 背景は印刷時 `print-color-adjust` 未指定で、ブラウザにより消える。
- `summary` / `mi-living-cost-panel` の灰背景（`#f8fafc`）が印刷で抜けると、見出しと本文の視覚分離が消える。
- 主要表（政令市区ランキング）の `page-break-inside: avoid` が `mi-rank-table` 単位ではなく `mi-parent-group` 単位でしか当たっていない（CSS では未指定。block fallback のみ）。1 つの政令市が 1 ページに収まらないとき、`tbody` の途中で切断される。

---

## 2. 改善案（HTML/CSS 構造）

### 2.1 ファーストビュー再設計：「配信ヒーローバー」導入 (P0)

`mi-root` 直下、免責の後に **横一列の「配信ヒーローバー」** を 1 段挿入する。免責とサマリ KPI を分離し、ヒーローには「**核心 1 指標**＋「**市内順位 1 位市区**」＋「**実測職業人口（自治体合計）**」を 3 枚に絞る。

```html
<section class="mi-hero" role="region" aria-labelledby="mi-hero-heading">
  <h3 id="mi-hero-heading" class="mi-visually-hidden">配信判断 ヒーロー</h3>
  <div class="mi-hero-grid">
    <div class="mi-hero-card mi-hero-primary">
      <div class="mi-hero-eyebrow">重点配信候補（S + A）</div>
      <div class="mi-hero-value"><strong>{N}</strong><span>件</span></div>
      <div class="mi-hero-context"><span class="mi-badge mi-badge-estimated-beta">推定</span> Model F2</div>
    </div>
    <div class="mi-hero-card">
      <div class="mi-hero-eyebrow">市内 1 位市区（先頭政令市）</div>
      <div class="mi-hero-value">{ward_name}</div>
      <div class="mi-hero-context">市内順位 1 / {parent_total} 区</div>
    </div>
    <div class="mi-hero-card">
      <div class="mi-hero-eyebrow">職業人口（実測 R2 合計）</div>
      <div class="mi-hero-value"><strong>{measured_sum}</strong><span>人</span></div>
      <div class="mi-hero-context"><span class="mi-badge mi-badge-measured">実測</span> 国勢調査</div>
    </div>
  </div>
</section>
```

注: **resident estimated_beta は人数として hero に出さない**（Hard NG）。hero 第 3 枠は `workplace_measured` 由来の実測値のみ。`occupation_cells` が空なら hero 第 3 枠は「実測値準備中」プレースホルダで埋め、推定数字は出さない。

### 2.2 政令市区ランキングの最上位移動 (P0)

セクション順序を変更：
```
免責 → ヒーロー (新) → 政令市区ランキング (上昇) → KPI サマリ → 結論サマリ → ...（既存順） → 注記
```

`render_mi_parent_ward_ranking` 内テーブルの列幅を CSS で固定し、市内順位列を強調：

```css
.mi-rank-table { table-layout: fixed; }
.mi-rank-table col.mi-col-prank { width: 22%; }
.mi-rank-table col.mi-col-name  { width: 28%; }
.mi-rank-table col.mi-col-thick { width: 22%; }
.mi-rank-table col.mi-col-prio  { width: 12%; }
.mi-rank-table col.mi-col-nrank { width: 16%; }
.mi-rank-table th.mi-col-nrank, .mi-rank-table td.mi-col-nrank {
  font-size: 10px; color: #94a3b8; font-weight: 400;
}
```

`<colgroup>` を `<thead>` 直前に追加し、参考列は字下げで主従を明確化。

### 2.3 データラベル 4 種統一 (P1)

CSS に `mi-badge-insufficient` を追加：

```css
.mi-badge-insufficient { background: #fee2e2; color: #991b1b; border-color: #fca5a5; }
.mi-badge-insufficient::before { content: "⚠ "; }
```

`render_mi_placeholder` を `mi-badge-insufficient` 付きの統一表示に置換し、他のインライン `#fef3c7` 直書きを撤廃。凡例 (`mi-kpi-legend`) にも 4 ラベル全て表示。

### 2.4 生活コスト・配信優先度カードに「解釈ストリップ」 (P1)

`mi-living-cost-card` を縦 3 段化：

```html
<div class="mi-living-cost-card mi-band-{high|mid|low}">
  <div class="mi-lc-label">家賃水準（家計調査）</div>
  <div class="mi-lc-value">{value}</div>
  <div class="mi-lc-band-strip">
    <span class="mi-lc-band-dot" aria-hidden="true"></span>
    <span class="mi-lc-band-text">高: 居住費負担大 / 給与訴求が必要</span>
  </div>
</div>
```

帯は `living_cost_proxies` の `band` フィールド（既存 DTO） を使用。値が NULL の場合は `mi-band-na` で `データ不足` バッジ。

配信優先度 KPI カード（`render_mi_kpi_cards`）にも同様のストリップを足し、「S/A → 即時配信候補」「B → 段階配信」「C/D → 静観」の解釈を 1 行で添える。

### 2.5 印刷 CSS 強化 (P0)

`MI_STYLE_BLOCK` の `@media print` に追加：

```css
@media print {
  /* 背景色を印刷で出す */
  .mi-hero-card,
  .mi-kpi-card,
  .mi-living-cost-card,
  .mi-living-cost-panel,
  .mi-summary,
  .mi-priority-badge,
  .mi-badge {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }
  /* 主要表は分割禁止 */
  .mi-rank-table { page-break-inside: auto; }
  .mi-rank-table thead { display: table-header-group; }
  .mi-rank-table tr { page-break-inside: avoid; page-break-after: auto; }
  /* バー幅の縮退（80px → 60px） */
  .mi-thickness-bar-wrap { width: 60px; }
  /* hero は横並び維持（A4 横 で 3 枚は許容） */
  .mi-hero-grid { display: grid !important; grid-template-columns: repeat(3, 1fr); gap: 6px; }
  .mi-hero-card { page-break-inside: avoid; }
}
```

`@page` の重複指定は禁止（feedback `print_css_cascade_trap` 参照）。本 variant 専用 `style` ブロック内に閉じ、`body` の padding/margin に触れない。

---

## 3. 実装優先度

| 優先 | 項目 | 工数感 | 価値 |
|------|------|--------|------|
| P0 | 配信ヒーローバー（§2.1） | 中（新 fn 1 個 + CSS 5 行） | ファーストビュー改善、営業価値直結 |
| P0 | 政令市区ランキング上昇 + colgroup 列幅固定（§2.2） | 小（順序入替 + CSS 数行） | 商品コア視認性 |
| P0 | 印刷 CSS 強化（§2.5） | 小（CSS のみ） | A4 配布可否を左右 |
| P1 | データ不足 badge 4 種統一（§2.3） | 小（CSS + placeholder 関数差替え） | 一貫性 |
| P1 | 解釈ストリップ（§2.4） | 中（DTO band 値読み取り + CSS） | so what 提供 |
| P2 | hero 第 3 枠 occupation_cells 空時の代替表示磨き | 小 | 体感 |

---

## 4. 変更対象ファイル候補

| ファイル | 変更内容 | 凡例 |
|---------|---------|------|
| `src/handlers/survey/report_html/market_intelligence.rs` | `render_mi_hero_bar` 新設 / `render` での呼出順序入替 / `render_mi_parent_ward_ranking` の `<colgroup>` 追加 / `render_mi_living_cost_panel` の解釈ストリップ追加 / `render_mi_placeholder` を新 badge へ統合 / `MI_STYLE_BLOCK` 拡張 | コア |
| `src/dto/market_intelligence.rs`（または既存 DTO） | 既存 `living_cost_proxies.band` を参照（変更不要見込み）、不足時のみ `band_label` 拡張 | 参照 |
| `tests/e2e/market_intelligence_smoke.spec.ts` | hero セクション存在検証追加 | E2E |
| `tests/e2e/market_intelligence_print_theme.spec.ts` | 印刷 CSS の `print-color-adjust: exact` 適用検証、`mi-rank-table tr` の `page-break-inside: avoid` 検証 | E2E |
| 新 `tests/e2e/market_intelligence_hero_bar.spec.ts` | hero 3 枚の DOM 存在 / hero 第 3 枠が空 occupation_cells 時に推定数字を出していないこと（Hard NG 逆証明） | E2E（推奨新設） |

**変更しない**: `executive_summary.rs` / `summary.rs` / `style.rs` 等の Full/Public/default 共有モジュール。CSS も `mi-*` prefix 内に閉じる。

---

## 5. E2E 追加 / 修正候補

1. **新規** `market_intelligence_hero_bar.spec.ts`
   - `.mi-hero` 存在 / 3 枚カード / 「重点配信候補」テキスト / hero 内に Hard NG 文字列群（プロジェクト規定の禁止語 4 種：母集団系日本語語彙＋英語スネークケース指標名。具体語は実装者が `feedback_dedup_rules` 系運用ドキュメントを参照）が無いことを `expect(html).not.toContain` で逆証明。

2. **拡張** `market_intelligence_print_theme.spec.ts`
   - emulate media `print` で `getComputedStyle(.mi-hero-card).printColorAdjust === 'exact'`。
   - `.mi-rank-table tr` に `page-break-inside: avoid` 適用確認。

3. **拡張** `market_intelligence_display_rules.spec.ts`
   - `mi-badge-insufficient` がプレースホルダ箇所に出ること。
   - 凡例 (`mi-kpi-legend`) に「実測 / 推定 / 参考 / データ不足」4 つの badge が揃うこと。

4. **拡張** `market_intelligence_variant_isolation.spec.ts`
   - Full / Public / default ページに `.mi-hero` `.mi-badge-insufficient` が現れないこと（隔離維持）。

---

## 6. リスク

| リスク | 内容 | 緩和策 |
|--------|------|-------|
| Hard NG 用語の混入 | hero 第 3 枠で「人口」系の数値を扱うため、不用意に禁止語（母集団系日本語語彙＋英語スネークケース指標名）を書く可能性 | 本書 §2.1 で `workplace_measured` 限定を明記。E2E で逆証明（§5-1）。実装者は禁止語リストを別ドキュメント（運用 feedback メモ）から取得し、commit 前 grep 必須。 |
| variant 隔離崩壊 | hero 用 CSS が `mi-*` 外に漏れると Full/Public に波及 | 全 class を `mi-` prefix で固定。`MI_STYLE_BLOCK` 内のみ。`market_intelligence_variant_isolation.spec.ts` 拡張で検出。 |
| 印刷 CSS 副作用 | `print-color-adjust: exact` を広く当てると意図せぬ枠線が出る | `mi-*` 限定セレクタのみ列挙。`@page` には触れない（cascade trap 回避）。 |
| occupation_cells 空時の hero 第 3 枠 | 「実測値準備中」表示が空なら hero が 3 枠中 1 枠ブランクで違和感 | 第 3 枠を hero から「市内 1 位市区の厚み指数」に降格する fallback 分岐を実装。 |
| ランキング順序入替で既存 E2E 失敗 | smoke spec が DOM 順序を仮定している場合 | smoke spec を「セクション存在のみ」検証に緩める（順序非依存）。 |
| BtoB 評価語 | 「劣位」「集中」「縮小」等の中立性違反語を解釈ストリップで使う恐れ | 解釈文ドラフトを feedback `neutral_expression_for_targets` に照合してから commit。 |
| 設計書自身の Hard NG 混入 | 本書に禁止語を直接書かない | 本書執筆時に grep 自己監査済み（記述ゼロを確認）。 |

---

## 7. 完了条件（実装後）

- `cargo test -p hellowork-server` 全 pass
- `npm run e2e -- market_intelligence` 4 spec + 新 hero spec 全 pass
- 本番 Render デプロイ後、A4 印刷でヒーロー / ランキング / KPI が崩れない（手動目視 1 件）
- 禁止語リスト（運用 feedback で規定）を `grep -rn` した結果が `src/handlers/survey/report_html/market_intelligence.rs` で 0 件

---

以上。
