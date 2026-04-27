# 媒体分析レポート デザイン刷新（Design v2）実装結果（2026-04-26）

## 実装サマリ

V2 HW Dashboard の媒体分析レポート HTML について、コンサル A が顧客に提示できるプロフェッショナルな品質を目標に、**色 + タイポグラフィ + 余白 + ビジュアル要素** の 4 軸でデザイン刷新を実施。

**戦略**: 既存 887 テストを破壊しない `dv2-*` 名前空間による完全分離。`design-v1`（既存）と `design-v2`（刷新）の dual mode。印刷時 (PDF/HTML download) は dv2 を主役として強制適用、画面表示は dual で重畳。

## 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src/handlers/survey/report_html/style.rs` | dv2 セクション CSS 約 320 行追加（既存ルールは一切変更せず） |
| `src/handlers/survey/report_html/helpers.rs` | dv2 ヘルパー 7 関数追加（badge / kpi / databar / progress / icon / trend / cover-highlights） |
| `src/handlers/survey/report_html/mod.rs` | dv2 表紙 (3 段構成) を既存 cover-page の前に追加 + 18 件の contract test 追加 |
| `src/handlers/survey/report_html/executive_summary.rs` | Executive Summary に Section 番号バッジ「01」を付与 |

## 実装内容詳細

### 1. カラースキーム再設計（CSS Variables）

dv2 名前空間に `--dv2-*` プレフィックスで定義。

```css
:root {
  /* 背景・テキスト */
  --dv2-bg: #ffffff;            /* 純白 */
  --dv2-bg-card: #f8fafc;       /* slate-50 */
  --dv2-bg-subtle: #f1f5f9;     /* slate-100 */
  --dv2-border: #e2e8f0;        /* slate-200 */
  --dv2-text: #1e293b;          /* slate-800 */
  --dv2-text-muted: #64748b;    /* slate-500 */
  --dv2-text-faint: #94a3b8;    /* slate-400 */

  /* アクセント (Indigo 中心) */
  --dv2-accent: #4f46e5;        /* indigo-600 */
  --dv2-accent-light: #6366f1;  /* indigo-500 */
  --dv2-accent-soft: #eef2ff;   /* indigo-50 */

  /* Severity (good/warn/crit) */
  --dv2-good: #10b981;          /* emerald-500 */
  --dv2-warn: #f59e0b;          /* amber-500 */
  --dv2-crit: #ef4444;          /* red-500 */

  /* 見出し色 */
  --dv2-navy: #1e293b;          /* slate-800 */
}
```

**設計思想**: 印刷耐性を最優先。light theme で純白背景 + slate ニュートラル + Indigo 単色アクセント。Severity 3 色は意味的に区別（good/warn/crit）。

### 2. タイポグラフィ 4 階層

```css
:root {
  /* Display: 表紙タイトル、Section 大見出し */
  --dv2-fs-display: 32pt;
  --dv2-fs-display-lg: 40pt;

  /* Heading: セクション見出し、KPI 数値 */
  --dv2-fs-heading: 18pt;
  --dv2-fs-heading-lg: 24pt;

  /* Body: 本文、表セル */
  --dv2-fs-body: 11pt;
  --dv2-fs-body-sm: 10.5pt;

  /* Caption: 注記、フッター */
  --dv2-fs-caption: 9pt;
  --dv2-fs-caption-sm: 8.5pt;
}
```

**KPI 数値**: `font-variant-numeric: tabular-nums` で等幅化（縦のラインがピクセル単位で揃う）。

**フォント**: 印刷時は `Noto Sans JP` 優先指定。本文 weight 400、見出し 700、KPI 値 700 + tabular-nums。

### 3. レイアウト刷新

#### 表紙（dv2-cover）3 段構成

```html
<section class="dv2-cover">
  <!-- 上段: ブランド + 生成メタ -->
  <div class="dv2-cover-header">
    <div class="dv2-cover-brand">株式会社For A-career</div>
    <div class="dv2-cover-meta">2026年04月 版</div>
  </div>

  <!-- 中央: タイトル + 副題 + 対象 -->
  <div class="dv2-cover-main">
    <div>
      <div class="dv2-cover-title-accent"></div> <!-- 4px Indigo 縦線 -->
      <h1 class="dv2-cover-title">求人市場<br>総合診断レポート</h1>
      <p class="dv2-cover-subtitle">ハローワーク掲載求人 + アップロード CSV クロス分析</p>
    </div>
    <div class="dv2-cover-target">対象: 東京都 千代田区</div>

    <!-- 下段: ハイライト 3 KPI (横並びカード) -->
    <div class="dv2-cover-highlights">
      <div class="dv2-cover-hl">
        <div class="dv2-cover-hl-label">サンプル件数</div>
        <div class="dv2-cover-hl-value">250<span class="dv2-cover-hl-unit">件</span></div>
      </div>
      <div class="dv2-cover-hl">
        <div class="dv2-cover-hl-label">主要地域</div>
        <div class="dv2-cover-hl-value">東京都 千代田区</div>
      </div>
      <div class="dv2-cover-hl">
        <div class="dv2-cover-hl-label">給与中央値</div>
        <div class="dv2-cover-hl-value">25.5<span class="dv2-cover-hl-unit">万円</span></div>
      </div>
    </div>
  </div>

  <!-- フッター: 機密 + 生成日時 -->
  <div class="dv2-cover-footer">
    <span>この資料は機密情報です...</span>
    <span>生成日時: 2026年04月26日 ...</span>
  </div>
</section>
```

**装飾**: 背景に subtle gradient (`linear-gradient(180deg, #ffffff, #eef2ff)`)。タイトル下に 64×4px の Indigo アクセントバー。対象地域に左 4px の Indigo 縦線。

#### Section 番号バッジ

```html
<div class="dv2-section-heading">
  <span class="dv2-section-badge">01</span>
  <span class="dv2-section-heading-title">Executive Summary</span>
</div>
```

CSS:
```css
.dv2-section-badge {
  width: 32px; height: 32px;
  background: var(--dv2-accent);
  color: #fff;
  font-size: 12pt; font-weight: 700;
  border-radius: 4px;
  font-variant-numeric: tabular-nums;
}
.dv2-section-heading {
  border-left: 4px solid var(--dv2-accent);
  padding: 6px 0 6px 12px;
}
```

#### dv2 KPI カード

```css
.dv2-kpi-card {
  background: var(--dv2-bg-card);
  border: 1px solid var(--dv2-border);
  border-radius: 8px;
  padding: 12px 14px;
}
.dv2-kpi-card.dv2-kpi-large {
  grid-column: span 2; /* 主要 KPI は 2 カラム幅 */
  background: linear-gradient(135deg, var(--dv2-accent-soft), var(--dv2-bg-card));
  border-color: var(--dv2-accent-light);
}
.dv2-kpi-card-value {
  font-size: 26pt; font-weight: 700;
  color: var(--dv2-navy);
  font-variant-numeric: tabular-nums;
}
.dv2-kpi-card.dv2-kpi-large .dv2-kpi-card-value {
  font-size: 32pt; color: var(--dv2-accent);
}
.dv2-kpi-card[data-status="good"] { border-left: 4px solid var(--dv2-good); }
.dv2-kpi-card[data-status="warn"] { border-left: 4px solid var(--dv2-warn); }
.dv2-kpi-card[data-status="crit"] { border-left: 4px solid var(--dv2-crit); }
```

### 4. ビジュアル要素強化

#### データバー（テーブル内の数値の隣に視覚的バー）

```rust
render_dv2_data_bar(75.0, 100.0, "good")
// → <span class="dv2-databar" data-tone="good">
//      <span class="dv2-databar-fill" style="width:75.0%"></span>
//    </span>
```

#### 進捗バー（充足度・パーセンタイル）

```rust
render_dv2_progress_bar(&mut html, 65.0, "65%");
// aria-valuenow / aria-valuemin / aria-valuemax で a11y 対応
```

#### SVG inline icon（4 種）

絵文字を排し、SVG path で軽量・色変更可能に。

```rust
render_dv2_icon("check") // U+2713 → SVG checkmark path
render_dv2_icon("warn")  // U+26A0 → SVG triangle path
render_dv2_icon("crit")  // SVG circle exclamation
render_dv2_icon("info")  // SVG circle info
```

各アイコン:
- 24x24 viewBox
- `currentColor` 継承（CSS で色制御可）
- `aria-hidden="true"` + `focusable="false"`

#### トレンド矢印

```rust
render_dv2_trend("up", "+5.2%")    // ↑ +5.2% (緑)
render_dv2_trend("down", "-3.1%")  // ↓ -3.1% (赤)
render_dv2_trend("flat", "±0.0%")  // → ±0.0% (グレー)
```

`aria-label` で意味を明示（"上昇" / "下落" / "横ばい"）。

### 5. 印刷時の最適化

```css
@media print {
  body {
    font-family: "Noto Sans JP", ...;
    color: var(--dv2-text);
    background: var(--dv2-bg);
  }
  @page {
    size: A4 portrait;
    margin: 15mm 12mm;  /* 上下 15mm / 左右 12mm */

    /* Running header */
    @top-left {
      content: "求人市場 総合診断レポート";
      font-size: 8pt; color: #94a3b8;
    }
    @top-right {
      content: counter(page) " / " counter(pages);
      font-size: 8pt;
      font-variant-numeric: tabular-nums;
    }

    /* Footer */
    @bottom-left {
      content: "株式会社For A-career | 機密情報";
      font-size: 8pt;
    }
  }
  @page :first {
    /* 表紙にはヘッダー/フッターを出さない */
    @top-left { content: ""; }
    @top-right { content: ""; }
    @bottom-left { content: ""; }
  }

  /* 既存 cover-page は印刷時非表示（dv2 表紙に置き換え） */
  .cover-page.cover-legacy { display: none !important; }
}
```

**印刷時のフォントサイズ調整**:
- `dv2-cover-title`: 40pt → 32pt（A4 に収める）
- `dv2-cover-target`: 16pt → 14pt
- `dv2-cover-hl-value`: 18pt → 16pt
- `dv2-section-heading-title`: 18pt → 16pt
- `dv2-kpi-card-value`: 26pt → 22pt
- `dv2-kpi-card.dv2-kpi-large value`: 32pt → 28pt

### 6. アイコン整理

| 種類 | 取り扱い |
|------|---------|
| 装飾絵文字 | 新規追加なし（既存も触らない） |
| 警告 ⚠ (U+26A0) | 維持（テスト互換 + memory ルール） |
| チェック ✓ (U+2713) | 既存維持 + 新規 SVG inline 提供（dv2-icon-check） |
| 数学記号 ≠ | 維持（相関≠因果） |
| カテゴリアイコン | dv2 では SVG inline で統一可能に |

### 7. Tab UI（画面表示）

dv2 KPI カードは `body.theme-dark` で自動的にダークモード対応:

```css
body.theme-dark .dv2-kpi-card {
  background: #1e293b;
  border-color: #334155;
  color: #e2e8f0;
}
```

## カラーパレット定義一覧

| トークン | 値 | 用途 |
|---|---|---|
| `--dv2-bg` | `#ffffff` | 背景（純白） |
| `--dv2-bg-card` | `#f8fafc` | カード背景 |
| `--dv2-bg-subtle` | `#f1f5f9` | サブ背景（datebar track 等） |
| `--dv2-border` | `#e2e8f0` | カード境界 |
| `--dv2-border-strong` | `#cbd5e1` | 強調境界 |
| `--dv2-text` | `#1e293b` | 本文 |
| `--dv2-text-muted` | `#64748b` | サブテキスト |
| `--dv2-text-faint` | `#94a3b8` | フッター/ヘッダー |
| `--dv2-accent` | `#4f46e5` | Primary アクセント |
| `--dv2-accent-light` | `#6366f1` | アクセント明 |
| `--dv2-accent-soft` | `#eef2ff` | アクセント背景 |
| `--dv2-good` | `#10b981` | 成功/良好 |
| `--dv2-warn` | `#f59e0b` | 注意 |
| `--dv2-crit` | `#ef4444` | 重大/警告 |
| `--dv2-navy` | `#1e293b` | 見出し色 |

## タイポグラフィ階層一覧

| 階層 | サイズ | weight | 用途 |
|---|---|---|---|
| Display | 32-40pt | 700 | 表紙タイトル |
| Heading | 18-24pt | 700 | Section 見出し、KPI 値 |
| Body | 10.5-11pt | 400 | 本文、表セル |
| Caption | 8.5-9pt | 400 | 注記、フッター |

## テスト結果

### 全体テスト
```
total tests: 940 (新 dv2 追加 18 件 + 既存 + market_tightness 並列追加)
- design_v2_contract_tests: 18/18 pass (100%)
- 私の Design 担当の変更による既存テスト破壊: 0 件
- ベースライン (Design 着手前): 908 passed
- 残課題: 並列の market_tightness エージェント追加コードに「すべき」「最適」禁止ワードが
  混入しており、p3_spec_9_4_forbidden_word_* 2 件が失敗。Design 担当の責任範囲外。
```

### 追加された 18 件の Contract Tests

| # | テスト名 | 検証内容 |
|---|---|---|
| 1 | `dv2_css_variables_defined` | --dv2-bg / --dv2-accent / --dv2-good/warn/crit / --dv2-fs-* の存在 |
| 2 | `dv2_cover_three_section_layout` | dv2-cover-header / -main / -footer の 3 段構成 |
| 3 | `dv2_cover_has_three_highlight_kpis` | 表紙 3 KPI ハイライトカード（サンプル件数/主要地域/給与中央値） |
| 4 | `dv2_section_badge_on_exec_summary` | Executive Summary に Section 番号バッジ「01」 |
| 5 | `dv2_kpi_card_css_defined` | .dv2-kpi-card / .dv2-kpi-large / data-status による色分け |
| 6 | `dv2_print_mode_activated` | A4 余白 15mm/12mm, running header, bottom-left footer |
| 7 | `dv2_svg_inline_icons_render` | check/warn/crit/info の 4 SVG icon |
| 8 | `dv2_data_bar_renders_correct_percentage` | value/max → width %、tone 属性、max=0 のフォールバック |
| 9 | `dv2_progress_bar_has_a11y_attributes` | role=progressbar / aria-valuenow / valuemin / valuemax |
| 10 | `dv2_trend_arrows_three_directions` | ↑↓→ + dv2-trend-up/down/flat + aria-label |
| 11 | `dv2_accent_color_indigo_defined` | #4f46e5 (indigo-600) の存在 |
| 12 | `dv2_legacy_cover_hidden_in_print` | 既存 cover-page は cover-legacy class でマーク + 印刷時非表示 |
| 13 | `dv2_typography_four_tier_hierarchy` | Display/Heading/Body/Caption + tabular-nums |
| 14 | `dv2_cover_title_accent_bar_present` | 表紙タイトル下の Indigo アクセントバー |
| 15 | `dv2_preserves_memory_rules` | 因果断定回避 + HW スコープ警告の維持 |
| 16 | `dv2_action_card_css_defined` | .dv2-action-card + data-priority 属性 |
| 17 | `dv2_print_typography_noto_sans_jp` | Noto Sans JP の指定 |
| 18 | `dv2_preserves_existing_kpi_labels` | 既存 5 KPI ラベル全維持（互換テスト） |

## memory ルール準拠

| ルール | 対応 |
|---|---|
| `feedback_correlation_not_causation.md` | 因果断定回避維持。dv2 で「相関」「傾向」「仮説」表現を破壊せず、Section バッジと KPI 表示のみ刷新 |
| `feedback_hw_data_scope.md` | HW 限定性の警告は完全維持（`dv2_preserves_memory_rules` テストで機械検証） |
| `feedback_test_data_validation.md` | 「要素存在」だけでなく具体値（width:50.0% / aria-valuenow=65 等）を検証 |
| `feedback_reverse_proof_tests.md` | 4 つの SVG icon が異なる class / path を持つこと、トレンド矢印 3 方向の差異を機械検証 |
| 絵文字最小化方針 | 装飾絵文字の追加なし。⚠（U+26A0）は機能アイコンとして維持。✓ は SVG inline 化の選択肢を提供 |

## API 変更なし

`render_survey_report_page` および `render_survey_report_page_with_enrichment` の公開シグネチャは不変。
変更はすべて HTML 出力内容と CSS のみ。

## dual mode 確認

- **design-v1**: 既存 CSS（slate/navy + 旧 KPI grid）→ 維持。テスト互換 100%
- **design-v2**: 新規追加（dv2-* 名前空間）→ 印刷時主役、画面表示でも併用可
- 既存 `cover-page` は `cover-legacy` class で印刷時非表示にマーキング、HTML 出力としては残存（テスト互換のため）

## 親セッションへの統合チェックリスト

- [x] `style.rs`: dv2 CSS 約 320 行追加（既存ルール非破壊）
- [x] `helpers.rs`: dv2 ヘルパー 7 関数追加
- [x] `mod.rs`: dv2 表紙 (3 段構成) を既存 cover-page の前に挿入
- [x] `mod.rs`: 既存 cover-page を `cover-legacy` class でマーキング + 印刷時非表示
- [x] `executive_summary.rs`: Section 番号バッジ「01」を h2 の前に追加
- [x] `cargo build --lib` パス（warnings のみ）
- [x] `cargo test --lib design_v2_contract_tests`: 18/18 pass
- [x] 既存 readability/ui2/ui3 contract tests: 全 pass（破壊 0 件）
- [x] memory ルール準拠（因果断定回避、HW スコープ維持）
- [x] 公開 API シグネチャ不変
- [ ] **未実装** （task-out-of-scope）:
  - 各 section.rs への Section 番号バッジ追加（Executive Summary のみ実施。02-13 は次フェーズ）
  - 推奨アクションを `dv2-action-card` 形式へ移行（既存 `exec-summary-action` は維持）
  - データバー / 進捗バー / SVG icon の各 section への実適用（helper のみ提供、利用は次フェーズ）

## 今後の発展余地（本タスク範囲外）

本タスクは「**基盤完了**（CSS + helpers + 表紙 + Executive Summary バッジ）」状態。
以下は機械検証可能な class / variable は定義済みだが、各 section.rs への実適用は未実施:

1. **全 13 section の番号バッジ**: Executive Summary のみ完了。02-13 は helper を呼ぶだけで実装可能
2. **dv2-action-card への移行**: 既存 `exec-summary-action` は維持。`render_dv2_action_card` helper は CSS のみ提供
3. **データバー実適用**: `render_dv2_data_bar()` 利用箇所は 0 件（テスト用のみ）。表テンプレに組み込むのは次フェーズ
4. **SVG icon 全置換**: 既存絵文字（📖 📝 等）の SVG 置換は範囲外（互換性優先）

## 変更ファイル絶対パス一覧

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\style.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\helpers.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\executive_summary.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_design_results.md` (本レポート)

## 並列エージェントとの競合状況

| エージェント | 担当範囲 | 競合状態 |
|---|---|---|
| Design (本) | style.rs / helpers.rs / mod.rs (表紙) / executive_summary.rs (バッジ) | 完了 |
| MarketTightness (並列) | 新規 market_tightness.rs / integration.rs / mod.rs (mod 追加) | 完了。ただし禁止ワード「すべき」「最適」混入により p3_spec_9_4_forbidden_word_* 2 件が失敗 |

**注意**: market_tightness.rs の禁止ワード問題は Design 担当の責任範囲外だが、親セッションでの確認を推奨。
