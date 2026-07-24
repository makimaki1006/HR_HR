//! 求人票生成パイプラインのハンドラ群 (2026-07-24 HR_HR 統合)。
//!
//! 移植元 `job_media_engine_rs/src/main.rs` 末尾の「求人票生成パイプライン」
//! セクションから抽出 (引き継ぎ資料 `求人票生成部_引き継ぎ_2026-07-24.md` §1.3)。
//! 正本設計: `docs/job_creation_media_engine_generation_pipeline_v1_2026-07-24.md`。
//! 検証はすべてコード (引用実在 / 数値照合[E] / NGワード / 文字数)。不合格は空欄+レビュー行き。
//!
//! HR_HR 統合での変更点:
//! - Gemini はプロセス共通レートリミッタ (12回/分) を共有 ([`crate::media_engine::gemini`])
//! - 認証は「APIトークン一致 → 通す / それ以外 → HR_HR セッション認証」の二段
//!   ([`jobgen_auth_middleware`]。ユーザー決定 2026-07-24: 生成系もトークン併用)
//! - NGワードルール・職種知識はバイナリ埋め込み (env `KNOWLEDGE_DIR` で差し替え可)

use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};

use crate::job_gen::{fact_extract, hrhacker, inputs, knowledge, ng_words, strategy, types as job_types};
use crate::media_engine::config::{gemini_api_key, gemini_model};
use crate::media_engine::gemini;

/// 求人票生成 UI ページ (自己完結 HTML、CDN 依存なし)。
pub async fn ui_jobgen() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../static/jobgen.html"))
}

/// 埋め込みNGワードルール (コンパイル時同梱。正本= Sheets「求人系」NGワードタブ)。
const EMBEDDED_NG_WORDS_JSON: &str = include_str!("../../assets/ng_words.json");

/// NGワードルールを読み込む。
///
/// env `KNOWLEDGE_DIR` (ng_words.json を含む階層) があればファイル、なければ埋め込み。
/// 公開デプロイ (Render) ではファイル配置に依存せず埋め込みで動く。
fn load_ng_rules() -> anyhow::Result<ng_words::NgRules> {
    if let Ok(dir) = std::env::var("KNOWLEDGE_DIR") {
        if !dir.trim().is_empty() {
            let path = std::path::PathBuf::from(dir).join("ng_words.json");
            let text = std::fs::read_to_string(&path)?;
            return ng_words::NgRules::load_from_str(&text);
        }
    }
    ng_words::NgRules::load_from_str(EMBEDDED_NG_WORDS_JSON)
}

fn body_str(body: &Value, key: &str) -> String {
    body.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Gemini を1回呼ぶ共通ヘルパ (キー未設定はエラー)。
/// media_engine::gemini 経由なのでプロセス共通の 12回/分予算を消費する。
async fn jobgen_llm(prompt: &str, schema: &Value, temperature: f64) -> anyhow::Result<Value> {
    let key = gemini_api_key();
    anyhow::ensure!(!key.is_empty(), "GEMINI_API_KEY が未設定です");
    let model = gemini_model();
    gemini::generate_json(prompt, Some(schema), &key, &model, temperature).await
}

/// jobgen 用認証: APIトークン一致なら通し、なければ HR_HR セッション認証へ委ねる。
///
/// - env `API_AUTH_TOKEN` 設定時、`X-Api-Token` または `Authorization: Bearer` の一致で通す
///   (掲載点検スクリプト等の自動化クライアント向け。ユーザー決定: 生成系もトークン併用)
/// - トークン不一致・未提示はセッション認証 (CSRF 検査込み) にフォールバック。
///   ブラウザ利用者はログイン済みセッションでそのまま使える
/// - `API_AUTH_TOKEN` 未設定ならトークン経路は存在しない (セッション認証のみ)
pub async fn jobgen_auth_middleware(
    session: tower_sessions::Session,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let expected = std::env::var("API_AUTH_TOKEN").unwrap_or_default();
    if !expected.is_empty() {
        let headers = request.headers();
        let provided = headers
            .get("x-api-token")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .or_else(|| {
                headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.strip_prefix("Bearer "))
                    .map(str::to_string)
            });
        if provided.as_deref() == Some(expected.as_str()) {
            return next.run(request).await;
        }
    }
    if let Err(msg) = crate::check_csrf(&request) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            format!("Forbidden: {}", msg),
        )
            .into_response();
    }
    crate::auth::require_auth(session, request, next).await
}

use axum::response::IntoResponse;

/// `POST /api/jobgen/normalize` — 入力6形式を求人原文テキストに正規化。
pub async fn jobgen_normalize(Json(body): Json<Value>) -> Json<Value> {
    let kind = match body_str(&body, "kind").as_str() {
        "free_text" => inputs::InputKind::FreeText,
        "url" => inputs::InputKind::Url,
        "csv" => inputs::InputKind::Csv,
        "excel" => inputs::InputKind::Excel,
        "pdf" => inputs::InputKind::Pdf,
        "html" => inputs::InputKind::Html,
        other => return Json(json!({"status":"error","message":format!("不明なkind: {other}")})),
    };
    let text = body.get("text").and_then(Value::as_str).map(String::from);
    let url = body.get("url").and_then(Value::as_str).map(String::from);
    let b64 = body.get("data_base64").and_then(Value::as_str).map(String::from);
    match inputs::normalize(kind, text, url, b64).await {
        Ok(jobs) => Json(json!({
            "status": "ok",
            "jobs": jobs
                .iter()
                .map(|j| json!({"title_hint": j.title_hint, "source_text": j.source_text}))
                .collect::<Vec<_>>(),
        })),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/extract` — 工程①: 事実抽出+引用実在チェック (コード照合)。
pub async fn jobgen_extract(Json(body): Json<Value>) -> Json<Value> {
    let source = body_str(&body, "source_text");
    if source.trim().is_empty() {
        return Json(json!({"status":"error","message":"source_text が必要です"}));
    }
    let prompt = fact_extract::build_extract_prompt(&source);
    let schema = fact_extract::response_schema();
    match jobgen_llm(&prompt, &schema, 0.0).await {
        Ok(raw) => {
            let facts = fact_extract::verify(&source, &raw);
            let facts_text = job_types::facts_to_text(&facts);
            Json(json!({"status":"ok","facts": facts, "facts_text": facts_text}))
        }
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/analyze` — 工程②: 市場分析 (該当職種の知識のみ注入)。
pub async fn jobgen_analyze(Json(body): Json<Value>) -> Json<Value> {
    let source = body_str(&body, "source_text");
    let job_title = body_str(&body, "job_title");
    // 既定=埋め込みバンドル、env KNOWLEDGE_DIR 設定時のみファイルシステム (knowledge.rs 参照)。
    let bundle = knowledge::lookup_default(&job_title).unwrap_or(knowledge::KnowledgeBundle {
        category: "その他".into(),
        sections: Vec::new(),
    });
    // 注入知識の有無は sections で判定 (bundle_to_text は空でも見出しを出すため)。
    let knowledge_used = !bundle.sections.is_empty();
    let knowledge_text = if knowledge_used {
        knowledge::bundle_to_text(&bundle)
    } else {
        String::new()
    };
    let prompt = strategy::build_analyze_prompt(&source, &knowledge_text);
    let schema = strategy::analyze_schema();
    match jobgen_llm(&prompt, &schema, 0.4).await {
        Ok(v) => Json(json!({
            "status":"ok",
            "category": bundle.category,
            "knowledge_used": knowledge_used,
            "analysis": v,
        })),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/personas` — 工程③: ペルソナ設計 (3〜5案)。
pub async fn jobgen_personas(Json(body): Json<Value>) -> Json<Value> {
    let source = body_str(&body, "source_text");
    let analysis = body.get("analysis").cloned().unwrap_or(Value::Null);
    let count = body.get("count").and_then(Value::as_u64).unwrap_or(5).clamp(3, 5) as usize;
    let prompt = strategy::build_personas_prompt(&source, &analysis, count);
    let schema = strategy::personas_schema();
    match jobgen_llm(&prompt, &schema, 0.7).await {
        Ok(v) => Json(json!({"status":"ok","personas": v.get("personas").cloned().unwrap_or(Value::Null)})),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/copy` — 工程④: キャッチコピー (1ペルソナ分)+NGワード検証。
pub async fn jobgen_copy(Json(body): Json<Value>) -> Json<Value> {
    let persona = body.get("persona").cloned().unwrap_or(Value::Null);
    let analysis = body.get("analysis").cloned().unwrap_or(Value::Null);
    let prompt = strategy::build_copy_prompt(&persona, &analysis);
    let schema = strategy::copy_schema();
    match jobgen_llm(&prompt, &schema, 0.9).await {
        Ok(v) => Json(apply_ng_gate(v, "copies", "text")),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/images` — 工程⑤: 画像ディレクション。
pub async fn jobgen_images(Json(body): Json<Value>) -> Json<Value> {
    let personas = body.get("personas").cloned().unwrap_or(Value::Null);
    let prompt = strategy::build_images_prompt(&personas);
    let schema = strategy::images_schema();
    match jobgen_llm(&prompt, &schema, 0.7).await {
        Ok(v) => Json(json!({"status":"ok","directions": v.get("directions").cloned().unwrap_or(Value::Null)})),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/image_prompts` — 工程⑤b: ディレクション文を画像生成AI用の
/// 日本語プロンプト (丸投げ可能な完成文+ネガティブ+アスペクト比) に変換。
///
/// 2026-07-24 追加 (ユーザー要望: 画像生成の文言をプロンプトライクに)。全ペルソナ分を
/// 1コールでまとめて変換するため、1求人あたりの Gemini 消費は +1 回。
/// 2026-07-25 強化: personas も受け取り訴求の核をペインに接地。全要素固定の指示書構造。
/// temperature は 0.4 (指示遵守を優先。演出の発散は工程⑤側で済んでいる)。
pub async fn jobgen_image_prompts(Json(body): Json<Value>) -> Json<Value> {
    let directions = body.get("directions").cloned().unwrap_or(Value::Null);
    let personas = body.get("personas").cloned().unwrap_or(Value::Null);
    if directions
        .get("directions")
        .and_then(Value::as_array)
        .or_else(|| directions.as_array())
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        return Json(json!({"status":"error","message":"directions(工程⑤の出力)が必要です"}));
    }
    let prompt = strategy::build_image_prompts_prompt(&directions, &personas);
    let schema = strategy::image_prompts_schema();
    match jobgen_llm(&prompt, &schema, 0.4).await {
        Ok(v) => Json(json!({"status":"ok","prompts": v.get("prompts").cloned().unwrap_or(Value::Null)})),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/mobile` — 工程⑥: スマホ原稿 (1ペルソナ分)+NGワード検証。
pub async fn jobgen_mobile(Json(body): Json<Value>) -> Json<Value> {
    let persona = body.get("persona").cloned().unwrap_or(Value::Null);
    let facts_text = body_str(&body, "facts_text");
    let prompt = strategy::build_mobile_prompt(&persona, &facts_text);
    let schema = strategy::mobile_schema();
    match jobgen_llm(&prompt, &schema, 0.8).await {
        Ok(v) => {
            let lines: Vec<String> = v
                .get("lines")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default();
            let joined = lines.join("\n");
            let violations = match load_ng_rules() {
                Ok(ng) => ng.detect(&joined),
                Err(_) => Vec::new(),
            };
            let review = !violations.is_empty();
            Json(json!({"status":"ok","lines": lines, "ng_violations": violations, "review_required": review}))
        }
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/hrhacker` — 工程⑦: 84列原稿+数値照合[E]+文字数+NGワード。
///
/// Python `generate_with_revalidation` 相当: 検証不合格があれば issues をフィードバック
/// して1回だけ再生成し、不合格項目が少ない方を採用する (工程別の再実行はUI側にもある)。
pub async fn jobgen_hrhacker(Json(body): Json<Value>) -> Json<Value> {
    let source = body_str(&body, "source_text");
    let strategy_hint = body_str(&body, "strategy_hint");
    let facts: job_types::ExtractedFacts = match body.get("facts").cloned() {
        Some(v) => match serde_json::from_value(v) {
            Ok(f) => f,
            Err(e) => return Json(json!({"status":"error","message":format!("facts の形式が不正: {e}")})),
        },
        None => return Json(json!({"status":"error","message":"facts が必要です"})),
    };
    let ng = match load_ng_rules() {
        Ok(n) => n,
        Err(e) => return Json(json!({"status":"error","message":format!("NGワードルール読込失敗: {e}")})),
    };
    let facts_text = job_types::facts_to_text(&facts);
    let schema = hrhacker::response_schema();
    let mut best: Option<std::collections::BTreeMap<String, hrhacker::GeneratedField>> = None;
    let mut attempts = 0usize;
    for attempt in 0..2 {
        let hint = if attempt == 0 {
            strategy_hint.clone()
        } else {
            let issues: Vec<String> = best
                .as_ref()
                .map(|g| g.values().flat_map(|f| f.issues.iter().cloned()).collect())
                .unwrap_or_default();
            format!("{strategy_hint}\n# 前回生成の問題点(必ず回避すること)\n{}", issues.join("\n"))
        };
        let prompt = hrhacker::build_generation_prompt(&facts_text, &hint);
        let raw = match jobgen_llm(&prompt, &schema, 0.4).await {
            Ok(v) => v,
            Err(e) => return Json(json!({"status":"error","message": e.to_string()})),
        };
        let generated = hrhacker::validate_generated(&source, &raw, &ng);
        attempts = attempt + 1;
        let review_count = generated.values().filter(|g| g.status == "review_required").count();
        let best_review = best
            .as_ref()
            .map(|g| g.values().filter(|f| f.status == "review_required").count())
            .unwrap_or(usize::MAX);
        if review_count < best_review {
            best = Some(generated);
        }
        if best
            .as_ref()
            .map(|g| g.values().all(|f| f.status != "review_required"))
            .unwrap_or(false)
        {
            break;
        }
    }
    let generated = best.unwrap_or_default();
    let row = hrhacker::assemble_row(&facts, &generated);
    // 列順の正本は HRHACKER_COLUMNS (serde_json preserve_order で挿入順のままUIへ届く)。
    let mut ordered = serde_json::Map::new();
    for col in hrhacker::HRHACKER_COLUMNS {
        ordered.insert(col.to_string(), Value::String(row.get(col).cloned().unwrap_or_default()));
    }
    let review: Vec<&String> = generated
        .iter()
        .filter(|(_, g)| g.status == "review_required")
        .map(|(k, _)| k)
        .collect();
    let unsupported: Vec<String> = generated
        .values()
        .flat_map(|g| g.issues.iter().cloned())
        .collect();
    Json(json!({
        "status":"ok",
        "attempts": attempts,
        "row": Value::Object(ordered),
        "generated_fields": generated,
        "review_required_fields": review,
        "unsupported_numbers": unsupported,
    }))
}

/// `POST /api/jobgen/ab` — 工程⑧: A/Bテスト助言。
pub async fn jobgen_ab(Json(body): Json<Value>) -> Json<Value> {
    let summary = body_str(&body, "summary");
    let prompt = strategy::build_ab_prompt(&summary);
    let schema = strategy::ab_schema();
    match jobgen_llm(&prompt, &schema, 0.4).await {
        Ok(v) => Json(json!({"status":"ok","steps": v.get("steps").cloned().unwrap_or(Value::Null)})),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/ng_check` — NGワード一括チェック (掲載中求人の点検用バッチ入口)。
///
/// LLMを使わない決定論検査のみ。Python点検層 (hr_listing_audit) から委譲される。
/// req: {"items":[{"key":"<求人id|列名>","text":"..."}]} / res: 違反のあった item のみ返す。
pub async fn jobgen_ng_check(Json(body): Json<Value>) -> Json<Value> {
    let ng = match load_ng_rules() {
        Ok(n) => n,
        Err(e) => return Json(json!({"status":"error","message":format!("NGワードルール読込失敗: {e}")})),
    };
    let items = match body.get("items").and_then(Value::as_array) {
        Some(a) => a,
        None => return Json(json!({"status":"error","message":"items(配列)が必要です"})),
    };
    const MAX_ITEMS: usize = 50_000; // 公開求人×十数列を1リクエストで賄える上限。
    if items.len() > MAX_ITEMS {
        return Json(json!({"status":"error","message":format!("items が多すぎます(最大{MAX_ITEMS})")}));
    }
    let mut results: Vec<Value> = Vec::new();
    let mut checked = 0usize;
    for item in items {
        let key = item.get("key").and_then(Value::as_str).unwrap_or("");
        let text = item.get("text").and_then(Value::as_str).unwrap_or("");
        if text.trim().is_empty() {
            continue;
        }
        checked += 1;
        let violations = ng.detect(text);
        if !violations.is_empty() {
            results.push(json!({
                "key": key,
                "violations": violations,
            }));
        }
    }
    Json(json!({"status":"ok","checked": checked, "flagged": results.len(), "results": results}))
}

/// LLM応答の配列 (items_key) の各要素 text_key に NGワード検証をかけ、結果を付与する。
fn apply_ng_gate(v: Value, items_key: &str, text_key: &str) -> Value {
    let items = v.get(items_key).cloned().unwrap_or(Value::Null);
    let mut all_violations: Vec<Value> = Vec::new();
    if let (Ok(ng), Some(arr)) = (load_ng_rules(), items.as_array()) {
        for item in arr {
            if let Some(text) = item.get(text_key).and_then(Value::as_str) {
                for viol in ng.detect(text) {
                    all_violations.push(serde_json::to_value(&viol).unwrap_or(Value::Null));
                }
            }
        }
    }
    let review = !all_violations.is_empty();
    json!({"status":"ok", items_key: items, "ng_violations": all_violations, "review_required": review})
}
