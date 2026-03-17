//! Turso HTTP Pipeline API クライアント
//!
//! LocalDb と同じインターフェース (query, query_scalar) を提供し、
//! 外部統計データ用の Turso DB に接続する。

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Turso HTTP接続（スレッドセーフ、Clone可能）
#[derive(Clone)]
pub struct TursoDb {
    inner: Arc<TursoInner>,
}

struct TursoInner {
    url: String,   // https://xxx.turso.io
    token: String, // Bearer token
    client: reqwest::blocking::Client,
}

impl TursoDb {
    /// Turso DB接続を作成
    pub fn new(url: &str, token: &str) -> Result<Self, String> {
        // libsql:// → https:// 変換
        let url = if url.starts_with("libsql://") {
            url.replace("libsql://", "https://")
        } else {
            url.to_string()
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("HTTP client creation failed: {e}"))?;

        // 接続テスト
        let inner = Arc::new(TursoInner {
            url: url.clone(),
            token: token.to_string(),
            client,
        });

        let db = Self { inner };
        db.query("SELECT 1", &[])?;
        tracing::info!("Turso DB connected: {url}");
        Ok(db)
    }

    /// SQL実行 → Vec<HashMap<String, Value>>
    /// LocalDb::query() と同じシグネチャ
    pub fn query(
        &self,
        sql: &str,
        params: &[&dyn ToSqlTurso],
    ) -> Result<Vec<HashMap<String, Value>>, String> {
        let args: Vec<TursoArg> = params.iter().map(|p| p.to_turso_arg()).collect();
        let result = self.execute_pipeline(sql, &args)?;

        let cols = result
            .get("cols")
            .and_then(|v| v.as_array())
            .ok_or("No cols in response")?;

        let col_names: Vec<String> = cols
            .iter()
            .map(|c| {
                c.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();

        let rows_array = result
            .get("rows")
            .and_then(|v| v.as_array())
            .ok_or("No rows in response")?;

        let mut results = Vec::new();
        for row in rows_array {
            let cells = row.as_array().ok_or("Row is not array")?;
            let mut map = HashMap::new();
            for (i, cell) in cells.iter().enumerate() {
                if i >= col_names.len() {
                    break;
                }
                let val = turso_cell_to_value(cell);
                map.insert(col_names[i].clone(), val);
            }
            results.push(map);
        }

        Ok(results)
    }

    /// スカラー値を取得（i64用の簡易版）
    pub fn query_scalar_i64(
        &self,
        sql: &str,
        params: &[&dyn ToSqlTurso],
    ) -> Result<i64, String> {
        let rows = self.query(sql, params)?;
        if let Some(row) = rows.first() {
            if let Some((_key, val)) = row.iter().next() {
                return val.as_i64().ok_or_else(|| "Not an integer".to_string());
            }
        }
        Err("No rows returned".to_string())
    }

    /// Turso HTTP Pipeline API呼び出し
    fn execute_pipeline(
        &self,
        sql: &str,
        args: &[TursoArg],
    ) -> Result<Value, String> {
        let mut stmt = serde_json::json!({"sql": sql});

        if !args.is_empty() {
            let json_args: Vec<Value> = args
                .iter()
                .map(|a| match a {
                    TursoArg::Null => serde_json::json!({"type": "null"}),
                    TursoArg::Integer(n) => {
                        serde_json::json!({"type": "integer", "value": n.to_string()})
                    }
                    TursoArg::Real(f) => serde_json::json!({"type": "float", "value": f}),
                    TursoArg::Text(s) => serde_json::json!({"type": "text", "value": s}),
                })
                .collect();
            stmt["args"] = Value::Array(json_args);
        }

        let payload = serde_json::json!({
            "requests": [
                {"type": "execute", "stmt": stmt},
                {"type": "close"},
            ]
        });

        let resp = self
            .inner
            .client
            .post(format!("{}/v2/pipeline", self.inner.url))
            .header("Authorization", format!("Bearer {}", self.inner.token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| format!("Turso HTTP request failed: {e}"))?;

        if resp.status() != 200 {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Turso API error {status}: {}", &body[..body.len().min(200)]));
        }

        let data: Value = resp
            .json()
            .map_err(|e| format!("Turso response parse failed: {e}"))?;

        // エラーチェック
        if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
            for r in results {
                if r.get("type").and_then(|t| t.as_str()) == Some("error") {
                    let msg = r
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown");
                    return Err(format!("Turso SQL error: {msg}"));
                }
            }
            // 最初のexecute結果を返す
            if let Some(first) = results.first() {
                if let Some(result) = first.get("response").and_then(|r| r.get("result")) {
                    return Ok(result.clone());
                }
            }
        }

        Err("Unexpected Turso response format".to_string())
    }
}

/// Turso HTTP APIパラメータ型
enum TursoArg {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

/// LocalDb の rusqlite::types::ToSql に相当する trait
pub trait ToSqlTurso {
    fn to_turso_arg(&self) -> TursoArg;
}

impl ToSqlTurso for String {
    fn to_turso_arg(&self) -> TursoArg {
        TursoArg::Text(self.clone())
    }
}

impl ToSqlTurso for &str {
    fn to_turso_arg(&self) -> TursoArg {
        TursoArg::Text(self.to_string())
    }
}

impl ToSqlTurso for i64 {
    fn to_turso_arg(&self) -> TursoArg {
        TursoArg::Integer(*self)
    }
}

impl ToSqlTurso for f64 {
    fn to_turso_arg(&self) -> TursoArg {
        TursoArg::Real(*self)
    }
}

/// Turso APIのセルをserde_json::Valueに変換
fn turso_cell_to_value(cell: &Value) -> Value {
    let type_str = cell.get("type").and_then(|t| t.as_str()).unwrap_or("null");
    match type_str {
        "null" => Value::Null,
        "integer" => {
            if let Some(v) = cell.get("value").and_then(|v| v.as_str()) {
                if let Ok(n) = v.parse::<i64>() {
                    return Value::from(n);
                }
            }
            Value::Null
        }
        "float" => {
            if let Some(v) = cell.get("value") {
                if let Some(f) = v.as_f64() {
                    return serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null);
                }
            }
            Value::Null
        }
        "text" => {
            if let Some(v) = cell.get("value").and_then(|v| v.as_str()) {
                Value::String(v.to_string())
            } else {
                Value::Null
            }
        }
        _ => Value::Null,
    }
}
