# 競合調査(/competitor)の React 移行 差分洗い出し・逆証明・実装計画

作成日: 2026-10-04 / レーン A 段階 1・2(+段階 3 以降の計画)/ ブランチ `feat/react-competitor`(`origin/main` 3290da7 = PR #53 マージ直後)
この文書は調査と計画だけで、コードは書いていない。

表記: 【確認済】= ファイルを読んだ事実(`ファイル:行`)。【推測】= 読んだ範囲からの推論で、実行して確かめていないもの。【要確認】= 着手前に実測が要るもの。
行番号は 3290da7 時点。

---

## 0. 結論(要約)

1. **旧画面の実体は小さい**。入力フォーム 1 枚(`templates/competitor.html` 59 行)+ ハンドラ 1 本(`src/handlers/competitor.rs` 291 行)+ 4 タブのレポートを組む Rust 関数(`competitor_report.rs` 533 行)+ PDF 生成(`competitor_pdf.rs` + `scripts/pdf/render.cjs` 45 行)。**session・フィルタ・クエリには一切依存しない**(読み書きとも無し)。移行の難所は画面の量ではなく、(a) 「POST して HTML 文書を丸ごと受け取る」形、(b) PDF が「サーバが組んだ HTML を Chromium で印刷する」形、の 2 点。
2. **レポート本体の推奨は「JSON + React を主、PDF は当面サーバ HTML のまま(方式 B)。W9 で React 印刷画面へ寄せる(方式 C)」**(§2)。いきなり PDF まで React 化(C)しない理由は、A3 横 4 ページ固定の収まり検証(`render.cjs`)が PR #53 で直したばかりで、画面側の描画を作り直すと同時に壊すリスクが大きいため。iframe に HTML を差し込む案(A)は、移行としては「React の殻を付けただけ」になり、W10 の CSP(`'unsafe-inline'` 撤去)とも衝突する。
3. **逆証明で見つかった「旧画面で実際に起きる」問題**(§3)のうち、React 版で必ず対処するもの:
   - 二重送信で Google 広告 API を 2 回(×2 本)叩く。サーバ側に抑止がない(`competitor.rs:148-152,240-256`)。
   - PDF 作成の最悪待ち時間が約 150 秒(Google 45 秒 + 混雑待ち 60 秒 + 描画 45 秒)で、React の既定アップロードタイムアウト 120 秒(`client.ts:21`)を超えうる。
   - 画面確認(html)モードは POST の応答がそのまま文書になるので、再読み込み・戻るで CSV 再送(Google も再取得)になる。
   - エラーが「400 + HTML の `role="alert"`」で返る。`/report/*` は 401 JSON の対象外(`/api/*` ではない)なので、ログイン切れは 303→`/login` を `response.redirected` で推測している(`templates/competitor.html` の script)。
   - 外部入力のエラー文(CSV 解析エラー `ヘッダー読み取りエラー: {e}`)がそのまま画面に出る(`competitor.rs:123-129` → `upload.rs:314`)。
4. **PR は 7 本に分ける**(§5)。先頭 2 本は「Rust の挙動を変えずに JSON 化できる形へ割る」リファクタで、先に旧 HTML の golden を固定してから割る。React 画面は PR-4 まで公開しない(KNOWN_SCREENS に載せない)。
5. **ユーザー判断が要る点**は §7(7 件)。特に (1) PDF の方式(B を段階採用でよいか)、(2) 生成結果を一時キャッシュして PDF で再利用するか(Google の二重取得をなくせるが、新しい状態をサーバに持つ)、(3) CSV サイズ・行数の上限を設けるか。

---

## 1. 段階 1: 旧画面の機能一覧と React への移し方

### 1.1 入口とルート

| # | 項目 | 事実【確認済】 | React 版の方針 |
|---|---|---|---|
| R-1 | ナビ | `nav.rs:185-194` で id `competitor`、`NavKind::Page`、target `/competitor`、hidden/requires なし。第 1 段(`nav.rs:750,888-890` のテストが位置を固定) | PR-6 で `kind: NavKind::App`、target `/app/competitor` に変えるだけ。位置・ラベル(競合調査)・title は変えない。`nav.rs` のテスト(`:888-890`、`:1163-1168`)の期待値も同じ PR で更新 |
| R-2 | 入力画面ルート | `GET /competitor`(`lib.rs:393`)→ `competitor::page`。認証必須(`protected_routes` 配下) | `GET /app/competitor` を `KNOWN_SCREENS`(`spa_shell.rs:38`)に追加、`frontend/src/entries/competitor.tsx` と `vite.config.ts` の input に追加。旧 `/competitor` は 302(並走 1 リリース後) |
| R-3 | 送信ルート | `POST /report/competitor`(`lib.rs:394-397`)。`DefaultBodyLimit` = `UPLOAD_BODY_LIMIT_BYTES` = 20MB(`lib.rs:51`) | 新設 `POST /api/competitor/report`(JSON を返す)と `POST /api/competitor/pdf`(PDF を返す)。どちらも `/api/*` 配下なので、未ログインは 401 JSON(`auth/mod.rs:34-83`)、POST は `X-Requested-With` 必須(`lib.rs:1016-1091`)になる。**旧 `/report/competitor` は並走中は変えない** |
| R-4 | 認証/CSRF | 旧: Origin/Referer 検査のみ。`competitor_tests.rs` に「未ログインは 303 `/login`」「外部 Origin は 403」のテストがある | 新 API でも同テストを `/api/competitor/*` 向けに複製(401 JSON になる点だけ変える) |
| R-5 | session / クエリ依存 | `competitor.rs` は `State(AppState)` だけを使い、session もクエリも読まない。`/?tab=%2Ftab%2Findeed` へのリンクが画面内にあるだけ | フィルタ共有(`/api/filters/current`)は不要。画面は Shell 非依存でよい(sales-kpi と同じ形)。戻り先リンクは `/app/survey` ではなく当面 `/`(媒体分析)と `/?tab=%2Ftab%2Findeed` を維持 |

### 1.2 入力項目と検証

旧画面の検証は「ブラウザ側(HTML 属性)」と「サーバ側(`competitor.rs`)」の二重。サーバ側が正で、ブラウザ側は一部しか持たない。

| 項目(name) | ブラウザ側 | サーバ側【確認済】 | 不一致・注意 | React 版の方針 |
|---|---|---|---|---|
| `csv_file` | `required`、`accept=".csv,.txt"`(`competitor.html` 内) | 空ならエラー「求人一覧CSVを選択してください…」(`competitor.rs:95-97`)。サイズは 20MB 上限(`lib.rs:51`)。拡張子は見ない | 0 バイトファイルは `required` を通る(旧 E2E `competitor.spec.ts` の 2 本目がこれを使っている)。拡張子偽装・巨大・文字コード違いはサーバの解析に任せている | 選択時に 0 バイト・20MB 超・拡張子を即時に弾く(サーバ側の検証は残す。ブラウザ側は親切のため)。ファイル名と大きさ・先頭の文字コード推定結果を表示 |
| `source_type` | select。`indeed_sp`(既定)/`indeed` | `indeed`→`UserSourceHint::Indeed`、`indeed_sp`→`IndeedSp`、他は 400「IndeedまたはIndeed (SP)を選択してください」(`:105-109`) | 値の取り違え(PC/SP)は解析側が補正する(`upload.rs:319-330` のコメント)。補正されたことは画面に出ない | select を維持。補正が入ったかを応答の `warnings` に出す(PR-2 で `parse` から取れるか要確認【要確認】) |
| `wage_mode` | select。`monthly`(既定)/`hourly` | `monthly`/`hourly` 以外は 400(`:110-114`) | 給与の単位を取り違えると全指標が無意味になるが、検知はない | 維持。結果画面の見出しに単位を必ず表示(旧と同じ「万円」「円/時」) |
| `top_n` | number、min 1・max 200・既定 45・required | `parse_top_n`(`section_05b_competitor.rs:35-40`): 空/非数/0 以下→45、200 超→200 に**黙って丸める** | 旧画面は丸めたことを知らせない | 送信前に 1〜200 の整数だけ通す。応答に実際に使った `top_n` を返し、入力と違えば注記 |
| `prefecture` | select(全国 + `PREFECTURE_ORDER` 47) | 空以外は `PREFECTURE_ORDER` に含まれるか検査、違えば 400(`:116-119`)。**CSV の行を絞る設定ではない**(注記あり) | 空(全国)だと人口・地域タブは「都道府県を選択してください」(`:187-188`) | 選択肢を `GET /api/competitor/options`(または既存 `/api/prefectures`、`geo_api`)から取る。全国選択時にタブ 4 の空状態を送信前に予告 |
| `market_title` | select。選択肢はサーバが `indeed_db` の `snapshot().titles` から作る(`competitor.rs:21-30`) | 値の検証なし(空以外はそのまま `by_title`/`by_pref` を引く)。見つからなければ「データがありません」(`:190-194`) | `indeed_db` が無いと選択肢が空で「使用しない」だけ。メッセージ「Indeed採用市場データは取得できません」(`:38-40`) | 選択肢は新設 `GET /api/competitor/options` が `{titles, prefectures, market_available}` を返す。React は `market_available=false` で select を無効化 |
| `survey_title` | text、maxlength 200 | フィールド値は一律 1,000 バイト超で 400(`:88-92`)。maxlength は見ていない | 200 文字超は(ブラウザ側を回避すれば)通る。レポートには `escape_html` 済みで出る | maxlength 200 を維持。サーバ側でも 200 文字で切る/弾くかは PR-2 で決める(§7) |
| `search_keyword` | text、maxlength 200 | 空かつ `market_title` 非空なら `「{title} 求人」`(`:140-144`)。カンマ・改行で複数語に割られる(`media_engine/handlers.rs` の `split_keywords`)【確認済: 呼び出し側のみ。split の実装本体は未読】 | 複数語 = Google への問い合わせ語数が増える | 入力欄の注記に「カンマ・改行で区切ると複数語を調べます」を追加。語数の上限は §7 で確認 |
| `include_google` | checkbox、既定 ON | `"1"` かつ keyword が空でない時だけ Google を呼ぶ(`:148-152`)。それ以外は `status:"not_requested"` | OFF のとき「検索需要を取得するには…」の文言がタブ 2 に出る | 維持。Google 資格情報が無い環境では ON でも `missing_credentials` が返る(`handlers.rs:133-139`)ので、タブ 2 の文言を状態別に出し分ける |
| `output_format` | 2 つの submit ボタン `pdf` / `html`(`name=output_format`) | `"pdf"` なら PDF、それ以外は HTML(`:164`) | 旧画面の JS は `pdf` を fetch に差し替え、`html` は通常のフォーム POST(ブラウザが POST 応答を文書として表示) | **形式の概念を分ける**: 「画面で確認」= `POST /api/competitor/report`(JSON)、「PDF」= `POST /api/competitor/pdf`。フォームの値は共通の `FormData` |

### 1.3 サーバの処理の流れ(`competitor.rs:63-182`)

1. multipart を 1 フィールドずつ読む。`csv_file` は全バイトをメモリに載せる(`:81-87`)。他のフィールドは `field.text()` で 1,000 バイト以内(`:88-92`)。読み取り失敗は固定文で 400(`:76-78,83-86,89-91`)。
2. 必須・列挙値の検証(`:95-119`)。
3. CSV 解析 + 集計を `spawn_blocking` で実行(`:121-125`)。`total_count > 0 && competitor.indeed_count > 0` でなければ「分析できるIndeed求人がありません」(`:127-130`)。解析 Err の文字列はそのまま画面に出る(`:128`)。
4. 人口・地域データ(`population_context`、`:134-135,201-221`)と Indeed 採用市場(`snapshot` → `indeed_context`、`:136-147,223-247`)を取る。
5. Google(`google_context`、`:148-152,249-283`): 検索需要と関連語の 2 本を `tokio::join!`、各 45 秒でタイムアウト(`:248`)。応答は `status/keyword/region/demand/suggestions`。失敗時の本文は資格情報を含みうるので、レンダラ側で固定文に置き換える(`competitor_report.rs` の `render_google` とそのテスト `google_error_does_not_render_raw_credentials…`)。
6. `render_competitor_report(&agg, top_n, survey_title, &indeed, &google, &population)` が HTML 文字列を作る(`:153-160`)。
7. PDF なら `pdf::generate`(§1.7)、そうでなければそのまま `Html` で返す(`:164-182`)。

**重要な構造上の事実**: レポート HTML の入力は `agg`(巨大な `SurveyAggregation`)と 3 つの `serde_json::Value`(indeed/google/population)。**Value の中身はハンドラが組んだものとほぼ同じ形で、そのまま「画面用 JSON」の素になる**。`SurveyAggregation` から実際に使うのは §1.4 の項目だけ。

### 1.4 4 タブの表示要素と数値の出どころ

タブの切り替えは `static/js/competitor-tabs.js`(WAI-ARIA tablist: クリック・←→・Home・End、非表示パネルは `hidden`)。タブ ID は `tab-excel / tab-google / tab-indeed / tab-population`、パネル ID は `panel-*`(PDF の収まり検査が `panel-excel, panel-google, panel-indeed, panel-population` の順序と数を前提にしている: `render.cjs:19-21`)。

| タブ | 表示要素 | 数値の出どころ【確認済】 | React 版の方針 |
|---|---|---|---|
| ① Excel再現 | 左: 調査名・雇用形態(最多)・該当都道府県・主な市町村・集計対象件数 / 給与表(総合・人気求人 × 下限・上限 × 平均・中央・最頻・集計件数)/ 差異表 / ワード表 2 つ(全体・上位 N、各上位 10 語、占有率)/ 注記。右: 棒グラフ 4 つ(上限・下限ボリュームゾーン、キーワード全体・上位 N 各 25 語) | `agg.competitor.{pop_all,pop_popular,salary_modes,tag_counts_all,indeed_count}`、`head_tag_counts(agg, top_n)`(`section_05b_competitor.rs`)、`agg.salary_{min,max}_values(_native)`、`agg.by_employment_type`、`dominant_prefecture/municipality`、`total_count`。**サーバで加工している点**: 月給は万円(÷10000、小数 2 桁)、時給は円(50 円刻みで階級化)、月給は 1 万円刻み。SP データが無い場合は `pop_all` が空で「給与分布から再計算」にフォールバック(`competitor_report.rs` の `use_fallback`)。最頻値は同数なら低い額 | **加工は Rust に残し、JSON で「表示する値そのもの」を返す**(React に再計算させない)。理由: フォールバックや最頻値の同数ルールを TS に移すと、旧との不一致の温床になる(2026-04-23 の契約事故と同種)。棒グラフはデータ `[{label, count}]` を返し、React が SVG で描く(§1.5) |
| ② Google | 注記(出典・指標の違い)/ 検索語・地域 / 検索需要表(語・平均月間検索数・広告競合度)/ 語ごとの月別検索数表(12 か月)/ 関連キーワード上位 20 / 状態別メッセージ | `google_context` の応答(`media_engine::handlers::{keywords_endpoint,suggest_endpoint}`)。`keywords[].{keyword,avg_monthly,competition,monthly_12m[]}`、`suggestions.suggestions[]`。取得地域は `demand.region.canonical_name`。地域を解決できないと全国になり、その旨の注記が出る | 状態(`not_requested / missing_credentials / error / timeout / ok`、子の demand と suggestions は別々に失敗しうる)を **タグ付き union の TS 型**にする。「取得できませんでした」の固定文は旧と同じにする(秘密情報を出さない)。語ごとの月別表は語数が多いと長い → 折りたたみ(DOM を全行出さない) |
| ③ Indeed | 見出し・出典・集計日・caveat / 月別表(月・求人数・求人を見た人数・募集企業数・1 求人あたりに見た人数)。欠測は「—」 | `indeed_context`(`competitor.rs:223-247`)。`snapshot(db)`(`crate::indeed::data`)。**全国 or 都道府県のどちらかを厳密一致で引く。都道府県が無い時に全国へ黙って落とさない**(単体テスト `indeed_region_match_preserves_missing_values…`)。`spp = seekers_per_posting()` | 欠測は `null` のまま返し、React が「—」にする(0 に丸めない)。`spp` は Rust が計算した値を使う |
| ④ 人口・地域 | 集計地域・出典 / 人口ピラミッド(SVG)/ 年齢別人口表(男・女・合計)/ 最低賃金(円/時)・改定年度・発効日・基準日・出典 / 労働統計(年度・完全失業率・離職率) | `population_context`(`competitor.rs:201-221`): `regional_analysis::fetch::{fetch_population_pyramid, fetch_wage_comparison, fetch_labor_stats}`。最低賃金は公式 CSV(`minimum_wage`)でローカル DB 無しでも出る(`competitor_tests.rs` の先頭テスト)。ピラミッドは `section_06_demographics::build_navy_pyramid_svg`(Rust で SVG 文字列) | 年齢帯は `age_group` の先頭の数字で並べ替えている(`competitor_report.rs`)。この並べ替えは Rust 側で済ませて返す。ピラミッドは React の SVG コンポーネントに作り直す(既存の `build_navy_pyramid_svg` の寸法を読んで同等に)。**都道府県が空のとき**は `status:"unavailable"` + 文言 |

### 1.5 グラフの方式

旧のグラフはすべて**サーバが SVG 文字列を組んだもの**(`competitor_report.rs` の `chart()`、人口ピラミッドは `build_navy_pyramid_svg`)。ECharts は使っていない。
→ React では **ECharts を使わず、同じ SVG を React の小コンポーネントで描く**(推奨)。理由: (a) 印刷時の収まり検査(`render.cjs`)が `svg` の矩形も検査しており、ECharts の canvas/resize 挙動を持ち込むと検査の前提が変わる、(b) 旧の SVG とレイアウト寸法(viewBox 900×200 / 440×450、ラベルの間引き規則)を 1:1 で移せば旧新比較が値だけでなく見た目でも取りやすい、(c) `components/EChart`(共通部品)の登録・resize の仕組みを使う必然性がない。共通部品の `DataTable`/`KpiCard`/`Note` は表と注記に使える。

### 1.6 画面状態・エラー表示・ログイン切れ

| 項目 | 旧画面の挙動【確認済】 | React 版の方針 |
|---|---|---|
| 送信中 | PDF: ボタン 2 つを `disabled`、`#submit-status`(`role=status`、`aria-live=polite`)に「PDFを作成しています。Googleのデータ取得には時間がかかる場合があります。」 / html: 通常フォーム送信(ブラウザの読み込み表示のみ。ボタンは無効化されない) | 両モードとも送信中は 2 ボタン無効 + 進捗文 + **キャンセルボタン**(`AbortController`)。アップロード進捗(`apiUpload` の `onProgress`)も表示 |
| 成功(PDF) | Blob を作り `<a download="competitor-report.pdf">` をクリック。先頭 `%PDF-` と末尾 `%%EOF` を確認(`competitor.html` の script)。「PDFをダウンロードしました…」 | 同じ検査を共通関数にして残す。サーバ側にも同じ検査がある(`competitor_pdf.rs:129-132`)ので二重だが、ネットワーク途中切断の検知としてブラウザ側も残す |
| 成功(画面確認) | POST 応答の HTML 文書にそのまま遷移(URL は `/report/competitor`)。上部に「調査条件に戻る / PDFダウンロード」リンク | 同じ画面内で入力フォーム ⇄ 結果の 2 状態。結果は画面上部に入力要約と「条件を変えて再作成」「PDFをダウンロード」を置く |
| エラー | サーバは `400` + 最小 HTML(`<p role="alert">…</p><a href="/competitor">入力画面へ戻る</a>`)(`competitor.rs:58-68`)。PDF 作成失敗は `503` + プレーン文字列(`:178`)。旧 PDF 経路の JS は content-type で HTML なら `[role=alert]` の文字を抜き出し、そうでなければ本文を表示 | 新 API は **JSON のエラー**(`{error: code, message}`)を返し、`ApiHttpError` の本文として表示。コード(`csv_missing / source_invalid / wage_invalid / pref_invalid / csv_parse / no_indeed_rows / field_too_long / pdf_busy / pdf_failed / pdf_timeout`)で React が出し分けられるようにする。`role="alert"` は維持 |
| ログイン切れ | `/report/*` は 401 JSON の対象外(`auth/mod.rs:34-83` は `/api/*` のみ)。fetch は 303→`/login` を追従するため、`response.redirected` で検知して「ログインの有効期限が切れました」(`competitor.html` の script)。html モード(通常のフォーム POST)はログイン画面に飛ぶだけで、入力した条件と CSV は消える | `/api/competitor/*` は 401 JSON になる(`AuthRequiredError`)ので、`redirected` 推測は不要になる。**401 のとき入力欄(CSV 以外)を保持し**、「再ログイン後にもう一度『作成』を押してください」と出す。再ログインは別タブ(`/login` を新規タブで開くリンク)で行わせ、画面の状態を失わない |
| `pageshow`(戻る・bfcache) | `pageshow` でボタンを有効化し status を消す(`competitor.html`) | 不要になる(SPA の状態は React が持つ)。ただし戻る/進むでブラウザが画面を復元しても CSV の `File` は復元できないため、結果状態は「再作成してください」に落とす |

### 1.7 PDF とダウンロード

| 項目 | 事実【確認済】 | React 版の方針 |
|---|---|---|
| 生成方式 | Rust が HTML を組み、`pdf::document()` が `<head>` に CSP(`default-src 'none'`…)、`LAYOUT` の `<style>`(A3 横、`.pdf-page` 1510×1045px 固定)、`FIT` の `<script>`(4 パネルを 1 ページずつ `<section class="pdf-page">` に包み、`zoom` で収める)を足す(`competitor_pdf.rs:60-66`) | **当面そのまま使う**(方式 B)。画面側が JSON + React になっても、PDF 用 HTML は既存の Rust レンダラで作る |
| 外部プロセス | 一時ディレクトリに HTML と `render.cjs` を書き、`node render.cjs <chromium> <file://…> <out.pdf> <playwright-core>` を起動(`:93-124`)。`PDF_CHROMIUM_PATH`、`PDF_NODE_PATH`、`PDF_PLAYWRIGHT_MODULE` で上書き可。既定は Windows が Edge、Linux が `chromium`(`:68-79`)。Docker は chromium と playwright-core を同梱(`Dockerfile:25,96,103-105`) | 変更しない |
| 同時実行 | `Semaphore::new(1)`(1 本ずつ)。取得待ちは 60 秒でタイムアウト(`:81-87`)。描画は 45 秒(`:125`) | 変更しない。ただし**待ち行列の存在を画面に出す**(「他の PDF を作成中です」。サーバは混雑を `pdf_busy` コードで返す) |
| 収まり検査 | `render.cjs` が 4 パネルの順序・`pdf-page` の枠内に文字矩形・svg・img が収まるかを検査し、収まらなければ `PDF page fitting failed` で失敗(`:19-46`) | **この検査が PDF の品質ゲート**。React 版でも維持(方式 B なら無改修) |
| 保存名 | `competitor-report.pdf`(固定、`competitor.rs:171`)。`Cache-Control: no-store` | 調査名・日付を入れたファイル名にするかは §7(固定名のままが既定) |
| 印刷 | 画面側の「印刷」は無い。注記で「印刷はダウンロードした PDF から」(`competitor.html` 末尾) | 同じ注記を維持 |

### 1.8 付随機能・その他

| 項目 | 事実 | 方針 |
|---|---|---|
| `{{MARKET_STATUS}}` 表示 | `indeed_db` から `snapshot` が取れれば「利用できます」、無ければ「取得できません。CSV の競合調査は利用できます」(`competitor.rs:38-43`)。`snapshot` は毎回 `spawn_blocking` で取る | `GET /api/competitor/options` の `market_available`(同じ判定) |
| 画面内ナビ | 「媒体分析 / 競合調査(現在) / 採用市場(`/?tab=%2Ftab%2Findeed`)」の 3 リンク(`competitor.html`)。ダーク固定の独自 CSS をページ内に持つ | React 画面は Shell(`AppShell`)に載せるか、sales-kpi と同様に Shell 非依存にするかを PR-4 で決める(Shell v1 は main にある: `frontend/src/shell/`)。スタイルはビルド型 Tailwind + 画面用 CSS に移す(旧のダーク固定色は `data-theme` の明暗に合わせて作り直すかを §7 で確認) |
| 結果画面 CSS/JS | `static/css/competitor-dashboard.css`(4,825 バイト)と `competitor-tabs.js`(984 バイト)を HTML に埋め込む(`include_str!`)。PDF 側はタブ JS を文字列置換で除去(`competitor_pdf.rs:64`) | React 画面では不要。**PDF 側の置換は、レポート HTML に埋め込むスクリプトの文字列が 1 バイトでも変わると効かなくなる**(§3 の P-14) |
| 監査ログ | `meaningful_activity`(`lib.rs:1000-1019` 付近)は `/tab/*` と一部 `/report/*` を記録。`/competitor`・`/report/competitor` を記録しているかは**読んでいない**【要確認】 | `/app/competitor` が `meaningful_activity` に入るか PR-6 で確認し、必要なら追加(react-full-migration-plan §4.1 の共通条件 5) |

---

## 2. レポート本体の扱い: 3 つの選択肢

### 方式 A: サーバ HTML をそのまま iframe(`srcdoc`)またはシャドウ DOM で差し込む

- 内容: React はフォームと送信だけを持つ。`POST /report/competitor`(html)の応答文字列を `iframe srcdoc` に入れる。PDF は今のまま。
- 長所: 変更が最小。旧新の表示が完全に同じ(同じ HTML だから)。PDF も変わらない。
- 短所:
  - 「React に移した」と言える中身が無い(画面の 9 割はサーバ HTML)。W9 でもう一度やり直す。
  - レポート HTML に `<script>`(タブ切り替え)と `<style>` が埋め込まれており、`srcdoc` の iframe は親の CSP を継承する(HTML 仕様)。W10 の CSP から `'unsafe-inline'` を外すと壊れる。`sandbox="allow-scripts"` で逃がすことはできるが、そうするとタブの a11y・フォーカス操作・Playwright の frame 越し操作が増える。【推測: srcdoc の CSP 継承は仕様上の理解で、この環境では実測していない】
  - レポート内の値を React が持てない(コピー・並べ替え・折りたたみ等を後から足せない)。
  - エラーは相変わらず HTML(`role=alert` の抜き出し)で返る。

### 方式 B(推奨・段階採用): 画面は JSON + React、PDF は当面サーバ HTML のまま

- 内容: `build_competitor_report(...) -> CompetitorReport`(serde + ts-rs)を作り、`POST /api/competitor/report` が JSON を返す。React が 4 タブを描く。PDF は従来どおり `render_competitor_report` → `pdf::generate` を通す(`POST /api/competitor/pdf`)。**HTML レンダラも新しい構造体 `CompetitorReport` を入力にする**ので、画面と PDF は同じ構造体から出る。
- 長所:
  - 画面は本当に React になり、W9 の準備にもなる。
  - 壊れやすい PDF(A3 横 4 ページ固定、収まり検査)に一切触れない。
  - 値の出どころが 1 つ(`CompetitorReport`)に集まるので、旧新の一致を「構造体 → HTML」と「構造体 → React」の 2 本で守れる。
- 短所:
  - 表示の実装が 2 つ(Rust の HTML と React)になる。**並走中は避けられない二重保守**。W9 までの期間限定で、HTML レンダラは凍結(機能追加しない)する。
  - PDF ボタンで再びフォームを送るので、Google 広告 API を再取得する(§3 P-02)。キャッシュ(§7)で解消できる。

### 方式 C(終点): 画面も PDF も React の同一コンポーネント、PDF は Chromium が `/app/print/competitor` を印刷

- 内容: PDF 用に、サーバが組んだ HTML ではなく、React の印刷用ルートを Chromium が開いて印刷する。React の 4 タブ表示と印刷用 4 ページが同じ部品を使う。
- 長所: レンダラが 1 つになる。W9(レポートの React 化、`react-full-migration-plan.md` §6-1)が目指す形そのもの。CSP も React 側で整う。
- 短所(今やらない理由):
  - 認証: Chromium は別プロセスなので、ログイン済み session を持たない。**使い捨てトークン付きの内部ルート**か、サーバが構造体を HTML に埋めて渡す方法のどちらかが要り、いずれも新しい攻撃面になる(設計・レビューが要る)。【推測】
  - Chromium が配信中の `static/app/*` を読む必要があり、今の「`file://` + CSP `default-src 'none'` で外部通信を遮断」(`competitor_pdf.rs:62`)をやめる。
  - A3 横 4 ページの収まり(`fitPages` の `zoom` 補正、`render.cjs` の検査)を React 側で作り直す必要がある。PR #53 で直したばかりの品質を再び動かす。
  - 実測が必要: Docker のメモリ(Chromium + Node)と `node_modules/playwright-core` の同梱は既にあるが、静的配信を読ませる構成は未検証【要確認】。

### 推奨

**B を実装し、C は W9 で行う**。B の HTML レンダラは「PDF 専用の凍結物」と明示し(コメント・テスト名)、W9 で React の印刷用ルートと Playwright の撮り比べ(`page.pdf` のページ数・改ページ・収まり検査)を満たしてから削除する。
A は採らない(上記の短所と、移行計画の趣旨に合わない)。

---

## 3. 段階 2: 逆証明(壊れる入力・状態)

凡例: 旧=旧画面で起きるか(根拠)。React=React 版で防ぐ方法。「起きる」は読んだコードからの確認、「起きうる」は実行していない推論。

| ID | 壊れる入力・状態 | 旧画面で起きるか | React 版で防ぐ方法 |
|---|---|---|---|
| P-01 | **空 CSV(0 バイト)** | 起きる(扱いは正しい): `competitor.rs:95-97` で 400「求人一覧CSVを選択してください」。ブラウザ側は 0 バイトを `required` で通す(旧 E2E 2 本目が実証) | ファイル選択時に 0 バイトを弾く。サーバ側の同じ検査は維持。テスト: 0 バイトを選ぶと送信ボタンが押せず、サーバにも送らない/送ってもエラー表示が残る |
| P-02 | **二重送信**(連打・PDF と画面確認の同時押し) | 起きる。PDF は JS がボタンを無効化するが、html モードは無効化されない(通常フォーム送信)ため連打で POST が重なる。サーバに重複抑止が無い(`competitor.rs:63-` に idempotency なし)。1 送信ごとに Google を 2 本呼ぶ(`:240-256`)。PDF は直列化されるが(`competitor_pdf.rs:81-87`)、集計・Google は並行に走る | 送信中はボタン無効 + 進行中の `AbortController` を 1 つだけ持ち、2 回目は無視。送信の瞬間に `submitting` を state ではなく ref で見る(連打で state 更新が間に合わない競合を防ぐ)。サーバ側の抑止(同一ユーザーの同時 1 件)を PR-2 で入れるかは §7 |
| P-03 | **巨大 CSV(20MB 上限付近)** | 上限は 20MB(`lib.rs:51`、`DefaultBodyLimit`)。超えた場合の応答は multipart の `next_field`/`bytes` のエラー → 400「CSVを読み込めませんでした。ファイルサイズと形式を確認してください」(`competitor.rs:76-78,83-86`)になるはず【推測: 413 でなく 400 になる。実測していない】。20MB 以内でも全バイトをメモリに載せ(`:81-87`)、デコードで複製し(`upload.rs:197-` の `to_vec`)、解析する。**行数の上限が無い**【確認済: 上限の定数を grep で見つけられなかった。`take(…)` はサンプル用のみ】。`spawn_blocking` の CPU 時間にタイムアウトが無い | ブラウザ側で 20MB 超を送信前に弾き、行数の目安(推定)を出す。サーバ側のタイムアウト/行数上限は §7(上限値はユーザー判断)。テスト: 約 15MB の CSV で応答時間とメモリを実測して記録(受け入れ基準は PR-2 で決める) |
| P-04 | **文字コード違い**(UTF-8 BOM / UTF-16 / Shift-JIS / BOM なし UTF-16) | 概ね対処済み: `decode_csv_bytes`(`upload.rs:197-`)が BOM・UTF-16・NUL 密度・Shift-JIS を判定する。誤判定の過去事例がコメントにある(`:216-221`)。画面は「UTF-8推奨」と書くだけで、**判定結果は利用者に見えない** | 応答の `meta.encoding` に判定結果を返して画面に出す(「UTF-8 として読みました」)。テスト: 同じ CSV を UTF-8 / BOM 付き / Shift-JIS / UTF-16LE で作り、集計結果が一致すること(旧新どちらでも) |
| P-05 | **列欠落・列名違い**(タイトル/会社名/勤務地/給与/雇用形態のどれかが無い、PC と SP の取り違え) | 解析は列名の自動検出と「データからの動的検出」(`upload.rs:307-330`)。Indeed の PC/SP の取り違えはサーバが補正する(`:319-330`)。**必須列が足りない時のメッセージの形は読んでいない**【要確認】。`indeed_count == 0` なら一律「分析できるIndeed求人がありません。CSVの列と内容を確認してください」(`competitor.rs:127-130`) | 失敗の種類(列が無い/行が無い/Indeed 求人が 0)を `error` コードで分ける。応答の `meta.detected_columns`(どの列を何として読んだか)を返し、画面に出す。テスト: 給与列だけ欠けた CSV、雇用形態列だけ欠けた CSV、PC/SP 取り違え CSV |
| P-06 | **CSV 解析エラー文が利用者に直接出る** | 起きる: `Ok(Err(message)) => return error(&message)`(`competitor.rs:128`)。メッセージは `ヘッダー読み取りエラー: {e}`(`upload.rs:314`)など内部ライブラリのエラー文を含む | 新 API はエラー文を固定の日本語にして、詳細は `tracing::warn!` にだけ出す(Render Logs)。テスト: 壊れた CSV(引用符が閉じない等)で、応答本文に Rust の内部語(`UnequalLengths` 等)が含まれない |
| P-07 | **Google 広告 API の失敗・遅延** | 資格情報なしは `missing_credentials`(`handlers.rs:133-139`)で、レンダラは「取得できませんでした」(固定文)。ネットワーク失敗・API エラーは `status:"error"` + 生メッセージで、**レンダラ側が本文を出さない**ことで秘密を守っている(テスト `google_error_does_not_render_raw_credentials…`)。タイムアウトは 45 秒、検索需要と関連語は別々に失敗しうる(`:248-256`) | 応答 JSON から**生のエラー本文を除去**(`message` は固定文にする)。これを契約テストで固定(`secret-token` が JSON のどこにも無い)。UI は demand と suggestions を別々に状態表示 |
| P-08 | **Google の応答が遅く PDF 全体が長引く** | 起きうる: PDF の最悪待ち = Google 45 秒 + セマフォ取得待ち 60 秒(`competitor_pdf.rs:83-86`)+ 描画 45 秒(`:125`)≒ 150 秒。React の既定アップロードタイムアウトは 120 秒(`client.ts:21`)。旧画面の `fetch` にはタイムアウトが無い。Render 側のリクエスト上限は**未確認**【要確認】 | 画面確認(JSON)と PDF で別のタイムアウト(JSON 90 秒、PDF 180 秒を仮置き)。UI に経過時間と「Google の応答待ち」の状態を出す。サーバ側は Google 45→30 秒に短縮するか §7 で相談。**根本策**: キャッシュ(§7)で PDF 時の Google 再取得をなくす |
| P-09 | **ログイン切れ**(作業中にセッションが切れる。MemoryStore は再デプロイで全員消える: `react-full-migration-plan.md` §7.2) | 起きる。PDF 経路は `redirected` 検知で文言を出す。html モードは `/login` に飛んで**入力と CSV が消える**(`competitor.html`) | §1.6 のとおり 401 JSON を `AuthRequiredError` として扱い、フォームを残す。テスト: Playwright で session を無効化して送信し、入力が残ること・別タブで再ログイン後に再送できること |
| P-10 | **PDF 生成失敗**(Chromium 無し・起動失敗・収まり検査失敗・タイムアウト) | 起きうる: 失敗は `503` + 文字列(`competitor.rs:178`)。`PDF page fitting failed` は `render.cjs:44` の例外 → 終了コード 1 → 「PDFを作成できませんでした。再度お試しください。」(`competitor_pdf.rs:134-` 付近)。診断は Render Logs に最大 2,000 文字(`:151-` 付近) | 失敗コードを `pdf_busy / pdf_timeout / pdf_failed` に分け、画面確認(JSON)の結果は PDF 失敗後も残す(画面は消さない)。テスト: `PDF_CHROMIUM_PATH` に存在しないパスを指定した状態で、JSON 画面は表示でき PDF だけエラーになること |
| P-11 | **多数のキーワード・長い語でレイアウトが崩れる** | 起きうる: PDF は `zoom` で収める(`competitor_pdf.rs` の `fitPages`)が、語が多すぎると極小になる/収まり検査で失敗する。画面側は SVG 内ラベルを間引く(`chart()`、`data.len() < 35 || i % …`) | React でも同じ間引き規則を移す。PDF 側は現状維持(方式 B)。上位 N の語数は最大 25 のまま |
| P-12 | **タブ切り替え中の再取得・戻る/リロード** | 旧(html): タブ切り替えはクライアントのみで再取得は無い。ただしリロードは POST 再送(CSV と Google を再取得)。戻るで入力に戻ると CSV の `<input type=file>` は空に戻る | 結果を state に保持(再取得しない)。リロードすると結果は消える(CSV を URL に載せられない)ので、画面に明記し、入力条件(CSV 以外)は `sessionStorage` に保存して復元(失敗しても動く try/catch)。タブの選択は URL クエリ `?tab=google` に入れて共有・戻るに耐える |
| P-13 | **入力値の XSS/インジェクション**(調査名に `<script>`、`'`、`"`) | 防いでいる: すべて `escape_html`(`competitor_report.rs`)。テスト `standalone_report_has_…` が `<script>危険</script>` を検査 | React は既定でエスケープ。`dangerouslySetInnerHTML` を使わない(ESLint で禁止ルールを足すか、テストで grep)。PDF 用 HTML は引き続き `escape_html` |
| P-14 | **PDF 用 HTML の文字列置換が壊れる** | 起きうる: `document()` が `html.replace("<head>", …)` と `html.replace(include_str!("…/competitor-tabs.js"), "")` で組み立てる(`competitor_pdf.rs:60-66`)。レポートの HTML に埋め込むスクリプトの内容が置換対象と 1 文字でも違うと、タブ JS が残って PDF 側の `FIT` と干渉する【推測】 | PR-1 で「PDF 用 HTML に `competitor-tabs.js` が残っていない」ことを golden テストで固定してから触る。新しい `render_html(&CompetitorReport)` でも同じ `include_str!` を共有 |
| P-15 | **雇用形態・月給/時給の取り違え** | 起きる(利用者の入力ミス): `wage_mode` の選択が CSV と合わないと、単位が無意味な数字になる。検知は無い | 結果の見出しに `monthly/hourly` を必ず表示し、「CSV の給与が時給なら時給を選んでください」の注記を送信前にも出す。**自動判定は入れない**(MVP 外)。応答の `meta` に `salary_parsed_count / salary_missing_count` を含め、ほぼ 0 なら画面で警告(給与列が読めていない可能性) |
| P-16 | **Indeed 採用市場データ無し/地域不一致** | 扱いは正しい: `indeed_db` 無し→「取得できません」、地域が無い→全国へ**落とさず**「選択した職種・地域のデータがありません」(`competitor.rs:190-194`、テスト有) | JSON の `indeed.status` を union にして、React は「データなし」と「0」を区別する(`—`) |
| P-17 | **人口・地域データ無し(DB 未接続)** | 最低賃金は公式 CSV で出るが、年齢別人口・労働統計は空(`competitor_tests.rs` 先頭テスト)。都道府県未選択なら「…選択してください」(`:187-188`) | `bands: []` と `labor: null` をそのまま返し、React は「データなし」と最低賃金だけ表示 |
| P-18 | **最低賃金の出典・基準日が古い** | 基準日は「日本時間の今日」(`minimum_wage_as_of`)と発効日を並べて出す(旧 E2E が 2026-10-01 を検査) | そのまま移す。日付は文字列で返し、React は整形しない(TZ ずれを避ける) |
| P-19 | **ブラウザのダウンロードがブロックされる/Blob URL の寿命** | 旧は 60 秒後に `revokeObjectURL`(`competitor.html`) | 同じ。`download` 属性が効かない環境向けに、失敗時は「保存」ボタン(Blob URL)を残す |
| P-20 | **同時に複数人が PDF を作る** | セマフォ 1。後続は最大 60 秒待って `pdf_busy`(`competitor_pdf.rs:83-86`)。Render のメモリ(Chromium)に対する保護 | 現状維持。UI に待機状態を出す。PDF 同時実行数を増やすかは Render のメモリ実測が要る【要確認】(§7) |
| P-21 | **`top_n` の黙った丸め** | 起きる(`section_05b_competitor.rs:35-40`)。画面は丸めたことを知らせない | 応答に `meta.top_n_effective` を含め、入力値と違えば注記 |
| P-22 | **キーワードにカンマ/改行を入れる(Google 問い合わせ数の増加)** | 起きうる: `split_keywords`(`media_engine/handlers.rs`)で複数語に割られ、語数だけ API 単位を使う(Keyword Planner の課金・クォータ上の扱いは**未確認**【要確認】) | 入力欄の注記と、語数の上限(例: 5 語)を送信前に検証。上限値は §7 |

### 3.1 逆証明の限界(読んでいないもの)

- `split_keywords` の本体、`fetch_*`(人口・賃金・労働統計)の失敗時の挙動、`snapshot()` の所要時間は読んでいない。
- 実行していないので、20MB 超の応答(413 か 400 か)、15MB 級 CSV の処理時間とメモリ、PDF の最悪待ち時間は実測値がない。PR-2 のテストで実測して本書を更新する。
- `/competitor`・`/report/competitor` が `meaningful_activity` に入っているかは未確認(§1.8)。

---

## 4. 新しい API と型(PR-2 で確定する案)

`src/handlers/competitor/` に分割(今の `competitor.rs` + `competitor_pdf.rs` + `competitor_tests.rs` はそのまま残し、追加する)。

```text
GET  /api/competitor/options      -> CompetitorOptions { titles: string[], prefectures: string[], market_available: bool }
POST /api/competitor/report       -> multipart (旧と同じ name) -> CompetitorReport (JSON) | CompetitorError
POST /api/competitor/pdf          -> multipart (同上)          -> application/pdf | CompetitorError(JSON)
```

`CompetitorReport`(`#[derive(Serialize, TS)]`、`frontend/src/generated/` に書き出し。生成差分ゼロを CI が検査: `app_api.rs:92-97`):

- `meta`: `title, employment_type, prefecture, municipality, unit("万円"|"円/時"), is_hourly, total_count, top_n_effective, top_n_requested, encoding, salary_parsed_count, salary_missing_count, warnings[]`
- `excel`: `salary_table`(行ラベル × 4 値(総合下限・総合上限・人気下限・人気上限)+ 集計件数 4)、`salary_diff`、`keyword_all[]`、`keyword_head[]`(語・件数・求人数・占有率)、`histograms.upper[]`、`histograms.lower[]`、`charts.keyword_all[25]`、`charts.keyword_head[25]`、`notes[]`
- `google`: タグ付き union(`not_requested | missing_credentials | error | timeout | ok{keyword, region, demand{status,…}, suggestions{status,…}}`)
- `indeed`: `unavailable{message} | ok{title, region, source, caveat, built_at, rows[{month, job|null, ctk|null, emp|null, spp|null}]}`
- `population`: `unavailable{message} | ok{region, bands[{age_group, male, female}](並べ替え済)、minimum_wage*, labor|null}`

数値の加工(万円換算・小数 2 桁・時給 50 円刻み・フォールバック・最頻値の同数ルール・年齢帯の並べ替え)は Rust に残し、**表示用の値そのもの**を返す(§1.4)。数値フォーマット(桁区切り・小数桁)は React が `Intl` で行うが、小数桁数は Rust が `decimals` として返すかを PR-2 で決める(旧は `format!("{:.2}")` 固定)。

`CompetitorError { error: code, message }`(固定日本語)。HTTP ステータス: 400(入力)、413(サイズ上限。axum が返す場合を実測)、422(CSV の中身が分析不能)、503(PDF)。

---

## 5. 実装計画(PR の分け方)

共通の約束: 1 PR 1 目的。`cargo` は必ず `bash C:/dev/cargo-slot.sh cargo ...`。`git add` はファイル名指定。React 画面は PR-4 で追加するが **PR-6 まで `KNOWN_SCREENS` 以外では到達できる状態でも、ナビには出さない**(`hidden` 相当にせずナビに未登録のまま)。

| PR | 内容 | 先に落ちるテスト(最初に書き、赤を確認してから実装) | 変更範囲 |
|---|---|---|---|
| **PR-1** 旧挙動の固定(テストのみ) | 現行 `main` の出力を golden にする。`render_competitor_report` に対し、fixture CSV(SP・人気タグ付き・約 60 行)× {月給, 時給} × {Google ok / missing_credentials / error / timeout / not_requested} × {Indeed ok / unavailable} × {人口 ok / unavailable} の代表 6〜8 組合せの HTML を `tests/fixtures/competitor/golden/*.html` に保存。PDF 用 `document()` の出力(タブ JS が除去されていること、LAYOUT・FIT が入っていること)も別 golden。fixture CSV を `tests/fixtures/competitor/` に追加(UTF-8 / BOM / Shift-JIS / UTF-16LE の 4 通り + 列欠落 + 巨大でない 1 万行) | **何も変えずに緑**であることを確認(golden 採取の正しさの確認)。次に `render_competitor_report` を 1 行いじって赤になることを手で確かめ、戻す | `tests/` と `src/handlers/competitor_tests.rs` のみ。本番コード無変更 |
| **PR-2** `CompetitorReport` を作り、HTML をその構造体から出す(挙動不変リファクタ + JSON API) | (a) `build_competitor_report(...) -> CompetitorReport`、(b) `render_html(&CompetitorReport)`(PR-1 の golden と**バイト一致**)、(c) `GET /api/competitor/options`・`POST /api/competitor/report`(JSON)、(d) エラーを `CompetitorError` に、(e) ts-rs 出力を `export_ts_bindings` に追加 | ① PR-1 の golden が緑のまま(割る前後で HTML 一致 = `react-full-migration-plan.md` §2.8 手順 3)。② 契約テスト(tempfile DB、具体値): fixture CSV で `salary_table` の中央値・最頻値・件数が `Ok` の具体値と一致、`keyword_all[0]` が既知の語と件数、`histograms` の階級境界、`population.bands` が年齢順。③ `secret-token` が JSON のどこにも出ない。④ 空 CSV・巨大 CSV・列欠落・壊れた CSV の各エラーコードと「内部語を含まない」こと。⑤ 未ログイン POST が 401 JSON、`X-Requested-With` 無しの POST が 403、外部 Origin が 403。⑥ ts-rs 生成差分ゼロ | `src/handlers/competitor*.rs`、`src/lib.rs`(ルート追加のみ)、`app_api.rs`(export 追記)、`frontend/src/generated/*` |
| **PR-3** `POST /api/competitor/pdf` | `output_format` の分岐を API に分離。`pdf::generate` は無改修。エラーコード(`pdf_busy/timeout/failed`)を JSON で返す | ① PDF 応答の先頭 `%PDF-` / 末尾 `%%EOF` / `content-disposition`(既存の `#[ignore]` テストを CI の Chromium ありジョブで走らせる設定があるか確認【要確認】)。② Chromium パスを壊した状態で JSON の `pdf_failed` が返り、**同じ入力の `/api/competitor/report` は成功する**。③ 同時 2 本で後続が `pdf_busy` か待って成功のどちらかに決まる | `competitor.rs`、`lib.rs` |
| **PR-4** React 画面(フォーム + 4 タブ + エラー/状態) | `frontend/src/screens/competitor/`: `CompetitorScreen.tsx`、`form.ts`(検証)、`api.ts`(`apiUpload`/`apiGet` ラッパ)、`tabs/{Excel,Google,Indeed,Population}.tsx`、`charts/{Histogram,KeywordBars,Pyramid}.tsx`(SVG、旧の寸法規則を移植)、`competitor.css`。`entries/competitor.tsx`、`vite.config.ts` の input、`KNOWN_SCREENS` に追加。**ナビは触らない** | Vitest: ① フォーム検証(0 バイト/20MB 超/拡張子/`top_n` 範囲/語数上限)。② 各 union 状態の描画(Google 5 状態 × 子 2 本、Indeed 2、人口 2)で、欠測が `—` で 0 でないこと、`null` と 0 を区別。③ 二重送信抑止(連打で `apiUpload` が 1 回だけ呼ばれる)。④ 401 で入力が残る。⑤ タブの a11y(←→/Home/End、`aria-selected`、非表示パネルの `hidden`)を旧 `competitor-tabs.js` と同じ操作で。⑥ ヒストグラムの棒の高さ・ラベル間引きが、PR-2 の JSON の値から旧と同じ規則で決まる(数値で assert) | `frontend/**` のみ + `spa_shell.rs`(1 行) |
| **PR-5** 旧新一致 E2E(PR ごとの E2E に追加) | `tests/e2e/pr/competitor_old_vs_app.spec.ts`(§6)。既存 `competitor.spec.ts`(旧画面 2 本)は並走中は維持 | §6 | `tests/e2e/pr/` |
| **PR-6** 切替(ナビ + 302 準備) | `nav.rs` の `competitor` を `NavKind::App`/`/app/competitor` に。`nav.rs` のテスト期待値更新。旧 `/competitor` は残す(302 は 1 リリース後の別 PR-7 で)。`meaningful_activity` に `/app/competitor` を追加 | `nav.rs` のテスト(`/api/nav` の competitor の `href` が `/app/competitor`、旧シェルの `<a>` も同じ)。`app_routes_no_conflict.rs`(ルート衝突なし) | `nav.rs`、`lib.rs` |
| **PR-7**(1 リリース後) | 旧 `/competitor` を 302 で `/app/competitor` に。さらに次リリースで旧テンプレート `competitor.html`・旧 `page()`・旧 `/report/competitor` と、旧 E2E を削除(**HTML レンダラ `render_competitor_report` と PDF は W9 まで残す**) | 302 の E2E(旧 URL → `/app/competitor`、フォームが出ること)。削除 PR は `git diff --cached --stat` で削除ファイルを確認 | — |

W9(別レーン/後続): 方式 C への移行(`/app/print/competitor`)。そのとき PR-1 の golden 群は「React 印刷画面 vs 旧 HTML の値一致」に作り直す。

### 5.1 工数の目安【推測】

PR-1: 1〜1.5 人日 / PR-2: 3〜5 / PR-3: 1〜1.5 / PR-4: 4〜7(SVG 3 種の移植が中心)/ PR-5: 1.5〜3 / PR-6: 0.5〜1 / PR-7: 0.5。合計 **約 12〜19 人日**。幅の主因は、PR-2 の「HTML を構造体から出す」リファクタで golden がバイト一致するまでの反復回数と、PR-4 の旧 CSS(ダーク固定)の扱い(§7)。

---

## 6. 旧新一致 E2E の方針

### 6.1 fixture

- **CSV**: `tests/fixtures/competitor/` に、実データでなく**合成**した SP 形式 CSV(約 60 行、人気・超人気タグ付き、雇用形態は正社員中心 + パート少数、都道府県は大阪府、給与は月給と時給の 2 本)。期待値は fixture 作成時に手計算(または独立した Python スクリプト `scripts/e2e/` で計算)して `tests/e2e/pr/helpers/fixture_values.ts` に書く(**Rust の出力をコピーして期待値にしない**。Rust の実装を検証する意味がなくなるため)。
- **文字コード版**: 同じ内容を UTF-8 / BOM 付き / Shift-JIS / UTF-16LE にした 4 ファイルで、集計結果が一致すること。
- **Indeed 採用市場**: `indeed_db` は E2E の globalSetup では起動していない可能性が高い【要確認: `global-setup.ts` の env に `INDEED_DB_PATH` 相当があるか】。無ければ `scripts/e2e/make_fixture_db.py` に、同じスキーマの小さい Indeed DB(2 職種 × 2 地域 × 6 か月)を作る関数を足し、`config.indeed_db_path` を環境変数で指す。
- **人口・地域**: 最低賃金は公式 CSV(`data/minimum_wage_rates.csv`)で DB 不要(既存 E2E が `1,231`・`2026-10-01` で確認済み)。年齢別人口は fixture DB の `v2_external_population_pyramid` 相当を `make_fixture_db.py` に足す(Turso は E2E で使わず、`TURSO_*` はテスト環境から除去済み: `global-setup.ts`)。
- **Google(外部 API)**: `GOOGLE_ADS_API_ROOT` は定数(`google_ads.rs:507`)で、環境変数で差し替えられない【確認済】。したがって E2E では**実 API を呼ばず**(`global-setup.ts` が `GOOGLE_*` を除去するので資格情報は常に空)、
  - サーバ経由では `missing_credentials` の経路だけを一致確認する。
  - Google `ok`・`error`・`timeout` の経路は、(a) Rust 側: `build_competitor_report` に `serde_json::Value` を直接渡す契約テスト、(b) React 側: `page.route('/api/competitor/report', …)` で JSON を差し替える Playwright テスト、で確かめる。
  - 外部 API 用のスタブサーバを Rust に足す案(`GOOGLE_ADS_API_ROOT` を env 化)は、本番コードに外部向け設定を増やすので採らない(§7 で確認)。

### 6.2 比較の仕方(値で判定。要素の存在では判定しない)

同じ入力で、旧画面(`/competitor` → 「画面で確認」→ `/report/competitor` の HTML)と新画面(`/app/competitor`)を開き、**同じ指標を同じ型で取り出して比べる**:

| 取り出す値 | 旧(DOM) | 新(DOM) |
|---|---|---|
| 給与表の 12 セル + 集計件数 4 セル | `#panel-excel .summary table` のセル文字 | `data-testid="salary-table"` のセル |
| ワード表(全体・上位 N)の語・件数・占有率(各 10 行) | `table.words` | `data-testid="keywords-all" / "keywords-head"` |
| ヒストグラム 2 本の階級ラベルと件数 | `svg rect` の `<title>`(「語: n件」)と軸ラベル `text` | 同じ構造の SVG を描くので同じセレクタ。さらに `data-count` 属性を足す |
| キーワード棒 2 本 | 同上 | 同上 |
| Indeed 月別表(5 列 × 行数) | `#panel-indeed table.table-navy` | `data-testid="indeed-table"` |
| 人口: 年齢別 3 列 × 行、最低賃金・年度・発効日・基準日・出典、失業率・離職率 | `#panel-population table.table-navy` | `data-testid="population-*"` |
| Google の状態文言(missing_credentials) | `#panel-google .note` | `data-testid="google-status"` |

3 点比較: **旧 == 新 == fixture の既知値**(`legacy_vs_app.example.spec.ts` の方針)。さらに「わざと値を変えて落ちる」ことを確かめる(例: fixture の 1 行を変える、描画側を 1 つ誤らせる)。

### 6.3 追加の E2E(旧新比較ではなく新画面単独)

- 画面確認の結果が出てからタブを 4 つ巡回して、`aria-selected` と表示パネルが 1 つだけであること。
- 送信中に連打しても POST が 1 回(`page.route` で数える)。
- 401(session を落とす)で入力が残る。
- PDF: `page.waitForEvent('download')`、`%PDF-` / `%%EOF`、ファイル名、サイズ > 10,000 バイト(既存の旧テストと同じ基準)。**PDF の中身は `pdftotext` 等でテキストを抜き、4 ページ・各ページの見出し・給与表の数値が含まれることを確かめる**(ページ数 = 4 は固定)。
- ブラウザの戻る/リロードで結果が消え、「再作成してください」の表示になること。

---

## 7. ユーザー判断が要る点

1. **PDF の方式**: B(画面は React、PDF は当面サーバ HTML)で進め、C(React 印刷画面)は W9 に回す方針でよいか。
2. **生成結果の一時キャッシュ**: 「画面確認」の結果を `report_id` 付きでサーバのメモリに 30 分だけ保持し、PDF ボタンは `report_id` で PDF を作る案。利点は Google 広告 API の再取得が無くなり、PDF 作成が速く・安定すること。欠点は新しい状態(ユーザーに紐づくキャッシュ、再デプロイで消える、他ユーザーが ID を推測できないようにする設計)を持つこと。採らない場合は、PDF ボタンで CSV と Google を再送・再取得する(今の旧画面と同じ挙動、費用・時間が倍)。
3. **CSV の上限**: 20MB(既存)に加え、行数・処理時間の上限を設けるか。設ける場合の値(例: 5 万行、解析 30 秒)。
4. **Google の語数上限とタイムアウト**: 検索語数の上限(例: 5 語)、Google 取得のタイムアウトを 45 秒から短縮するか。Keyword Planner の課金・クォータの扱いは未確認。
5. **サーバ側の同時送信制限**: 同一ユーザーが同時に複数の `report`/`pdf` を投げた場合に、2 本目を拒否するか(今は制限なし、PDF だけ全体で 1 本ずつ)。
6. **見た目**: 旧画面はダーク固定の独自 CSS。React 版を他の React 画面と同じ明暗テーマ(Shell 準拠)にそろえるか、旧の見た目を踏襲するか。結果の 4 タブ(レポート)の配色は PDF(白地)と画面を一致させるか。
7. **ファイル名**: PDF の保存名を `competitor-report.pdf` 固定のままにするか、調査名・日付入り(例: `競合調査_大阪府_施設長_2026-10-04.pdf`)にするか(日本語ファイル名の `content-disposition` は RFC 5987 形式が要る)。

---

## 8. 完了条件(レーン A)

`react-full-migration-plan.md` §4.1 の共通条件 + 競合調査固有:

1. 旧新一致 E2E(§6.2)が PR ごとの E2E で緑(旧 == 新 == 既知値)。わざと値を変えて赤になることを 1 回確認。
2. 新 JSON API に契約テスト、ts-rs の生成差分ゼロ、`secret-token` 等の漏洩なしテスト。
3. 逆証明の P-01〜P-22 のうち、React 版で防ぐと書いたものすべてに自動テストがあり、実装しないと決めたものは §7 の回答として記録されている。
4. PDF: 旧と同じ fixture で 4 ページ、収まり検査(`render.cjs`)が通る。**PDF の見た目(ページ数・各ページの見出し・給与表の値)が旧と一致**。
5. ナビが `/app/competitor`(旧シェル・React シェル両方)、`meaningful_activity` に `/app/competitor` が記録される。
6. 並走 1 リリースの後に旧 `/competitor` を 302(PR-7)。旧 HTML レンダラと PDF 生成は W9 まで残す(完了ではなく「W9 待ち」と報告する)。

完了報告の区分(CLAUDE.md の 3 段階): PR-2 完了時点は「基盤完了(UI 未適用)」、PR-4 完了時点は「機能完了(旧新比較の E2E は PR-5)」、PR-5 緑 + PR-6 でナビ切替後に「品質完了」とする。

---

## 付録: 読んだファイル(行は 3290da7)

`src/handlers/competitor.rs`(全体)、`competitor_pdf.rs`(全体)、`competitor_tests.rs`(前半)、`templates/competitor.html`、`scripts/pdf/render.cjs`、`static/js/competitor-tabs.js`、`src/handlers/survey/report_html/navy_report/competitor_report.rs`(全体)、`section_05b_competitor.rs:30-45`、`survey/upload.rs:197-330`、`src/media_engine/handlers.rs:122-267`、`src/auth/mod.rs:34-90`、`src/handlers/nav.rs`(competitor 関連)、`spa_shell.rs:1-80`、`lib.rs:380-420,1000-1090`、`frontend/src/api/client.ts`、`frontend/src/screens/sales-kpi/SalesKpiScreen.tsx`、`frontend/src/entries/sales-kpi.tsx`、`frontend/vite.config.ts`、`src/handlers/app_api.rs:80-140`、`tests/e2e/pr/{competitor.spec.ts,legacy_vs_app.example.spec.ts,playwright.pr.config.ts,global-setup.ts}`、`.github/workflows/e2e-pr.yml`、`Dockerfile`(PDF 関連)、`docs/architecture/{frontend-react-migration,architecture-decisions,react-full-migration-plan}.md`。
読んでいない主なもの: `split_keywords` 本体、`fetch_population_pyramid` 等の失敗時挙動、`aggregate_records_with_mode` の内部、`build_navy_pyramid_svg` の寸法、`meaningful_activity` の対象パスの具体。
