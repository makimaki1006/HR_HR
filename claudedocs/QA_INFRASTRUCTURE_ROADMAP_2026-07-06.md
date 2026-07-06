# QA 基盤ロードマップ（求人分析レポート／Rust axum）

**作成日**: 2026-07-06
**対象リポジトリ**: `makimaki1006/HR_HR`（V2 ハローワークダッシュボード、Rust/axum サーバサイドレンダリング）
**対象成果物**: 画面 HTML + ブラウザ印刷/PDF（A4）の BtoB 求人分析レポート（Section 01-09 + 07.5/07.6）
**開発体制**: 個人（ユーザー）+ AI agent。CI は GitHub Actions（public リポ=無料）。E2E は Playwright（python/TS 混在）+ nightly regression
**根拠**: WF7（外部調査 Sonnet×5 → MECE 統合 Opus → Fable 批判レビュー）。本文中の出典 URL はすべて WF7 調査由来で削らない
**位置づけ**: 本ロードマップは「もっとテストを書く」ではなく「防御線を層で分ける」再設計。Phase 1（本 WF8 で実装）/ Phase 2 / Phase 3 に分割し、各機構にサンセット基準を付す

---

## 1. 背景：「ユーザーがデバッガーになっている」問題

### 1.1 症状

10 reviewer × 5ラウンドの大規模セルフレビューを回し、CI に fmt/clippy/cargo test/build と nightly Playwright を持ち、`contract_tests.rs`・`*_audit_test.rs`・`e2e_print_verify.py`（`emulate_media('print')` まで実装）が揃っていた。**にもかかわらず** 最終的に数値バグ・単位ずれ・列欠落をユーザー自身が目視で見つけている。検証基盤は「存在するのに機能していない」状態にある。

### 1.2 design_verdict（3つの構造的欠陥）

Fable レビューが実コードと照合して「概ね正確」と認めた診断（HashMap 158箇所・`unwrap_or(0)` 96箇所・`e2e_print_verify.py` の『撮るだけ』・`ci.yml`/`regression.yml` の存在をすべて確認済み）。核心は「テスト不足」ではなく「**不正な状態を実行時に表現可能なまま放置し、検証を同一モデルの盲点と非決定的環境に依存させた**」構造にある。

| # | 欠陥 | 内容 | 証拠 |
|---|------|------|------|
| 1 | **型の空白** | rusqlite の生 f64・raw HashMap のまま単位/粒度/列対応を保持し、newtype/PhantomData で unrepresentable 化していない。100倍ずれ・粒度誤り・列不一致は「正しく設計すればコンパイルが通らない」クラス | 100倍ずれ2回（employee_delta_1y 2026-04-30, navy_report.rs:2729 2026-05-14）、silent 0 が3回再発。10-reviewer でも #1,#3 を見逃した |
| 2 | **検証の形骸化** | print スクリーンショットを撮っていたが committed baseline との差分比較がない。viewport も 1280 単一で 375/A4 を欠く。HashMap 非決定性は単発実行では原理的に検出不可なのに2回実行比較を持たない | `e2e_print_verify.py` が保存のみ、#3 が本番流出、#5 が単発で検出不能 |
| 3 | **検証者の相関盲点** | 書き手と判定者が同一 Claude。決定論的に grep/assert で落とせる単位・列名・print まで LLM 判断に委ね、自由探索レビュー（F1≈0.51）のまま構造的盲点を共有 | 2026-01-05 虚偽報告（「確認済み」と言いながら SQL 未実行） |

**処方**: 防御線を層で分ける。単位/粒度/列対応 → コンパイル時、レイアウト/print → Docker 固定 VRT の決定論的差分、数値/ラベル drift → スナップショット、意味的誤り（因果/範囲）→ 不変条件テストと grep lint。**LLM 判断は最後の backstop に降格する。**

> 外部前例の無検証適用も欠陥の一部：外部調査が推奨する sqlx コンパイル時チェックは本プロジェクトが **rusqlite 構成のため直接適用不可**。列名 SSoT const + 列存在 assert + `unwrap_or(0)` 禁止で代替する。

---

## 2. MECE 品質フレームワーク（D1-D5）

Opus 統合による5次元。各次元は「層」で相互排他になるよう設計。

| 次元 | 対象層 | 現状（温床） | 外部前例 | ギャップ |
|------|--------|-------------|----------|----------|
| **D1 データ正確性**（値・単位・粒度） | データ/計算層 | 生 f64 で DB 保持、ha/km²・%/比率が無検査で代入。粒度も raw Vec/HashMap で県平均を全国に渡せる。値域検証は audit_test に散在 | Rust newtype（Percent/Ratio, AreaHa/AreaKm2）、PhantomData\<Granularity\>、dbt 4分類（not_null/unique/accepted_range/relationships） | 不正状態が実行時に表現可能なまま。型で unrepresentable にする層が皆無で目視レビュー頼み |
| **D2 表示バインディング整合性**（SQL→構造体→HTML） | 計算/束縛層 | runtime 文字列クエリで列名アクセス、`.unwrap_or(0)`/HashMap→None で silent 0。survey 配下は HashMap 158 vs BTreeMap 22 で列ランダム欠落。指標がセクション分散で同名別指標 | sqlx query_as!（本件は rusqlite で不可→列名 SSoT const で代替）、HashMap→BTreeMap（工数 xs）、指標を1関数集約 pub(crate) | 列名が文字列直書きで SSoT なし。欠損時0 fallback を機械禁止する仕組みなし。決定性テストも snapshot もなく #5 は単発検出不可 |
| **D3 レイアウト・レンダリング**（viewport/print/PDF） | 表示層 | Playwright は Desktop Chrome 単一（~1280px）で 375/768/A4 794px 未検証。`e2e_print_verify.py` は print screenshot を撮るが baseline 比較なし | toHaveScreenshot()+公式 Docker、emulateMedia('print')→screenshot、viewport matrix（375/768/1280+A4 794px）、mask、font-render-hinting=none 等4フラグ | screenshot を撮るが committed baseline 差分がなく形骸化。Docker 統一なしで false-positive 地獄になるため未導入 |
| **D4 表現・文言の妥当性**（統計主張・因果・出典・粒度明示） | 意味/叙述層 | 相関→因果の効果約束、小サンプル断定、集計単位未明示。AI 生成示唆をそのまま採用。ルールは文書化済みだが機械チェックなし | Nature 2025 統計表現（因果断言不可、n<30注記、相関は『傾向がある』止まり）、総務省品質保証（集計単位・出典・丸め明記義務） | 因果断言ゼロ/n<30注記が grep チェック化されず、同一モデルの correlated blind spot 依存。出典・粒度が表フッターに構造的に埋まっていない |
| **D5 プロセス・検証設計**（CI/snapshot/AI grader/決定論検査） | 横断プロセス層 | CI 基盤は naive ではない（fmt/clippy/cargo test/build/nightly Playwright、contract/audit test 存在）が、検証が同一 Claude と単発 screenshot に集中し決定論項目まで LLM 依存。insta 未導入 | BugScope チェックリスト駆動 F1=0.87 vs 自由探索0.51、同一モデル writer/judge は correlated failure、Tool Receipts で根拠必須、担当分割、order-swap で bias 検証 | 事故クラスが grep シード+検出ガイドラインに落ちておらず自由探索のまま。決定論項目 #1,#2,#5 を人+LLM に委ねている |

### 2.1 Fable が指摘した D1-D5 の欠落次元（6件）

MECE フレームワークが「網羅的」を主張する以上、抜けを明記する。これらは D1-D5 のどこにも属していなかった。

| 欠落 | 内容 | 影響先 |
|------|------|--------|
| **A. fixture/seed DB（テストデータの決定性）** | VRT・insta・不変条件テストすべての前提。`regression.yml` は本番 Render URL を叩いており、committed baseline を本番/月次スナップショット更新データで回すとデータが動くたびに baseline が赤化する。seed 済みローカル SQLite fixture が必須 | Phase 1/2 の全機構の土台。無いと Phase 2 の VRT は構造的に維持不能 |
| **B. ETL/データ投入層の検証** | roadmap は Rust render 側のみ対象だが、silent 0 や単位誤り（職種カルテ ×10 等）の一部は python パイプライン→Turso 投入時に混入。Turso スキーマ vs Rust 期待のドリフト検査（テーブル存在・行数レンジ・列型 smoke test）が欠落 | D1/D2 の上流 |
| **C. edit-time hooks（.claude/hooks）** | ユーザー自身の feedback_hooks_runtime_guard の通り、CI 到達前の agent 逸脱はルール文書で止まらない。render 内 HashMap・`unwrap_or(0)`・因果語は CI grep だけでなく書いた瞬間に落とす層が必要 | D5 の前段 |
| **D. サンセット基準** | env-audit の教訓（使用実績なきツールは撤去）が未反映。各機構に撤去条項がないと「また事故→またルール追加」の仕組み腐敗に陥る | 全機構（→ 本書 §5） |
| **E. デプロイ後検証** | 部分コミット→Render deploy 失敗の事故クラス（feedback_partial_commit_verify）が既知なのに、`e2e_post_deploy.py` の維持・強化が roadmap のどの次元にも属していない | D5 の後段 |
| **F. スコープ外宣言** | パフォーマンス/i18n/a11y を「本件では不要」と判断したなら、その判断自体を1行書くべき（→ 本書 §6） | フレームワーク境界 |

---

## 3. 外部前例カタログ（5調査の key_findings）

出典 URL は削らない。適用可否と工数（xs/s/m/l）を併記。

### R1: 視覚回帰テスト（VRT）実務標準

| finding | 出典 | 適用可否 / 工数 |
|---------|------|----------------|
| Playwright `toHaveScreenshot()` + 公式 Docker イメージ（`mcr.microsoft.com/playwright:v1.57.0-noble`）で CI baseline 一致。baseline はコンテナ内生成し同一 PR にコミット | https://patricktree.me/blog/consistent-visual-assertions-via-playwright-server-in-docker | 直接適用可 / s（Windows 開発+Ubuntu CI はフォント差が必ず出るため Docker 統一必須。snapshotPathTemplate に -docker suffix） |
| `page.emulateMedia({media:'print'})` → `toHaveScreenshot()` で @media print 回帰テスト | https://qaskills.sh/blog/playwright-screenshots-pdf-guide-2026 | 直接適用可 / s（details 強制展開・chip box 消失を nightly で捕捉） |
| PDF 出力 VRT: ブラウザ内蔵 PDF ビューアにロード → clip screenshot → 差分 | https://medium.com/the-crc-tech-blog/pdf-visual-regression-testing-the-puppetmaster-approach-7a575d6c5559 | 適用可 / m（`file://` で生成済み PDF を読ませる。Playwright built-in 差分で代替可） |
| フォント安定化4フラグ: `--font-render-hinting=none`/`--force-device-scale-factor=1`/`--disable-gpu`/CSS `-webkit-font-smoothing` | https://medium.com/@ss-tech/the-ui-visual-regression-testing-best-practices-playbook-dc27db61ebe0 | 直接適用可 / xs（config の launchOptions.args 追記のみ） |
| Viewport matrix: Mobile 375×667 / Tablet 768×1024 / Desktop 1280×800 + レポート用 A4幅 794px | 同上 ss-tech | 直接適用可 / xs（projects 配列に viewport 追記） |
| `mask:[locator]` で動的値（日時・更新日）を黒塗り。しないと毎回 diff で形骸化 | https://css-tricks.com/automated-visual-regression-testing-with-playwright/ | 直接適用可 / xs |
| `reg-actions`（MIT）: S3/GCS 不要で Actions アーティファクトに baseline 保存・PR コメント diff | https://github.com/reg-viz/reg-actions | 適用可 / m（Playwright html reporter + artifacts で代替できる場合は不要） |
| Baseline 更新ガバナンス: CI で `--update-snapshots` 絶対禁止。開発者が同一コンテナで更新→PR コミット→レビュー承認 | https://testdino.com/blog/playwright-visual-testing | 直接適用可 / s（CI から --update-snapshots を外す） |
| `pdf-visual-diff`（MIT）: pdf.js で PNG 化 → jimp 差分。ブラウザ UI 映り込みなし | https://github.com/moshensky/pdf-visual-diff | 適用可 / m（TS テストスイート組込前提） |

### R2: データ密度の高いテーブルのレスポンシブ/印刷設計

| finding | 出典 | 適用可否 / 工数 |
|---------|------|----------------|
| `table-layout:fixed` + `<colgroup><col width>` で列幅先読み確定。overflow は td に設定 | https://developer.mozilla.org/en-US/docs/Web/CSS/table-layout | 適用可 / s |
| 水平スクロール: `<div role='region' aria-labelledby tabindex='0' style='overflow-x:auto'>` でラップ（role/aria/tabindex 必須） | https://blog.logrocket.com/creating-responsive-data-tables-css/ | 適用可 / xs（nowrap+fixed 狭幅崩れに直接効く） |
| `font-variant-numeric:tabular-nums` を全数値セルに。桁揃え。Baseline Widely Available | https://developer.mozilla.org/en-US/docs/Web/CSS/font-variant-numeric | 適用可 / xs（.num-cell に1行） |
| 印刷 sticky ヘッダー罠: `@media print{ thead{position:static!important; display:table-header-group} }` で各ページ反復 | https://www.customjs.space/blog/print-css-cheatsheet/ | 適用可 / xs |
| `<details>` 印刷展開: CSS のみの解決策は仕様上なし（csswg #2084 未解決）。JS `beforeprint` で open 付与が最安全 | https://github.com/w3c/csswg-drafts/issues/2084 | 直接適用必要 / s（既存事故クラスと完全一致） |
| `break-inside:avoid` を `<tr>` に。ただし1ページ超の要素は強制分割される | https://doppio.sh/guide/css-page-breaks | 適用可 / xs |
| A4 標準値: `@page{size:A4; margin:20mm}` で幅170mm、本文最小10pt、font-weight≧400 | 同上 customjs | 適用可 / xs |
| transform/opacity/filter 親が stacking context を生成し position:fixed のフッターが各ページに出ない | https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Positioned_layout/Stacking_context | 要確認 / xs（@media print で transform:none） |
| Carbon 密度モデル（XS-XL 5段階）: 数値右揃え・テキスト左揃えを強制規定 | https://carbondesignsystem.com/components/data-table/usage/ | 設計指針 / m |
| @page margin と body padding の二重インデントで本文幅縮小: `@media print{ body{padding:0; margin:0} }` | 同上 customjs | 直接適用必要 / xs（feedback_print_css_cascade_trap.md と一致） |

### R3: データ品質・契約テスト

| finding | 出典 | 適用可否 / 工数 |
|---------|------|----------------|
| dbt 4分類（not_null/unique/accepted_values/relationships）を言語非依存チェックリストに | https://docs.getdbt.com/docs/build/data-tests | ツール導入不要 / xs |
| sqlx `query_as!` はコンパイル時に列名・型不一致をビルドエラー化 | https://www.rustfaq.org/en/how-to-use-sqlx-for-compile-time-checked-sql-queries/ | **本件は rusqlite で直接適用不可** / m（代替策を採る） |
| `cargo sqlx prepare --check` を CI 化。SQLX_OFFLINE=true で本番 DB なしで動作 | https://leapcell.io/blog/offline-schema-management-leveraging-sqlx-cli-and-diesel-cli-for-robust-rust-applications | sqlx 移行時のみ / s |
| `#[sqlx(rename)]` は query_as! のコンパイル時チェックに無視される既知制限（issue #1121）→ SQL AS エイリアスで回避 | https://github.com/launchbadge/sqlx/issues/1121 | 規約化 / s（silent 0 の直接原因クラス） |
| Rust newtype（`Percent(f64)`/`Ratio(f64)`, `AreaHa`/`AreaKm2`）で単位安全性。ランタイムコスト0。Mars Climate Orbiter 教訓 | https://www.lurklurk.org/effective-rust/newtype.html | 適用可 / m |
| uom クレート（次元解析、ゼロコスト）。Meter+Second はコンパイルエラー | https://blog.nodraak.fr/2021/03/dimensional-analysis-in-rust/ , https://crates.io/crates/uom | 局所適用可 / l（まず newtype で十分、複雑化時に検討） |
| PhantomData\<G\> で粒度レベルを型パラメータに埋込。市区町村関数に県データを渡すとコンパイルエラー | https://www.greyblake.com/blog/phantom-types-in-rust/ | 適用可 / m |
| 「Parse, don't validate」: 受取時点で型付き構造体に変換。`unwrap_or(0)` の代わりに Option を返す API | https://www.rustfinity.com/blog/parse-dont-validate | 適用可 / s |
| Rust HashMap は反復順ランダム→列順が実行ごとに変化。BTreeMap はキー昇順で安定 | https://medium.com/@draft1967/rusty-garbage-my-hashmap-is-non-deterministic-0e518be0c5c6 | 適用可 / xs（型置換のみ） |
| datacontract CLI（OSS）で YAML コントラクトの breaking-changes を CI 検出 | https://github.com/datacontract/datacontract-cli | 適用可 / m（Python ETL→Rust 消費のスキーマ契約） |

### R4: AI agent による開発・検証の設計パターン

| finding | 出典 | 適用可否 / 工数 |
|---------|------|----------------|
| BugScope 2段階（seed 抽出+検出ガイドライン）。F1=0.87 vs 自由探索 0.51 | https://arxiv.org/html/2507.15671 | 適用可 / s（事故クラス毎にガイドライン+grep seed） |
| Anthropic 'Demystifying evals': 「2人のドメイン専門家が独立に pass/fail 合意できるか」。transcript を読まないと grader バグが見えない | https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents | 適用可 / m |
| 同一ベースモデル群は同一盲点を共有し correlated failures。writer=judge は自己バイアス | https://cogentinfo.com/resources/when-ai-agents-collide-multi-agent-orchestration-failure-playbook-for-2026 | 最重要 / xs（決定論項目を grep/assert に移す） |
| Tool Receipts（NABAOS）: 全 tool 呼出に署名レシート。捏造参照94.2%検出 | https://arxiv.org/pdf/2603.10060 | 軽量版適用可 / xs（「根拠なき主張は却下」をプロンプト明記） |
| LLM-as-judge 自己バイアス検出: 順序入替2回で判定逆転なら position bias 確定 | https://arxiv.org/html/2604.16790v1 | 適用可 / s |
| Anthropic multi-agent: subagent に objective/output format/tools/task boundaries を明示し重複探索低減 | https://www.anthropic.com/engineering/multi-agent-research-system | 適用可 / xs |
| Meta 半形式的推論: 前提→実行パス→形式的結論の証明書を先に埋める。パッチ検証で最大93%精度 | https://www.infoworld.com/article/4153054/meta-shows-structured-prompts-can-make-llms-more-reliable-for-code-review.html | 適用可 / s |
| Playwright VRT: baseline は CI（Ubuntu+公式 Docker）で生成。ローカル生成は常に fail | https://bug0.com/knowledge-base/playwright-visual-regression-testing | 適用可 / m |
| 'Do More Agents Help?': agent 数を増やすだけでは向上しない。protocol-aligned が自由探索より安定して高い | https://arxiv.org/pdf/2606.05670 | 方針決定 / xs |
| Agentic CLEAR: 多層 evaluation（span/trace/CI persona）。span-level 省略で individual step バグが埋もれる | https://arxiv.org/pdf/2605.22608 | 適用可 / m（axum handler 単体テストで span-level check） |

### R5: BtoB レポート出荷前 QA チェックリスト

| finding | 出典 | 適用可否 / 工数 |
|---------|------|----------------|
| BAN×内訳突合: サマリ数値と内訳合計が不一致なら注記。2名独立レビュー | https://medium.com/@bkornell/developer-qa-for-tableau-dashboards-0892a83747bd | 適用可 / s |
| cargo-insta スナップショット: HTTP レスポンス HTML を文字列スナップショット化。CI 不一致で fail | https://insta.rs/ | 適用可 / m |
| Playwright VRT の OS 固定 baseline: CI（ubuntu-latest）をマスタとする2段階 Action | https://medium.com/@haleywardo/streamlining-playwright-visual-regression-testing-with-github-actions-e077fd33c27c | 適用可 / m（nightly 限定） |
| Named Variations（SSOT）: 指標を1箇所集約。変形時は「指標A_prefecture」「指標A_national」で暗黙不一致を可視化 | https://www.contextawareanalytics.com/one-version-of-truth | 適用可 / m |
| dbt-expectations カラム契約: `expect_column_values_to_be_between`/`expect_column_to_exist`/`match_regex` | https://www.datadoghq.com/blog/dbt-data-quality-testing/ | 適用可 / s（rusqlite integration test で同等実装） |
| Power BI Checklist: 軸を0起点、スライサー同期、異なる画面/ブラウザ/コンテキストで確認、テストケース文書化 | https://data-goblins.com/report-checklist | 適用可 / s |
| 統計表現ガイドライン（Communications Psychology 2025）: 1観察研究から因果断言不可、小サンプルに信頼区間、相関は「傾向がある」止まり | https://www.nature.com/articles/s44271-025-00356-w | 適用可 / xs（禁止語リスト） |
| Monte Carlo データ一貫性監視: セマンティックレイヤーのドリフト継続監視。SQL 一発ファイル複製がアンチパターン | https://montecarlo.ai/blog-data-consistency | Rust は Monorepo でコンパイル時強制 / m |
| 総務省 公的統計品質保証: 調査・集計単位の明示、丸め方針統一、出典明記、サンプル数・カバレッジ付記を義務化 | https://www.stat.go.jp/data/guide/pdf/guideline.pdf | 適用可 / xs（表フッターに固定要素） |

---

## 4. 確定ロードマップ

Fable の approved_top（3,4,7,9）と challenged（1,2,5,6,8,10）に基づき Phase 分割。**Phase 1 = 決定論的・低工数・CI 強制で個人開発でも維持される項目のみ**。

### Phase 1（本 WF8 で実装）: #7 → #3 → #4 → #9

> **実装順序の根拠**（Fable 指摘 a）: **#7（style.rs 印刷・表 CSS 修正）を先頭に置く。** Phase 2 の VRT baseline を #7 の CSS 修正**前**に確定すると「現在のバグを正として焼き付ける」順序バグになる。CSS を正しくしてから baseline を撮る。

| # | タイトル | what | prevents | 工数 | 出典 |
|---|---------|------|----------|------|------|
| **#7** | style.rs 印刷・表 SSoT ハードニング（CSS 一元修正） | `.num-cell{font-variant-numeric:tabular-nums lining-nums}`、テーブルを `overflow-x:auto` ラッパー（role=region tabindex=0）で包むヘルパー、`@media print{ body{margin:0;padding:0} thead{position:static!important; display:table-header-group} tr{break-inside:avoid} }`、JS `beforeprint` で details に open 付与/`afterprint` 除去、`@page{size:A4; margin:20mm}` 本文10pt以上 | #3 nowrap 崩れ、#4 print details/二重インデント/sticky thead 消失。単位100倍の目視発見率も右揃え+tabular-nums で向上 | s | customjs print cheatsheet / MDN font-variant-numeric・table-layout / csswg #2084 / logrocket |
| **#3** | 列名 SSoT + rusqlite 列コントラクトテスト + `unwrap_or(0)` 禁止 | `src/db/columns.rs` に全 SELECT エイリアスを const 定義（SQL と Rust で同一シンボル）。tests に「各クエリの結果列集合が期待 const 集合と一致」「主要列が存在し NULL でない」assert（dbt expect_column_to_exist 相当、テーブル毎3-5行）。clippy disallowed-methods で DB 読取直後の `.unwrap_or(0)` を deny、grep CI で `get(...).unwrap_or(0)` を検出。**sqlx 移行は rusqlite 構成のため非推奨（over-engineering）** | #1 列名×SQLエイリアス不一致→silent 0（3回）、欠損キー→0 fallback による統計歪み | s | sqlx issue#1121→AS統一規約 / datadog dbt column contract / svix Parse-don't-validate |
| **#4** | 決定性保証: HashMap→BTreeMap 置換 + 2回実行バイト比較 + lint ガード | survey render パスの HashMap 158箇所のうち HTML/表シリアライズ関与分を BTreeMap または Vec（ソート済）に置換。clippy disallowed-types で render モジュール内 std HashMap を deny。tests に「同一パラメータで render を2回呼び生成 HTML をバイト比較して一致」の determinism テスト（**#5 は単発 snapshot では原理的に検出不可のため必須**） | #5 HashMap 反復順による表の列ランダム欠落 | s | rustfaq/leapcell 決定性 / VRT anti-pattern「2回連続実行して相互比較 or ソート正規化」 |
| **#9** | 統計表現 lint（禁止語 + 小サンプル注記） | 示唆生成出力に grep CI: 禁止語（「により増加」「効果がある」「原因」「改善される」）検出で fail、相関言及は「傾向がある」表現を要求、n<30 集計には注記の存在を要求。生成が AI/人いずれでも同ルール適用 | #6 相関→因果の効果約束、小サンプル断定 | xs | Nature 2025 統計表現 / 総務省品質保証 / feedback_correlation_not_causation |

**Phase 1 の性質**: 4項目すべて決定論的（grep/const assert/バイト比較/clippy lint）で CI 強制。LLM 判断に依存しない。Fable が「rank 3/4/7/9 は決定論的・低工数・CI 強制で個人開発でも維持される良い設計」と承認した集合そのもの。

### Phase 2（fixture DB 整備後）: VRT 基盤

> **前提条件**（Fable 指摘 b, missing A）: **fixture/seed DB を先に作る。** `regression.yml` は本番 Render URL を叩いており、VRT/insta を本番データで回すと月次データ更新のたびに baseline が赤化 → maxDiffPixelRatio 緩和 → 無効化の典型経路（個人開発で最も放置される「red化→無効化」パターン）に直行する。seed 済みローカル SQLite fixture + CI 内 axum サーバ起動が VRT 成立の必須前提。

**実装順序の根拠**（baseline 焼き付け）: Phase 1 の #7 で CSS を正しくした**後**に baseline を撮る。順序を誤ると現行バグが正解として固定される。Fable の「rank 7→rank 1 の順が必須」を Phase 境界に反映。

| ステップ | 内容 | 工数（Fable 再見積り） |
|---------|------|----------------------|
| 2-0 | fixture/seed DB（決定論データ）+ CI 内サーバ起動の仕組み | m（前提整備） |
| 2-1 | playwright.config.ts の projects に {794×1123(A4縦), 375, 768, 1280} 定義。report ビューは A4+375 必須 | s |
| 2-2 | 各セクションで `emulateMedia({media:'print'})` → `toHaveScreenshot('secNN-print.png', {mask:[動的日付], maxDiffPixelRatio:0.01})` | s |
| 2-3 | `regression.yml` の e2e job を `container: mcr.microsoft.com/playwright:vX-noble` で実行。baseline は同コンテナ生成・コミット、snapshotPathTemplate に -docker suffix。launchOptions.args に font-render-hinting=none 等4フラグ | s |
| 2-4 | 既存 `e2e_print_verify.py` の「撮るだけ」を「committed baseline 差分」に格上げ | s |

> Fable 注記: 元 rank 1 の effort 's' は過小。fixture data / CI 内サーバ起動 / Windows での Docker Desktop 運用の3前提を含めると実質 **m-l**。前提を欠いた着手は形骸化するため、Phase 1 完了 + fixture 計画確定まで着手保留。

### Phase 3（型設計）: #2 修正版 — 生 f64 を DB 層から出さない

> **Fable 指摘 b の反映**: 「コンパイルが通らない」は過大主張だった。newtype は**正しくタグ付けされた後の混用**しか防げず、100倍事故2回はいずれも**境界での初期タグ付け誤り**（DB 生値をどの単位と解釈するか）で起きている。`Percent(x)` に比率を渡す誤りは型では落ちない。

| 要素 | 内容 |
|------|------|
| 可視性戦略 | `src/domain/units.rs` に `struct Percent(f64); struct Ratio(f64); struct AreaHa(f64); struct AreaKm2(f64)`。**効果を出すには「生 f64 が DB 層から出ない」ことを可視性（pub 制限）+ clippy disallowed-types で強制する**。96箇所の `unwrap_or` と `get_f64` ヘルパー経由の全読取を触る実質 m-l |
| 境界 sentinel テスト | 型では落ちない「初期タグ付け誤り」を捕捉するため、DB 読取境界（型付き変換の1箇所）に**境界値 sentinel テスト**を置く。既知の変換係数（Ratio→Percent は ×100 のみ、明示 into()）を assert し、比率を Percent に直接渡す経路がないことをテストで固定 |
| 粒度 | granularity マーカー型（National/Prefectural/Municipal）+ `render_section<G>`。**ただし既存 `survey/granularity.rs`（市区町村粒度ヘルパー、別概念）と名前衝突するため別名にする** |
| 段階導入の注意 | まず section_03_salary / 07.5 / 07.6 の数値系から。**混在期間中は防御力ほぼゼロ**である点を計画に明記（Fable 指摘） |
| 工数 | 実質 m-l（当初 m は過小） |

出典: effective-rust newtype（Mars Orbiter）/ greyblake PhantomTypes / rustfinity Parse-don't-validate

### 4.1 却下項目とその理由

| 項目 | 却下理由（Fable） |
|------|------------------|
| **#5 insta 大量スナップショット** | 個人開発+AI agent 体制で形骸化リスク最高。意図的変更のたびに数十スナップショットのレビューが発生し、agent が `cargo insta accept` を打って通す = 本 roadmap 自身が #7 で批判する「agent が書いたコードを agent が承認する」構造の再生産。数値正確性は #4（バイト比較）+ #6（不変条件）で覆われ、insta 固有の増分は「ラベル文言 drift」のみ。**導入するなら数値・単位表記に限定した極小スナップショット + `accept` はユーザーのみ実行可のルール込み**でないと ROI が立たない → 却下（限定版のみ将来検討） |
| **#10 月次 grader 監査（順序入替2回・逆転率>20%）** | 企業の eval-ops をそのまま持ち込んだ over-engineering。個人開発で月次手動監査は1回実行後に放置される（SuperClaude/Serena の実績パターン）。維持されるのは「agent-written test を acceptance の唯一の根拠にしない」原則1行のみで、それは #8 の task boundary に吸収可能。**項目としては却下。`tests/acceptance/*.md`（acceptance criteria の別ファイル管理）のみ残す** |
| **#8 の4役割分割レビュー** | grep CI job 部分は #3/#9 と重複。4役割分割（unit/label/SQL/print 担当）は毎レビューのオーケストレーション費用が発生し個人開発では2-3回で省略される公算。BugScope F1=0.87 は専任チーム前提で個人+agent 体制への外挿根拠がない。ガイドライン文書化はユーザー自身の教訓（ルール文書では逸脱が止まらない→hooks で機械化）と逆行。**決定論部分を CI grep と `.claude/hooks` に落とし、文書とレビュー分割は捨てる**のが正しい縮約 → 4役割分割は却下、grep 部分のみ #3/#9 に統合 |
| **#6 の新規性過大表示** | `invariant_tests.rs`（10不変条件: 率0-100%、%合計100±0.1、matrix整合等）が 2026-04-30 から既に存在。新規価値は **BAN×内訳突合と dbt 4分類チェックリスト化のみ**。「dbt 4分類を配置」は既存テストとの重複整理を先にやらないと二重管理 → 却下ではなく「既存 `invariant_tests.rs` への BAN 突合追加」に縮約（Phase 1 後の差分実装） |

---

## 5. サンセット基準（env-audit の教訓）

**「使用実績なきツールは撤去」（Serena/SuperClaude/claude-mem の教訓）を全機構に適用する。** 各機構は導入後 N 週間で検出実績ゼロなら縮小/撤去を検討する。これがないと「また事故→またルール追加」の仕組み腐敗に陥る（Fable missing D）。

| 機構 | サンセット基準 | 撤去/縮小時の判断 |
|------|--------------|------------------|
| #7 CSS ハードニング | 恒久（CSS は一度正せば維持コスト極小）。ただし details JS hook が8週間で print 崩れを1件も防がなければ hook を撤去し CSS のみに縮小 | 恒久性が高い |
| #3 列コントラクトテスト | 8週間で列不一致/silent 0 を1件も検出せず、かつ列追加 PR で false-positive 修正コストが検出価値を上回れば assert を主要列のみに縮小 | 縮小（撤去はしない） |
| #4 determinism テスト | 12週間で非決定性を1件も検出せず、render 内 HashMap が clippy で完全に禁止済みなら2回実行比較を nightly のみに降格 | nightly 降格 |
| #9 統計表現 lint | 8週間で禁止語ヒットゼロなら「既に定着」とみなし grep を weekly に降格（撤去はしない、再発防止として残す） | weekly 降格 |
| Phase 2 VRT | **最も撤去リスクが高い。** false-positive 赤化が月2回を超えたら maxDiffPixelRatio を緩めるのではなく、fixture 決定性の欠陥を疑い一旦 nightly 隔離。4週間で真の print/layout 回帰を1件も検出しなければ対象セクションを A4+375 の2 viewport に絞る | 隔離 → 縮小 |
| Phase 3 型設計 | 混在期間中は防御力ゼロのため「検出実績」で測れない。代わりに **6週間で units.rs 導入セクションが全数値系に到達しなければ「段階導入が停滞 = 費用対効果なし」と判断し、境界 sentinel テストのみ残して newtype 展開を停止** | 展開停止（sentinel は残す） |
| tests/acceptance/*.md | 参照実績ゼロが8週間続けば陳腐化とみなし更新停止（削除はしない、監査証跡として保管） | 更新停止 |

> **メタ基準**: 本ロードマップ自体も四半期に1回、各機構の「検出実績カウント」を棚卸しする。カウントが取れていない機構は「効果不明 = 撤去候補」とする（env-audit の再発防止思想）。

---

## 6. スコープ外宣言

MECE を主張する以上、意図的に対象外とした領域を明記する（Fable missing F）。

| 領域 | 扱い | 根拠 |
|------|------|------|
| **パフォーマンス**（レスポンス時間・レンダリング速度・DB クエリ最適化） | **本ロードマップのスコープ外**。QA 基盤は「正しさ」に集中する。パフォーマンスは事故コーパスに1件も存在せず、現在の症状（デバッグをユーザーが担っている）と無関係。将来 P0 パフォーマンス問題が発生した時点で別ロードマップを起こす | 事故コーパスに該当なし。投機的機能を足さない（MVP 優先） |
| **i18n（国際化）** | **スコープ外**。本レポートは日本語 BtoB 専用で多言語要件が存在しない。i18n を今入れるのは典型的な over-engineering | 要件なし |
| **a11y（アクセシビリティ）** | **限定カバーのみ**。#7 の `role=region tabindex=0`（テーブル水平スクロール領域のスクリーンリーダー認識）で最低限を満たす。WCAG フルコンプライアンス（コントラスト比・キーボードナビ全網羅・ARIA ライブリージョン等）はスコープ外。Fable が「D3 の a11y は #7 の role=region で最低限カバーされており妥当」と評価 | BtoB 内部レポートで法的 a11y 要件なし。最低限のみ #7 に内包 |

---

## 7. 実装サマリ（本 WF8 の作業範囲）

- **今回実装**: Phase 1 の #7 → #3 → #4 → #9（この順序）
- **今回は着手しない**: Phase 2（fixture DB 未整備のため）、Phase 3（型設計、m-l で別タスク）
- **却下確定**: insta 大量スナップショット / 月次 grader 監査 / 4役割分割レビュー
- **縮約**: #6 は既存 `invariant_tests.rs` への BAN 突合追加のみ（Phase 1 後の差分）
- **各機構にサンセット基準を付与済み**（§5）

---

## 付録: WF7 出典一覧（トピック別）

| トピック | 調査エージェント | key_findings 件数 |
|---------|----------------|------------------|
| R1 視覚回帰テスト実務標準 | R1:visual-regression（Sonnet） | 9 |
| R2 データ密度テーブルのレスポンシブ/印刷設計 | R2:responsive-print-tables（Sonnet） | 10 |
| R3 データ品質・契約テスト | R3:data-contract-testing（Sonnet） | 10 |
| R4 AI agent 開発・検証パターン | R4:multi-agent-qa（Sonnet） | 10 |
| R5 BtoB レポート出荷前 QA | R5:btob-report-qa（Sonnet） | 9 |
| MECE 統合 | synthesize（Opus） | D1-D5 + roadmap 10項目 |
| 批判レビュー | review（Fable） | approved 4 / challenged 6 / missing 6 |

（各 finding の出典 URL は §3 の表に完全収録。削除・改変していない。）
