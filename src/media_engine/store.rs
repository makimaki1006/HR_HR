//! SerpApi の月次カウンタとレスポンスキャッシュの永続層 (2026-07-24 HR_HR 統合)。
//!
//! 移植元はローカルファイル (`../data/serpapi_cache/`) だったが、HR_HR の本番
//! (Render 無料プラン) はディスクが非永続で、デプロイごとにカウンタが消えて
//! 月240枠の管理が壊れる。そこで **Turso を一次ストア**にし、ローカルファイルは
//! 開発時の Python 実装とのキャッシュ共有用の副次ストアとして残す (ユーザー決定)。
//!
//! - 読み: ローカルファイル → Turso の順 (ローカル開発ではファイル共有が効く)
//! - 書き: 両方へ (ファイル書き込みは best-effort、失敗は無視)
//! - カウンタ: Turso が正。Turso 未接続時のみファイルへフォールバック
//!
//! テーブル (初回使用時に CREATE TABLE IF NOT EXISTS):
//! - `jme_serpapi_quota(month TEXT PRIMARY KEY, used INTEGER NOT NULL)`
//! - `jme_serpapi_cache(cache_key TEXT PRIMARY KEY, payload TEXT NOT NULL, created_at TEXT)`

use crate::db::turso_http::TursoDb;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

static TABLES_READY: AtomicBool = AtomicBool::new(false);

/// 現在の月キー ("YYYY-MM")。
fn current_month() -> String {
    chrono::Local::now().format("%Y-%m").to_string()
}

fn monthly_cap() -> i64 {
    std::env::var("SERPAPI_MONTHLY_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(240)
}

/// ローカルキャッシュディレクトリ (存在する場合のみ副次ストアとして使う)。
fn local_cache_dir() -> Option<std::path::PathBuf> {
    let p = match std::env::var("SERPAPI_CACHE_DIR") {
        Ok(p) if !p.is_empty() => std::path::PathBuf::from(p),
        _ => std::path::PathBuf::from("../data/serpapi_cache"),
    };
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

fn ensure_tables(turso: &TursoDb) {
    if TABLES_READY.load(Ordering::Relaxed) {
        return;
    }
    let q = turso.execute(
        "CREATE TABLE IF NOT EXISTS jme_serpapi_quota (month TEXT PRIMARY KEY, used INTEGER NOT NULL DEFAULT 0)",
        &[],
    );
    let c = turso.execute(
        "CREATE TABLE IF NOT EXISTS jme_serpapi_cache (cache_key TEXT PRIMARY KEY, payload TEXT NOT NULL, created_at TEXT)",
        &[],
    );
    if q.is_ok() && c.is_ok() {
        TABLES_READY.store(true, Ordering::Relaxed);
    } else {
        tracing::warn!("media_engine store: テーブル作成に失敗 (quota={:?}, cache={:?})", q.err(), c.err());
    }
}

/// カウンタに n 件加算する (Turso 一次。失敗しても本体処理は成功のまま)。
pub async fn quota_increment(turso: Option<TursoDb>, n: i64) {
    if n <= 0 {
        return;
    }
    if let Some(db) = turso {
        let month = current_month();
        let _ = tokio::task::spawn_blocking(move || {
            ensure_tables(&db);
            if let Err(e) = db.execute(
                "INSERT INTO jme_serpapi_quota (month, used) VALUES (?, ?) \
                 ON CONFLICT(month) DO UPDATE SET used = used + excluded.used",
                &[&month, &n],
            ) {
                tracing::warn!("media_engine store: quota_increment 失敗: {e}");
            }
        })
        .await;
        return;
    }
    // Turso 未接続 (ローカル開発等): 移植元と同じファイルカウンタへ
    if let Some(dir) = local_cache_dir() {
        let path = dir.join("_monthly_counter.json");
        let cur = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        if let Some(month) = cur.as_ref().and_then(|v| v.get("month")).and_then(Value::as_str) {
            let count = cur
                .as_ref()
                .and_then(|v| v.get("count"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                + n;
            let next = json!({"month": month, "count": count});
            let _ = std::fs::write(&path, serde_json::to_string(&next).unwrap_or_default() + "\n");
        }
    }
}

/// 現在の消費状況を読む ({used, cap, remaining, month})。取得不能は Null。
pub async fn quota_read(turso: Option<TursoDb>) -> Value {
    let cap = monthly_cap();
    if let Some(db) = turso {
        let month = current_month();
        let month_for_out = month.clone();
        let used = tokio::task::spawn_blocking(move || -> Option<i64> {
            ensure_tables(&db);
            db.query("SELECT used FROM jme_serpapi_quota WHERE month = ?", &[&month])
                .ok()
                .and_then(|rows| rows.first().and_then(|r| r.get("used")).and_then(Value::as_i64))
        })
        .await
        .ok()
        .flatten()
        .unwrap_or(0);
        return json!({"used": used, "cap": cap, "remaining": (cap - used).max(0), "month": month_for_out});
    }
    if let Some(dir) = local_cache_dir() {
        let path = dir.join("_monthly_counter.json");
        let cur = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        if let Some(month) = cur.as_ref().and_then(|v| v.get("month")).and_then(Value::as_str) {
            let count = cur
                .as_ref()
                .and_then(|v| v.get("count"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            return json!({"used": count, "cap": cap, "remaining": (cap - count).max(0), "month": month});
        }
    }
    Value::Null
}

/// キャッシュ取得。ローカルファイル → Turso の順。
pub async fn cache_get(turso: Option<TursoDb>, key: &str) -> Option<Value> {
    if let Some(dir) = local_cache_dir() {
        if let Ok(text) = std::fs::read_to_string(dir.join(format!("{key}.json"))) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                return Some(v);
            }
        }
    }
    if let Some(db) = turso {
        let k = key.to_string();
        return tokio::task::spawn_blocking(move || -> Option<Value> {
            ensure_tables(&db);
            db.query(
                "SELECT payload FROM jme_serpapi_cache WHERE cache_key = ?",
                &[&k],
            )
            .ok()
            .and_then(|rows| {
                rows.first()
                    .and_then(|r| r.get("payload"))
                    .and_then(Value::as_str)
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
            })
        })
        .await
        .ok()
        .flatten();
    }
    None
}

/// キャッシュ保存。Turso 一次 + ローカルファイル副次 (両方 best-effort)。
pub async fn cache_put(turso: Option<TursoDb>, key: &str, payload: &Value) {
    if let Some(dir) = local_cache_dir() {
        if let Ok(text) = serde_json::to_string_pretty(payload) {
            let _ = std::fs::write(dir.join(format!("{key}.json")), text + "\n");
        }
    }
    if let Some(db) = turso {
        let k = key.to_string();
        let text = serde_json::to_string(payload).unwrap_or_default();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = tokio::task::spawn_blocking(move || {
            ensure_tables(&db);
            if let Err(e) = db.execute(
                "INSERT INTO jme_serpapi_cache (cache_key, payload, created_at) VALUES (?, ?, ?) \
                 ON CONFLICT(cache_key) DO UPDATE SET payload = excluded.payload, created_at = excluded.created_at",
                &[&k, &text, &now],
            ) {
                tracing::warn!("media_engine store: cache_put 失敗: {e}");
            }
        })
        .await;
    }
}
