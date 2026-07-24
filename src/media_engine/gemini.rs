//! Gemini(Google Generative Language API)クライアント。
//!
//! Python `scripts/lib/llm_gateway.py` の移植。構造化 JSON 出力(responseSchema)対応。
//! Rust Web アプリ側で LLM を使うための入口(モデルは env `GEMINI_MODEL` で切替可、
//! 既定は [`crate::media_engine::config::DEFAULT_GEMINI_MODEL`])。リクエスト構築・レスポンス解析は
//! 純粋関数として切り出しユニットテスト可能にし、ライブ HTTP は `generate_json` のみ。

use serde_json::{json, Value};

/// Generative Language API のベース URL(Python `API_BASE` と一致)。
pub const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// リクエスト本文を組み立てる(純粋)。Python llm_gateway と同形状。
///
/// `schema` を渡すと responseSchema を付け構造化出力を強制する。常に JSON 出力
/// (responseMimeType=application/json)。
pub fn build_request_body(prompt: &str, schema: Option<&Value>, temperature: f64) -> Value {
    let mut gen_cfg = json!({
        "temperature": temperature,
        "responseMimeType": "application/json",
    });
    if let Some(s) = schema {
        gen_cfg["responseSchema"] = s.clone();
    }
    json!({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": gen_cfg,
    })
}

/// レスポンスから candidates[0].content.parts[0].text を取り、JSON としてパースする(純粋)。
pub fn parse_response(payload: &Value) -> anyhow::Result<Value> {
    let text = payload
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!("Gemini response missing candidates[0].content.parts[0].text")
        })?;
    Ok(serde_json::from_str(text)?)
}

/// Gemini を1回呼び、構造化 JSON を返す。api_key/model は [`crate::media_engine::config`] から得る想定。
///
/// PII を送らないこと(呼び出し側の責任)。ライブ HTTP。
pub async fn generate_json(
    prompt: &str,
    schema: Option<&Value>,
    api_key: &str,
    model: &str,
    temperature: f64,
) -> anyhow::Result<Value> {
    if api_key.is_empty() {
        anyhow::bail!("GEMINI_API_KEY が未設定です");
    }
    // 2026-07-24 HR_HR 統合: プロセス共通のレートリミッタ (12回/分) を通す。
    // 解説資料・商談準備と同じ予算を共有し、無料枠の分間制限を超えない。
    crate::gemini::acquire_rate_slot().await;
    let url = format!("{API_BASE}/{model}:generateContent");
    let body = build_request_body(prompt, schema, temperature);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("x-goog-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let payload: Value = resp.json().await?;
    parse_response(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_shape_matches_python() {
        let b = build_request_body("hello", None, 0.0);
        assert_eq!(b["contents"][0]["parts"][0]["text"], "hello");
        assert_eq!(b["generationConfig"]["responseMimeType"], "application/json");
        assert_eq!(b["generationConfig"]["temperature"], 0.0);
        assert!(b["generationConfig"].get("responseSchema").is_none());
    }

    #[test]
    fn build_body_includes_schema() {
        let schema = json!({"type": "object", "properties": {"a": {"type": "integer"}}});
        let b = build_request_body("x", Some(&schema), 0.2);
        assert_eq!(b["generationConfig"]["responseSchema"], schema);
        assert_eq!(b["generationConfig"]["temperature"], 0.2);
    }

    #[test]
    fn parse_response_extracts_and_parses_json() {
        let payload = json!({
            "candidates": [{"content": {"parts": [{"text": "{\"a\": 1, \"b\": \"x\"}"}]}}]
        });
        let v = parse_response(&payload).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], "x");
    }

    #[test]
    fn parse_response_errors_on_missing() {
        assert!(parse_response(&json!({"candidates": []})).is_err());
        assert!(parse_response(&json!({})).is_err());
    }
}
