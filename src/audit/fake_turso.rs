//! SQLite で裏打ちした偽 Turso (テスト専用)。
//!
//! `src/auth/login_flow_tests.rs` の偽 Turso は SQL 文字列で応答を切り替えるだけだが、
//! こちらは Turso HTTP Pipeline API (`POST /v2/pipeline`) を受けて **本物の SQLite で SQL を実行**し、
//! 結果を Turso のセル形式 (`{"type":"text","value":...}`) で返す。
//! DAO の SQL (`NULLS LAST`、`GROUP BY ... CASE`、JOIN) がそのまま流れるので、
//! admin / my の JSON API を DAO ごと contract テストできる。
//!
//! 使い方:
//! ```ignore
//! let (audit, conn) = start_sqlite_audit().await;   // スキーマ適用済み
//! conn.lock().unwrap().execute("INSERT INTO accounts ...", []).unwrap();
//! ```

use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::post, Json, Router};
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::{json, Value};

use super::AuditDb;
use crate::db::turso_http::TursoDb;

pub type SharedConn = Arc<Mutex<Connection>>;

fn arg_to_sql(v: &Value) -> SqlValue {
    match v["type"].as_str().unwrap_or("null") {
        "integer" => SqlValue::Integer(
            v["value"]
                .as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0),
        ),
        "float" => SqlValue::Real(v["value"].as_f64().unwrap_or(0.0)),
        "text" => SqlValue::Text(v["value"].as_str().unwrap_or("").to_string()),
        _ => SqlValue::Null,
    }
}

fn cell(v: SqlValue) -> Value {
    match v {
        SqlValue::Null => json!({"type": "null"}),
        SqlValue::Integer(n) => json!({"type": "integer", "value": n.to_string()}),
        SqlValue::Real(f) => json!({"type": "float", "value": f}),
        SqlValue::Text(s) => json!({"type": "text", "value": s}),
        SqlValue::Blob(_) => json!({"type": "null"}),
    }
}

/// 1 リクエスト = 1 statement (TursoDb::execute_pipeline は execute + close の 2 件を送る)。
fn run(conn: &Connection, sql: &str, args: &[SqlValue]) -> Result<Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let params = rusqlite::params_from_iter(args.iter());
    if stmt.readonly() {
        let cols: Vec<Value> = stmt
            .column_names()
            .iter()
            .map(|c| json!({"name": c}))
            .collect();
        let n = cols.len();
        let mut rows = stmt.query(params).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut cells = Vec::with_capacity(n);
            for i in 0..n {
                let v: SqlValue = row.get(i).map_err(|e| e.to_string())?;
                cells.push(cell(v));
            }
            out.push(Value::Array(cells));
        }
        Ok(json!({"cols": cols, "rows": out}))
    } else {
        let affected = stmt.execute(params).map_err(|e| e.to_string())?;
        Ok(json!({"cols": [], "rows": [], "affected_row_count": affected}))
    }
}

async fn pipeline(State(conn): State<SharedConn>, Json(body): Json<Value>) -> Json<Value> {
    let stmt = &body["requests"][0]["stmt"];
    let sql = stmt["sql"].as_str().unwrap_or("");
    let args: Vec<SqlValue> = stmt["args"]
        .as_array()
        .map(|a| a.iter().map(arg_to_sql).collect())
        .unwrap_or_default();
    let result = {
        let c = conn.lock().unwrap();
        run(&c, sql, &args)
    };
    Json(match result {
        Ok(result) => json!({"results": [
            {"type": "ok", "response": {"type": "execute", "result": result}},
            {"type": "ok", "response": {"type": "close"}}
        ]}),
        Err(message) => json!({"results": [
            {"type": "error", "error": {"message": message}},
            {"type": "ok", "response": {"type": "close"}}
        ]}),
    })
}

/// 偽 Turso を 127.0.0.1 の空きポートに立て、監査スキーマ (`ensure_audit_tables`) を
/// **pipeline 経由で**適用した `AuditDb` と、直接 seed するための SQLite 接続を返す。
pub async fn start_sqlite_audit() -> (AuditDb, SharedConn) {
    let conn: SharedConn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/v2/pipeline", post(pipeline))
        .with_state(conn.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    // TursoDb は reqwest::blocking なので spawn_blocking で作る (接続時に SELECT 1 が流れる)
    let turso = tokio::task::spawn_blocking(move || TursoDb::new(&base, "test-token"))
        .await
        .unwrap()
        .expect("偽 Turso に接続できない");
    let for_schema = turso.clone();
    tokio::task::spawn_blocking(move || super::schema::ensure_audit_tables(&for_schema))
        .await
        .unwrap()
        .expect("監査スキーマを適用できない");
    (AuditDb::new(turso, "test-salt".to_string()), conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::dao;

    /// DAO の SQL がそのまま偽 Turso → SQLite で動き、型 (integer / text / null) が往復すること
    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite裏の偽tursoでdaoが往復する() {
        let (audit, conn) = start_sqlite_audit().await;
        conn.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO accounts (id, email, role, first_seen_at, last_login_at, login_count)
                 VALUES ('a1', 'a@f-a-c.co.jp', 'user', '2026-01-01T00:00:00Z', NULL, 3);
                 INSERT INTO accounts (id, email, role, first_seen_at, last_login_at, login_count)
                 VALUES ('a2', 'b@f-a-c.co.jp', 'admin', '2026-01-01T00:00:00Z', '2026-09-01T00:00:00Z', 7);",
            )
            .unwrap();
        let turso = audit.turso().clone();
        let rows = tokio::task::spawn_blocking(move || dao::list_accounts(&turso, 10))
            .await
            .unwrap();
        // NULLS LAST: last_login_at が NULL の a1 は後ろ
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "a2");
        assert_eq!(rows[0].login_count, 7);
        assert_eq!(rows[0].role, "admin");
        assert_eq!(rows[1].id, "a1");
        assert_eq!(rows[1].last_login_at, "", "NULL は空文字で返る");
        assert_eq!(rows[1].display_name, "");

        // 書き込みも pipeline 経由で SQLite に届く
        let turso = audit.turso().clone();
        tokio::task::spawn_blocking(move || dao::update_profile(&turso, "a1", "名前", "会社"))
            .await
            .unwrap()
            .unwrap();
        let name: String = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT display_name FROM accounts WHERE id = 'a1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "名前");

        // SQL エラーは Err で返る (panic しない)
        let turso = audit.turso().clone();
        let err = tokio::task::spawn_blocking(move || turso.execute("SELECT * FROM no_such", &[]))
            .await
            .unwrap();
        assert!(err.is_err(), "{err:?}");
    }
}
