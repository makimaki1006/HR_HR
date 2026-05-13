# I 領域: パフォーマンス監査レポート

**対象**: hellowork-deploy / `src/handlers/`, `src/db/`, `src/lib.rs`
**監査日**: 2026-05-13
**対象環境**: axum + rusqlite + libsql (reqwest::blocking) / Render 無料プラン (cold start ~15-30s)
**手法**: read-only grep 静的解析 (build/bench なし)

---

## サマリー (全体評価)

健全性は中程度〜良好。`spawn_blocking` で同期 DB 呼び出しの runtime 占有は概ね回避され (30 箇所)、上位 HTML パスは `String::with_capacity(N_000)` 事前確保が広く適用、`AppCache` (DashMap, TTL 付き) が 20 ハンドラで利用済み。一方で **(a) loop-内 Turso HTTP 呼び出しによる本物の N+1**、**(b) survey/MI HTML 生成 (`build_market_intelligence_data`) で逐次 5+ 回の blocking Turso fetch**、**(c) 巨大 HTML モジュール (236KB の `report_html/market_intelligence.rs`) の `push_str` 759 回**、**(d) `report_html` build path にキャッシュ未適用** が cold-start タイムアウトの主因候補。Regex は本コードベースで未使用 (依存なし) のため lazy 化機会はゼロ。`OnceLock` は 3 箇所適切に使用。

---

## 発見事項

### P0-PERF-01: 通勤圏ピラミッドの N+1 (Turso HTTP loop)
**file:line**: `src/handlers/analysis/fetch/subtab7_other.rs:118-133`
**snippet**:
```rust
for m in munis {
    let rows = super::subtab5_phase4::fetch_population_pyramid(
        db, turso, &m.prefecture, &m.municipality,
    );
    for row in &rows { ... }
}
```
**問題**: `munis` の各市区町村につき個別 `fetch_population_pyramid` を呼ぶ。Turso 経由なら HTTP RTT が件数倍。通勤圏が 10-20 市区町村に及ぶケースで Render 無料プラン (RTT 100-300ms) では **2-6 秒** の追加遅延。
**実害推定**: `n=15 × 200ms = 3.0s` 追加。同タブが他処理と直列なら 60s タイムアウトに寄与。
**対策**: `WHERE (prefecture, municipality) IN ((...), ...)` で 1 クエリ化、または `municipality_code IN (...)`。

### P0-PERF-02: MarketIntelligence HTML 生成の逐次 fetch 連鎖
**file:line**: `src/handlers/survey/report_html/market_intelligence.rs:102-116`
**snippet**:
```rust
let recruiting_rows = fetch_recruiting_scores_by_municipalities(db, turso, ...);
let living_cost_rows = fetch_living_cost_proxy(db, turso, target_municipalities);
let commute_rows = fetch_commute_flow_summary(db, turso, dest_pref, dest_muni, top_n_inflow);
let occupation_rows = if let Some(first_code) = target_municipalities.first() {
    fetch_occupation_population(db, turso, first_code, "resident", &[])
} else { Vec::new() };
// + 後段で fetch_industry_structure_for_municipalities / fetch_ward_thickness / fetch_code_master / fetch_ward_rankings_by_parent / fetch_occupation_cells
```
**問題**: 8+ 個の Turso HTTP query が直列。`reqwest::blocking` のため `tokio::join!` で並列化不可。各 200-500ms × 8 = **1.6-4s** 確定追加。`spawn_blocking` 内全体が直列実行で本番 Render の cold start タイムアウト主因候補。
**対策**: (1) `rayon::join` / 複数 `spawn_blocking` task を `tokio::try_join!` で束ね並列化。(2) または PDF 生成パスに専用 cache (e.g. cache_key = `mi_{occ}_{munis_hash}`) を導入。survey report_html 経路には現状 `cache.get` 呼び出しゼロ (`Grep cache src/handlers/survey/handlers.rs = 0`)。

### P1-PERF-03: 巨大 HTML モジュールの push_str 集中
**file**: `src/handlers/survey/report_html/market_intelligence.rs` (236KB, 759 push_str)
**file**: `src/handlers/survey/report_html/mod.rs` (179KB, 71 push_str)
**file**: `src/handlers/survey/report_html/market_tightness.rs` (152KB, 105 push_str)
**問題**: 個別の `push_str` が大量 (759 回/ファイル) だが、ルート `with_capacity(64_000)` (`mod.rs:642`) は確保済み。subsection 内部関数 (`render_section_*`) は `&mut String` を受け取り上位の capacity を共有しているか、または `String::new()` で関数内生成し allocate-then-copy が発生している可能性 (テスト用 `String::new()` 多数)。
**実害推定**: capacity が 64KB に固定だが、最終 HTML サイズは MI variant で 200-400KB に達しうる (236KB のソースから推定)。複数回 `realloc` (容量倍々) が走り **メモリコピー累積 ~600KB-1MB**。1リクエスト 30-80ms 寄与。
**対策**: ルート capacity を variant ごとに動的調整 (`MarketIntelligence` で 256_000)。sub-renderer の signature を `&mut String` 一貫化 (現在は混在の可能性)。

### P1-PERF-04: `to_string()` 大量出現 in market_intelligence fetch
**file:line**: `src/handlers/analysis/fetch/market_intelligence.rs` (54 occurrences)
**file:line**: `src/handlers/analysis/fetch/market_intelligence.rs:91,106-107`:
```rust
let params = municipality_codes.iter().map(|s| s.to_string()).collect();
// ...
let mut params: Vec<String> = vec![occupation_group_code.to_string()];
params.extend(municipality_codes.iter().map(|s| s.to_string()));
```
**問題**: `&str` → `String` 変換が `&dyn ToSql` の都合で必要だが、47 都道府県 × 数十市区町村のケースで毎回 allocate。hot path で 100-1000 個の `String` allocate。
**実害推定**: 1リクエスト 1-5ms 寄与 (小〜中)。
**対策**: Turso `ToSqlTurso` trait を `&str` で実装するか、`Cow<'_, str>` でパラメータ受領。

### P1-PERF-05: report_html 経路でキャッシュ未適用
**file**: `src/handlers/survey/` 全域
**問題**: `Grep cache src/handlers/survey/handlers.rs` → 15 件あるが、大半は `to_lowercase()` 等の文字列処理であり、`AppCache::get/set` 呼び出しゼロ。MI variant のレポート HTML 生成は P0-PERF-02 の通り 8+ HTTP query を直列実行し**毎回**走る。同じ (occupation, municipality) ペアの再診断で全再計算。
**実害推定**: 同条件 2 回目以降が 1 回目と同等の数秒待ち。営業ツールとして即時性低下。
**対策**: `state.cache` を `tab_survey_report` 系ハンドラに導入し、cache_key = `survey_mi_{occ}_{prefs}_{munis}_{theme}` で完成 HTML を保存 (TTL 5-15 分)。

### P2-PERF-06: ECharts CDN ロード on 200KB+ HTML
**file:line**: `src/handlers/survey/report_html/mod.rs:655-657`
**snippet**:
```rust
html.push_str(
    "<script src=\"https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js\"></script>\n",
);
```
**問題**: 大きな印刷用 HTML で CDN ロード待ちが PDF 生成タイミングをずらす可能性。本観点 (バックエンド perf) では実害小だが、PDF 生成 (E 領域?) の cold start に寄与し得る。
**対策**: 自前 host or preload hint。範囲外のため記録のみ。

### P2-PERF-07: cache TTL/max_entries の env 経由設定
**file:line**: `src/main.rs:138`
**snippet**:
```rust
let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
```
**問題**: 良好。ただし max_entries 超過時の `evict_expired` が「期限切れのみ削除」で、全件が valid 状態だと退避できず insert 続行 → unbounded growth リスク (DashMap)。
**対策**: max 到達時 `evict_expired` 後も `len >= max_entries` なら LRU 風に 10% drop。現状コード:
```rust
if self.map.len() >= self.max_entries {
    self.evict_expired();
}
self.map.insert(key, ...);  // ← 期限切れがゼロなら無制限拡大
```
(`src/db/cache.rs:43-55`)

### P2-PERF-08: `serde_json::Value` 経由の row 変換
**file:line**: `src/handlers/analysis/fetch/subtab7_other.rs:126-128`, `src/db/turso_http.rs:50-80`
**問題**: Turso 行データ → `HashMap<String, Value>` → 各カラム `as_str()` 抽出のオーバーヘッド。strongly-typed struct (`#[derive(Deserialize)]`) で直接デシリアライズすれば allocate 数を半減可能。
**実害推定**: ホット fetch 1 回あたり 0.5-2ms。
**対策**: 重要 fetch 関数のみ専用 struct 導入。

---

## 良好な点 (cold start 抑止に効いている要素)

- `spawn_blocking` 利用: 30 箇所 (`api.rs`, `market.rs`, `analysis/handlers.rs` 等) で rusqlite/Turso blocking call を runtime から分離済み。tokio worker block を防いでいる。
- `String::with_capacity`: 10+ 箇所で hot HTML path に適用 (`diagnostic.rs:40,180,562,621`, `company/render.rs:49,151,424,1333`, `comparison/render.rs:106`, `analysis/render/subtab7.rs:30`)。
- `AppCache` 普及: 20 ハンドラで `cache.get/set` 利用。`overview`, `market`, `insight`, `trend`, `integrated_report` 等の主要タブはカバー済み。
- `OnceLock` で重い HashMap を遅延 1 回構築: `geo/city_code.rs:13`, `recruitment_diag/insights.rs:288`, `survey/location_parser.rs:105`。
- Turso HTTP timeout 30s 設定済み (`db/turso_http.rs:33`)。Render cold start の hang は防げる。
- Regex 依存なし → loop 内 re-compile 問題は構造的に発生しない。

---

## 推奨アクション (優先順)

1. **P0-PERF-02 を Turso 並列 fetch 化** (cold start 直撃)
2. **P1-PERF-05 で survey/MI report に cache 導入** (2 回目以降を即返却)
3. **P0-PERF-01 の N+1 を WHERE IN にまとめる** (subtab7_other)
4. **P2-PERF-07 cache の unbounded growth ガード**

## 出力済みファイル

- `docs/audit_2026_05_13/I_performance.md` (本ファイル)
