# P1 UX/動線改善 詳細実装プラン

**作成日**: 2026-04-26
**作成者**: P1 プランニングチーム (worktree: agent-af55cf9d28571fa4f)
**対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`)
**配置先**: `docs/audit_2026_04_24/plan_p1_ux_navigation.md`
**前提**: P0 (#1〜#5) は親セッションで対応中。本プランは P1+α+ε の P1 課題の **実装プラン** のみ。コード編集は禁止 (調査+提案)。

---

## エグゼクティブサマリ

### 範囲
監査統合レポート (`00_overall_assessment.md`) の P1 5 件 (#6〜#10) のうち UX 動線関連 3 件 (#6/#7/#9) + ε 推奨 3 件 (47県横断・都道府県カルテ・HW警告集約) + α 推奨 6 件 (overview 遺物処理・dead route 判定・産業セレクト統一・雇用形態セレクト統一・HW banner 追加・相関≠因果文言) の **計 12 課題**。

### キーインサイト
1. **「dead route 6 件」は単純削除ではない**: `/tab/trend`, `/tab/insight` は `analysis/handlers.rs:51,53` のグループボタンから到達可能、`/tab/overview`, `/tab/balance`, `/tab/workstyle` も `cross_nav` リンク (`analysis/render.rs:34,35,62,130,195,196`, `overview.rs:957`) で利用中。**dead** ではなく **「上位ナビ非表示の active route」**。⇒ #6 ナビ昇格で解決し、削除すべきは「未使用テンプレート」のみ。
2. **`templates/tabs/overview.html` は完全 dead** (`include_str!` で参照されるのは competitive/jobmap/recruitment_diag のみ)。balance/workstyle/demographics/region_karte の各 .html も同様に dead だが、`region_karte.html` は印刷用と思われるため要追加調査。
3. **タブ呼称ブレは 4 重 (求人検索/競合調査/企業調査/企業分析)**。「企業分析」だけは別タブ (company)。実質は **3 重ブレ** を「求人検索」に統一すれば解決。
4. **統合 PDF は新規実装大物**だが、既存の `/report/insight`, `/report/survey` を組み合わせる **薄いラッパー** 設計で工数削減可能。
5. **47 県横断ビューは新規** — 詳細分析の `Insight RegionalCompare` をテーブル UI 化する派生で実現可能。

### 優先順位 (期待 ROI 順)
| ランク | 課題 | 期待効果 | 工数 |
|---|---|---|---|
| ⭐⭐⭐ | #6 ナビ昇格 (insight/trend) | ペルソナ全員 +0.5/5 | XS |
| ⭐⭐⭐ | #7 タブ呼称統一 | 全員のオンボーディング | S |
| ⭐⭐⭐ | HW 警告集約 (1 画面 1 回) | B 人事 +0.5/5, 全員疲弊解消 | S |
| ⭐⭐ | 相関≠因果文言 (analysis タブ) | 誠実性メカ完成 | XS |
| ⭐⭐ | overview.html 削除 + dead テンプレ整理 | 事故防止 | S |
| ⭐⭐ | 産業セレクト 13 分類化 (diagnostic) | B 人事の入力ミス防止 | S |
| ⭐⭐ | 雇用形態セレクト統一 | 全員の混乱防止 | S |
| ⭐⭐ | 47 県横断比較ビュー | C リサーチャー +1.0/5 | M |
| ⭐⭐ | 都道府県カルテ (citycode 緩和) | C +0.5/5 | M |
| ⭐⭐ | 統合 PDF レポート | A コンサル +1.5/5 | L |
| ⭐ | 市場概況 H2 直下 HW banner | 誠実性 | XS |
| ⭐ | 企業プロフィール 30-100s 遅延 | B のチャーン抑止 | M |

---

## 課題別 詳細プラン

### P1-01: ナビ動線 — `/tab/insight`, `/tab/trend` を上位ナビへ昇格

**統合 # / 出典**: #6 (00:97-101) / α-Top10 / ε-CC-1 / B+C ペルソナ

#### 現状の問題 (file:line)
- `templates/dashboard_inline.html:70-89` のタブナビは 9 ボタンのみ。`/tab/insight` (総合診断) と `/tab/trend` (時系列) は登場しない。
- `analysis/handlers.rs:51,53` のグループボタンからのみ到達可能。詳細分析 → 「総合診断」グループ切替の二重階層。
- ε 報告 (`team_epsilon_walkthrough.md:84-87, 213-216`): 「USER_GUIDE.md にはトレンドが独立タブのように書かれている」「ナビには無い」という整合不全。

#### 提案する修正方針

**方針 A (推奨, XS 工数)**: 9 タブ → 11 タブに増設し、論理順序を「俯瞰→深堀」で再整列。

```html
<!-- templates/dashboard_inline.html:70-89 を以下に置換 -->
<nav class="bg-navy-800 border-b border-slate-700 px-6 flex gap-1 overflow-x-auto scrollbar-thin"
     role="tablist" aria-label="ダッシュボードタブ" style="-webkit-overflow-scrolling:touch;scrollbar-width:thin">
    <!-- 俯瞰 -->
    <button class="tab-btn active" role="tab" aria-selected="true" hx-get="/tab/market" ...>市場概況</button>
    <button class="tab-btn" role="tab" hx-get="/tab/jobmap" ...>地図</button>
    <button class="tab-btn" role="tab" hx-get="/tab/region_karte" ...>地域カルテ</button>
    <!-- 深掘 -->
    <button class="tab-btn" role="tab" hx-get="/tab/analysis" ...>詳細分析</button>
    <button class="tab-btn" role="tab" hx-get="/tab/insight" ...>総合診断</button>     <!-- NEW -->
    <button class="tab-btn" role="tab" hx-get="/tab/trend" ...>トレンド</button>       <!-- NEW -->
    <!-- 検索/診断 -->
    <button class="tab-btn" role="tab" hx-get="/tab/competitive" ...>求人検索</button>
    <button class="tab-btn" role="tab" hx-get="/tab/diagnostic" ...>条件診断</button>
    <button class="tab-btn" role="tab" hx-get="/tab/recruitment_diag" ...>採用診断</button>
    <button class="tab-btn" role="tab" hx-get="/tab/company" ...>企業検索</button>
    <button class="tab-btn" role="tab" hx-get="/tab/survey" ...>媒体分析</button>
</nav>
```

**方針 B (代替, S 工数)**: 「分析」をドロップダウン化。`詳細分析 ▼` で「構造分析/総合診断/トレンド」を表示。
- メリット: タブ数増加なし
- デメリット: タッチデバイス UX 劣化、HTMX 設定追加

⇒ **方針 A 推奨**。理由: 既存の `analysis/handlers.rs:46-54` のグループ切替は維持 (詳細分析タブ内のサブナビとして機能継続) しつつ、上位からも直接到達可能になる。冗長性は許容範囲。

#### 影響範囲
- 変更ファイル: `templates/dashboard_inline.html` のみ
- テスト対象: 既存 `tests/global_contract_audit_test.rs` でルートカバレッジ確認、Playwright で 11 タブ全クリック検証

#### 工数見積: **XS** (HTML 2 行追加で完了、15 分)

#### リスク
- UX 破壊: なし。タブ追加のみ。
- 既存機能: `analysis/handlers.rs` のグループ切替は残存させるため二重露出になる (insight/trend は上位ナビからもグループからも到達可)。これは「冗長だが害はない」+ 「既存ユーザーの再学習コストゼロ」を優先。
- モバイル: 11 タブで横スクロール頻度増加。`overflow-x-auto` 既設のため致命傷ではない。

#### 検証方法 (E2E)
```
1. dashboard_inline.html を読み込み 11 タブが表示される
2. 「総合診断」クリック → /tab/insight が #content に swap される
3. 「トレンド」クリック → /tab/trend が swap される
4. 詳細分析 → 総合診断グループ切替も並行動作する
5. モバイル幅 (375px) で全タブが横スクロールで到達可能
```

---

### P1-02: タブ呼称 4 重ブレ統一

**統合 # / 出典**: #7 (00:103-108) / α-7.1

#### 現状の問題 (file:line)
| 場所 | 表記 |
|---|---|
| `templates/dashboard_inline.html:79` | UI ボタン「**求人検索**」 |
| URL | `/tab/competitive` |
| `templates/tabs/competitive.html:1` | コメント「タブ8: **競合調査**」 |
| `templates/tabs/competitive.html:3` | H2「🔍 **企業調査**」 |
| `src/handlers/competitive/render.rs:29` | doc コメント「**競合調査**タブの初期HTML」 |
| `src/handlers/competitive/render.rs:524,545` | PDF タイトル「**競合調査**レポート」 |
| `src/handlers/competitive/fetch.rs:64,166` | doc「**競合調査**の基本統計」 |
| `src/handlers/competitive/handlers.rs:22` | doc「タブ8: **競合調査**」 |
| `src/handlers/company/render.rs:8` (別タブ) | H2「🔎 **企業分析**」 |

→ **4 単語が 9 箇所 で混在**。`competitive` URL は変更すると外部ブックマーク破壊リスクのため URL は維持。

#### 提案する修正方針

**統一ターゲット: 「求人検索」** (UI ボタンと一致、現状最多)

| ファイル | 変更前 | 変更後 |
|---|---|---|
| `templates/tabs/competitive.html:1` | `<!-- タブ8: 競合調査 ...` | `<!-- タブ8: 求人検索 ...` |
| `templates/tabs/competitive.html:3` | `🔍 企業調査` | `🔍 求人検索` |
| `src/handlers/competitive/render.rs:29` | `競合調査タブの初期HTML` | `求人検索タブの初期HTML` |
| `src/handlers/competitive/render.rs:524` | `<title>競合調査レポート - ...` | `<title>求人検索レポート - ...` |
| `src/handlers/competitive/render.rs:545` | `<h1>競合調査レポート</h1>` | `<h1>求人検索レポート</h1>` |
| `src/handlers/competitive/fetch.rs:64` | `競合調査の基本統計` | `求人検索の基本統計` |
| `src/handlers/competitive/fetch.rs:166` | `競合調査フィルタ用` | `求人検索フィルタ用` |
| `src/handlers/competitive/handlers.rs:22` | `タブ8: 競合調査` | `タブ8: 求人検索` |
| (※) `src/handlers/competitive/fetch.rs:106` | `fetch_competitive統合クエリ失敗` | tracing log は技術用語のため変更不要 |
| `src/handlers/company/render.rs:8` | `🔎 企業分析` | (別タブのため変更不要、ただし「企業検索」と H2 統一が望ましい) |

**「企業検索」タブ (company) の H2 推奨**: H2 ボタンラベル `企業検索` に揃え、`企業分析` を「企業詳細分析」or 「企業プロフィール」にする。

```rust
// src/handlers/company/render.rs:8 周辺
<h2>🔎 企業検索 <span>{location}</span></h2>  // 旧: 企業分析
```

#### 影響範囲
- 変更ファイル: 4 ファイル (`competitive.html`, `competitive/render.rs`, `competitive/fetch.rs`, `competitive/handlers.rs`) + 1 ファイル (`company/render.rs`)
- テスト対象: `tests/` で `競合調査` `企業調査` リテラル検索が空になることを assert 追加
- PDF レポート出力にも影響 (`render.rs:524,545`)

#### 工数見積: **S** (1 時間、grep + 機械置換)

#### リスク
- UX 破壊: 既存ユーザーの「企業調査」表記に慣れた層は迷う可能性 (リリースノート必須)
- PDF レポートの過去出力ファイル名と新規が乖離 (履歴整合性): フッター日付で識別可能のため許容
- URL `/tab/competitive` は変更しない (互換性維持)

#### 検証方法
```
1. cargo build --release で警告なし
2. grep -r "競合調査\|企業調査" src/ templates/ → 0 件 (handler 関数名 fetch_competitive 等は除外)
3. /tab/competitive 表示 → H2「求人検索」
4. /api/competitive/report HTML → タイトル「求人検索レポート」
5. /tab/company 表示 → H2「企業検索」
```

---

### P1-03: 統合 PDF レポート機能の新規実装

**統合 # / 出典**: #9 (00:120-124) / ε-A-3 (95-96) / α-Top10

#### 現状の問題 (file:line)
| タブ | 出力種類 | ファイル |
|---|---|---|
| 総合診断 | xlsx + JSON + HTML | `insight/render.rs:184-198`, `insight/export.rs` |
| 媒体分析 | HTML + 印刷 | `survey/render.rs:268-321` |
| 採用診断 | **なし** | - |
| 地域カルテ | 印刷 HTML | `region_karte.js:99-110` |
| 詳細分析 | チャート PNG のみ | `static/js/export.js` |

→ コンサル A が「介護×東京の戦略提案 PDF」を作るのに 8 枚スクショ → PowerPoint 貼り込みが必要 (ε-A-1)。

#### 提案する修正方針

**3 段階アプローチ**:

##### Phase 1 (M 工数): 既存 HTML レポートを 1 PDF にまとめるラッパー

- 新規ルート: `GET /report/integrated?prefecture=X&municipality=Y&industry=Z&job_type=W`
- ハンドラ: `src/handlers/integrated_report.rs` 新規作成
- 内部実装: 4 つの内部 fetch を Promise.all で実行、HTML を 1 ファイルに連結

```rust
// src/handlers/integrated_report.rs (擬似コード)
pub async fn integrated_report(State(state): State<Arc<AppState>>, Query(q): Query<ReportQuery>) -> Html<String> {
    let (recruitment_html, survey_html, karte_html, insight_html) = tokio::join!(
        recruitment_diag::generate_report_html(&state, &q),  // 新規 generate_report_html を抽出
        survey::generate_report_html(&state, &q),            // 既存 /report/survey から流用
        region::karte::generate_report_html(&state, &q),     // 既存印刷 HTML から流用
        insight::generate_report_html(&state, &q),           // 既存 /report/insight から流用
    );
    let combined = format!(r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>採用市場統合レポート - {region} × {industry}</title>
            <style>
                @media print {{
                    .page-break {{ page-break-before: always; }}
                }}
            </style>
        </head>
        <body>
            <div class="report-cover">
                <h1>採用市場統合レポート</h1>
                <p>{region} × {industry} × {job_type}</p>
                <p>作成日: {date}</p>
                <p class="hw-scope-note">※ 本レポートはハローワーク掲載求人データに基づきます。
                   民間求人サイト (Indeed等) は含まれません。</p>
            </div>
            <div class="page-break"></div>
            <h2>第1章 採用診断</h2>{recruitment_html}
            <div class="page-break"></div>
            <h2>第2章 地域カルテ</h2>{karte_html}
            <div class="page-break"></div>
            <h2>第3章 媒体比較</h2>{survey_html}
            <div class="page-break"></div>
            <h2>第4章 So What 示唆</h2>{insight_html}
            <footer>
                <p>出典: ハローワーク + e-Stat + Agoop 人流 + SalesNow 企業情報</p>
                <p>※ 相関関係を示すものであり、因果関係を主張するものではありません</p>
            </footer>
        </body>
        </html>
    "#);
    Html(combined)
}
```

クライアント側 (ブラウザ) で `window.print()` → 「PDF として保存」で 1 PDF 生成。

##### Phase 2 (M 工数): サーバーサイド PDF 生成

- 依存追加: `wkhtmltopdf` バイナリ or `headless_chrome` クレート (要 cargo.lock 検討)
- メモリ: Render 無料プランの 512MB 制約に注意
- 代替: `weasyprint` (Python) を sidecar 起動

##### Phase 3 (S 工数): クライアントロゴ・カラー差し替え UI

- `/report/integrated?logo_url=...&primary_color=...` 拡張

#### 影響範囲
- **Phase 1 のみ実装推奨** (P1 の範囲)
- 新規ファイル: `src/handlers/integrated_report.rs`
- 変更ファイル: `src/lib.rs` (route 追加), 各 handler の HTML 生成関数を `pub` 化 (`recruitment_diag/handlers.rs`, `survey/handlers.rs`, `region/karte.rs`, `insight/render.rs`)
- 新規テスト: `tests/integrated_report_e2e.rs`

#### 工数見積: **L** (Phase 1 だけで 2 日、Phase 2 で +2 日、Phase 3 で +1 日)

#### リスク
- UX 破壊: なし (新規エンドポイント)
- 既存機能: 各タブの個別 HTML 生成関数のリファクタが必要 (Public 化)。テスト境界の再定義リスク
- データ依存: 4 つのレポートが個別に「データ未接続」フォールバックする場合、結合レポートが歯抜けに。**「セクション欠落時はそのセクションを `<p>※ データ未投入のため割愛</p>` で表示」** が graceful degradation の継承
- パフォーマンス: 4 レポート並列 fetch で 5-10 秒見込み (現在の採用診断 8 パネル並列ロードと同等)

#### 検証方法
```
1. /report/integrated?prefecture=東京都&industry=医療,福祉&job_type=老人福祉・介護 をブラウザで開く
2. 4 章すべてが表示される (HW 投入済データのみ)
3. window.print() で 1 PDF として保存可能、改ページ正常
4. /report/integrated?prefecture=未投入県 で graceful degradation 確認
5. 採用診断タブの初回ロード時間と比較してレポート生成時間 ≤ 2x
```

---

### P1-04: 47 県横断比較ビュー (リサーチャー C 決定打)

**統合 # / 出典**: ε-C-2 (190-205) / Add-2

#### 現状の問題 (file:line)
- 詳細分析の `Insight RegionalCompare` (`insight/helpers.rs:11`) は generate_insights 結果のフィルタのみ
- 地域カルテ (`region/karte.rs:78`) は **citycode 必須**
- 47 県を一覧する画面が存在しない (リサーチャー C が **47 タブ切替する以外手段がない**)

#### 提案する修正方針

**新規サブタブ追加**: 詳細分析 → 「地域比較」サブタブ追加 (既存サブタブ 6 → サブタブ 7)

##### データ取得
既存の `v2_aggregate_summary` テーブル + `v2_employer_strategy_summary` から都道府県粒度を集計:

```sql
SELECT
    prefecture,
    SUM(postings) AS total_postings,
    AVG(median_salary) AS avg_median_salary,
    AVG(seishain_ratio) AS avg_seishain_ratio,
    AVG(vacancy_rate) AS avg_vacancy_rate,
    AVG(holiday_avg) AS avg_holiday_avg
FROM v2_aggregate_summary
WHERE industry = ?
GROUP BY prefecture
ORDER BY total_postings DESC
LIMIT 47;
```

##### UI 設計

```html
<!-- 詳細分析サブタブ7「47県横断比較」-->
<div class="space-y-4">
    <div class="flex gap-2">
        <select id="cross-pref-metric">
            <option value="postings">求人数</option>
            <option value="median_salary">中央値月給</option>
            <option value="seishain_ratio">正社員率</option>
            <option value="vacancy_rate">欠員補充求人比率</option>
            <option value="holiday_avg">平均年間休日</option>
        </select>
        <select id="cross-pref-sort"><option>降順</option><option>昇順</option></select>
        <button onclick="exportCrossPrefCsv()">CSV ダウンロード</button>
    </div>
    <table class="w-full">
        <thead>
            <tr><th>順位</th><th>都道府県</th><th>選択指標</th><th>全国比</th><th>カルテへ</th></tr>
        </thead>
        <tbody id="cross-pref-tbody">
            <!-- 47行 -->
        </tbody>
    </table>
    <!-- ECharts 横棒グラフ (47本) -->
    <div class="echart" data-chart-config='{...}' style="height:1000px"></div>
</div>
```

##### バックエンド
- 新規ルート: `GET /api/cross_prefecture?metric=X&industry=Y`
- 新規ハンドラ: `src/handlers/cross_prefecture.rs`

#### 影響範囲
- 新規: `src/handlers/cross_prefecture.rs` (250 行程度)
- 変更: `src/handlers/analysis/helpers.rs` の `ANALYSIS_SUBTABS` に追加、`src/handlers/analysis/render.rs` に `render_subtab_7` 追加、`src/handlers/analysis/handlers.rs` で受付け
- 新規: `tests/cross_prefecture_e2e.rs`

#### 工数見積: **M** (1.5 日)

#### リスク
- UX 破壊: なし
- データ依存: `v2_aggregate_summary` の都道府県粒度集計が完全か要検証 (β/γ 確認推奨)
- パフォーマンス: 47 行のテーブルは軽量、ECharts も問題なし

#### 検証方法
```
1. 詳細分析タブ → 「地域比較」サブタブクリック
2. 指標を「求人数」→「中央値月給」へ切替 → テーブル+チャート再描画
3. CSV ダウンロードで 47 行+ヘッダ確認
4. 各行「カルテへ」リンクで都道府県カルテ (P1-05) へ遷移
5. 産業フィルタ未指定で全産業集計を確認
```

---

### P1-05: 地域カルテを都道府県粒度でも生成可能に (citycode 必須緩和)

**統合 # / 出典**: ε-C-2 (198-203) / Add-5

#### 現状の問題 (file:line)
- `src/handlers/region/karte.rs:76-78` で「市区町村未選択時のガイダンス画面」を `render_empty_guide()` 表示。citycode 必須。
- リサーチャーが「神奈川県全体のカルテ」を作れない → 政令指定都市の集計困難。

#### 提案する修正方針

**`scope` パラメータ導入**:

```rust
// src/handlers/region/karte.rs

pub async fn tab_region_karte(...) -> Html<String> {
    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();

    if pref.is_empty() {
        return Html(render_empty_guide()); // 都道府県未選択は従来通りガイド
    }

    // NEW: 都道府県のみ指定の場合
    let scope = if muni.is_empty() {
        KarteScope::Prefecture(pref)
    } else {
        KarteScope::Municipality { pref, muni }
    };

    // 既存ロジックを scope で分岐
    match scope {
        KarteScope::Prefecture(p) => render_karte_prefecture(&state, &p).await,
        KarteScope::Municipality { pref, muni } => render_karte_municipality(&state, &pref, &muni).await,
    }
}
```

##### KPI / セクション差分

| セクション | 市区町村粒度 | 都道府県粒度 | 備考 |
|---|---|---|---|
| KPI 9 (求人/給与/正社員率) | あり | あり (集計) | OK |
| 人口動態 | citycode | prefcode | e-Stat 都道府県粒度あり |
| 産業構造 | citycode | prefcode | OK |
| 福祉 (病院/介護施設) | citycode | prefcode (合算) | OK |
| 人流 (Agoop) | mesh1km × city | mesh1km × pref | 集計重い、要 LIMIT |
| So What 示唆 | あり | あり (生成は可能) | エンジン共通 |
| 出典 | あり | あり | OK |

#### 影響範囲
- 変更: `src/handlers/region/karte.rs` (集計クエリ条件分岐 100 行程度の追加)
- 変更: `templates/tabs/region_karte.html` (もし参照されているなら)。実際は `render_karte_municipality` 内で動的 HTML 生成のため変更不要
- テスト: `tests/region_karte_prefecture_e2e.rs` (新規)

#### 工数見積: **M** (1 日)

#### リスク
- UX 破壊: 既存「市区町村必須」UI の挙動変更。要動作確認
- データ依存: Agoop 人流 mesh1km の都道府県集計が重い可能性 → 専用ビュー `v2_agoop_pref_summary` の作成検討
- 数値妥当性: 都道府県平均 vs 全国平均の比較指標の意味 (γ チームに事前確認推奨)

#### 検証方法
```
1. 産業: 医療,福祉, 都道府県: 神奈川県, 市区町村: 空 → 「神奈川県カルテ」表示
2. KPI 9 すべて値あり、人口動態は神奈川県データ
3. 人流ヒートマップが県全域でレンダリング
4. 「市区町村絞り込み」リンクで横浜市カルテへドリルダウン可能
5. 印刷 PDF が 1 ページに収まる (現在の市区町村カルテと同等)
```

---

### P1-06: HW 限定性警告の集約 (1 画面 1 回ルール)

**統合 # / 出典**: ε-CC-2 (240-241) / α-6.1 / 全ペルソナ

#### 現状の問題 (file:line)
- `templates/tabs/recruitment_diag.html:22-24, 179-190, 230-234` で **同じ画面に 3 回** 表示
- ε 報告: 「誠実だが、繰り返しすぎでユーザーは読まなくなる (学習性無視)」
- 一方、市場概況・求人検索・地図のメインページには H2 直下に banner なし (α-6.1, 表 124 行)

#### 提案する修正方針

**ルール化**: 「タブごとに H2 直下に 1 回、フッターに 1 回」を上限。Panel 内バナーは削除して「詳細はガイド (?) アイコン参照」に集約。

##### 変更マトリクス

| ファイル | 現状 | 変更後 |
|---|---|---|
| `templates/tabs/recruitment_diag.html:22-24` | H2 下 banner | **維持** |
| `templates/tabs/recruitment_diag.html:179-190` | Panel 5 amber banner (10 行) | **2 行に短縮** + 「→ ガイド」リンク |
| `templates/tabs/recruitment_diag.html:230-234` | フッター注意書き | **維持** |
| `src/handlers/overview.rs` market H2 直下 | なし | **追加 (α-6.1)** |
| `src/handlers/competitive/render.rs` H2 直下 | なし | **追加** |
| `src/handlers/jobmap/render.rs` H2 直下 | 求人マーカーへの注意なし | **追加** |
| `src/handlers/analysis/render.rs` フッター | なし | **追加 (相関≠因果と一括)** |

##### 共通コンポーネント化

```rust
// src/handlers/common.rs (新規 or 既存ファイルに追加)
pub fn hw_scope_banner_h2() -> &'static str {
    r#"<div class="bg-amber-900/20 border-l-2 border-amber-500 px-3 py-1.5 text-xs text-amber-200 mb-3">
       ⚠️ ハローワーク掲載求人のみが対象です (民間サイトは含まれません)。
       <a href="/tab/guide" class="underline ml-1">詳細</a>
       </div>"#
}
pub fn hw_scope_banner_footer() -> &'static str {
    r#"<p class="text-xs text-slate-500 mt-4 pt-2 border-t border-slate-800">
       出典: ハローワーク掲載求人 / e-Stat / Agoop / SalesNow ・ 相関≠因果
       </p>"#
}
```

→ 各 render で 2 回呼び出し。Panel 内の冗長 banner は撤廃。

#### 影響範囲
- 新規: `src/handlers/common.rs` (or 既存 `helpers.rs` 統合)
- 変更: 7 ファイル (recruitment_diag.html, overview.rs, competitive/render.rs, jobmap/render.rs, analysis/render.rs + 2)
- メモリルール `feedback_hw_data_scope.md` への適合 (姿勢は維持、表現を集約)

#### 工数見積: **S** (1 日、テスト含む)

#### リスク
- UX 破壊: 既存ユーザーが「Panel 5 の警告がなくなった = データが変わった」と誤解する可能性 → リリースノートに「警告は H2/フッターに集約しました」と明記
- 誠実性低下: メモリルール準拠のため、**完全削除ではなく 1 行短縮 + リンク先で詳細** を維持
- E2E: 既存テストで「Panel 5 amber banner 文字列」を assert している箇所があれば緩和必要

#### 検証方法
```
1. 採用診断タブ表示 → H2 下 1 回、Panel 5 短縮、フッター 1 回 = 3 箇所だが各 1-2 行
2. 市場概況タブ表示 → H2 下 banner、フッター = 新規追加
3. 求人検索タブ表示 → 同上
4. 地図タブ表示 → 求人マーカー注意も含む
5. grep -r "民間求人サイト" templates/ src/ → ガイド誘導 + 共通関数のみ参照
```

---

### P1-07: 詳細分析タブに「相関≠因果」明示文言追加

**統合 # / 出典**: α-6.2 (144) / 5.0 推奨

#### 現状の問題
- α 6.2 grep 結果: insight/karte/recruitment_diag/correlation で網羅的
- **詳細分析 (`/tab/analysis`) に明示なし**
- ペルソナ C リサーチャーが詳細分析を多用するため、誠実性メカ完成のため必須

#### 提案する修正方針

`src/handlers/analysis/handlers.rs:96-97` の widget div の前後にフッター追加:

```rust
// src/handlers/analysis/handlers.rs:96 周辺
html.push_str(r##"<div hx-get="/api/insight/widget/analysis" hx-trigger="load" hx-swap="innerHTML"></div>"##);
html.push_str(crate::handlers::common::hw_scope_banner_footer()); // P1-06 で作る共通関数
html.push_str(r##"<p class="text-xs text-slate-500 mt-2">
   ※ 本タブの統計指標は相関を示すものであり、因果関係を主張するものではありません。
   「傾向」「可能性」の範囲で解釈してください。
   </p>"##);
html.push_str("</div>");
```

#### 影響範囲
- 変更: `src/handlers/analysis/handlers.rs` (1 箇所)
- P1-06 の共通関数依存

#### 工数見積: **XS** (15 分)

#### リスク
- なし

#### 検証方法
```
1. /tab/analysis 開く → フッターに「相関≠因果」文言あり
2. grep で "相関" "因果" がフッター文言と一致
```

---

### P1-08: `templates/tabs/overview.html` の取扱い + dead テンプレート判定

**統合 # / 出典**: α-Top10-1 / α 残課題-1,5

#### 現状の問題 (file:line)
- `templates/tabs/overview.html:1` 「タブ1: **求職者**概況」 = V1 (job_seeker) の遺物
- `:18` `{{AVG_AGE}}` ラベルが「平均月給」、`:23` `{{MALE_COUNT}}` が「正社員数」 ← 変数名と意味が完全に取り違え
- include_str! 検索結果: `competitive.html`, `jobmap.html`, `recruitment_diag.html` のみ参照。**`overview.html` は完全 dead**
- 同様に `balance.html` (63行), `demographics.html` (116行), `workstyle.html` (90行), `region_karte.html` も include_str! 未参照
- ただし `templates/tabs/CLAUDE.md` ファイルが存在 (要確認)

#### 提案する修正方針

##### Step 1: 確認 (10 分)
```bash
# 各テンプレートの参照有無を完全 grep
grep -r "tabs/overview" "C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src" \
       "C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/static" \
       "C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/templates"
# 同様に balance, demographics, workstyle, region_karte, trend, insight
```
→ 0 件なら dead 確定

##### Step 2: 削除対象判定

| ファイル | 判定 | 対応 |
|---|---|---|
| `templates/tabs/overview.html` (95行) | dead (V1 遺物) | **削除** |
| `templates/tabs/balance.html` (63行) | dead (animated HTML 生成は `balance.rs::build_balance_html()`) | **削除** |
| `templates/tabs/demographics.html` (116行) | dead 推定 | **要再 grep + 削除** |
| `templates/tabs/workstyle.html` (90行) | dead 推定 | **要再 grep + 削除** |
| `templates/tabs/region_karte.html` | 印刷用かも | **要再 grep + 削除/維持** |
| `templates/tabs/competitive.html` | active | 維持 |
| `templates/tabs/jobmap.html` | active | 維持 |
| `templates/tabs/recruitment_diag.html` | active | 維持 |
| `templates/dashboard.html` (V1) | dead 推定 | **要 grep** |

##### Step 3: dead route の取扱い

| ルート | 内部リンク元 | 判定 | 対応 |
|---|---|---|---|
| `/tab/overview` | `analysis/render.rs:34,196` cross_nav | active (内部リンク) | **維持** |
| `/tab/balance` | `analysis/render.rs:35,130` cross_nav | active | **維持** |
| `/tab/workstyle` | `overview.rs:957` cross_nav | active | **維持** |
| `/tab/demographics` | (要確認) | 未確認 | 確認後判定 |
| `/tab/trend` | `analysis/handlers.rs:51` グループ | active (P1-01 で上位ナビ昇格) | **維持** |
| `/tab/insight` | `analysis/handlers.rs:53` グループ + `insight/render.rs:232` widget link | active | **維持** |

⇒ ルート 6 件は **dead ではない**。問題は「上位ナビに非表示」のみ。P1-01 で trend/insight 解決、overview/balance/workstyle/demographics は cross_nav で深掘り経路として残す。

#### 影響範囲
- 削除: テンプレートファイル 4-5 個 (合計 ~400 行)
- 変更: なし (route と handler は維持)
- テスト: `tests/dead_template_test.rs` 新規 — `templates/tabs/` 配下の各 .html が `include_str!` または `read_to_string` で参照されているか static check

#### 工数見積: **S** (4 時間、確認 grep + 削除 + テスト追加)

#### リスク
- 削除誤り: 印刷用 HTML やテスト用フィクスチャの可能性 → Step 1 の grep を `tests/`, `static/`, `docs/`, `e2e_*.py` まで拡張
- メモリルール `feedback_git_safety.md` 準拠 — 削除はファイル名指定 (`git rm templates/tabs/overview.html`)、`git add -A` 禁止

#### 検証方法
```
1. cargo build --release で警告なし、エラーなし
2. 全タブを E2E で巡回 → 表示崩れなし
3. cross_nav リンクから /tab/overview 等が表示される
4. テスト: tests/dead_template_test.rs が pass
```

---

### P1-09: 条件診断の「産業」を 13 分類セレクト化

**統合 # / 出典**: ε-Remove-2 (313) / ε-CC-4 (256-262)

#### 現状の問題 (file:line)
- `src/handlers/diagnostic.rs:66` `<input type="text">` 自由記述
- 採用診断は 13 分類固定セレクト (`templates/tabs/recruitment_diag.html:36-52`)
- ヘッダーフィルタは 2 階層ツリー
- ⇒ **3 種類の産業選択 UI が混在**
- B 人事ペルソナ: 「介護」と入力しても `industry` パラメータがマッチせず、ベンチマーク欠落リスク

#### 提案する修正方針

採用診断と同じ 13 分類セレクトに統一:

```rust
// src/handlers/diagnostic.rs:66 周辺
// 旧:
// <input type="text" name="industry" placeholder="例: 医療、介護" ...>

// 新:
<select name="industry" class="bg-navy-700 ...">
    <option value="">選択してください</option>
    <option value="医療,福祉">医療,福祉</option>
    <option value="老人福祉・介護">老人福祉・介護</option>
    <option value="情報通信業">情報通信業</option>
    <!-- ... 13 分類 ... -->
</select>
```

13 分類は recruitment_diag のものと完全一致。共通化:

```rust
// src/handlers/common.rs (P1-06 で作る)
pub const INDUSTRY_13_OPTIONS: &[(&str, &str)] = &[
    ("医療,福祉", "医療,福祉"),
    ("老人福祉・介護", "老人福祉・介護"),
    // ...
];
pub fn render_industry_select(name: &str, selected: &str) -> String { ... }
```

→ recruitment_diag/handlers.rs と diagnostic.rs 両方から呼び出し。

#### 影響範囲
- 変更: `src/handlers/diagnostic.rs` (フォーム HTML 部分)
- 変更: `src/handlers/recruitment_diag/render.rs:29-43` (DRY 化)
- 新規 or 拡張: `src/handlers/common.rs`

#### 工数見積: **S** (3 時間)

#### リスク
- UX 破壊: 既存ユーザーが「自分の業種が 13 分類にない」場合の対応 → 「その他」or「上記以外」オプション追加
- データ整合: 既存の自由記述で投入されたセッションデータの後方互換 (URL パラメータ `?industry=自由テキスト` の扱い)

#### 検証方法
```
1. /tab/diagnostic 開く → 産業セレクトボックスが 13+1 分類
2. 「医療,福祉」選択 → 診断実行 → ベンチマークマッチ確認
3. 採用診断と条件診断のセレクト選択肢が完全一致
```

---

### P1-10: 雇用形態セレクトの選択肢統一

**統合 # / 出典**: α-Top10-7 / α-7.6

#### 現状の問題 (file:line)
| ファイル | 行 | 選択肢 |
|---|---|---|
| `templates/tabs/competitive.html:44-47` | 3種 | 正社員/契約社員/パート |
| `templates/tabs/recruitment_diag.html:60-63` | 3種 | 正社員/パート/その他 |
| `templates/tabs/jobmap.html:35-39` | 4種 | 正社員/契約社員/パート/業務委託 |

→ 同じユーザーがタブ移動するたびに「契約社員はどこに行った?」「業務委託は?」と迷う。

#### 提案する修正方針

**統一基準: 4 種 (正社員/契約社員/パート/業務委託)** + 「全て」「その他」

memory ルール `feedback_dedup_rules.md` (2026-02-24) に「雇用形態が異なるレコードは重複ではない」とあり、employment_type は dedup キー必須。⇒ UI でも全種類を見せるべき。

```html
<!-- 全タブで以下を統一 -->
<select id="emp-type" name="employment_type" class="bg-navy-700 ...">
    <option value="">全て</option>
    <option value="正社員">正社員</option>
    <option value="契約社員">契約社員</option>
    <option value="パート">パート</option>
    <option value="業務委託">業務委託</option>
    <option value="その他">その他</option>
</select>
```

ただし、recruitment_diag は分類ロジック (`emp_classifier` 課題、γ チーム#8) と整合する必要あり。`expand_employment_type` で「契約社員→正社員グループ」の集約なら、UI 上は 4 種選択させても内部で 3 グループに集約する方針が妥当。

##### 共通化

```rust
// src/handlers/common.rs
pub const EMPLOYMENT_TYPES: &[(&str, &str)] = &[
    ("", "全て"),
    ("正社員", "正社員"),
    ("契約社員", "契約社員"),
    ("パート", "パート"),
    ("業務委託", "業務委託"),
    ("その他", "その他"),
];
pub fn render_employment_select(name: &str, selected: &str) -> String { ... }
```

#### 影響範囲
- 変更: 3 テンプレート (competitive/recruitment_diag/jobmap) + 共通関数
- 注: recruitment_diag のフィルタロジックは `expand_employment_type` で内部集約。UI 統一しても集計値の整合性が崩れない

#### 工数見積: **S** (4 時間、テスト含む)

#### リスク
- データ整合: jobmap.html だけ「業務委託」が存在し、competitive にない理由を要確認 (γ チームの emp_classifier の進捗待ち)
- UX 破壊: なし (選択肢追加方向のみ)

#### 検証方法
```
1. 全タブで同じ選択肢 6 種が表示
2. 「業務委託」を選択 → competitive で結果あり (現状は選べない)
3. recruitment_diag で「業務委託」選択 → 内部で expand_employment_type 経由で正社員グループに集約 (γ #8 修正後)
```

---

### P1-11: 市場概況/詳細分析の H2 直下に HW 限定 banner 追加

**統合 # / 出典**: α-Top10-4 / α-6.1 / P1-06 の一環

#### 現状の問題
- α-6.1 表 (124 行): 市場概況のメインページ H2 直下に banner なし。フッター注記のみ
- 同様に求人検索 (competitive)・地図 (jobmap) も H2 直下に banner なし

#### 提案する修正方針

P1-06 で作る `hw_scope_banner_h2()` 共通関数を以下に挿入:

```rust
// src/handlers/overview.rs:867 周辺 (H2 出力直後)
html.push_str(&format!(
    r#"<h2 class="text-xl font-bold text-white">📊 市場概況 <span ...>{location} {industry}</span></h2>"#
));
html.push_str(crate::handlers::common::hw_scope_banner_h2()); // NEW
// ... 既存の KPI 出力
```

同様の挿入箇所:
- `src/handlers/competitive/render.rs` (or `templates/tabs/competitive.html:3` 直下)
- `src/handlers/jobmap/render.rs` (or `templates/tabs/jobmap.html` H2 直下)
- `src/handlers/analysis/handlers.rs:42` (詳細分析 H2 直下)

#### 影響範囲
- 変更: 4 ファイル
- P1-06 共通関数依存

#### 工数見積: **XS** (1 時間)

#### リスク
- なし (誠実性向上方向)

#### 検証方法
```
1. /tab/market, /tab/competitive, /tab/jobmap, /tab/analysis 各々で H2 直下 1 行 banner 確認
2. 既存 recruitment_diag.html:22-24 の banner と統一表示
3. grep でちゃんと共通関数経由になっていることを確認
```

---

### P1-12: 企業プロフィール初回 30-100 秒遅延の解消方針

**統合 # / 出典**: ε-B-3 (152-160)

#### 現状の問題 (file:line)
- `src/handlers/company/handlers.rs:62` コメント: 「初回生成に 30〜100秒かかる」「同じ corporate_number への再アクセスは AppCache に 15分 TTL で保持」
- B 人事ペルソナのチャーン要因

#### 提案する修正方針

**段階 lazy load + プリレンダ**

##### 段階 1: 即レスポンス + skeleton
```rust
// /api/company/profile/{corporate_number} の改修
pub async fn company_profile(...) -> Html<String> {
    // 1. 5秒以内にレスポンス可能な部分のみ即返却
    let (basic, hw_summary) = tokio::join!(
        fetch_salesnow_basic(&state, corp_no), // SalesNow 基本情報 (高速)
        fetch_hw_summary(&state, corp_no),     // HW 求人数のみ (高速)
    );
    let html = render_skeleton_with_basic(&basic, &hw_summary);
    // 残りのセクションは HTMX で個別 fetch
    Html(html + r#"
        <div hx-get="/api/company/profile/{corp_no}/section/financial" hx-trigger="load"></div>
        <div hx-get="/api/company/profile/{corp_no}/section/competitors" hx-trigger="load"></div>
        <div hx-get="/api/company/profile/{corp_no}/section/postings_detail" hx-trigger="load"></div>
        <div hx-get="/api/company/profile/{corp_no}/section/recruitment_history" hx-trigger="load"></div>
    "#)
}
```

##### 段階 2: バックグラウンド プリレンダ
- ヒット率の高い corporate_number (top 1000) を起動時に warm up
- AppCache 既存機能流用、TTL を 15分 → 6 時間に延長

##### 段階 3: 重い計算の事前 ETL
- `recruitment_history` のような時系列クエリは Turso 投入時に事前計算しておく

#### 影響範囲
- 変更: `src/handlers/company/handlers.rs` (大改修)
- 新規: `src/handlers/company/sections.rs` (各セクションの hx-get エンドポイント分割)
- 設定: `AppCache` の TTL 拡張 (`config.rs`)
- ETL: hellowork_etl.py に事前計算 step 追加

#### 工数見積: **M** (2-3 日。ただし P1 範囲では「段階 1 のみ」を推奨し、**段階 2,3 は P2 へ移送**)

#### リスク
- データ整合: 各セクションが独立 fetch するため transaction 境界が壊れる → 各 fetch の時刻スタンプを表示
- UX 破壊: skeleton 表示中の見栄えが悪い → spinner デザイン重要

#### 検証方法
```
1. 初回プロフィール表示 → 5秒以内に基本情報 + skeleton 表示
2. 各セクションが順次 fill される (合計 30-60 秒)
3. 2回目以降は AppCache hit で即時表示
4. Render cold start シナリオでも段階表示が機能
```

---

## 依存関係マップ

```
[共通関数の作成 (P1-06 prerequisites)]
└── src/handlers/common.rs 新規作成
    ├── hw_scope_banner_h2()
    ├── hw_scope_banner_footer()
    ├── INDUSTRY_13_OPTIONS / render_industry_select()
    └── EMPLOYMENT_TYPES / render_employment_select()

[Phase 1: Quick Wins (1日以内)]
P1-01 ナビ昇格 (XS, 単独実装可)
P1-07 相関≠因果文言 (XS, common.rs 依存) ← P1-06 後
P1-11 H2 直下 banner (XS, common.rs 依存) ← P1-06 後

[Phase 2: 表記統一 (1-2日)]
P1-02 タブ呼称統一 (S, 単独実装可)
P1-09 産業セレクト (S, common.rs 依存) ← P1-06 後
P1-10 雇用形態セレクト (S, common.rs 依存) ← P1-06 後
P1-06 HW 警告集約 (S, common.rs と同時) ★前提

[Phase 3: クリーンアップ (1日)]
P1-08 dead テンプレ削除 (S, 確認後)

[Phase 4: 新機能 (1週間以上)]
P1-04 47 県横断比較 (M, 単独実装可)
P1-05 都道府県カルテ (M, 単独実装可)
P1-12 プロフィール遅延解消 (M, 段階 1 のみ)
P1-03 統合 PDF (L, 各タブの公開関数化が前提)

依存方向:
  P1-06 (common.rs) ──→ P1-07, P1-09, P1-10, P1-11
  P1-02 (タブ呼称)  ──→ P1-03 (PDF タイトル統一)
  P1-04 (47県比較)  ──→ P1-05 (カルテリンク先)
```

### 推奨実装順序

```
Day 1 (Quick Wins, 半日):
  ✓ P1-01 ナビ昇格 (15min)
  ✓ P1-06 共通関数 + HW 警告集約 (4時間)
  ✓ P1-07 相関≠因果文言 (15min)
  ✓ P1-11 H2 直下 banner (1時間)

Day 2 (表記統一):
  ✓ P1-02 タブ呼称統一 (1時間)
  ✓ P1-09 産業セレクト 13分類 (3時間)
  ✓ P1-10 雇用形態セレクト統一 (4時間)

Day 3 (クリーンアップ):
  ✓ P1-08 dead テンプレ削除 + テスト追加 (4時間)

Day 4-5 (リサーチャー C 機能):
  ✓ P1-04 47県横断比較 (1.5日)
  ✓ P1-05 都道府県カルテ (1日 ※並列可)

Day 6-7 (コンサル A 決定打):
  ✓ P1-03 統合 PDF Phase 1 (2日)

Day 8-9 (B 人事チャーン抑止):
  ✓ P1-12 プロフィール遅延 段階1 (2日)
```

---

## Quick Wins セクション (1 時間以内)

| # | 課題 | 工数 | 効果 |
|---|---|---|---|
| **P1-01** | ナビ昇格 (insight/trend を上位ナビへ) | **15分** | ペルソナ全員 +0.5/5、機能発見性 50% UP |
| **P1-07** | 詳細分析タブに相関≠因果文言 | **15分** | 誠実性メカ完成 |
| **P1-11** | 市場概況/求人検索/地図/詳細分析の H2 直下 banner | **1時間** | 誠実性向上 (ただし P1-06 共通関数を先に作る場合 +30分) |
| **市場概況 KPI tooltip** | 「平均月給」KPI に「HW は市場実勢より低めの慣習あり」tooltip | **30分** | α-Top10-8 |
| **`/my/profile`, `/my/activity` の整理** | ヘッダー上テキストリンクをユーザーメニュードロップダウンへ | **1時間** | ε-Remove-3 |

→ **計 3 時間で 5 つの即時改善が完了**。

---

## 検出した追加課題 (監査レポートに無いもの)

監査統合レポートで明示されていなかったが、本プラン作成中に発見した課題:

### A1: `/api/insight/report` の外部利用者状況確認 (要)
- α-残課題には「dead route 6 件」とあるが、`docs/contract_audit_2026_04_23.md:30` には `/api/insight/report` JSON は frontend consumer なしと記載
- 一方 `e2e_final_verification.py:517,557` で REPORT-01 / REPORT-03 として E2E カバレッジあり
- 状況: **テスト用エンドポイント** 化している可能性。`integrated_report` (P1-03) で代替可能か再判定推奨

### A2: テンプレート CLAUDE.md の存在
- `templates/tabs/CLAUDE.md` ファイルが存在 (`ls` 結果)
- これは何? 各テンプレートの規約? V1/V2 区別 marker? P1-08 削除前に内容確認必須

### A3: `templates/dashboard.html` (V1) と `dashboard_inline.html` (V2) の併存
- α 残課題 2 で指摘あり
- ルーティング `lib.rs:79-237` に V1 用の route が残っているか要確認 (sandbox 制約で未調査)
- 統合 PDF (P1-03) 実装時に「V1 用 endpoint を呼んだら何が起きるか」を確認

### A4: 内部 cross_nav の表記揺れ
- `analysis/render.rs:34` 「地域概況」 ← `/tab/overview` のリンク文言
- `analysis/render.rs:35` 「企業分析」 ← `/tab/balance` のリンク文言
- `overview.rs:957` 「雇用形態詳細」 ← `/tab/workstyle` のリンク文言
- これらの文言と上位ナビ文言の一貫性検証 (P1-02 タブ呼称統一の延長)

### A5: モバイル時の 11 タブ horizontal scroll の発見性
- P1-01 で 9 → 11 タブに増設後、375px 幅では 5 タブ程度しか初期表示されない
- 「右にスクロール可能」インジケータの追加検討 (`templates/dashboard_inline.html:70` の nav に陰影 or 矢印アイコン)

### A6: ガイド (`?` アイコン) の発見性低い
- ε-CC-1 でも指摘あり
- `dashboard_inline.html:61-62` の `?` ボタンは小さい
- 推奨: 「初回ログイン時にガイドを開くオンボーディング」or 「右下フローティングボタン」

---

## 親セッションへの申し送り Top 5

### 1. **共通関数 `src/handlers/common.rs` を最優先で作成**
P1-06/07/09/10/11 のすべてが依存する。各課題の修正を独立に進めるのではなく、`common.rs` を先に作って参照させる方が DRY で工数削減 (重複定義 5 箇所 → 1 箇所)。

### 2. **「dead route 6 件」は実は active**
team_delta_codehealth.md と team_alpha_userfacing.md の指摘した「dead route」は cross_nav リンクで生きている。**route と handler は維持**。dead なのは「上位ナビ非表示」と「テンプレート 4-5 個」のみ。P1-01 ナビ昇格で trend/insight は解決、overview/balance/workstyle/demographics の cross_nav は深掘り経路として残す方針が UX/コード健全性両立。

### 3. **P1-03 統合 PDF は「Phase 1 のみ」で P1 範囲完了とする**
`/report/integrated` の HTML 連結ラッパー (Phase 1) で「window.print() → PDF」が成立。サーバーサイド PDF 生成 (Phase 2) と差し替え UI (Phase 3) は P2 移送で問題なし。各タブの個別 HTML 生成関数を `pub` 化するリファクタ作業が前提なので、γ チームの `emp_classifier` 単一化と並行で進めると衝突リスクあり。**先後関係注意**。

### 4. **タブ呼称統一は「求人検索」固定。`/tab/competitive` URL は変更しない**
外部ブックマーク互換性のため URL は維持。9 箇所の Rust/HTML 文字列のみ置換。`fetch_competitive` 関数名や tracing log の `"fetch_competitive統合クエリ失敗"` は技術用語として残す。これにより `cargo build` で関数シグネチャ変更による波及はゼロ。

### 5. **検出した追加課題 6 件のうち 3 件は P1 完了後に再監査必要**
- A1 `/api/insight/report` の外部利用者: nginx ログ要 (本監査スコープ外)
- A2 `templates/tabs/CLAUDE.md` の内容: 削除判断前の必読
- A3 V1 `templates/dashboard.html` の生死: ルーティング再確認

これらは P1 着手前に「**1 時間程度の追加調査セッション**」で解決推奨。誤って削除するとロールバック工数が大きい (memory ルール `feedback_partial_commit_verify.md` 準拠)。

---

## 検証方法のまとめ (E2E シナリオ集)

### 統合 E2E (Day 1-3 完了後)
```yaml
scenario: "ペルソナ B - 採用診断完遂"
steps:
  - login
  - navigate: /tab/recruitment_diag
  - assert: H2 直下 HW banner 1行 (短縮版)
  - assert: Panel 5 amber banner が短縮されている
  - assert: フッター注釈 1 回のみ
  - select: 業種 = "老人福祉・介護" (13 分類セレクトから)
  - select: 雇用形態 = "正社員" (4種統一)
  - click: 診断実行
  - wait: 8 panel 並列ロード完了
  - assert: 各 panel データあり
  - assert: タブナビに「総合診断」「トレンド」表示

scenario: "ペルソナ C - 47県比較"
steps:
  - navigate: /tab/analysis
  - click: サブタブ「地域比較」 (新規)
  - assert: 47行テーブル表示
  - select: 指標 = "中央値月給"
  - assert: テーブル+チャート再描画
  - click: CSV ダウンロード
  - assert: 47行 + ヘッダのCSV
  - click: 1行目「カルテへ」リンク
  - assert: 都道府県カルテ表示 (citycode 不要)

scenario: "ペルソナ A - 統合 PDF"
steps:
  - select: 都道府県=東京都, 産業=医療,福祉
  - navigate: /report/integrated?prefecture=東京都&industry=医療,福祉
  - assert: 4章構成で表示
  - print: window.print() → PDF
  - assert: 改ページ正常、HW 限定注記がカバーページに 1 回
```

---

**プラン終了**
**次回**: P1 着手前に追加課題 A1-A3 の確認セッション推奨
