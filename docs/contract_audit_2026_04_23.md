# Team δ: 全タブ Frontend ⇔ Backend 契約整合性監査

**監査日**: 2026-04-23
**監査者**: Quality Engineer (Team δ)
**対象**: V2 HW Dashboard 全タブ（採用診断を除く）
**背景**: 2026-04-23 採用診断8パネル全滅事故の再発防止。親セッションで
`src/handlers/recruitment_diag/contract_tests.rs` は整備済みだが、他タブでも
同種の契約ミスマッチが潜んでいないか網羅監査する。

---

## 1. 監査手順（Method）

1. **Backend response shape**: 各ハンドラが返す `Json(json!({...}))` の top-level
   キーおよび nested キーを grep で列挙。
2. **Frontend access pattern**: 対応する template/JS の `data.X`, `d.X.Y`,
   `response.X` 参照を grep で列挙。
3. **Cross-check**: 各 endpoint について両者を突き合わせ、不一致を検出。
4. **L5 逆証明**: 主要 endpoint を tempfile hw_db + minimal state で実際に呼び出し、
   レスポンス形状が frontend 期待と一致するかアサート（新規
   `src/handlers/global_contract_audit_test.rs`）。

---

## 2. タブ別 契約整合性サマリ

| タブ | エンドポイント種別 | 主な戻り値型 | JSON 契約リスク | 不一致件数 |
|------|-------------------|--------------|-----------------|-----------|
| 市場概況 (market) | HTML partial | `Html<String>` | **なし**（ECharts config を `data-chart-config` 属性に埋め込み、app.js が直接 `setOption` する方式） | 0 |
| 詳細分析 (insight) | HTML partial + 1 JSON | 主に `Html<String>` | **低**（`/api/insight/report` JSON は現時点で frontend consumer なし） | 0 |
| 求人検索 (competitive) | HTML partial | `Html<String>` | **なし** | 0 |
| 条件診断 (diagnostic) | HTML partial | `Html<String>` | **なし** | 0 |
| 企業検索 (company) | HTML partial + CSV | `Html<String>` / `Response` | **なし** | 0 |
| 媒体分析 (survey) | HTML partial + 1 スタブ JSON | 主に `Html<String>` | **低**（`/api/survey/report` JSON は `{"status":"upload_csv_first"}` スタブ、frontend consumer なし） | 0 |
| 地域カルテ (region_karte) | HTML partial + 1 JSON endpoint | 主に `Html<String>` | **低**（`/api/region/karte/{citycode}` JSON endpoint は現時点で frontend consumer なし） | 0 |
| 地図 (jobmap) | JSON 多数 + HTML partial | `Json<Value>` / `Html<String>` | **高**（10+ JSON endpoints, 複数 consumer） | **5 件**（下記詳細） |

**結論**: 採用診断以外で JSON 契約ミスマッチが発生しているのは **jobmap タブのみ**。
他タブは HTML partial ＋ `data-chart-config` パターン（app.js が自動 `setOption`）のため、
構造上 key ミスマッチが発生しない設計。

---

## 3. JSON エンドポイント別 契約表（jobmap）

### 3.1 `/api/jobmap/heatmap`（`jobmap/heatmap.rs`）

| key | Backend 返却 | Frontend 参照 | 一致 |
|-----|-------------|--------------|------|
| `error` | ✓ (エラー時のみ) | ✓ (`data.error`) | ✅ |
| `points[]` | ✓ (`{lat,lng,population}`) | ✓ (`data.points`, `p.lat/p.lng/p.population`) | ✅ |
| `data_count` | ✓ | ✓ | ✅ |
| `max` | ✓ | ✓ (`data.max`) | ✅ |
| `meta.{granularity,aggregate_mode,covid_notice,data_source,data_period}` | ✓ | ✓ | ✅ |
| `truncated` / `row_limit` | ✓ | ✓ | ✅ |

### 3.2 `/api/jobmap/inflow`（`jobmap/inflow.rs`）

| key | Backend | Frontend | 一致 |
|-----|--------|---------|------|
| `error` | ✓ | ✓ | ✅ |
| `sankey.nodes[]` / `sankey.links[]` | ✓ | ✓ | ✅ |
| `summary[]` (`area_name`, `population`, `share`) | ✓ | ✓ (`s.area_name, s.population, s.share`) | ✅ |
| `year`, `month`, `total_population`, `data_warning` | ✓ | ✓ | ✅ |

### 3.3 `/api/jobmap/correlation`（`jobmap/correlation.rs`）

| key | Backend | Frontend | 一致 |
|-----|--------|---------|------|
| `error` | ✓ | ✓ | ✅ |
| `correlation.{r,n,note}` | ✓ | ✓ (`c.r, c.n, c.note`) | ✅ |
| `points[]` (`mesh,lat,lng,population,job_count,z_pop,z_job,category`) | ✓ | ✓ (`p.population,p.job_count,p.mesh,p.category`) | ✅ |
| `outliers.hiring_hard[]` / `outliers.underserved[]` | ✓ | ✓ (`it.mesh, it.job_count, it.population`) | ✅ |

### 3.4 `/api/jobmap/markers`（`jobmap/handlers.rs::jobmap_markers` → `markers_to_json`）

| key | Backend | Frontend (postingmap.js) | 一致 |
|-----|--------|----------|------|
| `markers[]` (MarkerRow: `id,lat,lng,facility,jobType,emp,salaryType,salaryMin,salaryMax`) | ✓ (serde rename) | ✓ | ✅ |
| `total` / `totalAvailable` | ✓ | ✓ (`i.totalAvailable`) | ✅ |
| `center` | **`{lat,lng}` オブジェクト形式** (line 597) | `i.center.lat, i.center.lng` として使用 | ✅（オブジェクト同士一致） |

### 3.5 `/api/jobmap/choropleth`（`jobmap/handlers.rs::jobmap_choropleth`）

| key | Backend | Frontend (choropleth_overlay.js) | 一致 |
|-----|--------|----------|------|
| `choropleth{}` / `legend[]` / `geojsonUrl` | ✓ | ✓ | ✅ |
| `center` | **`[lat, lng]` 配列形式** (line 981) | `apiCenter.length === 2` で分岐後 `map.setView(apiCenter,...)` | ✅（配列同士一致） |

### 3.6 `/api/jobmap/seekers`（`jobmap/handlers.rs::jobmap_seekers`）

| key | Backend | Frontend (postingmap.js `j()`) | 一致 |
|-----|--------|----------|------|
| `markers[]` (`municipality,lat,lng,count`) | ✓ | **`e.name` を参照** (backend は `municipality`) | **❌ Mismatch #1** |
| `choropleth{}` | ✓ | ✓ | ✅ |
| `flows[]` | **未実装** (キー自体返していない) | `a.flows \|\| []` で defensive 読み | **⚠ Mismatch #2** (silent empty) |
| `total` / `message` | ✓ | - | ✅ |
| `center` | **`{lat,lng}` オブジェクト** (line 460) | `a.center.lat, a.center.lng` | ✅ |

### 3.7 `/api/jobmap/detail-json/{id}`（`jobmap/handlers.rs::jobmap_detail_json`）

| Backend 返却 key | Frontend (postingmap.js `pinCard`) 参照 | 一致 |
|------------------|----------------------------------------|------|
| `facility_name, access, employment_type, salary_type, salary_min, salary_max, headline, job_description, requirements, benefits, working_hours, holidays, tier3_label_short, job_number, hello_work_office, recruitment_reason` | 同名 key を使用 | ✅ |
| (未返却) | `n.service_type` | **❌ Mismatch #3** |
| (未返却) | `n.salary_detail` | **❌ Mismatch #3** |
| (未返却) | `n.education_training` | **❌ Mismatch #3** |
| (未返却) | `n.special_holidays` | **❌ Mismatch #3** |
| (未返却) | `n.tags` | **❌ Mismatch #3** |
| (未返却) | `n.geocode_confidence`, `n.geocode_level` | **❌ Mismatch #3** |

### 3.8 `/api/jobmap/stats`（`jobmap/handlers.rs::jobmap_stats` → `stats::StatsResult`）

| Backend | Frontend | 一致 |
|--------|---------|------|
| `count, min_avg, min_median, min_mode, max_avg, max_median, max_mode` | 全 key 参照 | ✅ |

### 3.9 `/api/jobmap/labor-flow`（`jobmap/company_markers.rs::labor_flow`）

| key | Backend | Frontend (laborflow.js) | 一致 |
|-----|--------|----------|------|
| `error` / `industries[]` | ✓ | ✓ (`data.error, data.industries`) | ✅ |
| `industries[].sn_industry, companies, total_emp, net_change_1y, net_change_3m, avg_delta_1y` | ✓ | ✓ | ✅ |
| `prefecture` | ✓ | ✓ (`data.prefecture`) | ✅ |
| **市区町村** | `"location"` として返却 | **`data.municipality` を参照** | **❌ Mismatch #4** |
| `total_industries` | ✓ | - | ✅ |

### 3.10 `/api/jobmap/company-markers`（`jobmap/company_markers.rs::company_markers`）

| key | Backend | Frontend (companymap.js) | 一致 |
|-----|--------|----------|------|
| `markers[]` | ✓ | ✓ (`data.markers`) | ✅ |
| `total` | ✓ | ✓ (`data.total`) | ✅ |
| `zoom_required` | ✓ | ✓ (`data.zoom_required`) | ✅ |
| `shown` / `error` | ✓ | - | ✅ |

---

## 4. 発見した契約ミスマッチ（優先度高 → 低）

### 🟡 Mismatch #1: `/api/jobmap/seekers` マーカー名キー不一致

- **場所**: `src/handlers/jobmap/handlers.rs:399`（backend）／`static/js/postingmap.js` の `j()` 内 `e.name+": "+e.count+"人"` 箇所
- **症状**: 市区町村別求職者バブルのツールチップが `undefined: 0人` として表示される（実データがある場合でも）。
- **原因**: Backend が `municipality` フィールドで名前を返しているが、Frontend は `e.name` を期待。
- **影響度**: **中**。ツールチップのみが崩れるがマーカー自体は描画される。
- **修正候補**:
  1. Backend を `"name": m_name` に変更（もしくは両方返す）
  2. Frontend を `e.municipality` に変更
- **推奨**: backend に `"name"` を追加（既存呼び出し元への影響最小）。

### 🟡 Mismatch #2: `/api/jobmap/seekers` `flows` キー未実装

- **場所**: `src/handlers/jobmap/handlers.rs::jobmap_seekers`（backend）／`static/js/postingmap.js` の `j()` 内
- **症状**: Frontend が `a.flows \|\| []` と defensive に読むため**エラーは発生しないが、フロー線が永久に描画されない**。silent empty。
- **原因**: talentmap モジュール削除時に seekers ハンドラが簡素化され、`flows` が返却されなくなった。
- **影響度**: **中**（UI 機能の欠落、事故ではないが UX 劣化）。
- **確認項目**: これが**意図的な削除**か**未完の移植**かを確認すること。意図的なら postingmap.js から flow 描画コードを削除すべき。未完なら backend に戻す。
- **推奨**: まず要件確認、その後いずれかに統一。

### 🟡 Mismatch #3: `/api/jobmap/detail-json/{id}` ピンカード 7 フィールド欠落

- **場所**: `src/handlers/jobmap/handlers.rs:267-287`（backend）／`static/js/postingmap.js::pinCard` 内
- **Frontend が期待する未返却フィールド**:
  - `service_type`（サービス種別）
  - `salary_detail`（給与詳細）
  - `education_training`（教育研修）
  - `special_holidays`（特別休暇）
  - `tags`（タグ）
  - `geocode_confidence`, `geocode_level`（ジオコード精度）
- **症状**: ピンカードでこれらのトグル ON でも**常に表示されない**（falsy なので条件分岐が常に false）。
- **原因**: `jobmap_detail_json` の返却オブジェクトがピンカード UI の要件を満たしていない。DetailRow 構造体自体は一部フィールドを持っていない可能性あり（要確認）。
- **影響度**: **中〜高**（ピンカードカスタマイズ機能が 18 項目中 7 項目機能しない）。
- **修正候補**: DetailRow 構造体に該当カラムを追加 + DB select に含める + `jobmap_detail_json` のレスポンスに追加。

### 🟡 Mismatch #4: `/api/jobmap/labor-flow` `municipality` キー不一致

- **場所**: `src/handlers/jobmap/company_markers.rs:128`（backend）／`static/js/laborflow.js:226` 付近
- **症状**: 市区町村が指定されていても `data.municipality` が undefined となり、
  テーブル行クリック時の `loadIndustryCompanies('pref', '', 'industry')` が
  空文字で呼び出される → 産業詳細絞り込みが市区町村で機能しない可能性。
- **原因**: Backend が `"location"` キーで `"{pref} {muni}"` 結合文字列を返しているが、
  Frontend は `municipality` 単独値を期待。
- **影響度**: **中**（市区町村絞り込み時の産業ドリルダウンが都道府県全体にフォールバック）。
- **推奨**: Backend に `"municipality": muni` を追加する（既存 `location` は維持しても破壊的変更を起こさない）。

### 🔵 Mismatch #5（観察のみ）: `center` の形式が endpoint により object / array の 2 種類存在

- **場所**: 
  - `handlers.rs:460` (`jobmap_seekers`): `{"lat": ..., "lng": ...}` object
  - `handlers.rs:597` (`markers_to_json`): `{"lat": ..., "lng": ...}` object
  - `handlers.rs:981` (`choropleth`): `[lat, lng]` array
- **症状**: Frontend も consumer ごとに使い分け（choropleth_overlay.js は array として、postingmap.js は object として）正常動作中。
- **影響度**: **低**（現時点で不一致は起きていないが、新規 consumer がどちらの形式を取るか誤るとバグになる）。
- **推奨**: 将来的に形式を統一する。OpenAPI 文書化または型定義を検討。

---

## 5. L5 逆証明（新規テスト）

以下 3 endpoint について tempfile hw_db + minimal AppState で実際にハンドラを呼び、
レスポンス JSON の top-level key を frontend 期待キーと突き合わせる。

| Endpoint | Panel | Test Name | 検証キー |
|----------|-------|-----------|----------|
| `/api/jobmap/heatmap` | 地図 - ヒートマップ | `jobmap_heatmap_contract` | `points, data_count, max, meta, truncated, row_limit` |
| `/api/jobmap/inflow` | 地図 - 流入サンキー | `jobmap_inflow_contract` | `sankey.{nodes,links}, summary, total_population, data_warning` (error 時: `error`) |
| `/api/jobmap/correlation` | 地図 - 相関散布図 | `jobmap_correlation_contract` | `correlation.{r,n,note}, outliers.{hiring_hard,underserved}, points` (error 時: `error`) |

加えて、**発見済みミスマッチ #1/#4 を FAILED テストとして残す**ことで、
バグが修正されるまで CI に見える形で残す:

| Test Name | 目的 | 期待状態 |
|-----------|------|---------|
| `jobmap_seekers_marker_name_key_MISSING_bug_marker` | Mismatch #1 の記録（`name` キー不在を検出） | **FAILED（現状）** |
| `jobmap_labor_flow_municipality_key_MISSING_bug_marker` | Mismatch #4 の記録（`municipality` キー不在を検出） | **FAILED（現状、salesnow_db 未接続時はスキップ）** |

ただし、CI を不必要に赤くしないため実装では `#[ignore]` 属性を付与し、
`cargo test -- --ignored` でのみ実行させる運用とする。修正完了後に属性を外す。

---

## 6. 残課題（次ステップ）

1. **Mismatch #1, #4 修正**: backend に key を追加（破壊的変更なし）。
2. **Mismatch #2 の要件確認**: flows 機能を戻すか UI 削除するか判定。
3. **Mismatch #3 修正**: DetailRow 拡張 + detail_json レスポンス拡張。
4. **型定義統一**: `center` 形式（object vs array）を統一して OpenAPI に明記。
5. **採用診断と同レベルの契約テスト整備**: jobmap の主要 10 endpoint を
   `src/handlers/jobmap/contract_tests.rs` として整備（本監査では 3 endpoint のみ）。
6. **定期監査**: agent による並列実装時は実装直後に本監査を実施することを
   `feedback_agent_contract_verification.md` に追記（MEMORY）。

---

## 付録 A: 監査対象外（既存 contract_tests.rs でカバー済み）

- `src/handlers/recruitment_diag/contract_tests.rs`: Panel 1-8 の 8 endpoint を
  tempfile hw_db で実ハンドラ呼出しテスト済み。2026-04-23 事故の直接対応。

## 付録 B: 監査対象外（優先度低）

- `/admin/*` 管理画面エンドポイント（管理者のみアクセス、ユーザー影響最小）
- `/my/*` 自己サービスエンドポイント（ログイン済ユーザー個別データ）
- `/api/v1/*` 外部公開 API（OpenAPI 文書化済み、本件スコープ外）
