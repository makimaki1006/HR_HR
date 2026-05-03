# 媒体分析レポート拡張 Phase 0〜2 準備資料

作成日: 2026-05-03

対象: ハローワーク分析システムV2 / 媒体分析タブ / レポート作成機能

参照モック:

- `docs/recruiting_market_intelligence_full_mock.html`
- `docs/recruiting_living_cost_distribution_report_mock.html`
- `docs/recruiting_target_open_data_mock.html`

## 1. 目的

既存の媒体分析レポートに、国のオープンデータ、既存DB上の外部統計、媒体求人データ、生活コスト、通勤圏を安全に組み込むための準備を完了する。

この資料では、以下の3フェーズを実装前準備として固定する。

1. Phase 0: 現状把握
2. Phase 1: データ設計
3. Phase 2: 指標定義

この段階では、既存アプリのコード修正、DB投入、UI移植は行わない。

## 2. Phase 0: 現状把握

### 2.1 実装対象アプリ

実装対象の現行アプリは、以下のRust/Axum版を基準にする。

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy`

理由:

- `Cargo.toml`、`src/lib.rs`、`src/handlers/survey/*` が存在する。
- `/tab/survey`、`/api/survey/upload`、`/report/survey` などの媒体分析レポート経路が確認できる。
- 直近レビュー対象の `theme=v8/v7a/default` もこの系統に存在する。

### 2.2 媒体分析タブの処理経路

既存ルート:

| 種別 | URL | 主な役割 |
|---|---|---|
| 画面 | `/tab/survey` | 媒体分析タブのCSVアップロード画面 |
| API | `/api/survey/upload` | CSVアップロード、パース、集計、セッションキャッシュ保存 |
| API | `/api/survey/analyze` | 既存分析取得 |
| API | `/api/survey/integrate` | CSV × HW × 外部統計の統合表示 |
| API | `/api/survey/report` | レポートJSON |
| HTML | `/report/survey` | レポートHTML表示 |
| Download | `/report/survey/download` | HTMLファイルとしてダウンロード |

主要処理:

| 処理 | 現在の入口 |
|---|---|
| CSVパース | `handlers::survey::upload::parse_csv_bytes_with_hints` |
| CSV集計 | `handlers::survey::aggregator::aggregate_records_with_mode` |
| 求職者視点分析 | `handlers::survey::job_seeker::analyze_job_seeker` |
| 統合レポート | `handlers::survey::integration::render_integration_with_ext` |
| レポートHTML | `handlers::survey::report_html::render_survey_report_page_with_variant_v3_themed` |
| HTMLダウンロード | `handlers::survey::handlers::survey_report_download` |

### 2.3 既存レポートの拡張ポイント

既存レポートは以下をすでに持つ。

- `ReportVariant`
- `ReportTheme`
- CSV主要市区町村ヒートマップ
- 市区町村ベンチマークレーダー
- HW連携セクション
- 外部統計セクション
- 産業構造セクション
- SalesNow企業リスト/企業セグメント
- PDF保存を想定したHTML出力

拡張方針:

- `default` テーマの既存挙動は維持する。
- 新しい採用マーケットインテリジェンス機能は、既存レポートに追加セクションとして段階導入する。
- 実装時は `SurveyExtensionData` の考え方を拡張し、新しい地域分析データをまとめて渡す。
- 重い集計をレポートHTML生成時に行わず、事前集計テーブルを参照する。

### 2.4 既存DB上の国・外部統計データ

確認DB:

- `data/hellowork.db` (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db`)

注: 当初本資料では `country_stats_local.db` と記載していたが、実機を 2026-05-03 に直接 SELECT して確認した結果、外部統計テーブルは `data/hellowork.db` に格納されていた。`scripts/fetch_household_spending.py:24` でも `DB_PATH = ...\hellowork.db` と明示されており、これが正本である。

#### 2.4.1 hellowork.db に実存する `v2_external_*` テーブル (2026-05-03 SELECT 検証)

| テーブル | 実機行数 | 粒度 | 使い道 |
|---|---:|---|---|
| `v2_external_population` | 1,742 | 都道府県・市区町村 | 総人口、男女、年少/生産年齢/高齢人口 |
| `v2_external_population_pyramid` | 15,660 | 都道府県・市区町村・年齢階級 | 年齢性別ピラミッド |
| `v2_external_daytime_population` | 1,740 | 都道府県・市区町村 | 昼夜間人口、流入/流出 |
| `v2_external_migration` | 1,741 | 都道府県・市区町村 | 転入/転出/純移動 |
| `v2_external_commute_od` | 83,402 | 出発地市区町村・到着地市区町村 | 通勤OD実数 |
| `v2_external_foreign_residents` | 1,742 | 都道府県・市区町村 | 在留外国人 |
| `v2_external_job_opening_ratio` | 47 | 都道府県 | 有効求人倍率 |
| `v2_external_minimum_wage` | 47 | 都道府県 | 最低賃金 |
| `v2_external_prefecture_stats` | 47 | 都道府県 | 賃金、物価、充足率などの県別集約 |

#### 2.4.2 hellowork.db に **不在** のテーブル (要追加投入)

以下5テーブルは当初棚卸しに記載していたが、2026-05-03 の SELECT 確認では hellowork.db に存在しなかった。Phase 3 着手までに e-Stat APIなどから取り込みが必要。

| テーブル | 要件 | 取り込み方針 |
|---|---|---|
| `v2_external_industry_structure` | 市区町村コード×産業 | 経済センサス CSV → 投入スクリプト未確認 |
| `v2_external_establishments` | 都道府県×産業 | 経済センサス（県別） |
| `v2_external_household_spending` | 都市×カテゴリ | `scripts/fetch_household_spending.py` 実行 (テーブル定義と DB_PATH は確認済) |
| `v2_external_land_price` | 都道府県×用途 | 地価公示 |
| `v2_external_labor_stats` | 都道府県×年度 | 賃金構造基本統計など |

#### 2.4.3 SalesNow 企業データの状態

| ファイル | 形態 | 行数 | 状態 |
|---|---|---:|---|
| `data/salesnow_companies.csv` | CSV (UTF-8) | 198K行想定 | 存在 (492 MB) |
| `v2_salesnow_companies` テーブル | SQLite | 0 (テーブル自体不在) | 未投入 |

→ Phase 3 で本機能から SalesNow を結合する場合は、CSV → DB 投入スクリプトの整備が必要。

#### 2.4.4 既存DBで既に使えること

- 市区町村別の総人口・年齢性別構成。
- 市区町村別の昼夜間人口と流入/流出傾向。
- 市区町村レベルの通勤OD実数（83,402行）→ `commute_flow_summary` の生データに使える。
- 都道府県単位の有効求人倍率・最低賃金・賃金/物価/充足率。

#### 2.4.5 不足していること

- 市区町村 × 職業大分類 × 年齢 × 性別の常住地/従業地データ（`municipality_occupation_population` の生データ）。
- 住宅・土地統計ベースの市区町村別家賃proxy。
- 小売物価統計ベースの月次生活コスト補正。
- 産業構造（市区町村×産業）と事業所数。
- SalesNow 企業データの DB 投入。
- 媒体求人データと外部統計を結合した配信優先度の事前集計。

#### 2.4.6 文字化け確認結果 (2026-05-03 実施)

**結果サマリ**: `hellowork.db` に実存する 9 テーブルすべて、および `salesnow_companies.csv` で **文字化けなし** (UTF-8 で正常保持)。

##### 確認手順

1. Python `sqlite3` で `hellowork.db` を直接 open。
2. 各テーブルの TEXT 列について `SELECT ... LIMIT 5` および `SELECT DISTINCT prefecture / municipality LIMIT 8` を実行。
3. 結果を UTF-8 で stdout 出力。

##### 確認結果テーブル

| テーブル | prefecture サンプル | municipality サンプル | 判定 |
|---|---|---|---|
| `v2_external_population` | 三重県, 京都府, 佐賀県, 兵庫県, 北海道, 千葉県, 和歌山県, 埼玉県 | いなべ市, 亀山市, 伊勢市, 伊賀市, 南伊勢町, 名張市, 四日市市, 多気町 | ✅ 正常 |
| `v2_external_population_pyramid` | 同上 | 同上 | ✅ 正常 |
| `v2_external_daytime_population` | 同上 | 同上 | ✅ 正常 |
| `v2_external_migration` | 同上 | 同上 | ✅ 正常 |
| `salesnow_companies.csv` | — | 日本紙通商株式会社 / 合同会社ＫＴＴサービス / 三和陸運株式会社 (会社名カラム) | ✅ 正常 (UTF-8) |

##### 発生層の切り分け

| 層 | 観察 |
|---|---|
| 投入元CSV | UTF-8 正常 (salesnow_companies.csv で確認) |
| SQLite保存値 | UTF-8 正常 (Python `sqlite3` で SELECT 結果が日本語表示) |
| Turso保存値 | 本確認では未検証 (Phase 3 投入時に再検証) |
| Rust/HTML表示 | E2E テーマ検証 (audit_2026_05_03) で日本語が正常表示済み (画面・PDF) |
| PowerShell コンソール表示 | コードページ依存で文字化けすることがあるが、SQLite 中身は影響なし |

##### 注意点 (検証中の副次発見)

- `v2_external_population` の最初の行が `('都道府県', '市区町村', '2020-10-01')` というヘッダー風の値を含んでいた。これは文字化けではなく **データ品質問題** (1行目にヘッダー文字列がレコードとして混入)。Phase 3 で集計に使う前に `WHERE prefecture <> '都道府県'` でフィルタするか、再投入時に skip する設計が必要。

##### Phase 3 への影響

| 項目 | 結論 |
|---|---|
| 結合キー方針 | `municipality_code` を正にする方針は妥当。文字化けリスク低いため `prefecture + municipality` テキスト結合も補助的に使える |
| 一時除外テーブル | 文字化け起因では **なし** |
| 不在テーブルの扱い | `industry_structure / establishments / household_spending / land_price / labor_stats / v2_salesnow_companies` の 6 件は Phase 3 着手前に投入が必要 |
| Turso 同期 | 投入後、Turso V2 DB と SELECT 結果が一致するか別途検証 (MEMORY/Turso優先ルール) |

#### 2.4.7 残課題 (Phase 3 着手前に解消)

1. 不在 6 テーブルの投入 (e-Stat API / CSV)。`scripts/fetch_household_spending.py` のような既存スクリプトがあるか調査し、無いものは新規作成。
2. `v2_external_population` のヘッダー混入レコードの除外設計 or 再投入。
3. Turso V2 DB に同テーブルが存在し、文字列が一致するかの確認。

## 3. Phase 1: データ設計

### 3.1 結合キー

原則:

- 内部結合キーは `municipality_code` を正とする。
- 表示用に `prefecture`、`municipality_name` を持つ。
- CSV由来の地域名は、既存 `location_parser` と市区町村コードマスタで正規化する。
- 市区町村名だけでの結合は、同名自治体や表記揺れのリスクがあるため避ける。

### 3.2 追加する事前集計テーブル

DDL案は以下に分離する。

- `docs/survey_market_intelligence_phase0_2_schema.sql`

追加候補:

1. `municipality_occupation_population`
2. `municipality_living_cost_proxy`
3. `commute_flow_summary`
4. `municipality_recruiting_scores`
5. `media_area_performance_future`

初期実装で必須:

- `municipality_occupation_population`
- `municipality_living_cost_proxy`
- `commute_flow_summary`
- `municipality_recruiting_scores`

将来検証用:

- `media_area_performance_future`

### 3.3 データ結合フロー

基本フロー:

1. CSVから求人勤務地、給与、雇用形態、職種、タグを抽出する。
2. 勤務地を `prefecture`、`municipality_name`、`municipality_code` に正規化する。
3. CSV内の主要市区町村TOP Nを抽出する。
4. 主要市区町村について、既存HW/媒体求人DBから競合求人件数と給与分布を取得する。
5. 国勢調査の職業人口・年齢性別データを結合する。
6. 通勤ODの流入元TOP Nを結合する。
7. 生活コストproxyを結合する。
8. 配信優先度、推定母集団、給与魅力度を計算する。
9. レポートHTMLには集計済み構造を渡す。

### 3.4 レポート生成時のデータ境界

レポート生成時に行ってよい処理:

- 事前集計テーブルの読み取り。
- CSV集計結果との軽い結合。
- 表示用のランキング、上位N抽出。
- 欠損時の非表示判定。

レポート生成時に避ける処理:

- 全ODデータの都度集計。
- 全市区町村 × 職業 × 年齢 × 性別の都度集計。
- 大量のTurso書き込み。
- 表示HTML内で複雑な統計推定を都度実行すること。

## 4. Phase 2: 指標定義

詳細定義は以下に分離する。

- `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md`

初期実装で固定する指標:

| 指標 | 区分 | 用途 |
|---|---|---|
| 対象職業人口 | 実測 | 主要ターゲットの厚み |
| 近接職種人口 | 推定 | 未経験歓迎/異業種転換候補 |
| 競合求人密度 | クロス分析 | 人口に対する求人競争の強さ |
| 通勤到達性 | 推定 | 配信対象地域としての現実性 |
| 生活コスト補正後給与魅力度 | 参考 | 額面給与の地域差補正 |
| 給与競争力 | クロス分析 | 媒体給与・公式賃金・企業規模別水準の比較 |
| 配信優先度 | 推定 | 媒体配信すべき地域の優先順位 |
| 保守/標準/強気母集団 | 推定 | 採用ターゲット母集団のレンジ |

## 5. 実装時の組み込み順

### Step 1: 読み取り専用のデータアクセス層

追加テーブルを読み取る関数を作る。

想定:

- `fetch_recruiting_scores_by_municipalities`
- `fetch_living_cost_proxy`
- `fetch_commute_flow_summary`
- `fetch_occupation_population`

### Step 2: レポート用DTO

既存の `SurveyExtensionData` に相当する新しい構造を追加する。

候補:

- `SurveyMarketIntelligenceData`
- `MunicipalityRecruitingScore`
- `LivingCostProxy`
- `CommuteFlowSummary`
- `OccupationPopulationCell`

### Step 3: 既存HTMLへセクション追加

初期追加:

1. 結論サマリーカード
2. 配信地域ランキング
3. 人材供給ヒートマップ
4. 給与・生活コスト比較
5. 保守/標準/強気レンジ

後続追加:

1. 採用余地タイルマップ
2. 年齢性別ピラミッド
3. 給与競争力散布図
4. 通勤流入Sankey

### Step 4: フラグ制御

初期は安全のため以下のいずれかで制御する。

- `variant=market_intelligence`
- `theme` とは別の `analysis=market_intelligence`
- データが存在する場合のみ追加セクション表示

推奨:

- `variant=market_intelligence`

理由:

- `theme` は見た目の切替として維持する。
- `variant` はレポート構成の切替として既存思想に近い。

## 6. 受け入れ条件

Phase 0〜2完了時点:

- 現行レポート経路が説明できる。
- 既存DBにあるデータと不足データが説明できる。
- 既存DB文字列の文字コード状態を確認し、文字化けがある場合は発生層と除外/修正方針を記録できている。
- 追加テーブルDDL案がある。
- 指標定義と計算式が固定されている。
- `実測 / 推定 / 参考` の表示ルールが固定されている。
- 実装担当者が次にコード実装へ入れる。

コード実装開始後の最低条件:

- `default` テーマの既存レポートが変わらない。
- 追加データ欠損時もレポート生成が失敗しない。
- すべての推定値に `推定` または `参考` ラベルを付ける。
- 応募数・採用数を断定予測しない。
