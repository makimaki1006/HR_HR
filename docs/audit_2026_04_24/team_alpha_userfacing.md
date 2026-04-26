# Team α: User-facing Audit Report

**監査日**: 2026-04-24
**監査者**: Team α (User-facing 品質)
**対象リポ**: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`
**監査範囲**: L1 (Mission/Persona) / L5 (UX) / L6 (Honesty) / 用語一貫性
**手法**: Read / Grep（編集なし）
**主要参照ファイル数**: 19 ファイル
**出力先メモ**: 元の指示パス `docs/audit_2026_04_24/team_alpha_userfacing.md` は監査対象リポ (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`) への書き込みが sandbox により拒否されたため、worktree 内 (`docs/audit_2026_04_24/team_alpha_userfacing.md`) に出力。後続ハンドオフ時にリポへコピーが必要。

---

## エグゼクティブサマリ

V2 HW Dashboard は 9 タブで構成された採用市場分析 SaaS。**全体としてはユーザー向け品質は高水準**で、特に以下が良質：
- L6 誠実性: `phrase_validator` で「100%」「必ず」等の断定表現を排除する仕組みが実装されており（`insight/phrase_validator.rs:5,21`）、insight・karte・recruitment_diag は「傾向」「可能性」表記が徹底
- HW 限界の注意書きが採用診断・媒体分析・地域カルテで明示（`recruitment_diag/competitors.rs:273-277`、`region/karte.rs:808`）
- 給与バイアス（HW は低めに出る）の記載は recruitment_diag Panel 5 で明示（`templates/tabs/recruitment_diag.html:183-184`）

ただし以下の **構造的・整合性課題** が User-facing で残存：

**最重要課題 Top 3**:
1. 🔴 **`templates/tabs/overview.html` が V1 求職者ダッシュボードの遺物テンプレート** — 「平均月給」ラベル変数名が `{{AVG_AGE}}`、「正社員/パート」を表示する変数名が `{{MALE_COUNT}}/{{FEMALE_COUNT}}`（`templates/tabs/overview.html:18,23,46`）。現行 V2 では `src/handlers/overview.rs::render_overview()` が直接 HTML 生成しているため**未使用ファイルだが**、誤って使われると意味取り違えの大事故になる。
2. 🟡 **タブ呼称の三重ブレ** — dashboard nav 表示「求人検索」/ ファイル名・URL `competitive` / テンプレ H2「企業調査」/ コメント「競合調査」（4 種混在、`templates/tabs/competitive.html:1,3`、`templates/dashboard_inline.html:79`、`src/handlers/competitive/render.rs::render_competitive`）
3. 🟡 **`tab_analysis` の DB 未接続フォールバックタイトル不一致** — タブ表示は「詳細分析」だが `render_no_db_data("雇用形態別分析")` を返す（`src/handlers/analysis/handlers.rs:23,36`）。旧名「雇用形態別分析」が残存。

---

## L1 ペルソナ整合

| # | タブ名（UI） | URL | 想定ユーザー | 達成タスク | 整合性 | 根拠 |
|---|---|---|---|---|---|---|
| 1 | 市場概況 | `/tab/market` | HR担当 / 営業 | 地域×産業の求人数・平均給与・正社員率を一目把握 + 全国比較 | 高 | `src/handlers/market.rs:60-114`: 概況→雇用条件→企業分析→採用動向の4セクション、3層比較バー (`src/handlers/overview.rs:642-751`) |
| 2 | 地図 | `/tab/jobmap` | 営業 / フィールドリサーチ | 地理的な求人分布 + Agoop人流ヒートマップ + コロプレス6種 | 高 | `templates/tabs/jobmap.html:69-82` 6 レイヤー切替、半径検索、企業マーカー |
| 3 | 地域カルテ | `/tab/region_karte` | 採用コンサル / 経営層 | 1市区町村の構造・人流・求人を1画面で診断、印刷可能 | 高 | `src/handlers/region/karte.rs:439-495` 7 セクション (KPI×9 / 人口動態 / 産業 / 福祉 / 人流 / So What / 出典) + 印刷ボタン |
| 4 | 詳細分析 | `/tab/analysis` | アナリスト / コンサル | サブタブ 7 つで給与構造・テキスト分析・市場構造・空間ミスマッチを深掘り | 中 | `src/handlers/analysis/handlers.rs:60-67` グループ「構造分析/トレンド/総合診断」+ サブタブ複層、UX 複雑 |
| 5 | 求人検索 | `/tab/competitive` | 営業 / 採用担当 | 都道府県+市区町村+雇用形態+産業で求人一覧抽出、近隣エリアも含めた検索 | 高 | `templates/tabs/competitive.html:18-94` 多次元フィルタ、施設形態 2 階層、近隣 km 半径 |
| 6 | 条件診断 | `/tab/diagnostic` | 中小企業HR / 単発求人検証 | 月給/休日/賞与を入力してパーセンタイル位置を可視化 | 中 | `src/handlers/diagnostic.rs:39-79` 入力フォーム、レーダーチャート結果。ペルソナ（自社条件診断）が「採用診断」と機能重複 |
| 7 | 採用診断 | `/tab/recruitment_diag` | HR管理職 / 採用コンサル | 業種×エリア×雇用形態で採用難度・人材プール・競合・市場動向を 8 パネル統合診断 | 高 | `templates/tabs/recruitment_diag.html:13-228` 8 パネル並列ロード、自社条件 details 折畳、AI示唆 |
| 8 | 企業検索 | `/tab/company` | 営業 / コンサル | 236,000 社 SalesNow データから企業検索 → プロフィール×HW×外部統計の統合表示 | 高 | `src/handlers/company/render.rs:5-38` 検索フィールド + 信用スコア・上場・SN スコアバッジ |
| 9 | 媒体分析 | `/tab/survey` | HRマーケ / メディアプランナー | 他媒体（Indeed/求人ボックス）CSV をアップロードし、HW と相対比較 | 中 | `src/handlers/survey/render.rs:20-105` ソース指定→CSV→TL;DR→HW 統合分析、ブラウザ内処理 |

**観察**:
- 9 タブは全て同一ヘッダーフィルタ（都道府県/市区町村/産業）を共有する設計（`templates/dashboard_inline.html:22-58`）。地域カルテ・診断系は専用フォームで上書きできるが、フィルタ階層が二重化（条件診断は別フォーム + ヘッダーフィルタ）。
- **条件診断 vs 採用診断のペルソナ重複**: 両方とも「自社条件を入力 → 市場との比較」。前者は単発レーダーチャート、後者は 8 パネル統合。差別化が UI 上不明瞭でユーザーに迷いを生む。`templates/tabs/recruitment_diag.html:99-115` の details 内でも自社条件入力。

---

## L5 UX / 認知負荷

### 5.1 初期状態の挙動

| タブ | CSV/DB未接続時 | 地域未選択時 | スコア |
|---|---|---|---|
| 市場概況 | `render_no_db_data("市場概況")` 表示 (`src/handlers/market.rs:17`) | 全国データ表示（`src/handlers/overview.rs:373-380`） | 良 |
| 地図 | `render_no_data_message(job_type)` (`src/handlers/jobmap/render.rs:193-209`) | 都道府県選択を促す UI | 良 |
| 地域カルテ | `render_no_db()` (`src/handlers/region/karte.rs:412-418`) | `render_empty_guide()` で「市区町村を選択するとカルテが生成」と明示誘導 (`src/handlers/region/karte.rs:421-437`) | 優 |
| 詳細分析 | **「雇用形態別分析」とフォールバック表示**（`src/handlers/analysis/handlers.rs:23`、タブ名と不一致） | サブタブ 1 を初期表示 | 中 |
| 求人検索 | `<p class="text-slate-400 text-sm">都道府県を選択して検索してください</p>` (`templates/tabs/competitive.html:98`) | 同 | 良 |
| 条件診断 | フォーム表示、未入力で submit すると amber バナー (`src/handlers/diagnostic.rs:107`) | フィルタ無関係 | 良 |
| 採用診断 | `rd-initial-msg` で「フォームで業種・エリアを選択して診断実行」明示 (`templates/tabs/recruitment_diag.html:120-123`) | 同 | 優 |
| 企業検索 | `render_search_page()` 検索フィールドのみ表示、検索後にプロフィール (`src/handlers/company/render.rs:5-38`) | 同 | 良 |
| 媒体分析 | アップロードフォーム + ドラッグ&ドロップゾーン (`src/handlers/survey/render.rs:69-86`) | CSV 未上传時無内容 | 良 |

### 5.2 情報密度

| タブ | KPI/セクション数 | 観察 |
|---|---|---|
| 市場概況 | KPI 4 + 比較バー 3 + チャート 5（産業別/職業/雇用形態/給与帯/求人理由） | `src/handlers/overview.rs:874-942` バランス良好 |
| 地域カルテ | KPI 9 + チャート 8+ + 示唆カード可変 + フッター | `src/handlers/region/karte.rs:528-538` 9 KPI は1画面でやや密集（pink/lime/violet 等 9 色使い） |
| 詳細分析 | サブタブ 7 × 各 3-5 セクション ≒ 30+ ビュー | `src/handlers/analysis/render.rs:24-150` 認知負荷高、「構造分析/トレンド/総合診断」のグループナビゲーションで分割は良 |
| 採用診断 | パネル 8 + AI示唆カード | `templates/tabs/recruitment_diag.html:128-228` 並列ロードで段階的、各パネル独立 |
| 求人検索 | フィルタ 8+ + 結果テーブル + 分析パネル | `templates/tabs/competitive.html:18-115` フィルタ多すぎ感あり |

### 5.3 配色一貫性

- **意味色**: emerald=良好 / amber=注意 / red(rose)=警告 / blue=情報、で一貫（`src/handlers/survey/render.rs:8-9` でドキュメント化、`src/handlers/overview.rs:666-672` で diff_color 統一）
- **絵文字バッジ**: 各タブ H2 に絵文字（📊 市場概況 / 🗺️ 求人地図 / 📋 地域カルテ / 🔎 企業分析 / 媒体分析（絵文字なし、`src/handlers/survey/render.rs:26`）
- **ブランドカラー**: blue-400 を H2 サブテキストの統一カラーに使用（多くの render で確認）
- **企業検索だけ `bg-purple-900` `bg-amber-900` `bg-blue-900` の信用スコアバッジ**（`src/handlers/company/render.rs:69-96`）— 他のタブの severity 色とは独立した「企業属性」用。要凡例。

### 5.4 専門用語密度

専門用語が多い場面:
- `src/handlers/analysis/render.rs:24` `vacancy_by_industry` `resilience` `transparency`
- `src/handlers/analysis/render.rs:88-112` `text_quality` `keyword_profile` `temperature`
- `src/handlers/analysis/render.rs:114-150` `monopsony` `spatial_mismatch` `cascade`
- `src/handlers/region/karte.rs:533` `単独世帯率` `可住地密度` `医師数 (人/10k)`

→ `src/handlers/guide.rs:206-260` に詳細解説あり、ただし各タブからの直接リンクは弱い。

### 5.5 モバイル対応

- viewport meta タグあり: `templates/dashboard_inline.html:5`、`templates/base.html:5`
- ヘッダーが `flex-wrap gap-2 overflow-x-hidden`、フィルタ `sm:` `md:` ブレイクポイント使用 (`templates/dashboard_inline.html:17-66`)
- 地図タブは固定ピクセルが残存（`templates/tabs/jobmap.html:30` `w-20`）でモバイルだと見づらい可能性
- `static/css/dashboard.css` に `@media (max-width: 640px)` 等のメディアクエリ複数（`static/css/dashboard.css:226,233,240,256,266`）と `@media print`（`596,1007`）

### 5.6 タブ間遷移

- `cross_nav` 関数で内部リンク提供（`src/handlers/overview.rs:956-957` 「詳細分析」「雇用形態詳細」）
- 詳細分析サブタブ 1 が「関連: 地域概況 / 企業分析」（`src/handlers/analysis/render.rs:32-36`）
- ブレッドクラムが `templates/dashboard_inline.html:92-100` で「全国 / 産業」状態を常時表示（優）
- ただし採用診断 → 競合企業詳細などの **タブ越えクロスリンク** はなく、診断結果から行動につなげる導線は弱い

---

## L6 誠実性

### 6.1 「HW 掲載求人のみ」注意書きの棚卸し

| 場所 | 文言 | 評価 |
|---|---|---|
| `src/handlers/guide.rs:21` | 「⚠️ このダッシュボードはHW掲載求人のみが対象です。民間サイト（Indeed等）の求人は含まれません」 | 優（ガイドトップ） |
| `src/handlers/guide.rs:127` | 「ここに表示される数値はすべてHW掲載求人ベースです」 | 良 |
| `src/handlers/company/render.rs:1350` | 「※ ハローワーク掲載求人のみが対象です。民間求人サイト（Indeed等）の求人は含まれません」 | 良（PDF 出力） |
| `src/handlers/diagnostic.rs:445-447` | 「ハローワーク掲載求人ベースの分析です」+ 「選択された産業はHW掲載が少なく、実際の市場とは乖離がある可能性があります」 | 優（条件付き warning） |
| `templates/tabs/recruitment_diag.html:22-24` | 「データ範囲: HW掲載求人のみ」+ Panel 5 「HW求人データの限界」amber バナー | 優 |
| `src/handlers/recruitment_diag/competitors.rs:273-277` | 「HW掲載求人のみを対象。全求人市場ではない。HW求人は市場実勢より給与を低めに設定する慣習あり」 | 優（最も詳細） |
| `src/handlers/recruitment_diag/mod.rs:59` | 「本分析は HW（ハローワーク）掲載求人のみを対象とする」 | 良 |
| `src/handlers/region/karte.rs:807-808` | 「HW掲載求人は全求人市場の一部を構成します。IT・通信等HW掲載が少ない産業は参考値」 | 優 |
| `src/handlers/insight/render.rs:99` | 「HW（ハローワーク）掲載求人に基づく分析です。IT・通信等のHW掲載が少ない産業は参考値」 | 優 |
| `src/handlers/balance.rs:44` / `src/handlers/workstyle.rs:44` / `src/handlers/overview.rs:352` / `src/handlers/analysis/render.rs:1556` | 「出典: ハローワーク掲載求人データ / 外部統計: e-Stat API / SSDSE-A」 | 良 |
| `src/handlers/jobmap/correlation.rs:155` | 「HW掲載求人のみ対象（全求人市場ではない）。人流は2019-2021 Agoopデータ」 | 優 |
| **市場概況・求人検索・地図のメインページ Hタイトル直下** | （明示的 banner なし） | **要追加** |

**矛盾・抜け**:
- 🟡 **市場概況タブのタイトル下**には HW限界の注意がない。出典フッター (`src/handlers/overview.rs:352`) のみ。最初に開くタブだけにユーザーが見落とすと「これが市場全体」と誤認する危険。
- 🟡 **求人検索タブ (`templates/tabs/competitive.html`)** には HW 限定明示なし（フィルタ「都道府県を選択しない場合、都道府県全体で検索します」のみ）。
- 🟡 **地図タブ (`templates/tabs/jobmap.html`)** ヒートマップ部分に Agoop 注釈はあるが、求人マーカーが HW 限定である注意書きは希薄。

### 6.2 「相関≠因果」の明記

- `src/handlers/insight/engine.rs:1362` `phrase_validator で検証する（相関≠因果原則）`
- `src/handlers/insight/engine_flow.rs:6` `# 相関≠因果原則の徹底`
- `src/handlers/insight/phrase_validator.rs:21` 禁止表現リスト (「確実に」「必ず」「100%」等)
- `src/handlers/jobmap/correlation.rs:8` 「**相関≠因果**: Pearson r を算出するが、レスポンス内に必ず注釈を含める」
- `src/handlers/jobmap/correlation.rs:149` レスポンスに `"note": "Pearson相関係数。相関係数は関連の強さを示すが因果関係を示すものではありません。"`
- `src/handlers/region/karte.rs:746` カルテ示唆フッター「※ 示唆は「傾向」「可能性」の範囲に留めています。因果関係は示していません」
- `templates/tabs/recruitment_diag.html:233` 「※ 相関関係と因果関係は別物です。「傾向がある」「可能性がある」という表現に留めています」
- `src/handlers/recruitment_diag/competitors.rs:8` 「相関分析に留め因果は主張しない」
- `src/handlers/recruitment_diag/condition_gap.rs:12` 「中央値 vs 自社の差分は相関指標。因果 (給与を上げれば応募増) は保証しない」

→ insight/karte/recruitment_diag/correlation で網羅的。**詳細分析（analysis）タブには明示的な相関≠因果文言は見つからず**（`src/handlers/analysis/render.rs` を grep）。

### 6.3 給与バイアスの明記

- `src/handlers/recruitment_diag/competitors.rs:275` 「HW求人は市場実勢より給与を低めに設定する慣習あり」
- `templates/tabs/recruitment_diag.html:183-184` Panel 5 amber バナー「HW求人は市場実勢より給与を低めに設定する慣習あり → HW中央値で勝っていても民間サイト中央値では負けている可能性」
- `src/handlers/guide.rs:159` 「給与について: HW求人の給与は「基本給+手当」の月額表示が多いですが、時給表示のパート求人も含まれます」

→ recruitment_diag のみ明示。**市場概況の「平均月給（下限）」KPI（`src/handlers/overview.rs:884`）にバイアス注記なし**。

### 6.4 サンプル件数の明記

- `src/handlers/analysis/render.rs:989` `<span>サンプル数</span><span class="text-white">{sample_s}</span>` — 業種別給与等で表示
- `src/handlers/analysis/render.rs:414` 「業種別 欠員補充率ランキング（n≥30）」— 統計的に意味のある下限を明示
- recruitment_diag は各 panel で件数表示あり（`templates/tabs/recruitment_diag.html` 内 panel body）

### 6.5 外れ値処理の明記

- `src/handlers/survey/aggregator.rs:101,280,634` IQR 法 (Q±1.5IQR) で外れ値除外（コード上）
- ただしユーザー向け UI 文言として「外れ値除外しています」と明示しているのは**確認できず** — 媒体分析の集計値は除外後だが、その旨が UI 上に出ていない可能性。

### 6.6 直感外れ・断定表現

- `src/handlers/insight/phrase_validator.rs` で「確実に」「必ず」「100%」を禁止し走時検証
- `src/handlers/recruitment_diag/handlers.rs:572,578` で「広域求人媒体への出稿は費用対効果が低下する見込み」「全国媒体・引越し支援の訴求で応募母集団を広げられる可能性」← 「可能性」「見込み」表現で OK
- `src/handlers/recruitment_diag/competitors.rs:1` 「Panel 4: 競合企業ランキング」 — 「ランキング」自体は事実ベース（従業員数・売上高ソート）なので妥当（`src/handlers/recruitment_diag/competitors.rs:4`）
- `src/handlers/insight/engine.rs:706,718-719` `RC-1: 総合ベンチマーク順位` ← 「ランキング」「順位」は使われるが、phrase_validator で因果断定はブロック

---

## 言葉の一貫性

### 7.1 タブ呼称の三重ブレ（最重要）

| 場所 | 表記 |
|---|---|
| `templates/dashboard_inline.html:79` (UI ボタン) | **「求人検索」** |
| `templates/dashboard_inline.html:79` URL | `/tab/competitive` |
| `templates/tabs/competitive.html:1` (コメント) | **「タブ8: 競合調査」** |
| `templates/tabs/competitive.html:3` (H2 表示) | **「🔍 企業調査」** |
| `src/handlers/competitive/render.rs:30` (Rust 関数) | `render_competitive` |
| `src/handlers/company/render.rs:8` H2 | **「🔎 企業分析」** ← 別タブ「企業検索」の方 |

→ ユーザーが「求人検索」をクリックすると「企業調査」表示。さらに別タブ「企業検索」では「企業分析」と表示。**4 単語が交錯**。

### 7.2 詳細分析タブの呼称ブレ

| 場所 | 表記 |
|---|---|
| `templates/dashboard_inline.html:77` UI | **「詳細分析」** |
| `src/handlers/analysis/handlers.rs:42` H2 | 「詳細分析」 |
| `src/handlers/analysis/handlers.rs:23,36` フォールバック | **「雇用形態別分析」** ← 旧名残 |

### 7.3 「正社員率/正社員割合」

- 主要箇所（overview/insight/karte/recruitment_diag/diagnostic）: 「正社員率」で統一（`src/handlers/overview.rs:730,889`、`src/handlers/company/fetch.rs:358`）
- 媒体分析だけ「**CSV内 正社員割合**」（`src/handlers/survey/report_html.rs:1276,1288`）— 意図的に「HW の正社員率と区別」のため命名した模様（コメントで明示）。これは妥当な区別。

### 7.4 「HW / ハローワーク」

意図的な略称使用と思われるが混在:
- 略称「HW」: 47+ 箇所（多くは UI 短縮表示用）
- 「ハローワーク」: 出典フッター・ガイド・PDF 出力（誠実用途）
- 「ハロワ」表記なし — 良
- `src/handlers/jobmap/render.rs:78,80` 「管轄HW」（jobmap.rs と competitive.rs で揃っている）

### 7.5 「媒体 / ソース / チャネル」

| 用語 | 用例 |
|---|---|
| 媒体 | `templates/tabs/recruitment_diag.html:188`「給料の比較は別途競合調査を参照」「Indeed・求人ボックス CSV 取込」(`src/handlers/survey/render.rs:27`)、survey タブ自体「媒体分析」 |
| ソース | `src/handlers/survey/render.rs:50` `source_type`、`src/handlers/company/render.rs:36`「データソース」、`src/handlers/guide.rs:245` |
| チャネル | `src/handlers/guide.rs:255-267`「全産業・全求人チャネル」、`src/handlers/insight/engine_flow.rs:114`「採用チャネル拡大余地」 |

→ 用例の文脈は概ね差別化されているが、ユーザーから見ると「媒体」「ソース」「チャネル」は同義に感じる可能性。とくに `src/handlers/guide.rs:255` で「全チャネル」と「外部統計」の関係が初学者にやや難解。

### 7.6 雇用形態セレクトの不統一

`src/handlers/jobmap/render.rs:166-173` の `emp_badge_class` で「正職員」「正社員」「フルタイム」を同一バッジ色に集約 — **データソース毎の表記揺れに対応**しており賢明だが、ユーザー側 UI で混在表記が見える可能性あり。
- `templates/tabs/competitive.html:44-47` セレクトは「正社員/契約社員/パート」3種
- `templates/tabs/recruitment_diag.html:60-63` セレクトは「正社員/パート/その他」3種
- `templates/tabs/jobmap.html:35-39` セレクトは「正社員/契約社員/パート/業務委託」4種
- → セレクト選択肢が**タブ毎に違う**

---

## 優先 Top 10 改善項目

| # | 項目 | 重要度 | 工数 | 該当ファイル |
|---|---|---|---|---|
| 1 | `templates/tabs/overview.html` の遺物テンプレ削除（V1 用、V2 では未使用、`{{AVG_AGE}}` 等の誤用変数あり） | 🔴 高（誤使用すれば即事故） | 小 | `templates/tabs/overview.html` (95 行)、削除可否の確認後 |
| 2 | タブ呼称統一: 「求人検索」or「競合調査」or「企業調査」のいずれかに揃える（H2/コメント/関数名/URL） | 🟡 中 | 中 | `templates/tabs/competitive.html:3`、`src/handlers/competitive/render.rs:30`、`templates/dashboard_inline.html:79`、`src/handlers/company/render.rs:8` |
| 3 | `src/handlers/analysis/handlers.rs:23,36` の `render_no_db_data("雇用形態別分析")` を `"詳細分析"` に修正 | 🟡 中 | 小 | `src/handlers/analysis/handlers.rs` |
| 4 | 市場概況タブの H2 直下に「HW 掲載求人ベース」warning を 1 行追加（現状フッターのみ） | 🟡 中 | 小 | `src/handlers/overview.rs:867-870` |
| 5 | 詳細分析タブに「相関≠因果」「HW掲載求人ベース」共通フッター注意書き追加 | 🟡 中 | 小 | `src/handlers/analysis/handlers.rs:96-97` 付近 |
| 6 | 媒体分析タブの集計値に「外れ値除外（IQR法）」を UI 上に明記（コードはやっているが UI 文言なし） | 🟡 中 | 小 | `src/handlers/survey/render.rs` 各カード |
| 7 | 雇用形態セレクトの選択肢を全タブで統一（competitive=3種、recruitment_diag=3種、jobmap=4種） | 🟡 中 | 中 | `templates/tabs/competitive.html:44-47`、`templates/tabs/recruitment_diag.html:60-63`、`templates/tabs/jobmap.html:34-40` |
| 8 | 市場概況タブの「平均月給（下限）」KPI に「※HW求人は市場実勢より給与を低めに設定する慣習あり」tooltip 追加 | 🟢 低 | 小 | `src/handlers/overview.rs:884` |
| 9 | 条件診断 vs 採用診断のペルソナ重複解消（カードで「単発診断は条件診断、統合診断は採用診断」と誘導） | 🟢 低 | 中 | `src/handlers/diagnostic.rs:39-79`、`templates/tabs/recruitment_diag.html:120-123` |
| 10 | 詳細分析サブタブの専門用語（monopsony, resilience, vacancy_by_industry 等）に hover tooltip でガイドリンク追加 | 🟢 低 | 中 | `src/handlers/analysis/render.rs` 各 section ヘッダ |

---

## 残課題（深堀り推奨）

1. **`templates/tabs/overview.html` の所属判定**: V1 (job_seeker) のテンプレが V2 デプロイリポにも残っている。`include_str!` で参照している箇所があるか追加調査要（`include_str!.*overview.html` で grep）。Team β/γ で確認推奨。
2. **`templates/dashboard.html` (V1)** が V2 デプロイリポに存在: V1 用、`templates/dashboard_inline.html` が V2 用と推測されるが、ルーティングで両方提供されているか不明。
3. **競合分析・採用診断のクロスリンク不在**: 採用診断 Panel 4 で見つけた競合企業を直接「企業検索」タブで詳細表示する導線が（grep の限り）無い。
4. **`src/handlers/diagnostic.rs.bak`** がリポに残存（`src/handlers/diagnostic.rs.bak`）— 本番ビルドには影響しないが、コミット汚染。Team β に通知。
5. **`templates/tabs/balance.html` (63行) / `demographics.html` (116行) / `workstyle.html` (90行)** は静的テンプレだが、`market.rs` ではこれらを使わず `balance.rs::build_balance_html()` 等の動的生成のみ。**未使用テンプレファイルの可能性**。
6. **モバイル UX の実機検証なし**: viewport は OK、`@media (max-width: 640px)` あるが、地図タブ・詳細分析サブタブの幅狭表示の実体験は未検証。Team γ で Playwright 検証推奨。
7. **企業検索の信用スコアバッジ・SNスコアバッジ**: 数値だけが出るが「信用スコアとは何か」「SN とは SalesNow か」の凡例がタブ内にない（`src/handlers/company/render.rs:69-96`）。
8. **アクセシビリティ**: aria-label は ECharts に付与されているが（`src/handlers/overview.rs:816-861` で top3 をテキスト化）、スクリーンリーダー実機検証は未実施と推測。

---

**監査サマリ終了**
