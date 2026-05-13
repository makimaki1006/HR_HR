# E. 並行性監査レポート

**監査日**: 2026-05-13
**対象**: hellowork-deploy / crate `rust_dashboard`
**スコープ**: `src/main.rs`, `src/lib.rs`, `src/db/`, `src/handlers/`, `src/auth/`, `src/audit/`
**監査方法**: read-only (build/test 実行せず)

---

## サマリー (要約)

全体として並行性設計は概ね健全。`AppState` は `Arc` で共有され `Mutex` 等の競合資源は最小限 (RateLimiter のみ)。`tokio::spawn` も 1 箇所 (audit purge) のみで、`tokio::sync::Mutex/RwLock` の濫用は無く、ロック順序問題やデッドロックの構造的リスクは見当たらない。

最大の問題は **blocking I/O (rusqlite + reqwest::blocking) を async fn から `spawn_blocking` 経由せず直接呼ぶ箇所が複数存在**すること。とくに `TursoDb` は `reqwest::blocking::Client` を保持し、HTTP RTT ~数百 ms を tokio worker thread 上でブロックする。同時アクセスが worker 数 (Render 無料プランで CPU=0.1 vCPU の場合 1〜2) を超えると **全 request が hang** する致命的リスクがある。

最重要対応:
- **P0-1**: `admin_*`, `my_profile_*` handler が `audit.turso()` 経由で blocking HTTP を直接実行 (5 ファイル, 10 箇所超)。
- **P0-2**: ログイン処理 (`lib.rs`) で `audit::log_failed_login` / `record_event` が sync で実行され、ログイン RPS が tokio worker 数で頭打ち。
- **P1**: `AppCache::get` の `drop(entry) → remove(key)` パターンに TOCTOU race (期限切れ判定→他スレッドが再 insert→誤って削除) がある。
- **P2**: `RateLimiter` 内 `record_failure` は read-modify-write が atomic だが、`is_allowed` と `record_failure` 間に race window が存在 (max_attempts 境界で 1〜2 回多く許される)。

---

## 検出された問題

### [P0-1] 管理画面 / マイページ handler が blocking HTTP を async fn 内で直接実行

**ファイル**:
- `src/handlers/admin/handlers.rs:23, 39, 42, 43, 55`
- `src/handlers/my/handlers.rs:29, 58, 72`

**証拠** (`admin/handlers.rs:16-25`):
```rust
pub async fn admin_users_list(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    let Some(audit) = &state.audit else { ... };
    let accounts = dao::list_accounts(audit.turso(), 500);  // ← blocking reqwest::Client::post
    Html(render::users_list_page(&accounts))
}
```

`dao::list_accounts` は `TursoDb::query` → `reqwest::blocking::Client::post()` → `.send()` を実行。これは tokio worker thread を完全に block する。HTTP RTT 200〜500ms × 同時アクセス数だけ worker が占有される。

**発火条件**: `/admin/users` 等への同時アクセスが tokio worker 数 (Render 無料: 1〜2) を超えた瞬間。
**影響**: 全 endpoint hang (login も含む)。
**修正**: `tokio::task::spawn_blocking(move || dao::list_accounts(...)).await` で包む。

### [P0-2] ログインフロー内の audit 書き込みが sync blocking

**ファイル**: `src/lib.rs:584-594, 622-632, 644 付近` および `src/audit/mod.rs:73-101 record_event`

**証拠** (`audit/mod.rs:92-100`):
```rust
pub async fn record_event(...) {
    ...
    dao::insert_activity(audit, &account_id, &sid, ...); // sync blocking HTTP
}
```

`record_event` は `async fn` だが内部の `dao::insert_activity` (turso_http POST) を spawn_blocking 無しで呼ぶ。`lib.rs:584` の `log_failed_login` も同様。

**発火条件**: ログイン失敗/成功時。ブルートフォース攻撃時に各試行で worker が占有 → 認証エンドポイント自体が DoS 状態に。
**影響**: 認証経路の latency 倍増 + 高負荷時に全 worker 枯渇。
**修正**: `record_event` 内部の dao 呼び出し、および `lib.rs` の `audit::log_failed_login` 呼び出しを `spawn_blocking` でラップ。あるいは `mpsc::UnboundedSender` で fire-and-forget の audit writer task に送る (応答 latency へ影響させない)。

### [P0-3] `overview.rs:1073` `build_population_context` の Turso 呼び出し経路要確認

**ファイル**: `src/handlers/overview.rs:1073`

`build_population_context` は sync fn として実装されており、`turso.query()` を直接呼ぶ。本 fn は overview tab handler 経由で呼ばれるが、`tab_overview` の async/spawn_blocking 設計を全行追跡できなかったため確証は得られていない (要追加調査)。もし render パイプライン (例: `render_subtab_*`) と同様に spawn_blocking 内なら問題なし、そうでなければ P0。

**確認手順**: `rg "build_population_context|build_balance_html" src/` で呼び出し階層を辿り、最寄りの `.await` 境界の手前に spawn_blocking があることを確認。

### [P1-1] `AppCache::get` の TOCTOU race

**ファイル**: `src/db/cache.rs:30-40`

**証拠**:
```rust
pub fn get(&self, key: &str) -> Option<Value> {
    if let Some(entry) = self.map.get(key) {
        if Instant::now() < entry.expires_at {
            return Some(entry.data.clone());
        }
        drop(entry);
        self.map.remove(key);  // ← この瞬間に他スレッドが set した新値を消す可能性
    }
    None
}
```

`drop(entry)` と `remove(key)` の間で他スレッドが同じキーに新しい値を `set` するとそれが消える。実害は「キャッシュミス 1 回」なので軽微だが、cache stampede を悪化させる可能性。

**修正**: `remove_if` ベースで「期限切れの場合のみ削除」する。DashMap は `remove_if` を提供しないので、`entry().and_modify()` または時刻ベースのキー検証で対処。

### [P1-2] `AppCache::evict_expired` 時の max_entries 超過

**ファイル**: `src/db/cache.rs:43-56`

`set` 内で `len() >= max_entries` の場合のみ `evict_expired` を呼ぶが、期限内エントリしか無い場合 evict 0 で素通り → max_entries を超えてメモリ使用量無制限化のリスク。Render 512MB プランで OOM 既往あり (main.rs:144 コメント参照)。

**修正**: evict_expired で 0 件削除なら LRU 風に古いものを強制削除する fallback を追加。

### [P2-1] `RateLimiter::is_allowed` と `record_failure` の race

**ファイル**: `src/auth/session.rs:27-37, 40-60`

`is_allowed` で lock を取って判定後 release → その後 `record_failure` で再 lock → count++。この間に複数 IP からの並列リクエストが check-then-act の race を起こし、max_attempts より 1〜2 回多く許容される。セキュリティ的影響は最小 (10000 試行が 10002 になる程度) なので P2。

**修正**: `is_allowed_and_record_attempt(&str) -> bool` で 1 lock 内で判定+カウントを atomic に。

### [P2-2] `reqwest::blocking::Client` のコネクション枯渇

**ファイル**: `src/db/turso_http.rs:32-35`

`reqwest::blocking::Client` はデフォルトで connection pool を持つが、`pool_max_idle_per_host` のデフォルト (usize::MAX) と timeout 30s のみ設定。Turso 側がスロットルした際に 30s 全 worker 占有 → cascade failure に。

**修正**: `pool_max_idle_per_host(8)` 等で上限化し、より短い `connect_timeout` を別途設定。

### [P2-3] r2d2 SQLite pool の max_size=10 + 同時 spawn_blocking 数

**ファイル**: `src/db/local_sqlite.rs:39-43`

r2d2 max_size=10、tokio default blocking thread = 512。SQLite handler が 10 個並列に走った後、11 個目以降は pool 待ち (`Pool::get()` で同期 block) → spawn_blocking thread を 502 個まで占有可能 → メモリ圧迫。

実害は限定的だが、`Pool::builder().connection_timeout(Duration::from_secs(5))` を設定すべき。

---

## 健全な点 (記録)

- `Arc<AppState>` で全 handler に共有、内部に `Mutex` 等の競合資源を最小化
- `tokio::spawn` は audit purge 1 箇所のみ。spawn した task の panic は `Result::Err(JoinError)` で catch されている (main.rs:223-227)
- `LocalDb` `TursoDb` は両方 `Clone` 可能で内部 `Arc` 共有 → handler 間で安全に share
- `spawn_blocking` 内で `db.clone()` してから move する正しいパターンが多数 (api.rs, balance.rs, analysis/handlers.rs 等)
- async fn 内で `std::sync::Mutex` guard を `.await` 越えで保持する箇所は検出されず
- Arc 循環参照リスクなし (内部参照は全て一方向 AppState→DB)

---

## 推奨対応優先度

| ID | 領域 | 優先度 | 想定工数 |
|----|------|--------|---------|
| P0-1 | admin/my handler を spawn_blocking 化 | 即時 | 30 min |
| P0-2 | audit::record_event / log_failed_login を spawn_blocking 化 (or fire-and-forget channel) | 即時 | 1 h |
| P0-3 | overview / recruitment_diag / competitors 経路の spawn_blocking 監査 | 即時 | 1 h |
| P1-1 | AppCache TOCTOU 修正 | 1 週間 | 30 min |
| P1-2 | AppCache max_entries 強制 evict | 1 週間 | 20 min |
| P2-1 | RateLimiter 1-lock 化 | 任意 | 15 min |
| P2-2 | reqwest pool_max_idle_per_host | 任意 | 5 min |
| P2-3 | r2d2 connection_timeout | 任意 | 5 min |

---

## 注意事項

- 本監査は build/test を実行しておらず、動的挙動 (実際の deadlock 再現等) は未確認
- P0-3 は呼び出し階層全追跡を実施しておらず、確証なし。次フェーズで詳細追跡推奨
- handlers ディレクトリは 70+ ファイルあり、全数監査ではなく代表サンプリング
