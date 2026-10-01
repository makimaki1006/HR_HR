# React 全画面移行 全体計画

更新日: 2026-09-29(決定事項を反映。同日のユーザー指示「フロントエンドの React 化は決定事項で、全てにおいて実行する」により、レポートと非表示画面も移行対象に確定)
位置付け: **HR_HR の全画面を React へ移す計画の正本**。方針は `frontend-react-migration.md`、既決事項は `architecture-decisions.md`(ADR-002 の「全画面の範囲」、ADR-014 の補足)にある。本書はそれを波・完了条件・撤去手順に落としたもの。
経緯: ユーザー指示「React に移行するのは、いずれは全部だから、それも計画立てておいてほしい」(2026-09-29)を受けて棚卸しと計画を作った。同日、ユーザーが §8 の決定をした。途中で「レポートは凍結」「非表示画面は削除/HTMX のまま残す」とした案は、最終的に「すべて React に移す」で取り消した。
棚卸しの時点: 作業ツリー `gurnard`(c61a8cb)と Phase 0 ブランチ(PR #28、6c35c5b。PR #28 は main にマージ済み、ad7d918)。数値はこの時点のもので、以後のコード変更で変わる。
参照する作業文書: 「1A 計画」= Phase 0 / 1A の実装計画(`claudedocs/REACT_PHASE0_1A_PLAN_2026-09-29.md`。作業文書で、リポジトリには入れていない)。本書で引用する ID(U-4, U-5, U-8, C-10, 1A-5 など)はその文書のもの。

表記: 【確認済】= ファイルを読んだか再計測した事実(ファイル:行)。【推測】= 推論で、実行・実測していないもの。【要確認】= 着手前に実測が要るもの。
行数は `wc -l`。特に断りがなければテストを含む。

---

## 0. 結論(要約)

- **規模**: 入口単位で Web 画面 **約 40**(ダッシュボードのタブ 23、独立ページ約 17)、印刷・ダウンロード系レポート **約 10 系統**。Rust の `src/handlers/` は 211,453 行で、うちテストは約 71,000 行、テスト以外は約 140,000 行。これとは別に `src/job_gen/` が 22,221 行、`src/media_engine/` が 7,679 行ある。フロント資産(templates / static の js・css・html)は 27,641 行で、ほかに改行なしの minified JS が約 36KB ある。【確認済】§1
- **やり方**: Strangler を続ける。1 画面ずつ `/app/{screen}` に作り、旧画面と並走させる。値の一致を確かめてから旧 URL を 302 で新 URL に向け、旧テンプレートとハンドラを消す。**最後の波(W10)で HTMX・tabcache・旧シェル・CDN・precompiled CSS・`/api/set_*` を撤去し、CSP から `'unsafe-inline'` を外す**。
- **範囲**: レポートも、非表示の画面(非表示 8 タブ・dead route 4・proposal-mock・架電の雛形タブ)も React に移す。非表示の画面は削除せず、React に移したうえでナビ定義の `hidden` フラグで隠す。いま追加実装中のもの(媒体分析の競合調査 = survey-team、Headless CRM = crm-team、コンサルKPI = UI/UX 改修中)は、それぞれの作業が落ち着いてから扱う(CRM は最初から React)。login だけは Rust 側に残す。
- **波**: W1(1A 採用診断と Headless CRM を並行) / W2(営業KPI + App Shell v1) / W3(調べる▾の軽量画面) / W4(地域分析・企業検索・採用市場。HTML partial の JSON 化が中心) / W5(地図) / W6(媒体分析の画面) / W7(コンサルKPI。改修が落ち着いてから) / W8(架電・consult・admin/my・求人票作成) / **Wh(非表示の画面。React に移して隠す)** / **W9(レポートを React の印刷用画面へ)** / W10(撤去)。W2 以降は担当チームを分けて並行で進める(§3)。
- **先に作る共通基盤**: App Shell(ナビは Rust が JSON で返し、`hidden` フラグで隠し方を統一)、フィルタの受け渡し(過渡期は「session が正」のまま、URL クエリへ移していく)、client の POST・upload・ジョブ待ちの拡張、`/api/*` の 401 JSON 化、共通コンポーネント(表・KPI カード・注記・ECharts ラッパ・Leaflet ラッパ)、ビルド型 Tailwind、PR 時の E2E、partial を JSON にする標準手順。platform-team が作る。
- **工数**: W2〜W10(Wh を含む)で **約 360〜695 人日**【推測】。W1・CRM・競合調査は別チームの作業なので含めない。チームを並行させるので暦の上の期間は短くなるが、合計の手間は減らない。律速はレビュー時間【推測】。
- **決定事項**(2026-09-29、§8): 全画面を React に移す(レポートと非表示画面を含む) / 非表示画面は `hidden` フラグで隠す / コンサルKPI は UI/UX 改修が落ち着いてから / login は Rust に残す / 401 JSON 化と CSRF 補強を実施 / App Shell と共通部品は platform-team が作り、各チームが使う / 並走は最低 1 リリース / Render は有料プラン(`render.yaml` の `plan: free` は実態と違う。ビルド時間の上限と CI 緑待ちの設定は未確認)。
- **ADR への反映**: ADR-002 に「全画面の範囲」を追記、ADR-014 に補足(CRM は 1A を待たない)。どちらも既存 ADR の変更ではなく、範囲の明確化と補足。

---

## 1. 全画面の棚卸し

凡例:
- J = JSON を返すルートの本数、H = HTML partial を返すルートの本数(タブの入口は除く)。判定はハンドラの戻り値の型(`Json` / `Html`)で行った。
- E = ECharts、L = Leaflet。
- テスト = `#[test]` / `#[tokio::test]` の件数 / E2E の本数(spec と e2e_*.py のうち、そのパスに grep で当たったファイル数)。
- 利用状況は、ナビ上の位置とコメントから判断した。**アクセス件数は照会していない**(§1.5)。

### 1.1 ダッシュボードのタブ(`templates/dashboard_inline.html`、1,585 行)

ナビ構造【確認済】:
- 第 1 段は `dashboard_inline.html:137-155`。
- 「調べる▾」配下は `:174-188`(初期状態は hidden)。
- 非表示にした経緯は、2026-05-15 の 8 タブが `:123-128`、2026-07-28 の求人検索が `:129-131`。

| 状態 | 画面 | URL | ハンドラ | Rust 行 | テンプレ/JS | J | H | E/L | POST | テスト/E2E | 難易度・理由 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 表示 | 媒体分析 | /tab/survey | handlers/survey/ | 82,162(テスト約 32,716。うちレポートの `report_html/` 61,552。report_html 内のテスト名ファイル 4,293 を含む) | inline JS 共用 + survey_explore.js | 6 | 5 | E | 2(upload, upload/start) | 1,410 / 9 | **XL**: CSV アップロード、非同期ジョブ、セッション、印刷 HTML |
| 表示(条件付き※1) | キーワード需要 | /tab/keyword_tools → iframe /keywords-ui | src/media_engine/ | 7,679 | static/keywords.html 824 | 8 | 0 | — | 0 | 114 / 0 | M: API は JSON 済み |
| 表示(条件付き※2) | 求人票作成 | /tab/jobgen_tools → iframe | src/job_gen/ | 22,221 | jobgen.html 1,075 + journey_beta 1,540 + journey_map.js 757 ほか | 19(全 POST、Gemini) | 0 | — | 19(保存はしない【推測】) | 334 / 0 | L: API は JSON だが画面の状態が多い |
| 調べる | 地図 | /tab/jobmap | handlers/jobmap/ | 6,447 | jobmap.html 1,295 + JS 約 1,220 行 + postingmap.js 25KB(minified) | 16 | 14 | E+L | 1(stats。参照系) | 69 / 5 | **XL**: Leaflet と ECharts、JSON と HTML が混在、元ソースが無い minified JS |
| 調べる | 地域分析 | /tab/regional_analysis | handlers/regional_analysis/ | 2,166 | 183 | 0 | 10 | E | 0 | 27 / 0 | M: partial 10 本の JSON 化が要る |
| 調べる | 企業検索 | /tab/company | handlers/company/ | 5,734 | Rust 内 HTML | 0 | 10 | E | 0 | 56 / 9 | L: すべて Rust で HTML を組んでいる |
| 調べる | 職種辞典 | /tab/driver | handlers/driver/ | 1,306 | askama テンプレ 1,098 | 4 | 2 | E | 0 | 0 / 0 | S〜M: JSON 済み。テストが 0 件 |
| 調べる | 資格辞書 | /tab/license | handlers/license/ | 744 | askama テンプレ 425 | 0 | 1 | E | 0 | 0 / 0 | S |
| 調べる | 採用市場 | /tab/indeed | handlers/indeed/ + src/indeed/ | 9,386 + 5,012 | Rust 内 HTML | 1 | 2 | E | 0 | 151 / 3 | L: HTML を組む量が多い。E2E 3 本は転用できる |
| ヘッダー | 使い方ガイド | /tab/guide | guide.rs | 572 | — | 0 | 0 | — | 0 | 0 / 0 | S |
| 非表示 | 市場概況 | /tab/market | market.rs | 225 | — | 0 | 4 | — | 0 | 0 / 5 | S〜M |
| 非表示 | 地域カルテ | /tab/region_karte | handlers/region/ | 1,935 | region_karte.js が無い(読み込みはコメントアウト、`:343-345`) | 1 | 0 | E | 0 | 14 / 0 | M: いま正しく動くか未確認 |
| 非表示 | 詳細分析 | /tab/analysis | handlers/analysis/ | 12,958 | — | 0 | 1(サブタブ) | E | 0 | 126 / 3 | **XL**: グループ × サブタブ 7 × 28 セクション |
| 非表示 | トレンド | /tab/trend | handlers/trend/ | 2,286 | — | 0 | 1 | E | 0 | 49 / 0 | M |
| 非表示 | 総合診断 | /tab/insight | handlers/insight/ | 9,383 | — | 1(+xlsx) | 2 | E | 0 | 156 / 6 | L: 38 パターン |
| 非表示 | 都道府県比較 | /tab/comparison | handlers/comparison/ | 1,049 | — | 0 | 0 | E | 0 | 14 / 0 | S |
| 非表示 | 条件診断 | /tab/diagnostic | diagnostic.rs | 1,221 | — | 0 | 2 | E | 0 | 0 / 1 | M: API が 0 本、テストも 0 件 |
| 非表示 | **採用診断(Phase 1A)** | /tab/recruitment_diag | handlers/recruitment_diag/ | 4,598 | recruitment_diag.html 957 | 9 | 0 | E | 0 | 97 / 0 | M: 別チームが実装中 |
| 非表示 | 求人検索 | /tab/competitive | handlers/competitive/ | 4,882 | competitive.html 545 | 0 | 18 | E | 0 | 61 / 0 | L: partial 18 本 |
| dead | overview / demographics / balance / workstyle | /tab/… | 各 .rs | 1,310 / 606 / 581 / 861 | 未参照のテンプレ 3 本(計 269 行) | 0 | 0 | E | 0 | 16(overview のみ) | React に移して隠す(Wh)。到達経路は analysis の cross_nav だけ |

※1 GoogleAds の環境変数がすべてそろっているときだけ出る(`src/lib.rs:1441-1446`)。※2 `GEMINI_API_KEY` があるときだけ出る(`src/lib.rs:1449-1453`)。【確認済】

### 1.2 独立ページ(ダッシュボードの外)

| 状態 | 画面 | URL | ハンドラ | Rust 行 | フロント行 | J | H | E/L | 書込 | 認証 | テスト/E2E | 難易度・理由 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ナビ `<a>` | 営業KPI | /sales-kpi | sales_kpi/ | 3,370 | sales_kpi.html 1,262(JS 994) | 1(`Value`) | 0 | — | 0 | 一般 | 55 / 0 | M: 毎朝使われている。Google Sheets に依存 |
| ナビ `<a>` | コンサルKPI | /consulting | cs_dashboard/ | 12,339(tests.rs 5,782) | cs_dashboard.html 7,126(JS 約 6,340、`:787-7124`) | 15(全 GET) | 0 | 手組み SVG【推測】 | 0 | 一般 | 128 + node 339 / 0 | **XL**: 16 画面を切り替える。**UI/UX 段 1 の改修が進行中**(直近 15 コミットがすべて cs 関係) |
| 非表示(ユーザー判断 2026-09-07) | 架電クオリティ | /call-quality | call_quality/ | 25,069 | テンプレ 17 枚 1,740 + CSS 950 + JS 968 | 24(GET 20 / POST 4) | 0 | E | 4 | 一般 | 307 / 1 | M〜L: 16 タブ中 14 は JS が無く雛形のまま【推測】(`_layout.html:283-296`) |
| window.open | consult 5 画面 | /consult/{hearing, hypothesis_review, brief, hearing_sheet, action_memo} | consult/ | 14,480 | Rust 内 HTML | 1(evidence_pack.json) | 0 | — | 2(SQLite INSERT) | 一般 | 178 / 0 | L: フォームと書き込みがある。brief・hearing_sheet・action_memo は印刷用(§1.3) |
| ヘッダーから | 管理 | /admin/*(4) | admin/ | 715 | Rust 内 HTML | 0 | 0 | — | 0 | admin | 少 / 0 | S |
| ヘッダーから | 個人設定 | /my/profile, /my/activity | my/ | 328 | Rust 内 HTML | 0 | 0 | — | 1 | 一般 | 0 / 0 | S |
| 公開 | ログイン | /login, /logout | lib.rs:835-837、src/auth | 339 + α | login_inline.html 47 | 0 | 0 | — | 1 | 公開 | — | S。React 化しない(F-4) |
| 参照なし | proposal-mock | /proposal-mock | lib.rs:892 | 数行 | 249 | 0 | 0 | — | 0 | 一般 | 0 / 0 | React に移して隠す(Wh)。使われていないと見られる【推測】 |

### 1.3 印刷・ダウンロード系レポート

共通の事実【確認済】:
- HTML は全部 Rust の文字列連結(`format!` / `push_str`)で組んでいる。テンプレートエンジンは使っていない。
- PDF はブラウザの印刷(`window.print`)に任せている。サーバで PDF を作る経路は無い(Dockerfile に Chrome が無い)。
- ECharts は CDN から読み込み、SVG 描画と `beforeprint` での resize で印刷崩れを避けている。
- **VRT は `/report/survey` だけ**: `tests/vrt/report.spec.ts` と png 16 枚、`.github/workflows/vrt.yml`。

| レポート | ルート | Rust 行 | 印刷 CSS | ECharts | 利用【推測】 |
|---|---|---|---|---|---|
| 媒体分析 | /report/survey(variant 3 種)、/download、job 版(lib.rs:410-440) | 61,552(report_html/ を再帰で 50 ファイル) | style.rs:41-262 ほか | SVG | 主力。ただし顧客向けは現在使っていない(ユーザー発言) |
| 総合診断 | /report/insight、/api/insight/report/xlsx | 1,706 + export 207 | あり | SVG | UI から到達できない |
| 統合 | /report/integrated | 882 + 39 | あり | — | dashboard_inline.html:1564 から開ける |
| 企業 | /report/company/{cn}、/api/company/bulk-csv | 1,385 | あり | CDN @5(未固定) | 企業検索から |
| 採用市場 | /report/indeed | 837 | あり | 5.5.1 | 本番では 404(INDEED_PUBLIC が off) |
| 求人検索 | /api/report | 726 | あり | — | 非表示タブから |
| コンサル印刷物 | /consult/{brief, hearing_sheet, action_memo} | 1,315 / 1,400 / 1,351 | @page | — | 社内用 |
| 求人票 顧客レポート | POST /api/jobgen/journey-customer-report | 1,299 | あり | — | 求人票作成から |
| 架電 CSV | POST /api/call-quality/browse/export | — | — | — | 架電クオリティから |

**ユーザー発言(2026-09-29): 顧客向けレポートは現在使っていないので自由に修正してよい。**

### 1.4 合計(重複を除いて再計測)【確認済】

| 項目 | 値 |
|---|---|
| Web 画面(入口単位) | ダッシュボードのタブ 23(表示 10 / 非表示 9 / dead 4) + 独立ページ約 17(営業KPI 1、コンサルKPI 1、架電 1(内部 16 タブ)、consult 2(画面として使うもの)、admin 4、my 2、login 1、proposal-mock 1、jobgen / keywords の iframe 本体 4) ≒ **40** |
| レポート | 約 10 系統(§1.3) |
| Rust `src/handlers/` | 211,453 行(テスト約 71,118、テスト以外約 140,335) |
| Rust `src/job_gen/` + `src/media_engine/` + `src/indeed/` | 22,221 + 7,679 + 5,012 |
| フロント資産 | templates 16,750 行 / static/js 3,800 行(ほかに minified 5 本 約 36KB、元ソースは無い) / static/css 3,367 行 / static/*.html 3,724 行。計 27,641 行・1.7MB |
| ルート | src/lib.rs に 190 本(/api/ 142、/tab/ 20、/report/ 7、その他 21)。これとは別に子ルーターが call_quality 25、cs_dashboard 16、driver 7、indeed 5 ほか |
| JSON API | ダッシュボードで 65 本、独立ページで約 50 本(consulting 15、call-quality 24 ほか) |
| HTML partial | 約 83 本。これらが JSON 化の対象になる |

数え方の限界:
- テスト行は「`#[cfg(test)]` 以降をファイル末尾まで」数えた概算。
- 担当ごとの集計では indeed / driver / license / regional / jobgen / keywords が重複していたので、この表では 1 回だけ数えた。

### 1.5 利用状況について【確認済・未照会】

- `src/lib.rs:1000-1019` の `meaningful_activity` は、GET の `/tab/*` を `view_tab` として audit DB の activity テーブルに記録している。`/report/survey` と `/report/integrated` も記録対象。つまり **タブごとの実際の利用件数は audit DB を読めば出せる。今回は照会していない**。
- 記録されないもの: `/consulting`、`/sales-kpi`、`/call-quality`、`/app/*`。そのため、移行後の利用状況を比べるには `meaningful_activity` に `/app/*` を足す必要がある(W2 の作業に含める)。

---

## 2. 共通基盤(W2 までに作るものと、必要になった波で足すもの)

### 2.1 App Shell とナビ(W2)

- `frontend/src/shell/` に置く。中身はヘッダー(ログイン中のメール・設定・管理リンク・ログアウト)、ナビ、ヘッダーフィルタ(フィルタは W4)。
- **ナビの定義は Rust が JSON で返す**(新規 `GET /api/nav`)。
  - 理由: タブを出すかどうかの判定(`{{KEYWORDS_TAB}}` / `{{JOBGEN_TAB}}` の環境変数、admin 判定)が Rust 側にあるため【確認済 lib.rs:1441-1453】。
  - 旧シェル(`dashboard_inline.html`)と React シェルの両方が、同じ定義からナビを描く。
  - **各項目に `hidden` フラグを持たせ、非表示画面の隠し方をこれで統一する**。現在はコメントアウト、初期状態で隠す、ナビに載せないだけ、が混在している。`hidden` の画面はナビに出さないが、`/app/{screen}` は生かして URL 直打ちでは開ける。復活はフラグを外すだけにする。platform-team が実装する。
- 並走期間中の移動: 旧シェルから React 画面へは `<a href="/app/x">`(営業KPI とコンサルKPI が既に使っている形。`dashboard_inline.html:152-155`)。React シェルから旧画面へは `<a href="/?tab=/tab/y">`(既存の `?tab=` 復元を使う)。
- **Headless CRM の画面も同じ Shell を使う**。CRM の React 画面は既に着手済みなので、Shell v1(ヘッダー、ナビ、`/api/nav`)と共通部品を CRM 側と W2 側で二重に作らないよう、置き場所は `frontend/src/shell/` と `frontend/src/components/` で、新設の platform-team が作り、crm-team と react-team が使う(F-6)。

### 2.2 ヘッダーフィルタ(W4 の前提)

現状【確認済】:
- フィルタは `tower_sessions::MemoryStore` に保存している(`lib.rs:92-105`)。
- 書き込みは `POST /api/set_{job_type, prefecture, municipality, industry_filter}` の 4 本で、応答は `Html("OK")`(`lib.rs:560-570, 1518-1580`)。
- 読み出しは `get_session_filters`(`overview.rs:188`)を 67 箇所から呼んでいる。
- URL には状態を持たない。

方針:
1. 過渡期は **「session が正」のまま**にする。React 画面は起動時に新規 `GET /api/filters/current` で session の値を読む。変更するときは既存の `POST /api/set_*` を呼び、同時に URL クエリ(`?pref=&muni=&ind=`)にも書く。これで旧画面と新画面の間でフィルタが共有される。
2. API 側に「クエリがあればクエリ、無ければ session」を返す helper を 1 つ足し、React 向けの JSON API はこの helper で読む。
3. W10 で session 経路(`set_*` 4 本と `get_session_filters`)を消し、URL を正にする。非表示の画面も React に移る(Wh)ので、W10 の時点で session 経路を使う画面は無くなる。
- 背景: MemoryStore なので、再デプロイのたびに全員のフィルタとログインが消える【推測】。URL を正にすればフィルタのほうはこの影響を受けなくなる。

### 2.3 API client の拡張と、401・CSRF(W6 までに。401 は W2 で)

- Phase 0 の `client.ts` は GET 専用で、`Result` 型を返す【確認済】。これに `post<T>()`、`postForm()`、`upload()`(multipart、進捗付き)、`pollJob()`(survey の非同期ジョブ用)を足す。
- **401(Rust の変更)**: 現在は `/api/*` でも 303 → `/login` の HTML を返している(`auth/mod.rs:20-27`)。これを「`/api/*` かつ `Accept: application/json` で、`HX-Request` ヘッダーが無い」ときだけ 401 JSON を返すようにする。
  - HTMX は `HX-Request` を送るので、旧画面の挙動は変わらない【推測: 実装時にテストで確かめる】。
  - これは 1A 計画 U-5 の案 B にあたる。client 側のリダイレクト検出(案 A)は、変更後も残しておく。
- **CSRF**: 現在はトークンを使わず、Origin/Referer を検査している。**Origin も Referer も無いリクエストは通してしまう**(`lib.rs:908-963`)。
  - 案: React からの POST には `X-Requested-With: fetch` を付けることを必須にし、この穴を塞ぐ。
  - トークン方式への移行は ADR に無いので、ここでは提案しない。
  - 実施は決定済み(F-5)。W2 で入れる。
- OIDC(ADR-017)に移っても、React はセッション Cookie だけに依存する。ログインの手段は知らない(1A 計画 C-10 と同じ)。

### 2.4 共通コンポーネント

| 部品 | 方針 |
|---|---|
| `DataTable` | 500 行を超えるときだけ仮想化する(設計書の「巨大 table の全行 DOM 化」禁止に従う) |
| `KpiCard` | 値・単位・n(サンプル件数)を必須 props にする(CLAUDE.md §11 の誠実性) |
| `Note` | 「HW 掲載求人のみ」の注記と「相関≠因果」の注記を部品にする。`phrase_validator` の禁止語リストを ts-rs か JSON で共有し、Vitest で UI 文言を検査する |
| `EChart` | npm の `echarts/core` に使うチャートだけ登録し、dynamic import で読む。ResizeObserver と `beforeprint` での resize を持たせる。E2E が初期化完了を確かめられるよう `data-testid` を付ける(`feedback_e2e_chart_verification`)。**CDN の 5.5.1 / @5 未固定の混在(§1.3)はここで解消する** |
| `LeafletMap` | react-leaflet は使わず、useEffect の薄いラッパにする。レイヤーは props で宣言し、unmount 時に `map.remove()` を呼ぶ。地図の画面だけで lazy load する(設計書の「Leaflet を initial load しない」) |

### 2.5 URL 設計

- React 画面は `/app/{screen}`(kebab-case)。`KNOWN_SCREENS`(`spa_shell.rs:38`)に登録すれば公開、外せば撤去。【確認済: Phase 0 の実装】
- 旧 URL は、その波の完了条件を満たしたら 302 で `/app/...` に向ける。`?tab=/tab/x` とフィルタのクエリも変換する。**並走は最低 1 リリース**。ブックマークは 302 で引き継がれる。
- `meaningful_activity` に `/app/*` を加え、旧 URL と新 URL の利用件数を同じ物差しで比べられるようにする(§1.5)。

### 2.6 スタイル

- 現状【確認済】: `tailwind-precompiled.css` 38KB は手で保守していて、config は無い。これとは別に `dashboard.css` 42KB と `call_quality.css` 24KB がある。
- 方針: **Tailwind をビルド型(Vite プラグイン)で入れるが、二重には読み込まない**。`/app/*` は Vite が出力する CSS だけを読み、旧画面は precompiled だけを読む。
  - navy-700 などの色トークンは precompiled から抜き出して `@theme` に再現する。
  - `dashboard.css` から移すのは、ヘッダーとフィルタの共通部分だけにする。
- `tests/css_classes_exist.rs` は .tsx を対象外にする(クラスが存在するかはビルドが保証する)。
- W10 で precompiled、dashboard.css、call_quality.css を消す(レポートと非表示画面も React に移るので、残す理由が無くなる)。

### 2.7 E2E / VRT

- **PR ごとの E2E**: fixture の SQLite を作り、Rust を起動して Playwright を当てる。1A 計画 1A-5 の経路を、全波で使うジョブとして共通化する。現状は PR で Playwright が 1 本も走っていない【確認済: ci.yml】。
- **旧新一致の spec**: 各波で 1 本以上作る。同じ fixture を使い、旧画面と新画面の両方から表示値(KPI、件数、表の先頭 N 行、チャートの series)を取り出して比べる。**「要素がある」ことではなく、値で判定する**(`feedback_test_data_validation`)。
- VRT: 既存の vrt.yml と同じく、Linux で `toHaveScreenshot` を撮る。レポートの 16 枚は、W9 で旧レポートと新しい印刷用画面を同じ fixture で撮り比べ、差が説明できる状態にしてから新画面側で撮り直す(§6-1)。
- nightly の本番 smoke(regression.yml)は残す。移行した画面ごとに、対象 URL を `/app/*` に書き換える。
- Render が CI の緑を待ってからデプロイするかは【要確認】(1A 計画 U-8)。

### 2.8 HTML partial を JSON にする標準手順(W3〜W8 共通)

1. partial の render 関数が受け取っている値を特定する。
2. その値を、そのまま `#[derive(Serialize, TS)]` の Response struct にする。
3. 既存のハンドラを「struct を作る → render」の 2 段に分ける。挙動は変えない。**分ける前と後で HTML が一致することを snapshot で確かめる**。
4. `/api/...` に `Json<struct>` のルートを足す。
5. contract test を書く(tempfile DB を使い、具体値で assert する)。`frontend/src/generated` の再生成差分がゼロであることも確かめる。
6. React の画面を作る。`escape_html` の役目は React の既定のエスケープに移る。
7. 旧画面を撤去するときに、旧ハンドラと render を消す。

- 手順 3 を先に入れるのは、JSON 化の途中で旧画面の値が変わる事故(2026-04-23 の 8 panel 全滅と同じ種類)を避けるため。

---

## 3. 移行の波

W2 以降は担当チームを分けて並行で進める(2026-09-29 の体制。チーム共通の決まりは作業文書 `C:\dev\WAVE_TEAM_BRIEF.md`)。波の番号は範囲の区切りで、着手順を固定するものではない。

| 波 | 対象 | 担当 | 選んだ理由・順番の制約 | 前提になる基盤 |
|---|---|---|---|---|
| **W0 済** | Phase 0(PR #28、main にマージ済み ad7d918) | — | — | — |
| **W1 進行中** | 採用診断(1A)。並行して Headless CRM の React 画面 | react-team / crm-team | 1A は移行経路が成り立つかの確認(ADR-014)。CRM は 1A の完了を待たない(下の注) | client.ts(GET) |
| **W2** | 営業KPI + App Shell v1 | wave-a | 毎朝使われていて、最初に目に見える成果になる。独立ページなのでフィルタの共有が要らない。JSON は 1 本、POST は 0 本 | Shell v1(フィルタ無し)、`/api/nav`、401 JSON 化、Google Sheets の fixture(無いと 503 になる) |
| **W3** | 職種辞典、資格辞書、キーワード需要、ガイド | wave-a | session フィルタを読まない画面なので、フィルタ共有の問題を避けつつ「React 画面をナビに載せる」経路を先に作れる | Shell v1 と、旧シェルとの相互リンク |
| **W4** | 地域分析、企業検索、採用市場 | wave-b | HTML partial を JSON にする作業の中心。採用市場は既存の E2E 3 本を旧新比較に使える | フィルタの受け渡し(§2.2)、DataTable / KpiCard / Note、§2.8 の手順 |
| **W5** | 地図 | wave-c | 技術リスクがいちばん大きい(Leaflet のライフサイクル、元ソースの無い minified JS 25KB) | LeafletMap と EChart のラッパ |
| **W6** | 媒体分析の画面部分 | wave-c(W5 の後) | POST・multipart・ジョブ待ちが要る。survey-team が同じ画面に競合調査を追加実装中なので、その作業と時期を合わせる(survey-team の Indeed 競合調査 Phase B の後。Phase B は、媒体分析のアップロード画面に職種 `norm_title` の選択欄を足し、Indeed の市場データ `insight_*` を §05B に結合する作業) | client の POST / upload / pollJob |
| **W7** | コンサルKPI | 未定(後で割り当て) | 規模が XL で、UI/UX 改修が進行中。**着手条件は「改修が落ち着いた合意」かつ「`cs_dashboard.html` に 2 週間変更が無い」**(F-3) | Shell、DataTable、KpiCard |
| **W8** | 架電クオリティ(全 16 タブ。未実装の 14 タブは雛形のまま移して `hidden`)、consult の画面 2 つ、admin 4、my 2、求人票作成 | wave-d | 利用者が限られる画面と、JSON API が既にそろっている画面(架電・求人票作成) | POST client。OIDC の後なら権限表示も |
| **Wh** | 非表示 8 タブ(市場概況・地域カルテ・詳細分析・総合診断・トレンド・都道府県比較・条件診断・求人検索)、dead route 4、proposal-mock | wave-h | 削除せず React に移し、`hidden` で隠す(いつ復活させるか分からないため)。詳細分析(XL)と総合診断(L)を含む | Shell の `hidden` フラグ、§2.8 の手順、フィルタの受け渡し |
| **W9** | レポート(§1.3 の全系統。consult の印刷物 3 つを含む) | wave-e | 印刷品質(A4、改ページ、`beforeprint`)を落とさずに React の印刷用画面へ移す(§6-1) | EChart の SVG 描画と印刷時の resize、印刷用 CSS、レポートの集計を JSON にする作業 |
| **W10** | 撤去(§5) | platform-team | 表示中の画面、非表示の画面、レポートがすべて `/app/*` にそろってから。W7 と競合調査が残っている間は、それらが使う部分(インラインスクリプト等)を残して先に撤去できる範囲だけ進める | — |

注(ADR-014 の補足。ユーザー判断 2026-09-29): 「HubSpot の能力をアプリ側に持たせる件は、HTMX で作ると React 移行時の実装コストが高いので、最初から React で作ってよい」。ADR-014 は「CRM を React 導入の実験台にしない」という決定だが、Phase 0(ビルド、`/app/{screen}`、ts-rs、CI の frontend ジョブ)は main にマージ済み(ad7d918)で、この範囲の経路は成り立っている。PR ごとの Playwright E2E はまだ 0 本で、1A-5 で整備中。そのため CRM を 1A と並行して進めてもこの趣旨には反しないと扱い、共通ジョブが整う前に CRM 画面を公開する場合は CRM 側で E2E を持つ。

---

## 4. 各波の完了条件と、旧画面の撤去手順

### 4.1 共通の完了条件(各波で全部満たす)

1. **旧新一致の spec が PR ごとの E2E で緑**。同じ fixture で、KPI・件数・表の値・チャートの series を突き合わせる。
2. ECharts は初期化が終わったことを確認する(canvas や svg が存在するだけでは合格にしない。`echarts.getInstanceByDom(el)` が null でなく、series のデータ件数が合う)。
3. 新しく作った JSON API すべてに contract test があり、ts-rs の生成差分がゼロ。
4. 注記(HW スコープ、相関≠因果)が React 側にも出ていることを文言で確認する。
5. `meaningful_activity` で `/app/{screen}` の利用が記録されている。

### 4.2 波ごとの追加条件

| 波 | 追加条件 |
|---|---|
| W1 | 1A 計画 §5.2 のとおり。contract の破壊を CI で検出した実績が 1 回あること、PR ごとの E2E が 3 回続けて緑であること |
| W2 | 営業KPI の全 KPI の値が旧新で一致(Sheets fixture 上で)。営業担当が 1 週間使って問題なし(ユーザーが確認) |
| W4 | partial ごとに、旧 HTML に出ていた値と新 JSON の値が等しいことを contract test で確かめる。採用市場の E2E 3 本を `/app/indeed` に移し、緑にする |
| W5 | 6 レイヤーそれぞれで、マーカー件数・コロプレスの階級値・半径検索の件数が旧新で一致。tabcache の除外リストから jobmap を外せる状態になっている |
| W6 | upload → analyze → integrate の結果の値が一致。同じ CSV から同じ集計になる |
| W7 | node テスト 339 件の検証項目を、Vitest と Playwright へ移す対応表を先に作り、全項目を移し終える。16 画面それぞれで fixture の値が一致 |
| Wh | 移した画面がナビに出ず(`hidden`)、URL 直打ちでは開けること。旧画面と値が一致すること。地域カルテのように旧画面がいま正しく動かないものは、旧画面の不具合を記録したうえで、React 側の値を tempfile DB の具体値で確かめる |
| W9 | 同じ fixture で、旧レポートと新しい印刷用画面の表示値(表・KPI・チャートの series)が一致。A4 の印刷プレビュー(Playwright の `page.pdf`)でページ数と改ページ位置を旧レポートと比べ、差を説明できる。VRT 16 枚を新画面で撮り直す |

### 4.3 撤去手順(画面ごと)

1. `/app/x` を公開し、旧シェルのナビは旧画面のまま残す(並走開始)。
2. 完了条件を満たしたら、ナビのリンク先を `/app/x` に切り替える(非表示の画面は `hidden` のまま)。
3. 1 リリース以上並走させる。その間、audit で新旧の利用件数を比べ、旧 URL への直接アクセスを確かめる。
4. 旧 URL を 302 で `/app/x` へ向ける。
5. 次のリリースで、旧テンプレート・旧ハンドラ・旧 partial ルートを削除する。削除時は `git diff --cached --stat` で削除ファイルを確かめ、ファイル名を指定して add する(CLAUDE.md の絶対ルール)。
6. 戻す手順: 302 を外すだけで旧画面に戻れるのは手順 5 の前まで。手順 5 の後は git revert で戻す。

---

## 5. 最終段(W10)で撤去するもの

| 撤去対象 | 条件 |
|---|---|
| `templates/dashboard_inline.html`、`/` の旧シェル | ダッシュボードのタブ(非表示を含む)がすべて `/app/*` にそろった |
| `static/js/tabcache.js`、`app.js`、`charts.js`、`a11y.js`、`axis_drag.js` ほか | 参照が 0 件(grep で確認) |
| CDN の HTMX 2.0.4、ECharts、Leaflet の読み込み | 参照が 0 件。レポートも React に移る(W9)ので、ECharts は npm 版だけになる |
| `POST /api/set_*` 4 本、`get_session_filters`(67 箇所) | 全画面が URL のフィルタに移った |
| 旧 `/tab/*` ルートと partial ルート(約 83 本)、旧 `/report/*` の HTML 生成 | 302 期間が終わった |
| `tailwind-precompiled.css`、`dashboard.css`、`call_quality.css`、`tests/css_classes_exist.rs` の対象範囲 | 旧画面が 0 になった |
| CSP の `'unsafe-inline'` と CDN 3 ドメイン(`lib.rs:846-858`) | インラインスクリプト(onclick 103 箇所、`<script>` を埋め込む .rs 27 ファイル)が 0 になった。**W7(コンサルKPI)と競合調査の画面、login ページが残っている間は外せない**。login ページ(`login_inline.html`)のインラインは W10 で外部ファイルへ移す |
| ドキュメント | ルートの CLAUDE.md §2・§3・§7(9 タブの記述は既に現状と合っていない)、`src/handlers/CLAUDE.md`、`docs/tab_naming_reference.md`、E2E ガイド、`frontend-react-migration.md` の「最終」節を書き換える |

- 完了の確認: 上の撤去をした状態で全 E2E が緑になり、ルート衝突テスト(`app_routes_no_conflict.rs`)で旧ルートが 0 本であることを確かめる。

---

## 6. 論点と決定(2026-09-29)

### 6-1. レポート系(→ React の印刷用画面へ移す)

- 経緯: 棚卸しの段階では「印刷専用のサーバ HTML として凍結する」案を推奨し、いったん承認された。同日のユーザー指示「フロントエンドの React 化は決定事項で、全てにおいて実行する」で取り消し、React に移すことに決まった。
- 事実【確認済】: `/report/survey` の `report_html/` だけで 61,552 行(テスト名ファイル 4,293 行を含む)あり、すべて文字列連結で組んでいる。印刷崩れの対策(SVG 描画、`beforeprint` での再描画、軸ラベル欠けの対処)が実装に埋め込まれている。VRT(png 16 枚)はこのレポートにしか無い。ユーザーは「顧客向けレポートは現在使っていない」と言っている。
- 方式: `/app/print/{report}` の React 画面にする。SSR は ADR-015 により使わず、クライアントで描いてブラウザの印刷で PDF にする。
  - 集計と描画を分ける: survey は集計と HTML の組み立てが一体なので、§2.8 の手順 3 と同じく「集計結果の struct を作る → HTML」に分けてから JSON API を足す。
  - EChart は SVG 描画にし、`beforeprint` で resize する(旧レポートの対策を部品に持たせる)。
  - 印刷用 CSS(A4、改ページ、`@page`)は印刷用画面の共通部品にする。
- 検証: W9 の完了条件(§4.2)。旧レポートは新画面の値と印刷結果がそろうまで残す。
- 工数: 60〜120 人日【推測】。

### 6-2. login / admin / my(→ login は Rust に残す)

- login は Rust 側に残す。OIDC のコールバック、SameSite=Strict の Cookie、レート制限が Rust にあるため(1A 計画 C-10、ADR-017)。これが「全部」の唯一の例外。
- admin と my は W8 で React 化する。

### 6-3. 非表示タブ・dead route・未実装タブ(→ React に移して隠す)

- 経緯: 「アクセス 0 件なら削除」→「削除せず HTMX のまま隠す」と変わり、最終的に「React に移して隠す」に決まった(いつ復活させるか分からないため、削除はしない)。
- 対象: 非表示 8 タブ(市場概況、地域カルテ、詳細分析、総合診断、トレンド、都道府県比較、条件診断、求人検索)、dead route 4、proposal-mock、架電クオリティの雛形 14 タブ。採用診断は非表示タブだが W1 で移す。
- 隠し方: `/api/nav` の `hidden` フラグに統一する(§2.1)。
- 影響: W10 で HTMX・旧シェル・session 経路まで撤去できる(§5)。
- 工数: 60〜110 人日【推測】(詳細分析 XL、総合診断 L、求人検索 L ほか)。
- 復活の判断材料として、audit の `view_tab`(§1.5)で URL 直打ちの利用件数を見られる。照会は未実施(audit DB の読み取り専用の接続情報が作業環境に無い)。

### 6-4. 並走方式(→ 独立ページ)

- 独立ページのままにする。「React の Shell が旧画面を iframe で包む」方式は採らない。
  - 理由: 旧画面は session フィルタ、tabcache、onclick(103 箇所)に依存している。iframe で包むと、高さの調整、postMessage によるフィルタ同期、Playwright の frame をまたぐ操作が要り、どれも並走期間が終われば捨てる作業になる(10〜20 人日【推測】)。
  - 逆向き(旧シェルが React 画面を iframe で包む)は keywords と jobgen に前例があるので、W3 の暫定策としては許容する。

---

## 7. 工数とリスク

### 7.1 工数の目安(人日。Claude が実装し、ユーザーがレビューする前提。すべて【推測】)

| 波 | 幅 | 根拠 |
|---|---|---|
| W2 | 18〜35 | sales_kpi の JS 994 行、struct 化 1 件、Sheets fixture(8〜15)。Shell v1、`/api/nav`、相互リンク、401 JSON 化(10〜20) |
| W3 | 15〜30 | driver 5〜10(JSON 4 本。テストが 0 件なので契約テストを新しく作る)、license 3〜6、keywords 6〜12(824 行)、guide 1〜3 |
| W4 | 50〜95 | 地域分析 10〜20(partial 10 本の JSON 化)、企業検索 20〜35(partial 10 本、全部 HTML)、採用市場 20〜40(9.4k 行) |
| W5 | 40〜80 | JSON 16 本 + partial 14 本、Leaflet ラッパ、元ソースの無い minified JS の読み直し |
| W6 | 25〜50 | 画面部分。upload、非同期ジョブ、POST client |
| W7 | 50〜90 | JS 6,340 行、API 15 本、16 画面、テスト 339 件の置き換え |
| W8 | 35〜75 | 架電 10〜25、consult 10〜20、admin/my 5〜10、求人票作成 15〜30(JSON 19 本は既存。画面 JS 約 3,400 行) |
| Wh | 60〜110 | 詳細分析(XL、12.9k 行)、総合診断(L、38 パターン)、求人検索(L、partial 18 本)、ほか S〜M の 9 画面 |
| W9 | 60〜120 | report_html 61,552 行の集計と描画の分離、印刷用部品、VRT の撮り直し。他のレポート 8 系統 |
| W10 | 5〜10 | 撤去、CSP の締め直し、ルート衝突テスト |
| **合計** | **約 360〜695** | W1・CRM・競合調査は別チームの作業なので含めない |

- 幅が広い理由は 3 つ: HTML partial を JSON 化する量が画面ごとに違うこと、テストが 0 件の画面(driver / license / diagnostic / my)では契約テストを新しく作る必要があること、レビューの待ち時間を見積もれないこと。

### 7.2 リスク

| リスク | 内容 | 対策 |
|---|---|---|
| 表示の回帰 | JSON 化の途中で旧画面の値が変わる | §2.8 の手順 3(分ける前と後で HTML が一致することを snapshot で確かめる)と、旧新一致の spec |
| 並行作業の衝突 | 6 チームが `src/lib.rs` のルート登録、`spa_shell.rs` の `KNOWN_SCREENS`、`app_api.rs` の export、`vite.config.ts` の input を同時に触る | 追記だけにして衝突を小さくする。共通部品は platform-team だけが作る |
| ビルドの負荷 | 並行チームが cargo を同時に走らせると、手元の PC が過負荷になる(以前、強制終了した) | cargo は同時 4 本までに制限する(チーム共通の決まり) |
| Render のビルド時間とメモリ | Node 段を足した Docker ビルドの負荷が未計測。**Render は有料プラン**(ユーザー確認 2026-09-29。プラン名はダッシュボード参照。`render.yaml` の `plan: free` は実態と違う)。ビルド時間の上限と、CI の緑を待ってデプロイする設定かは未確認(1A 計画 U-4, U-8)。「Render のデプロイが約 40 分反映されなかった」という記録がある(`claudedocs/AUTONOMOUS_SESSION_REVIEW_2026-07-20.md:41`) | 画面が増えても `npm ci` のキャッシュが効くかを確かめる。ビルド時間を記録する |
| バンドルサイズ | Phase 0 の dummy で 222KB(gzip 後 69KB)。ほぼ React 本体 | 画面ごとのエントリ(実装済み)に加え、React 本体を共有 chunk にする。ECharts と Leaflet は dynamic import にする。予算は gzip 後で「画面ごと +150KB 以下」を目安にする【推測】 |
| 印刷の品質 | レポートを React で描くと、旧レポートの印刷崩れ対策を作り直すことになる | W9 の完了条件(A4 のページ数・改ページ位置の比較、VRT の撮り直し) |
| 並行する改修との衝突 | cs_dashboard は UI/UX 改修が続いている。媒体分析には競合調査が追加されている | W7 の着手条件、W6 の時期を survey-team と合わせる |
| 二重保守 | 並走期間は、旧画面と新画面の両方に修正が要る | 並走期間を 1〜2 リリースに限り、並走中の旧画面は機能を凍結する |
| セッションの消失 | MemoryStore なので、再デプロイで全員ログアウトする | 移行とは別の論点。フィルタは URL を正にすることで影響が減る |
| minified JS | postingmap.js など 5 本は元ソースが無い(初出は ed990c3、2026-03-01)【推測】 | W5 では、コードではなく現在の挙動を仕様として読み取り、E2E で固定してから作る |
| ドキュメントのずれ | ルートの CLAUDE.md の「9 タブ公開」が現状と違う | W10 でまとめて書き換える。それまでは、各波の PR で該当する節に注記する |

---

## 8. 決定事項(2026-09-29、ユーザー本人)

| ID | 論点 | 決定 | 記録先 |
|---|---|---|---|
| F-1 | レポート系 | **React の印刷用画面に移す**(W9)。印刷品質を落とさない。※いったん「凍結」と決めたが、同日のユーザー指示で取り消した | ADR-002 追記、本書 §6-1 |
| F-2 | 非表示 8 タブ・dead route 4・proposal-mock・架電の雛形タブ | **削除しない。React に移し、`/api/nav` の `hidden` フラグで隠す**(Wh)。※「アクセス 0 件なら削除」→「HTMX のまま隠す」を経て、この形に決まった | ADR-002 追記、本書 §6-3 |
| F-3 | コンサルKPI の時期 | UI/UX 改修が落ち着いてから W7 で移す。着手条件は「落ち着いた合意」かつ「`cs_dashboard.html` に 2 週間変更が無い」 | 本書 §3 |
| F-4 | login | Rust 側に残す。admin と my は W8 で React 化する | ADR-002 追記 |
| F-5 | Rust 側の変更 | `/api/*` の 401 JSON 化(1A 計画 U-5 の案 B)と、React からの POST に `X-Requested-With: fetch` を必須にする CSRF 補強を、W2 で入れる。旧画面は `HX-Request` で区別し、挙動を変えない | 本書 §2.3 |
| F-6 | 共通基盤の担当 | App Shell(ヘッダー・ナビ・`/api/nav`・フィルタ)と共通部品(DataTable、KpiCard、Note、EChart、LeafletMap、client の拡張)は **platform-team が作る**。他のチームはそれを使い、自前で作らない。置き場所は `frontend/src/shell/` と `frontend/src/components/` | 本書 §2・§3 |
| F-7 | 並走期間 | 最低 1 リリース。audit で新旧の利用件数を見て、302 に切り替える時期を決める | 本書 §4.3 |
| F-8 | Render | **有料プラン**(ユーザー確認。プラン名はダッシュボード参照)。`render.yaml` の `plan: free` と CLAUDE.md §1 の「Render Free」は実態と違う(本 PR では CLAUDE.md に注記だけ入れ、`render.yaml` は変えない)。ビルド時間の上限と、CI の緑を待ってデプロイする設定かは未確認 | 本書 §7.2、CLAUDE.md §1 |
| — | 範囲外(追加実装中) | 媒体分析の競合調査(survey-team)、Headless CRM(crm-team、最初から React)、コンサルKPI(改修中、F-3)。それぞれの作業が落ち着いてから扱う | 本書 §0 |

---

## 付録 A. 棚卸しの方法

- 棚卸しは 4 本のサブエージェントに分けた。A はダッシュボードのタブ、B は独立ページ、C はレポート、D は横断基盤を担当した。
- その後、別のサブエージェント(Opus)が行数・API 本数・ルート数を抜き取りで測り直した。survey、cs_dashboard、call_quality、job_gen、jobmap、competitive、analysis、insight、indeed、consult はすべて一致した。
- 数え方の違いとして注記したもの:
  - job_gen は `src/handlers` の外にある。
  - report_html は再帰で数えて 50 ファイル。
  - jobmap のルートは全部で 31 本(入口を含む)。
  - lib.rs の 190 本には子ルーターの分が入っていない。
  - minified JS は改行が無いので `wc -l` では 0 行になる。
- 波の組み立てと rule 16 の論点整理は、別のサブエージェント(Fable)に案を出させ、リーダーが統合した。統合時には、採用診断(Phase 1A)を非表示画面の扱いから外した。
