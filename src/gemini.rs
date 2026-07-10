//! Gemini API クライアント (構造化 JSON 出力専用・graceful degradation)
//!
//! # 用途
//! LLM を「クリティカルパスに置かない」補助推論のためのクライアント。想定用途:
//! - CSV アップロード時の**列マッピング推定**フォールバック
//!   (ヘッダ名がヒューリスティックに一致しないときの最終手段)
//! - 求人テキストからの**年間休日抽出**フォールバック
//!   (正規表現で拾えなかった場合の補完)
//!
//! いずれも「取れたら嬉しいが、取れなくても従来ロジックで動く」経路でのみ使う。
//! そのため本モジュールは **一切 panic せず、失敗時は常に `None` を返す**。
//! 呼び出し側は `None` を受けたら従来の非 LLM 動作にフォールバックすること。
//!
//! # キー設定方法
//! - 本番 (Render): サービスの Environment に `GEMINI_API_KEY` を設定
//! - ローカル: プロジェクトルートの `.env` に `GEMINI_API_KEY=...` を記載
//!   (`main.rs` 冒頭の `dotenvy::dotenv()` で読み込まれる)
//! - モデル ID は `GEMINI_MODEL` で上書き可。未設定時は [`DEFAULT_MODEL`]。
//!   `GEMINI_API_KEY` が無ければ [`GeminiClient::from_env`] は `None` を返し、
//!   機能自体が無効化される (= 常に従来動作)。
//!
//! # 有料 tier 前提の注記
//! 構造化出力 (response_schema) と実運用レートは Gemini API の **有料 tier** を前提とする。
//! 無料枠では枠切れ (429) で `None` になり得るが、それも graceful degradation として
//! 呼び出し側が従来動作に落ちるだけで、致命的エラーにはならない。
//!
//! # サンセット基準
//! 以下のいずれかを満たしたら本モジュールを撤去する:
//! - 列マッピング推定・年間休日抽出のフォールバック発火率がログ上ほぼ 0 に張り付いた
//!   (= 非 LLM のヒューリスティックで十分に賄えている)
//! - Gemini 由来の抽出結果に対する下流の品質不良が観測され、費用対効果が負に転じた
//! - モデル/エンドポイント (v1beta) が非推奨化され、無改修で追随できなくなった
//!
//! # 機密の扱い
//! - API キーは URL クエリに載せるが **ログには一切出さない** (status のみ warn)。
//! - プロンプト (system/user) に認証情報・Turso 接続情報を含めてはならない。
//!   呼び出し側の責務だが、本モジュールもキー等を prompt に混ぜない。

use serde_json::{json, Value};
use std::time::Duration;

/// デフォルトのモデル ID (エイリアス)。
///
/// ユーザーは実運用で 3.1 Lite を使う予定だが、固有 ID は環境ごとに
/// `GEMINI_MODEL` で指定する。ここではサンセットに強いエイリアスを既定にする。
pub const DEFAULT_MODEL: &str = "gemini-flash-lite-latest";

/// リクエストタイムアウト。LLM をクリティカルパスに置かないため短めに固定。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Gemini API クライアント (構造化 JSON 出力専用)。
///
/// `GEMINI_API_KEY` が設定されているときのみ [`GeminiClient::from_env`] で生成される。
/// 生成できない/呼び出しに失敗した場合は常に `None` を返す設計。
#[derive(Clone)]
pub struct GeminiClient {
    api_key: String,
    model: String,
}

impl GeminiClient {
    /// 環境変数からクライアントを構築する。
    ///
    /// - `GEMINI_API_KEY` が未設定または空白のみ → `None` (機能無効)
    /// - `GEMINI_MODEL` があればそれを、無ければ [`DEFAULT_MODEL`] を使う
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let model = resolve_model(std::env::var("GEMINI_MODEL").ok());
        Some(Self { api_key, model })
    }

    /// 使用中のモデル ID を返す (ログ・診断用、キーは含まない)。
    pub fn model(&self) -> &str {
        &self.model
    }

    /// system/user プロンプトと JSON schema を与えて、構造化 JSON を得る。
    ///
    /// 成功時は Gemini が返した JSON (schema に沿う想定) の [`Value`] を返す。
    /// 以下はすべて `tracing::warn` を 1 行出して `None`:
    /// - HTTP クライアント構築失敗 / 送信失敗 / タイムアウト
    /// - 非 2xx ステータス (認証エラー・枠切れ 429 等)
    /// - レスポンス本文の取得失敗
    /// - candidates/parts/text 構造の欠落、text が壊れた JSON
    ///
    /// reqwest は blocking 版のみ有効なため、async ランタイムをブロックしないよう
    /// `spawn_blocking` 内でクライアント生成〜送信を行う (main.rs の既存方針に準拠)。
    pub async fn generate_json(&self, system: &str, user: &str, schema: Value) -> Option<Value> {
        let body = build_request_body(system, user, schema);
        // URL には API キーを載せる。クロージャ内ローカルに閉じ込め、ログには出さない。
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let join = tokio::task::spawn_blocking(move || -> Option<String> {
            // blocking::Client は async コンテキストで new するとパニックするため、
            // spawn_blocking の別スレッド内で構築する。
            let client = match reqwest::blocking::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Gemini: HTTP client build failed: {e}");
                    return None;
                }
            };

            // 429 (レート制限、無料枠は 15 リクエスト/分) は 1 分でリセットされるため、
            // Retry-After (無ければ 30 秒、上限 45 秒) 待って 1 回だけ再試行する。
            // それ以外の失敗は従来どおり即 None (呼び出し側が graceful skip)。
            for attempt in 0..2 {
                let resp = match client.post(&url).json(&body).send() {
                    Ok(r) => r,
                    Err(e) => {
                        // e には URL(=キー) が含まれ得るため、キーを除いた種別のみログ出力。
                        tracing::warn!(
                            "Gemini: request failed (timeout={}, connect={})",
                            e.is_timeout(),
                            e.is_connect()
                        );
                        return None;
                    }
                };

                let status = resp.status();
                if status.as_u16() == 429 && attempt == 0 {
                    let wait_secs = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(30)
                        .min(45);
                    tracing::warn!(
                        "Gemini: rate limited (429), retrying once after {}s",
                        wait_secs
                    );
                    std::thread::sleep(Duration::from_secs(wait_secs));
                    continue;
                }
                if !status.is_success() {
                    tracing::warn!("Gemini: non-success status {}", status.as_u16());
                    return None;
                }

                return match resp.text() {
                    Ok(t) => Some(t),
                    Err(_) => {
                        tracing::warn!("Gemini: failed to read response body");
                        None
                    }
                };
            }
            None
        })
        .await;

        let raw = match join {
            Ok(Some(t)) => t,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!("Gemini: spawn_blocking join failed: {e}");
                return None;
            }
        };

        parse_response(&raw)
    }
}

/// `GEMINI_MODEL` の値からモデル ID を決める。
/// 空白のみ/未設定なら [`DEFAULT_MODEL`]。(env を触らずテストできるよう純関数化)
pub(crate) fn resolve_model(env_val: Option<String>) -> String {
    env_val
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// generateContent のリクエストボディを構築する。
///
/// - `systemInstruction`: system プロンプト
/// - `contents`: user プロンプト (role=user)
/// - `generationConfig`: 構造化出力 (JSON MIME + response_schema) + temperature 0
///
/// テスト可能にするため副作用のない純関数として分離。
pub(crate) fn build_request_body(system: &str, user: &str, schema: Value) -> Value {
    json!({
        "systemInstruction": {
            "parts": [{ "text": system }]
        },
        "contents": [{
            "role": "user",
            "parts": [{ "text": user }]
        }],
        "generationConfig": {
            "response_mime_type": "application/json",
            "response_schema": schema,
            "temperature": 0
        }
    })
}

/// generateContent のレスポンス JSON 文字列を解析する。
///
/// Gemini は構造化出力でも `candidates[0].content.parts[0].text` に
/// **JSON 文字列** を入れて返す。その text を取り出し、さらに JSON として parse する。
/// 構造欠落・text が壊れた JSON のいずれも `None`。
pub(crate) fn parse_response(raw: &str) -> Option<Value> {
    let root: Value = serde_json::from_str(raw).ok()?;
    let text = root
        .get("candidates")?
        .as_array()?
        .first()?
        .get("content")?
        .get("parts")?
        .as_array()?
        .first()?
        .get("text")?
        .as_str()?;

    // text 自体が JSON 文字列。壊れていれば None (フォールバック)。
    serde_json::from_str::<Value>(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_model_defaults_when_absent_or_blank() {
        assert_eq!(resolve_model(None), DEFAULT_MODEL);
        assert_eq!(resolve_model(Some("   ".to_string())), DEFAULT_MODEL);
        assert_eq!(
            resolve_model(Some("gemini-3.1-lite-xyz".to_string())),
            "gemini-3.1-lite-xyz"
        );
    }

    #[test]
    fn build_request_body_shapes_prompts_and_config() {
        let schema = json!({
            "type": "object",
            "properties": { "holidays": { "type": "integer" } }
        });
        let body = build_request_body("SYS", "USR", schema.clone());

        // system プロンプト
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], json!("SYS"));
        // user プロンプト + role
        assert_eq!(body["contents"][0]["role"], json!("user"));
        assert_eq!(body["contents"][0]["parts"][0]["text"], json!("USR"));
        // 構造化出力設定
        let gc = &body["generationConfig"];
        assert_eq!(gc["response_mime_type"], json!("application/json"));
        assert_eq!(gc["response_schema"], schema);
        assert_eq!(gc["temperature"], json!(0));
    }

    #[test]
    fn build_request_body_does_not_leak_key_field() {
        // ボディに認証系フィールドが混入しないこと (キーは URL 側のみ)
        let body = build_request_body("s", "u", json!({}));
        assert!(body.get("key").is_none());
        assert!(body.get("api_key").is_none());
    }

    #[test]
    fn parse_response_extracts_inner_json() {
        // parts[0].text の中に JSON 文字列が入っている正常系
        let raw = json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": "{\"holidays\": 120, \"ok\": true}" }]
                }
            }]
        })
        .to_string();

        let parsed = parse_response(&raw).expect("should parse");
        assert_eq!(parsed["holidays"], json!(120));
        assert_eq!(parsed["ok"], json!(true));
    }

    #[test]
    fn parse_response_none_on_broken_inner_json() {
        // text が壊れた JSON → None
        let raw = json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": "{not valid json" }]
                }
            }]
        })
        .to_string();
        assert!(parse_response(&raw).is_none());
    }

    #[test]
    fn parse_response_none_on_missing_structure() {
        // candidates 欠落
        assert!(parse_response(&json!({ "error": "boom" }).to_string()).is_none());
        // parts が空配列
        let empty_parts = json!({
            "candidates": [{ "content": { "parts": [] } }]
        })
        .to_string();
        assert!(parse_response(&empty_parts).is_none());
        // そもそも壊れた外側 JSON
        assert!(parse_response("<<<not json>>>").is_none());
    }
}
