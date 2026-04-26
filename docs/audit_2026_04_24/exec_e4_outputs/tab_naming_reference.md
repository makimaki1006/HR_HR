# タブ呼称統一リファレンス

**最終更新**: 2026-04-26
**対象範囲**: V2 ハローワークダッシュボード 9 タブの UI 表示 / URL / 関数名 / コメント の統一意思決定
**マスター**: ルート [`CLAUDE.md`](../CLAUDE.md) §3.1

---

## 1. 用語ブレの現状診断

| 出現箇所 | 現状の表記 |
|----------|----------|
| `templates/dashboard_inline.html:79` (UI ボタン) | **求人検索** |
| URL / 関数名 (`src/lib.rs:232`, `src/handlers/competitive/render.rs:30`) | **competitive** |
| `templates/tabs/competitive.html:1` (HTMLコメント) | **タブ8: 競合調査** |
| `templates/tabs/competitive.html:3` (H2 表示) | **🔍 企業調査** |
| `src/handlers/company/render.rs:8` (別タブ H2) | **🔎 企業分析** |

ユーザー視点フロー: 「求人検索」タブをクリック → 「企業調査」と表示される → 別タブ「企業検索」では「企業分析」と表示。**4 単語が交錯**。

---

## 2. 4 案の比較

| 案 | UI/UX 影響 | コード変更 | ペルソナ整合 | デメリット |
|----|----------|----------|------------|----------|
| A. **求人検索** に統一 | UI ナビ既に「求人検索」のため変更ゼロ。ユーザーが既に慣れている | H2 / コメント / 関数名 / URL を `competitive` から `job_search` 等へ。中規模 | ペルソナ B/C (HR担当・営業) が「求人を検索する」と直感的に読める | URL 変更で外部ブックマーク破壊。`competitive` の歴史的経緯 (競合他社調査) との整合喪失 |
| B. **競合調査** に統一 | UI ナビを「競合調査」に変更 (慣れ直し) | コメントは既に「競合調査」、URL `competitive` も整合 | ペルソナ A (採用コンサル) は「競合調査」用語に親和的 | UI 既に「求人検索」で公開済み、再学習コスト。ペルソナ B/C は「競合」と聞いてピンとこない |
| C. **企業調査** に統一 | UI ナビ + H2 + URL 全変更 | 影響箇所最大 | "企業" は別タブ「企業検索」(`/tab/company`) と完全に重複 | 「企業検索」と「企業調査」の区別が UI 上不可能。混乱が悪化 |
| D. **企業分析** に統一 | 同上 | 同上 | "企業分析" も別タブ company の H2 と重複 | C と同じ問題 + 「分析」が `/tab/analysis` (詳細分析) とも被る |

---

## 3. 最終決定: A. 「求人検索」に統一

### 3.1 根拠

1. **UX 連続性**: ナビが既に「求人検索」で公開済 (`templates/dashboard_inline.html:79`)。ユーザーの再学習コストゼロ。
2. **ペルソナ整合**: ペルソナ B (HR担当) 監査達成度 3.7、C (リサーチャー) 3.7。両者の言語が「求人を絞り込む」 (`team_alpha_userfacing.md §L1`)。コンサル A も「競合調査の一部として求人検索する」フローのため、上位概念は「求人検索」で問題ない。
3. **別タブとの非衝突**: 「企業検索」(`/tab/company`) との区別が明確。「求人=jobs」と「企業=companies」の英語対応も自然。
4. **既存 URL 維持可**: `/tab/competitive` の URL は維持しつつ、UI 表示・H2・コメントのみを「求人検索」に統一する**部分的統一**で十分。

### 3.2 URL `/tab/competitive` は不変

外部ブックマーク・関数名 internal を保護するため URL は変更しない。UI 層 (HTML / H2 / コメント) のみを統一。

---

## 4. 9 タブ呼称リファレンステーブル (4 列)

| # | UI 表示 (推奨統一後) | URL | 関数名 / ファイル | コメント (推奨統一後) |
|---|--------------------|-----|------------------|--------------------|
| 1 | 市場概況 | `/tab/market` | `tab_market` / `market.rs` | `// タブ1: 市場概況` |
| 2 | 地図 | `/tab/jobmap` | `tab_jobmap` / `jobmap/handlers.rs` | `// タブ2: 地図 (jobmap)` |
| 3 | 地域カルテ | `/tab/region_karte` | `tab_region_karte` / `region/karte.rs` | `// タブ3: 地域カルテ` |
| 4 | 詳細分析 | `/tab/analysis` | `tab_analysis` / `analysis/handlers.rs` | `// タブ4: 詳細分析` |
| 5 | **求人検索** ★ | `/tab/competitive` | `tab_competitive` / `competitive/handlers.rs` | `// タブ5: 求人検索 (URL は competitive)` |
| 6 | 条件診断 | `/tab/diagnostic` | `tab_diagnostic` / `diagnostic.rs` | `// タブ6: 条件診断` |
| 7 | 採用診断 | `/tab/recruitment_diag` | `tab_recruitment_diag` / `recruitment_diag/handlers.rs` | `// タブ7: 採用診断 (8 panel)` |
| 8 | **企業検索** ★ | `/tab/company` | `tab_company` / `company/handlers.rs` | `// タブ8: 企業検索 (SalesNow + HW 結合)` |
| 9 | 媒体分析 | `/tab/survey` | `tab_survey` / `survey/handlers.rs` | `// タブ9: 媒体分析 (CSV upload)` |

★ = 用語統一による呼称変更箇所。

---

## 5. 旧称対応 (移行期間用)

| 旧称 | 新称 |
|------|------|
| 競合調査 / 企業調査 | **求人検索** |
| 企業分析 (タブ 8 H2) | **企業検索** |
| 雇用形態別分析 | **詳細分析** (`analysis/handlers.rs:23,36` のフォールバック文言) |
| トレンド (独立タブ風) | **詳細分析 → トレンドサブグループ** (UI では analysis 内) |
| 総合診断 | **詳細分析 → 総合診断サブグループ** (insight、UI では analysis 内) |

---

## 6. 影響箇所一覧 (修正範囲)

| ファイル:行 | 現状 | 修正案 |
|-----------|------|--------|
| `templates/tabs/competitive.html:1` (HTMLコメント) | `<!-- タブ8: 競合調査 -->` | `<!-- タブ5: 求人検索 (URL: /tab/competitive) -->` |
| `templates/tabs/competitive.html:3` (H2 表示) | `🔍 企業調査` | `🔍 求人検索` |
| `src/handlers/competitive/render.rs:30` (関数 doc) | `/// 競合調査タブのレンダリング` | `/// 求人検索タブのレンダリング (URL: /tab/competitive)` |
| `src/handlers/competitive/handlers.rs` 各 fn doc | コメント参照「競合」 | 「求人検索」 (検索 + grep で精査) |
| `templates/dashboard_inline.html:79` (UI ボタン) | `求人検索` | (変更なし、既に整合) |
| `src/handlers/company/render.rs:8` (H2) | `🔎 企業分析` | `🔎 企業検索` |

---

## 7. 検証チェックリスト

修正後に以下を grep で確認:
```
grep -rn "競合調査\|企業調査\|企業分析" src/ templates/
# → 0 hit になること (関数名 internal は除外)
```

UI 確認:
- ナビバー: 「求人検索」「企業検索」が独立して表示
- `/tab/competitive` ページ H2: 「🔍 求人検索」
- `/tab/company` ページ H2: 「🔎 企業検索」
- `/tab/analysis` ページ H2: 「📈 詳細分析」(変更なし)

---

**改訂履歴**:
- 2026-04-26: 新規作成 (P4 / audit_2026_04_24 #10 対応)。Plan P4 §6, §7 から独立リファレンス化
