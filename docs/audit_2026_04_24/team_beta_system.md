# Team β: System Integrity Audit Report

**監査日**: 2026-04-24
**監査者**: Team β (System Integrity)
**対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`) — L2 Information Architecture / L3 Data Pipeline
**手法**: 読み取り + grep のみ (コード編集禁止)。すべての発見は file:line で裏付け。

---

## エグゼクティブサマリ

- **データソース**: 4 系統 (ローカル `hellowork.db` / Turso `country-statistics` / Turso `salesnow` / 監査用 Turso) を確認。すべての外部 DB は **`Option<TursoDb>`** で握っており、未接続でもパニックせず空応答を返す graceful degradation。
- **タブ構成**: 9 タブ (`market / jobmap / region_karte / analysis / competitive / diagnostic / recruitment_diag / company / survey`) が `dashboard_inline.html` で公開中。**`/tab/overview, /tab/balance, /tab/workstyle, /tab/demographics, /tab/trend, /tab/insight` は router に残っているが UI に到達経路なし** (旧 `dashboard.html` 用、現 UI からは dead route)。
- **2026-04-23 契約監査の解消状態**: **Mismatch #1〜#5 はすべて未修正**。`global_contract_audit_test.rs` に `#[ignore]` 付き bug marker テスト 2 件で見える形に固定済み。新規ミスマッチは検出されず。
- **CTAS fallback**: `flow.rs` 11 箇所 + `flow_context.rs` 3 箇所、計 14 箇所の `// FALLBACK: GROUP BY, replace with CTAS after May 1` がすべて使用中。5/1 後に置換要。`docs/flow_ctas_restore.md` で戻し手順は文書化済み。
- **最重要課題**: 2026-04-23 で発見した jobmap 4 件のミスマッチが**1 日経過しても 1 件も修正されていない**。bug marker は CI 上 `#[ignore]` 扱いで通常実行されないため、永続化はしているが「気づき」には繋がらない。

---

## 1. データソース棚卸し

### 1.1 接続定義 (`src/lib.rs:42-54`, `src/main.rs:38-217`)

| データソース | 型 | 環境変数 | 接続失敗時 | 行数推定 (CLAUDE.md/MEMORY 由来) |
|------------|---|---|---|---|
| **ローカル SQLite** `hellowork.db` | `Option<LocalDb>` | `hellowork_db_path` | `tracing::warn!` + `None`、UI に赤バナー (`lib.rs:777`) | ~200 万行級 (postings) |
| **Turso country-statistics** | `Option<TursoDb>` | `TURSO_EXTERNAL_URL` / `TOKEN` | `tracing::warn!` + `None`、Tab 個別で空応答 | 14 外部 + 10 拡張 + 6 ts_turso + 3 v2_flow_mesh1km_YYYY = ~30+ テーブル、~16 万行 (HW時系列) + 38M 行 (mesh1km) |
| **Turso salesnow** | `Option<TursoDb>` | `SALESNOW_TURSO_URL` / `TOKEN` | 同上、企業検索/採用診断/labor_flow が空応答 | 198K 社 (`v2_salesnow_companies` + `v2_industry_mapping` + `v2_company_geocode`) |
| **Turso audit** | `Option<AuditDb>` | `AUDIT_TURSO_URL` / `TOKEN` | `tracing::warn!` + `None`、管理者画面 403 | 内部運用ログ |
| **CSV upload** (Indeed/求人ボックス) | tower-sessions (memory) | UPLOAD_BODY_LIMIT_BYTES = 20MB (`lib.rs:39`) | 20MB 超は 413 で即拒否 | 一時 (セッション内のみ) |
| **GeoJSON 静的** | `static/geojson/*.json(.gz)` | n/a | `tracing::warn!`、地図描画失敗 | 47 県 + 市区町村 |

### 1.2 接続初期化方式

- すべての Turso 接続は `tokio::task::spawn_blocking` で別スレッド初期化 (`main.rs:87, 117, 165`)。理由は `reqwest::blocking::Client` を async コンテキスト内で作るとパニックするため (`main.rs:80-81` コメント)。
- Turso 接続は `inner.client.post(...).timeout(30s)` (`turso_http.rs:32-35`)。タイムアウト時は `format!("Turso HTTP request failed: {e}")` を `Err` として返し、上位ハンドラがフォールバック処理。
- **`company_geo_cache: Option<Vec<...>>` は無効化済み** (`main.rs:155-158`)。Render 無料 512MB OOM 対策で「リクエスト時に Turso へ直接クエリ」に切替済み。

### 1.3 hellowork.db 自動 INDEX (`main.rs:42-67`)

19 個の INDEX を起動時に `CREATE INDEX IF NOT EXISTS` で発行 + `ANALYZE`。`postings` テーブルにのみ対象。Index 設計は (job_type, prefecture)、(prefecture, salary_min DESC) など複合主体で N+1 緩和に寄与。

---

## 2. タブ × データソース マトリクス

### 2.1 公開 9 タブ (`templates/dashboard_inline.html:71-87`)

| タブ | path | hellowork.db | Turso country-stats | SalesNow | CSV | テンプレ実体 |
|------|------|:---:|:---:|:---:|:---:|---|
| 市場概況 | `/tab/market` (`market.rs`) | ✓ | ✓ (`v2_external_*`) | - | - | inline 文字列生成 |
| 地図 | `/tab/jobmap` (`jobmap/handlers.rs`) | ✓ (postings + 求人座標) | ✓ (`v2_flow_*`, `v2_external_*`) | ✓ (`v2_salesnow_*`, `v2_company_geocode`) | - | `templates/tabs/jobmap.html` |
| 地域カルテ | `/tab/region_karte` (`region/karte.rs`) | ✓ | ✓ (`v2_flow_*` + `v2_external_*` 多数) | - | - | `templates/tabs/region_karte.html` |
| 分析 | `/tab/analysis` (`analysis/handlers.rs`) | ✓ | ✓ (`v2_external_*` 30+, `v2_vacancy_rate`, `v2_regional_resilience`, etc.) | - | - | inline + sub-tab 動的 |
| 求人検索 | `/tab/competitive` (`competitive/handlers.rs`) | ✓ | - | - | - | `templates/tabs/competitive.html` |
| 条件診断 | `/tab/diagnostic` (`diagnostic.rs`) | ✓ | ✓ (`v2_vacancy_rate` 等) | - | - | inline |
| 採用診断 | `/tab/recruitment_diag` (`recruitment_diag/`) | ✓ | ✓ (`v2_external_*`, `v2_flow_*`) | ✓ (競合企業) | - | `templates/tabs/recruitment_diag.html` |
| 企業検索 | `/tab/company` (`company/`) | ✓ (求人結合) | ✓ (`v2_external_prefecture_stats`) | ✓ (主) | - | inline |
| 媒体分析 | `/tab/survey` (`survey/`) | ✓ (HW 求人 enrichment) | ✓ (`ts_turso_counts`) | ✓ (企業情報補強) | ✓ (主) | inline |

### 2.2 dead route (UI 到達不可、ルートのみ存在)

`src/lib.rs:79-86, 235-249` 参照。ルーターに `get(handlers::overview::tab_overview)` 等が登録されているが、`dashboard_inline.html` のタブボタンには対応 `hx-get` がない。

| dead route | ハンドラ | 旧 UI 残骸テンプレ |
|---|---|---|
| `/tab/overview` | `handlers::overview::tab_overview` | `templates/tabs/overview.html` |
| `/tab/balance` | `handlers::balance::tab_balance` | `templates/tabs/balance.html` |
| `/tab/workstyle` | `handlers::workstyle::tab_workstyle` | `templates/tabs/workstyle.html` |
| `/tab/demographics` | `handlers::demographics::tab_demographics` | `templates/tabs/demographics.html` |
| `/tab/trend` | `handlers::trend::tab_trend` | (inline) |
| `/tab/insight` | `handlers::insight::tab_insight` | (inline) |

`templates/dashboard.html` は旧 6 タブ UI で、`grep -rn "dashboard\.html" src/` で参照ゼロ。`include_str!("../templates/dashboard_inline.html")` のみが `lib.rs:790` で使われる。

**観察**: dead route の `tab_*` ハンドラは依然 lib.rs から参照されるためコンパイル対象。削除すれば数百行のコード削減・テストコスト減。ただし `/api/insight/report`, `/api/insight/widget/*` は **insight タブが UI に無いにもかかわらず外部からアクセス可能なエンドポイント**として残存しており、レポート出力経路として実用されている可能性あり (要要件確認)。

---

## 3. 機能重複・抜けの検出 (L2)

### 3.1 重複指標の疑い

| 指標 | 計算箇所 A | 計算箇所 B | リスク |
|------|---------|---------|------|
| 都道府県人口 | `analysis/fetch.rs:661 v2_external_population` | `overview.rs:1055 v2_external_daytime_population` | **異なる外部テーブル**から取得しているため数値ズレ可能性。総人口 vs 昼間人口の用途違いではあるが、UI ラベルで誤認リスク |
| 求人数 (postings) | `jobmap/handlers.rs` 直接 SQL | `competitive/fetch.rs` filter 経由 | フィルタ条件が一致しない可能性 (industry_raw vs occupation_major)。重複計算ではないが整合性検証なし |
| labor flow industries | `jobmap/company_markers.rs` (SalesNow) | `recruitment_diag/competitors.rs` (SalesNow + HW結合) | データソース同じだが集計粒度違い。混乱の温床 |

### 3.2 機能の抜け (ペルソナ視点)

- **media campaign 効果検証は survey タブのみ**。market・analysis・jobmap には CSV upload 連動なし → 求人媒体データの広域比較不可。
- **タブ間遷移リンクなし** (確認: `dashboard_inline.html` 内に「→このエリアの企業を見る」等の jobmap → company リンクは存在しない)。地域カルテ → 採用診断 → 企業検索の自然な動線が断絶。
- **ヘルプ/ガイド** は `/tab/guide` と `/login` 表示のみで、各タブ内ヘルプは未実装。

### 3.3 トグル・フィルタ整合性

- 上部フィルタ (都道府県/市区町村/産業) はセッション保存 (`SESSION_PREFECTURE_KEY` 等) で全タブ共通。
- ただし **採用診断 / 企業検索は独自に Query パラメータでオーバーライド可能**。タブ内クエリと共通フィルタの優先順位が UI 上明示されていない。

---

## 4. 契約整合性 (再 cross-check)

### 4.1 既存 Mismatch #1〜#5 の現状

| Mismatch | 修正箇所 | 状態 (2026-04-24 現在) | 証拠 |
|----|---|----|----|
| **#1** seekers `name` キー欠落 | `jobmap/handlers.rs:399` | **未修正** | `handlers.rs:398-403` の json! には `"municipality": m_name` のみ。`"name"` 追加なし |
| **#2** seekers `flows` キー欠落 | `jobmap_seekers` | **未修正** (silent empty 継続) | `grep "flows"\|flows:" handlers.rs` 結果ゼロ |
| **#3** detail-json 7 フィールド欠落 | `jobmap/handlers.rs:267-287` | **未修正** | json! に `service_type, salary_detail, education_training, special_holidays, tags, geocode_confidence, geocode_level` のいずれも未追加 |
| **#4** labor_flow `municipality` キー欠落 | `jobmap/company_markers.rs:128` | **未修正** | `grep "municipality"\|"location"` 結果は `"location": loc` の 1 件のみ |
| **#5** `center` 形式不一致 (object/array) | 観察のみ | **既知放置** | `global_contract_audit_test.rs:478` で `#[ignore = "Observation only"]` |

`global_contract_audit_test.rs:438-` に Mismatch #4 を bug marker テストとして固定済 (`#[ignore = "Known contract mismatch #4"]`)。CI 上では `cargo test -- --ignored` のみで実行され、通常の赤化はしない。

### 4.2 採用診断タブの状態

`recruitment_diag/contract_tests.rs` (360 行) で 8 panel の契約テストが整備済み (`docs/contract_audit_2026_04_23.md` Section 5)。本監査では契約テスト整備済みなので追加検証は省略。

### 4.3 新規ミスマッチの cross-check

`market / analysis / competitive / diagnostic / company / survey / region_karte / insight` は **HTML partial + `data-chart-config` パターン** (`static/js/app.js`, `static/js/region_karte.js`, `static/js/export.js` の 3 ファイルが `data-chart-config` を読む) を採用しており、構造的に JSON key ミスマッチが発生しない設計。grep 結果でも該当する `data.X` 参照と backend 返却 key の不一致は新規発見なし。

**唯一の例外**: `/api/insight/report` JSON endpoint (`insight/handlers.rs`) は frontend consumer が見当たらず stub の可能性 (`docs/contract_audit_2026_04_23.md` Section 2 確認済み)。同じく `/api/survey/report` も `{"status":"upload_csv_first"}` スタブ。

---

## 5. パフォーマンス・信頼性

### 5.1 Turso 接続失敗時の挙動

| エンドポイント | Turso None 時の挙動 | ファイル:行 |
|---|---|---|
| `/api/jobmap/heatmap` | `error_response("DB未接続")` を返却し空 points | `jobmap/heatmap.rs:67-69` |
| `/api/flow/karte/*` | `error_response("DB未接続")` 返却 | `jobmap/flow_handlers.rs:51-54, 93-96, ...` |
| `/api/insight/*` | `state.turso_db.clone()` を `as_ref()` で渡し、各 fetch が空配列返却 | `insight/handlers.rs:42, 129, 172` |
| `/api/trend/subtab/{id}` | `turso_db.as_ref()` を渡し render_subtab_N 側で空テンプレ生成 | `trend/handlers.rs:24-27, 90-96` |
| `/api/region/karte/{citycode}` | turso 必須だが None でも 200 で空オブジェクト | `region/karte.rs:85, 124` |
| labor_flow (SalesNow) | `state.salesnow_db` 必須、None なら明示エラー JSON | `global_contract_audit_test.rs:412-` で逆証明 |

**結論**: panic 0 件。すべての Turso/SalesNow 経路は graceful degradation。ただし「データが空」と「DB未接続で空」が同じ表示になり、ユーザーから区別不可 (UI 改善余地)。

### 5.2 N+1 パターン

- **labor_flow** (`jobmap/company_markers.rs:196-`) は `industry_mapping` で `hw_job_type → sn_industry` を引き、その後 `v2_salesnow_companies` を別クエリ。**産業ごとループでクエリ発行**の疑い → 詳細確認要だが行数 200 弱で展開されていれば許容範囲。
- **company_markers** は z (zoom) 別にクエリ件数が変動するが `LIMIT` が必ず効く設計 (`company_markers.rs:70, 88, 196, 206`)。
- 38M 行 mesh1km は `BETWEEN mesh_min AND mesh_max` で範囲検索 (`flow.rs:69-83`)。bbox 由来 mesh_id レンジ + month + dayflag/timezone の複合 INDEX が前提だが、**Turso 上の INDEX 状況は本監査ではコードからは未確認**。Python 側 ETL 実装に依存。

### 5.3 キャッシュ戦略

- `AppCache` (`db/cache.rs`) は DashMap + TTL + max_entries の 3 層。
- TTL は `config.cache_ttl_secs`、max_entries は `config.cache_max_entries` で `AppConfig::from_env` から注入。
- キー命名規則: `{tab_name}_{filter_summary}` パターン (`market_*`, `balance_*`, `competitive_*`, `insight_tab_*`, `trend_sub*`, `choropleth_*`, `geojson_*`, `industry_tree_*`, `company_profile_html_*`)。
- **不整合点**: `api.rs:26 "geojson_{}"` は他のタブ別キャッシュとは独立。フィルタ依存しないため正常だが、`remove_prefix("geojson_")` で全 GeoJSON 一括削除すると意図しないユーザーキャッシュも消える可能性 (現状そのコードは無いが要注意)。
- `set_industry_filter` ハンドラ (`lib.rs:831-`) は `cache.clear()` を呼ばず**キャッシュキーにフィルタを含める**戦略でフィルタ変更時の整合性を担保。`lib.rs:819` のコメント「他ユーザーのキャッシュまで破棄してしまうため削除」は妥当。

### 5.4 大規模データクエリ (mesh1km 38M 行)

`v2_flow_mesh1km_YYYY` (3 年分) に対するクエリは **すべて FALLBACK GROUP BY 方式**で動的集計中。CTAS が無いため:

- `get_city_agg` (`flow.rs:142-`): 47 都道府県 × 12ヶ月 × 36ヶ月 = **数千万行 GROUP BY**。実行計画次第で 30 秒タイムアウトのリスク。
- `get_mesh3km_heatmap` (`flow.rs:92-`): bbox 範囲限定で実用可能だが 5/1 の CTAS 投入後は 10x 程度高速化期待。

---

## 6. CTAS fallback 残課題

### 6.1 残存箇所 (14 箇所、すべて使用中)

| ファイル | 行番号 | 関数 | 期待 CTAS テーブル |
|---|---|---|---|
| `src/handlers/insight/flow_context.rs` | 51, 138, 208 | `calc_ratio_from_profile`, `calc_covid_recovery`, `build_flow_context` | `v2_flow_city_agg` |
| `src/handlers/jobmap/flow.rs` | 88, 112, 137, 163, 196, 213, 229, 238, 266, 281 (+ コメント 17) | `get_mesh3km_heatmap`, `get_city_agg`, `get_karte_profile`, `get_karte_monthly_trend`, `get_karte_daynight_ratio` 等 | `v2_flow_city_agg`, `v2_flow_mesh3km_agg` |

すべて `// FALLBACK: GROUP BY, replace with CTAS after May 1` コメント付きで `grep -rn "FALLBACK: GROUP BY"` で確実に追跡可能。

### 6.2 戻し作業の準備状況

`docs/flow_ctas_restore.md` (120 行) で以下を明文化済:

- CTAS 投入 SQL 仕様 (citycode/year/month/dayflag/timezone カラム + INDEX)
- Rust 側戻し方針 (各 FALLBACK コメント直下を CTAS SELECT に置換)
- 逆証明用検証 SQL (CTAS と FALLBACK の総和一致 + 特定 citycode 一致)
- double count 防御 (`AggregateMode::where_clause()` 経由必須)

**準備状況評価**: 良好。Git 履歴から 2026-04-22 以前の CTAS ベース実装を復元可能であることも明記 (`flow_ctas_restore.md:51`)。

---

## 優先 Top 10 改善項目

| # | 優先度 | 課題 | 影響 | 推奨アクション | 着手目安 |
|---|---|---|---|---|---|
| 1 | 🔴 | jobmap Mismatch #1 (`name` キー欠落) | UI ツールチップが `undefined: 0人` 化 | `handlers.rs:399` json! に `"name": m_name` 追加 | 即座 (5分修正) |
| 2 | 🔴 | jobmap Mismatch #4 (`municipality` キー欠落) | 市区町村ドリルダウン失敗 | `company_markers.rs:128` json! に `"municipality": muni` 追加 | 即座 (5分修正) |
| 3 | 🔴 | jobmap Mismatch #3 (detail-json 7 フィールド欠落) | ピンカードカスタマイズ機能 18 項目中 7 項目失敗 | DetailRow 拡張 + DB select 拡張 + json! 拡張 | 1 時間 |
| 4 | 🟡 | jobmap Mismatch #2 (`flows` 永久空) | サンキー線描画なし | 要件確認後どちらかに統一 (UI から削除 or backend 復活) | 要件確認後 |
| 5 | 🟡 | dead route 6 件 (overview/balance/workstyle/demographics/trend/insight) | コード重複・テストコスト | UI 復活させるか、route 削除 + テンプレ削除 | 1 日 |
| 6 | 🟡 | CTAS fallback 14 箇所 | 5/1 後の戻し忘れリスク | `flow_ctas_restore.md` 通り 5/1 後に置換 + 検証 SQL 実行 | 5/1 |
| 7 | 🟡 | `bug_marker_*_MISSING` テストが `#[ignore]` で気づきにくい | 修正されないまま放置リスク | 修正完了後 `#[ignore]` 外しを workflow に組込 | 1〜2 と同時 |
| 8 | 🟢 | center 形式 object/array 不統一 (Mismatch #5) | 新規 consumer 誤読リスク | OpenAPI 文書化 + 一形式 (推奨: object) に統一 | 1 週間 |
| 9 | 🟢 | タブ間遷移リンク不在 | UX 動線断絶 | jobmap → company / region_karte → recruitment_diag リンク追加 | 設計検討要 |
| 10 | 🟢 | Turso None と「データなし」の表示が同じ | サポート問合せ増 | `meta.data_source_status` フィールド + UI 警告 | 1 週間 |

---

## 残課題 (今回監査では深掘り未実施)

1. **Turso 側の INDEX 設計**: Rust コードからは見えない。Python 側 ETL の `CREATE INDEX` 文を確認し、`v2_flow_mesh1km_YYYY` の `(month, dayflag, timezone, mesh1kmid)` インデックスが効いているか要確認。
2. **labor_flow の N+1 リスク**: `competitors.rs:139` `industry_mapping` 引きと `salesnow_companies` 引きの合計 SQL 発行回数を計測 (本監査では行数のみ確認、実行計画は未取得)。
3. **`/api/insight/report` の用途**: contract_audit_2026_04_23.md で「frontend consumer なし」とされたが、PDF/XLSX エクスポート (`/report/insight`, `/api/insight/report/xlsx`) との関係要確認。
4. **company_geo_cache 無効化の影響**: 起動時 cache 廃止 → リクエスト時 Turso クエリ。レイテンシ影響と Turso 読込みクォータ消費の計測未実施。
5. **`/api/v1/*` 外部公開 API のスコープ**: `api_v1.rs` 全体の契約テスト未整備 (本監査スコープ外と明記されているが、外部利用者向けには重要)。

---

## 監査ファクト一覧 (file:line)

主要な確認ポイントを再掲:

- ルーター: `src/lib.rs:63-340` (protected_routes 全 70+ ルート)
- AppState: `src/lib.rs:42-54`
- 9 タブ UI: `templates/dashboard_inline.html:71-87`
- Mismatch #1: `src/handlers/jobmap/handlers.rs:398-403`
- Mismatch #3: `src/handlers/jobmap/handlers.rs:267-287`
- Mismatch #4: `src/handlers/jobmap/company_markers.rs:128`
- CTAS fallback: `src/handlers/jobmap/flow.rs:88,112,137,163,196,213,229,238,266,281` + `src/handlers/insight/flow_context.rs:51,138,208`
- bug marker: `src/handlers/global_contract_audit_test.rs:438-468`
- AppCache TTL/max: `src/db/cache.rs:20-27` (`config.cache_ttl_secs`, `config.cache_max_entries`)
- Turso graceful degradation: `src/main.rs:82-145`
- company_geo_cache 無効化: `src/main.rs:155-158`
- Audit purge スケジューラ: `src/main.rs:222-242`
