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

use axum::extract::State;
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

use crate::job_gen::{
    fact_extract, hrhacker, inputs, journey, knowledge, ng_words, strategy, types as job_types,
};
use crate::media_engine::config::{gemini_api_key, gemini_model};
use crate::media_engine::gemini;
use crate::AppState;

/// 求人票生成 UI ページ (自己完結 HTML、CDN 依存なし)。
pub async fn ui_jobgen() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../static/jobgen.html"))
}

/// 競合求人との比較から採用ポジションを設計するベータ UI。
/// 既存の求人票生成パイプラインとは画面・状態・APIを共有しない。
pub async fn ui_jobgen_competitive_beta() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../static/jobgen_competitive_beta.html"))
}

/// 顧客求人・競合求人CSV・口コミCSVから応募者ジャーニーを診断するベータ UI。
/// 既存の求人票生成・競合比較求人作成とは画面とブラウザ状態を共有しない。
pub async fn ui_jobgen_applicant_journey_beta() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!(
        "../../static/jobgen_applicant_journey_beta.html"
    ))
}

/// 埋め込みNGワードルール (コンパイル時同梱。正本= Sheets「求人系」NGワードタブ)。
const EMBEDDED_NG_WORDS_JSON: &str = include_str!("../../assets/ng_words.json");

/// 埋め込み表現レビュー辞書 (警告レベル。法令NGとは別系統。severity=warning)。
const EMBEDDED_EXPRESSION_RULES_JSON: &str =
    include_str!("../../assets/expression_review_rules.json");

const JOURNEY_CASE_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
struct PreparedJourneyCase {
    created_at: Instant,
    case_profile: Value,
    job_facts: Vec<Value>,
    customer_statements: Vec<Value>,
    competitor: journey::CompetitorSummary,
    reviews: journey::ReviewSummary,
    public_stats: Value,
    prepare_result: Value,
    allowed_evidence_refs: HashSet<String>,
}

fn journey_case_store() -> &'static AsyncMutex<HashMap<String, PreparedJourneyCase>> {
    static STORE: OnceLock<AsyncMutex<HashMap<String, PreparedJourneyCase>>> = OnceLock::new();
    STORE.get_or_init(|| AsyncMutex::new(HashMap::new()))
}

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

/// 表現レビュー辞書 (警告レベル) を読み込む。
///
/// env `KNOWLEDGE_DIR` に expression_review_rules.json があればファイル、無ければ埋め込みへ
/// フォールバックする (新アセットなので KNOWLEDGE_DIR に無い運用があり得るため、
/// load_ng_rules と違いファイル欠落でも埋め込みで動かす)。
fn load_expression_rules() -> anyhow::Result<ng_words::NgRules> {
    if let Ok(dir) = std::env::var("KNOWLEDGE_DIR") {
        if !dir.trim().is_empty() {
            let path = std::path::PathBuf::from(dir).join("expression_review_rules.json");
            if path.exists() {
                let text = std::fs::read_to_string(&path)?;
                return ng_words::NgRules::load_from_str(&text);
            }
        }
    }
    ng_words::NgRules::load_from_str(EMBEDDED_EXPRESSION_RULES_JSON)
}

/// 数値照合ゲート: `source_text` に照らして生成テキスト群の「原文にない数値」を集約する。
///
/// 戻り値 `(number_violations, number_check, review)`:
/// - `source_text` 空 → `([], "skipped(source_text未提供)", false)` (照合しない)
/// - それ以外 → 違反テキストを `{"text":..., "numbers":[...]}` で列挙、`number_check="checked"`
fn number_gate(source_text: &str, texts: &[String]) -> (Vec<Value>, String, bool) {
    if source_text.trim().is_empty() {
        return (Vec::new(), "skipped(source_text未提供)".to_string(), false);
    }
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let viols = crate::job_gen::validate::collect_number_violations(source_text, &refs);
    let out: Vec<Value> = viols
        .into_iter()
        .map(|(text, numbers)| json!({"text": text, "numbers": numbers}))
        .collect();
    let review = !out.is_empty();
    (out, "checked".to_string(), review)
}

/// 表現ゲート: 生成テキスト群に法令NG(ng_violations)と表現レビュー辞書(expression_warnings)を
/// 別リストで適用する。戻り値 `(ng_violations, expression_warnings, review)`。
/// 法令NGと警告は別フィールドに分けるが、どちらか非空なら review を立てる。
fn ng_and_expression_gate(texts: &[String]) -> (Vec<Value>, Vec<Value>, bool) {
    let mut ng_out: Vec<Value> = Vec::new();
    let mut ex_out: Vec<Value> = Vec::new();
    if let Ok(ng) = load_ng_rules() {
        for t in texts {
            for v in ng.detect(t) {
                ng_out.push(serde_json::to_value(&v).unwrap_or(Value::Null));
            }
        }
    }
    if let Ok(ex) = load_expression_rules() {
        for t in texts {
            for v in ex.detect(t) {
                ex_out.push(serde_json::to_value(&v).unwrap_or(Value::Null));
            }
        }
    }
    let review = !ng_out.is_empty() || !ex_out.is_empty();
    (ng_out, ex_out, review)
}

fn body_str(body: &Value, key: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
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

/// `POST /api/jobgen/competitive-analyze` — 戦略選択前の競合比較。
pub async fn jobgen_competitive_analyze(Json(body): Json<Value>) -> Json<Value> {
    let client = body_str(&body, "client_job");
    if client.trim().chars().count() < 20 {
        return Json(json!({"status":"error","message":"顧客求人を20文字以上入力してください。"}));
    }
    let competitors: Vec<String> = body
        .get("competitors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .take(10)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if competitors.is_empty() {
        return Json(json!({"status":"error","message":"競合求人を1件以上入力してください。"}));
    }
    let excerpt = |text: &str, limit: usize| text.chars().take(limit).collect::<String>();
    let competitor_text = competitors
        .iter()
        .enumerate()
        .map(|(i, text)| format!("## 競合求人{}\n{}", i + 1, excerpt(text, 2500)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let prompt = format!(
        r#"あなたは採用コンサルタントです。採用戦略を選ぶ前に、顧客求人と登録された競合求人を比較してください。

# ルール
- 書かれている事実だけを使い、推測で条件を補わない。
- 各競合求人を個別に比較し、参照先を必ず「競合求人1」のように明示する。
- 「有利・不利」は比較できる事実がある場合だけ記載し、それ以外は「確認できない」とする。
- 給与、休日・時間、応募条件、仕事内容、勤務地・通勤、教育、福利厚生、訴求表現を確認する。
- 年齢・性別・MBTIタイプで人物を決めつけない。
- この段階では求人を書き直さず、戦略候補を考える材料だけを返す。

# 顧客求人
{client}

# 競合求人
{competitors}"#,
        client = excerpt(&client, 7000),
        competitors = competitor_text
    );
    let schema = json!({
        "type":"object","properties":{
            "competitor_rows":{"type":"array","items":{"type":"object","properties":{
                "competitor":{"type":"string"},"dimension":{"type":"string"},
                "client_value":{"type":"string"},"competitor_value":{"type":"string"},
                "assessment":{"type":"string"},"source_note":{"type":"string"}
            },"required":["competitor","dimension","client_value","competitor_value","assessment","source_note"]}},
            "common_patterns":{"type":"array","items":{"type":"string"}},
            "client_strengths":{"type":"array","items":{"type":"string"}},
            "client_gaps":{"type":"array","items":{"type":"string"}},
            "strategy_questions":{"type":"array","items":{"type":"string"}}
        },
        "required":["competitor_rows","common_patterns","client_strengths","client_gaps","strategy_questions"]
    });
    match jobgen_llm(&prompt, &schema, 0.2).await {
        Ok(result) => Json(json!({"status":"ok","result":result})),
        Err(error) => Json(json!({"status":"error","message":error.to_string()})),
    }
}

/// `POST /api/jobgen/competitive-generate` — 競合比較ベータ専用の差別化求人生成。
///
/// 競合求人は比較材料としてのみ使用し、生成文の事実根拠は顧客求人に限定する。
pub async fn jobgen_competitive_generate(Json(body): Json<Value>) -> Json<Value> {
    let client = body_str(&body, "client_job");
    if client.trim().chars().count() < 20 {
        return Json(json!({"status":"error","message":"顧客求人を20文字以上入力してください。"}));
    }
    let competitors: Vec<String> = body
        .get("competitors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .take(10)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if competitors.is_empty() {
        return Json(json!({"status":"error","message":"競合求人を1件以上入力してください。"}));
    }
    let strategy = body.get("strategy").cloned().unwrap_or(Value::Null);
    let movable = body.get("movable").cloned().unwrap_or_else(|| json!([]));
    let notes = body_str(&body, "notes");
    let excerpt = |text: &str, limit: usize| text.chars().take(limit).collect::<String>();
    let competitor_text = competitors
        .iter()
        .enumerate()
        .map(|(i, text)| format!("## 競合求人{}\n{}", i + 1, excerpt(text, 2500)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let prompt = format!(
        r#"あなたは採用コンサルタントです。選択済みの採用戦略に沿って、顧客求人を差別化して書き直してください。

# 最重要ルール
- 労働条件・数値・制度・仕事内容の事実根拠は「顧客求人」だけに限定する。
- 競合求人は比較と重複回避にだけ使い、競合固有の条件・表現・制度を顧客求人へ移さない。
- 原文で確認できない内容は生成せず、client_questions に確認事項として出す。
- 給与や休日で負けている場合も隠したり誇張したりしない。
- 年齢・性別・MBTIタイプを応募条件や人物評価に使わない。
- 「必ず」「業界最高」など根拠のない断定をしない。
- 競合求人ごとの差を確認し、複数社に共通する傾向と一社だけの特徴を混同しない。
- 条件そのものを変えた箇所と、同じ事実の伝え方だけを変えた箇所を明確に区別する。
- どの設計判断も、顧客求人にある根拠と結びつける。判断できない場合は「確認できない」とする。

# 選択戦略
{strategy}

# 変更を検討できる条件
{movable}

# その他の制約・補足
{notes}

# 顧客求人（生成事実の唯一の根拠）
{client}

# 競合求人（比較専用）
{competitors}

# 出力
求人タイトル、冒頭文、求人本文を作る。求人本文は「この仕事について」「具体的な仕事内容」「この求人が合う可能性のある人」「条件・働き方」の順で、原文に根拠がない節は無理に埋めない。
comparison_rows には、比較軸ごとに顧客求人の元の状態、競合求人の傾向、今回の設計判断、その判断に使った顧客求人の根拠を返す。competitor_pattern では「競合求人1・2」のように参照先を明示し、確認できない傾向を作らない。
before_after には、元求人から実際に変えた箇所だけを、変更前・変更後・理由・変更種別（「条件変更」または「表現変更」）に分けて返す。
avoided_overlap には、競合と重なるため主軸にしなかった訴求と参照した競合求人番号を返す。
差別化ポイント、顧客への確認事項、注意事項も分けて返す。"#,
        strategy = strategy,
        movable = movable,
        notes = excerpt(&notes, 1500),
        client = excerpt(&client, 7000),
        competitors = competitor_text,
    );
    let schema = json!({
        "type":"object",
        "properties":{
            "strategy_summary":{"type":"string"},
            "persona":{"type":"string"},
            "title":{"type":"string"},
            "lead":{"type":"string"},
            "job_body":{"type":"string"},
            "comparison_rows":{"type":"array","items":{"type":"object","properties":{
                "dimension":{"type":"string"},"client_original":{"type":"string"},
                "competitor_pattern":{"type":"string"},"design_decision":{"type":"string"},
                "client_evidence":{"type":"string"}
            },"required":["dimension","client_original","competitor_pattern","design_decision","client_evidence"]}},
            "before_after":{"type":"array","items":{"type":"object","properties":{
                "aspect":{"type":"string"},"before":{"type":"string"},"after":{"type":"string"},
                "reason":{"type":"string"},"change_type":{"type":"string"}
            },"required":["aspect","before","after","reason","change_type"]}},
            "avoided_overlap":{"type":"array","items":{"type":"string"}},
            "differentiation_points":{"type":"array","items":{"type":"string"}},
            "client_questions":{"type":"array","items":{"type":"string"}},
            "caveats":{"type":"array","items":{"type":"string"}}
        },
        "required":["strategy_summary","persona","title","lead","job_body","comparison_rows","before_after","avoided_overlap","differentiation_points","client_questions","caveats"]
    });
    match jobgen_llm(&prompt, &schema, 0.6).await {
        Ok(result) => {
            let texts = ["title", "lead", "job_body"]
                .iter()
                .filter_map(|key| result.get(*key).and_then(Value::as_str).map(str::to_string))
                .collect::<Vec<_>>();
            let (ng, expression, ng_review) = ng_and_expression_gate(&texts);
            let (numbers, number_check, number_review) = number_gate(&client, &texts);
            Json(json!({
                "status":"ok", "result":result,
                "ng_violations":ng, "expression_warnings":expression,
                "number_violations":numbers, "number_check":number_check,
                "review_required":ng_review || number_review
            }))
        }
        Err(error) => Json(json!({"status":"error","message":error.to_string()})),
    }
}

/// `POST /api/jobgen/journey-diagnose` — 3入力からペルソナ別採用ジャーニーを診断。
///
/// 入力:
/// - 顧客求人 (HTML またはテキスト)
/// - 競合求人 CSV (base64)
/// - Google ビジネスプロフィール等の口コミ CSV (base64、星評価不要)
///
/// 求人の不変条件、CSV件数、給与分布、人気タグ、口コミ本文の有無はコードで確定する。
/// LLM は事実抽出後のペルソナ・検索行動・離脱仮説・対策生成にだけ使用する。
#[allow(dead_code)]
async fn jobgen_journey_diagnose_legacy(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let raw_client = body_str(&body, "client_job");
    if raw_client.len() > 5 * 1024 * 1024 {
        return Json(json!({
            "status":"error",
            "message":"顧客求人は5MB以内のHTMLまたはテキストを指定してください。"
        }));
    }
    if raw_client.trim().chars().count() < 20 {
        return Json(json!({
            "status":"error",
            "message":"顧客求人を20文字以上入力するか、求人HTMLを選択してください。"
        }));
    }
    let client_kind = body_str(&body, "client_kind");
    let input_kind = if client_kind == "html" {
        inputs::InputKind::Html
    } else {
        inputs::InputKind::FreeText
    };
    let normalized = match inputs::normalize(input_kind, Some(raw_client), None, None).await {
        Ok(mut jobs) if !jobs.is_empty() => jobs.remove(0),
        Ok(_) => {
            return Json(json!({
                "status":"error",
                "message":"顧客求人から本文を取得できませんでした。"
            }))
        }
        Err(error) => {
            return Json(json!({"status":"error","message":error.to_string()}));
        }
    };
    if normalized.source_text.trim().chars().count() < 20 {
        return Json(json!({
            "status":"error",
            "message":"顧客求人から分析可能な本文を取得できませんでした。"
        }));
    }

    let competitor_bytes =
        match journey::decode_csv_base64(&body_str(&body, "competitor_csv_base64"), "競合求人CSV")
        {
            Ok(bytes) => bytes,
            Err(message) => return Json(json!({"status":"error","message":message})),
        };
    let review_bytes =
        match journey::decode_csv_base64(&body_str(&body, "review_csv_base64"), "口コミCSV") {
            Ok(bytes) => bytes,
            Err(message) => return Json(json!({"status":"error","message":message})),
        };

    let competitor_filename = body_str(&body, "competitor_filename");
    let review_filename = body_str(&body, "review_filename");
    let competitor_captured_at = optional_body_str(&body, "competitor_captured_at");
    let review_captured_at = optional_body_str(&body, "review_captured_at");
    let employer_note = body_str(&body, "employer_note");

    let competitor = match journey::summarize_competitor_csv(
        &competitor_bytes,
        if competitor_filename.is_empty() {
            "競合求人.csv"
        } else {
            &competitor_filename
        },
        competitor_captured_at,
    ) {
        Ok(summary) => summary,
        Err(message) => {
            return Json(json!({
                "status":"error",
                "message":format!("競合求人CSVを解析できません: {message}")
            }))
        }
    };
    let reviews = match journey::summarize_review_csv(
        &review_bytes,
        if review_filename.is_empty() {
            "口コミ.csv"
        } else {
            &review_filename
        },
        review_captured_at,
    ) {
        Ok(summary) => summary,
        Err(message) => {
            return Json(json!({
                "status":"error",
                "message":format!("口コミCSVを解析できません: {message}")
            }))
        }
    };

    // 顧客求人の条件は既存の引用照合ゲートを必ず通す。
    let fact_prompt = fact_extract::build_extract_prompt(&normalized.source_text);
    let fact_raw = match jobgen_llm(&fact_prompt, &fact_extract::response_schema(), 0.0).await {
        Ok(value) => value,
        Err(error) => {
            return Json(json!({
                "status":"error",
                "message":format!("顧客求人の事実抽出に失敗しました: {error}")
            }))
        }
    };
    let facts = fact_extract::verify(&normalized.source_text, &fact_raw);
    let facts_value = serde_json::to_value(&facts).unwrap_or_else(|_| json!({}));

    let salary_text = verified_fact_value(&facts, "salary");
    let employment_type = verified_fact_value(&facts, "employment_type");
    let client_salary =
        journey::client_salary_position(&salary_text, &employment_type, &competitor);
    let work_location = verified_fact_value(&facts, "work_location");
    let public_stats = fetch_journey_public_stats(&state, &work_location).await;

    let prompt = journey::build_diagnosis_prompt(
        &normalized.source_text,
        &facts_value,
        &competitor,
        &reviews,
        client_salary.as_ref(),
        &public_stats,
        &employer_note,
    );
    let result = match jobgen_llm(&prompt, &journey::diagnosis_schema(), 0.4).await {
        Ok(value) => value,
        Err(error) => {
            return Json(json!({
                "status":"error",
                "message":format!("ペルソナ・採用ジャーニー診断に失敗しました: {error}")
            }))
        }
    };
    let persona_count = result
        .get("personas")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    Json(json!({
        "status":"ok",
        "generated_at":chrono::Utc::now().to_rfc3339(),
        "facts":facts,
        "competitor_summary":competitor,
        "review_summary":reviews,
        "client_salary_position":client_salary,
        "public_stats":public_stats,
        "result":result,
        "review_required":persona_count < 3,
        "notes":{
            "truth_scope":"顧客企業について確定事実として扱うのは、顧客求人から引用照合できた項目だけです。",
            "review_scope":"口コミは求職者が触れ得る外部観測であり、会社の労働実態を断定する根拠にはしません。",
            "persona_scope":"ペルソナと離脱地点は、競合求人・公的統計・職種一般論を組み合わせた検討仮説です。"
        }
    }))
}

/// `POST /api/jobgen/journey-diagnose` — 品質優先モードの準備工程。
///
/// 引用照合済みの求人事実、比較可能な競合母集団、口コミ原文を確定してから、
/// 4ペルソナと検索仮説を作る。品質基準を満たすまで後工程用の case_id は発行しない。
pub async fn jobgen_journey_diagnose(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let raw_client = body_str(&body, "client_job");
    if raw_client.len() > 5 * 1024 * 1024 {
        return Json(json!({
            "status":"error",
            "message":"顧客求人は5MB以内のHTMLまたはテキストを指定してください。"
        }));
    }
    if raw_client.trim().chars().count() < 20 {
        return Json(json!({
            "status":"error",
            "message":"顧客求人を20文字以上入力するか、求人HTMLを選択してください。"
        }));
    }
    let input_kind = if body_str(&body, "client_kind") == "html" {
        inputs::InputKind::Html
    } else {
        inputs::InputKind::FreeText
    };
    let normalized = match inputs::normalize(input_kind, Some(raw_client), None, None).await {
        Ok(mut jobs) if !jobs.is_empty() => jobs.remove(0),
        Ok(_) => {
            return Json(json!({
                "status":"error",
                "message":"顧客求人から本文を取得できませんでした。"
            }))
        }
        Err(error) => return Json(json!({"status":"error","message":error.to_string()})),
    };
    if normalized.source_text.trim().chars().count() < 20 {
        return Json(json!({
            "status":"error",
            "message":"顧客求人から分析可能な本文を取得できませんでした。"
        }));
    }

    let competitor_bytes =
        match journey::decode_csv_base64(&body_str(&body, "competitor_csv_base64"), "競合求人CSV")
        {
            Ok(bytes) => bytes,
            Err(message) => return Json(json!({"status":"error","message":message})),
        };
    let review_bytes =
        match journey::decode_csv_base64(&body_str(&body, "review_csv_base64"), "口コミCSV") {
            Ok(bytes) => bytes,
            Err(message) => return Json(json!({"status":"error","message":message})),
        };
    let competitor_filename = body_str(&body, "competitor_filename");
    let review_filename = body_str(&body, "review_filename");
    let competitor_label = if competitor_filename.is_empty() {
        "競合求人.csv"
    } else {
        &competitor_filename
    };
    let review_label = if review_filename.is_empty() {
        "口コミ.csv"
    } else {
        &review_filename
    };
    let competitor_captured_at = optional_body_str(&body, "competitor_captured_at");
    let review_captured_at = optional_body_str(&body, "review_captured_at");
    let employer_note = truncate_text(&body_str(&body, "employer_note"), 2_000);
    let statement_speaker = optional_body_str(&body, "customer_statement_speaker")
        .unwrap_or_else(|| "顧客担当者".to_string());
    let customer_statements = build_customer_statement_evidence(
        &body_str(&body, "customer_statements"),
        optional_body_str(&body, "customer_statement_date").as_deref(),
        &statement_speaker,
    );

    let competitor_source_summary = match journey::summarize_competitor_csv(
        &competitor_bytes,
        competitor_label,
        competitor_captured_at.clone(),
    ) {
        Ok(summary) => summary,
        Err(message) => {
            return Json(json!({
                "status":"error",
                "message":format!("競合求人CSVを解析できません: {message}")
            }))
        }
    };
    let reviews =
        match journey::summarize_review_csv(&review_bytes, review_label, review_captured_at) {
            Ok(summary) => summary,
            Err(message) => {
                return Json(json!({
                    "status":"error",
                    "message":format!("口コミCSVを解析できません: {message}")
                }))
            }
        };

    // 1回目: 顧客求人の8項目を抽出し、コードで引用の実在を照合する。
    let fact_prompt = fact_extract::build_extract_prompt(&normalized.source_text);
    let fact_raw = match jobgen_llm(&fact_prompt, &fact_extract::response_schema(), 0.0).await {
        Ok(value) => value,
        Err(error) => {
            return Json(json!({
                "status":"error",
                "message":format!("顧客求人の事実抽出に失敗しました: {error}")
            }))
        }
    };
    let facts = fact_extract::verify(&normalized.source_text, &fact_raw);
    let facts_value = serde_json::to_value(&facts).unwrap_or_else(|_| json!({}));
    let job_facts = journey::build_job_fact_evidence(&facts_value);

    // 2回目: 比較母集団を選ぶための職種・地域プロフィールだけを抽出する。
    let profile_prompt = journey::build_case_profile_prompt(&normalized.source_text, &facts_value);
    let mut case_profile =
        match jobgen_llm(&profile_prompt, &journey::case_profile_schema(), 0.0).await {
            Ok(value) => value,
            Err(error) => {
                return Json(json!({
                    "status":"error",
                    "message":format!("比較条件の抽出に失敗しました: {error}")
                }))
            }
        };
    let employment_type = verified_fact_value(&facts, "employment_type");
    let work_location = verified_fact_value(&facts, "work_location");
    apply_verified_profile_fields(&mut case_profile, &employment_type, &work_location);

    let occupation_keywords = case_profile
        .get("occupation_keywords")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (cohort, competitor) = match journey::build_comparison_cohort(
        &competitor_bytes,
        competitor_label,
        competitor_captured_at,
        &value_text(&case_profile, "job_title"),
        &value_text(&case_profile, "occupation"),
        &occupation_keywords,
        &value_text(&case_profile, "prefecture"),
        &value_text(&case_profile, "municipality"),
        &employment_type,
    ) {
        Ok(result) => result,
        Err(message) => {
            return Json(json!({
                "status":"error",
                "message":format!("比較母集団を作成できません: {message}")
            }))
        }
    };

    if cohort.status == "blocked" {
        let warning = cohort.warning.clone();
        return Json(json!({
            "status":"ok",
            "phase":"cohort_blocked",
            "generated_at":chrono::Utc::now().to_rfc3339(),
            "facts":facts,
            "job_fact_evidence":job_facts,
            "customer_statement_evidence":customer_statements,
            "case_profile":case_profile,
            "competitor_source_summary":competitor_source_summary,
            "comparison_cohort":cohort,
            "review_summary":reviews,
            "quality_gate":{"passed":false,"issues":[warning]},
            "review_required":true,
            "llm_calls":2
        }));
    }
    let competitor = match competitor {
        Some(summary) => summary,
        None => {
            return Json(json!({
                "status":"error",
                "message":"比較対象求人を集計できませんでした。"
            }))
        }
    };
    let salary_text = verified_fact_value(&facts, "salary");
    let client_salary =
        journey::client_salary_position(&salary_text, &employment_type, &competitor);
    let public_stats = fetch_journey_public_stats(&state, &work_location).await;

    // 3回目以降: 4ペルソナと検索仮説。構造不備は最大2回まで自動補修する。
    let prepare_prompt = journey::build_prepare_prompt(
        &case_profile,
        &job_facts,
        &customer_statements,
        &competitor,
        &cohort,
        &reviews,
        client_salary.as_ref(),
        &public_stats,
        &employer_note,
    );
    let allowed_evidence_refs =
        journey::allowed_evidence_refs(&job_facts, &customer_statements, &competitor, &reviews);
    let mut llm_calls = 3;
    let mut result = match jobgen_llm(&prepare_prompt, &journey::prepare_schema(), 0.25).await {
        Ok(value) => value,
        Err(error) => {
            return Json(json!({
                "status":"error",
                "message":format!("ペルソナと検索仮説の生成に失敗しました: {error}")
            }))
        }
    };
    journey::normalize_evidence_aliases(&mut result);
    set_case_profile(&mut result, &case_profile);
    let mut quality_issues = journey::validate_prepare_result(&result, &allowed_evidence_refs);
    if !quality_issues.is_empty() {
        tracing::warn!(
            target: "jobgen_journey",
            attempt = 0,
            issues = ?quality_issues,
            "prepare quality gate requested repair"
        );
    }
    for _ in 0..2 {
        if quality_issues.is_empty() {
            break;
        }
        let repair_prompt =
            journey::build_prepare_repair_prompt(&prepare_prompt, &result, &quality_issues);
        result = match jobgen_llm(&repair_prompt, &journey::prepare_schema(), 0.1).await {
            Ok(value) => value,
            Err(error) => {
                quality_issues.push(format!("自動補修APIに失敗しました: {error}"));
                break;
            }
        };
        llm_calls += 1;
        journey::normalize_evidence_aliases(&mut result);
        set_case_profile(&mut result, &case_profile);
        quality_issues = journey::validate_prepare_result(&result, &allowed_evidence_refs);
        if !quality_issues.is_empty() {
            tracing::warn!(
                target: "jobgen_journey",
                issues = ?quality_issues,
                "prepare quality gate still failing after repair"
            );
        }
    }

    if !quality_issues.is_empty() {
        return Json(json!({
            "status":"ok",
            "phase":"quality_blocked",
            "generated_at":chrono::Utc::now().to_rfc3339(),
            "facts":facts,
            "job_fact_evidence":job_facts,
            "customer_statement_evidence":customer_statements,
            "case_profile":case_profile,
            "competitor_source_summary":competitor_source_summary,
            "competitor_summary":competitor,
            "comparison_cohort":cohort,
            "review_summary":reviews,
            "client_salary_position":client_salary,
            "public_stats":public_stats,
            "result":result,
            "quality_gate":{"passed":false,"issues":quality_issues},
            "review_required":true,
            "llm_calls":llm_calls
        }));
    }

    let case_id = uuid::Uuid::new_v4().to_string();
    {
        let mut store = journey_case_store().lock().await;
        store.retain(|_, value| value.created_at.elapsed() < JOURNEY_CASE_TTL);
        store.insert(
            case_id.clone(),
            PreparedJourneyCase {
                created_at: Instant::now(),
                case_profile: case_profile.clone(),
                job_facts: job_facts.clone(),
                customer_statements: customer_statements.clone(),
                competitor: competitor.clone(),
                reviews: reviews.clone(),
                public_stats: public_stats.clone(),
                prepare_result: result.clone(),
                allowed_evidence_refs,
            },
        );
    }

    Json(json!({
        "status":"ok",
        "phase":"prepared",
        "case_id":case_id,
        "case_expires_in_minutes":30,
        "generated_at":chrono::Utc::now().to_rfc3339(),
        "facts":facts,
        "job_fact_evidence":job_facts,
        "customer_statement_evidence":customer_statements,
        "case_profile":case_profile,
        "competitor_source_summary":competitor_source_summary,
        "competitor_summary":competitor,
        "comparison_cohort":cohort,
        "review_summary":reviews,
        "client_salary_position":client_salary,
        "public_stats":public_stats,
        "result":result,
        "quality_gate":{"passed":true,"issues":[]},
        "review_required":cohort.status == "limited",
        "llm_calls":llm_calls,
        "notes":{
            "truth_scope":"顧客企業について確定事実として扱うのは、顧客求人から引用照合できた項目と顧客発言だけです。",
            "review_scope":"口コミは求職者が触れ得る外部観測であり、会社の労働実態を断定する根拠にはしません。",
            "persona_scope":"ペルソナと離脱地点は、比較母集団・公的統計・職種一般論を組み合わせた検討仮説です。"
        }
    }))
}

/// `POST /api/jobgen/journey-persona-detail` — 検索実測値を反映して1ペルソナを診断。
pub async fn jobgen_journey_persona_detail(Json(body): Json<Value>) -> Json<Value> {
    let case_id = body_str(&body, "case_id");
    let persona_id = body_str(&body, "persona_id");
    if case_id.is_empty() || persona_id.is_empty() {
        return Json(json!({
            "status":"error",
            "message":"準備済みケースとペルソナを指定してください。"
        }));
    }
    let keyword_metrics = body
        .get("keyword_metrics")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let prepared = {
        let mut store = journey_case_store().lock().await;
        store.retain(|_, value| value.created_at.elapsed() < JOURNEY_CASE_TTL);
        store.get(&case_id).cloned()
    };
    let prepared = match prepared {
        Some(value) => value,
        None => {
            return Json(json!({
                "status":"error",
                "message":"準備データの有効期限が切れました。最初の分析からやり直してください。"
            }))
        }
    };
    let persona = prepared
        .prepare_result
        .get("personas")
        .and_then(Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .find(|value| value.get("id").and_then(Value::as_str) == Some(persona_id.as_str()))
        })
        .cloned();
    let persona = match persona {
        Some(value) => value,
        None => {
            return Json(json!({
                "status":"error",
                "message":"準備結果に存在しないペルソナです。"
            }))
        }
    };

    let base_prompt = journey::build_persona_detail_prompt(
        &prepared.case_profile,
        &persona,
        &prepared.job_facts,
        &prepared.customer_statements,
        &prepared.competitor,
        &prepared.reviews,
        &prepared.public_stats,
        &keyword_metrics,
    );
    let mut llm_calls = 1;
    let mut result = match jobgen_llm(&base_prompt, &journey::persona_detail_schema(), 0.25).await {
        Ok(value) => value,
        Err(error) => {
            return Json(json!({
                "status":"error",
                "message":format!("8段階ジャーニーの生成に失敗しました: {error}")
            }))
        }
    };
    journey::normalize_evidence_aliases(&mut result);
    let mut quality_issues =
        journey::validate_persona_detail(&result, &persona, &prepared.allowed_evidence_refs);
    if !quality_issues.is_empty() {
        tracing::warn!(
            target: "jobgen_journey",
            persona_id = %persona_id,
            issues = ?quality_issues,
            "persona detail quality gate requested repair"
        );
    }
    for _ in 0..2 {
        if quality_issues.is_empty() {
            break;
        }
        let repair_prompt =
            journey::build_detail_repair_prompt(&base_prompt, &result, &quality_issues);
        result = match jobgen_llm(&repair_prompt, &journey::persona_detail_schema(), 0.1).await {
            Ok(value) => value,
            Err(error) => {
                quality_issues.push(format!("自動補修APIに失敗しました: {error}"));
                break;
            }
        };
        llm_calls += 1;
        journey::normalize_evidence_aliases(&mut result);
        quality_issues =
            journey::validate_persona_detail(&result, &persona, &prepared.allowed_evidence_refs);
        if !quality_issues.is_empty() {
            tracing::warn!(
                target: "jobgen_journey",
                persona_id = %persona_id,
                issues = ?quality_issues,
                "persona detail quality gate still failing after repair"
            );
        }
    }

    if !quality_issues.is_empty() {
        return Json(json!({
            "status":"ok",
            "phase":"quality_blocked",
            "quality_gate":{"passed":false,"issues":quality_issues},
            "result":result,
            "review_required":true,
            "llm_calls":llm_calls
        }));
    }
    Json(json!({
        "status":"ok",
        "phase":"complete",
        "quality_gate":{"passed":true,"issues":[]},
        "result":result,
        "review_required":false,
        "llm_calls":llm_calls
    }))
}

fn build_customer_statement_evidence(
    raw: &str,
    stated_at: Option<&str>,
    speaker: &str,
) -> Vec<Value> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(20)
        .enumerate()
        .map(|(index, statement)| {
            json!({
                "source_ref":format!("U{}", index + 1),
                "speaker":truncate_text(speaker, 120),
                "stated_at":stated_at.unwrap_or("日付未入力"),
                "statement":truncate_text(statement, 500)
            })
        })
        .collect()
}

fn apply_verified_profile_fields(profile: &mut Value, employment_type: &str, work_location: &str) {
    let Some(object) = profile.as_object_mut() else {
        return;
    };
    if !employment_type.trim().is_empty() {
        object.insert(
            "employment_type".to_string(),
            Value::String(employment_type.trim().to_string()),
        );
    }
    if work_location.trim().is_empty() {
        return;
    }
    let location = crate::handlers::survey::location_parser::parse_location(work_location, None);
    if let Some(prefecture) = location.prefecture {
        object.insert("prefecture".to_string(), Value::String(prefecture));
    }
    if let Some(municipality) = location.municipality {
        object.insert("municipality".to_string(), Value::String(municipality));
    }
}

fn value_text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn set_case_profile(result: &mut Value, case_profile: &Value) {
    if let Some(object) = result.as_object_mut() {
        object.insert("case_profile".to_string(), case_profile.clone());
    }
}

fn truncate_text(value: &str, limit: usize) -> String {
    let mut output = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        output.push('…');
    }
    output
}

fn optional_body_str(body: &Value, key: &str) -> Option<String> {
    body.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn verified_fact_value(facts: &crate::job_gen::types::ExtractedFacts, key: &str) -> String {
    facts
        .get(key)
        .filter(|field| field.status == "verified")
        .map(|field| field.value.clone())
        .unwrap_or_default()
}

/// 求人の検証済み勤務地から、既存の公的統計を読み取り専用で取得する。
///
/// 地域の母集団・通勤・住居費をペルソナ仮説の補助にするが、
/// 個人の応募意向や採用可能人数には変換しない。
async fn fetch_journey_public_stats(state: &Arc<AppState>, location_text: &str) -> Value {
    use crate::handlers::survey::location_parser::parse_location;

    if location_text.trim().is_empty() {
        return json!({
            "available":false,
            "reason":"顧客求人から勤務地を引用確認できなかったため、公的統計は未取得です。"
        });
    }
    let location = parse_location(location_text, None);
    let prefecture = match location.prefecture.clone() {
        Some(value) => value,
        None => {
            return json!({
                "available":false,
                "reason":"勤務地から都道府県を特定できなかったため、公的統計は未取得です。",
                "location_observation":location_text
            })
        }
    };
    let municipality = location.municipality.clone().unwrap_or_default();
    let db = match state.hw_db.clone() {
        Some(db) => db,
        None => {
            return json!({
                "available":false,
                "reason":"統計参照用データベースに接続できないため、公的統計は未取得です。",
                "prefecture":prefecture,
                "municipality":municipality
            })
        }
    };
    let turso = state.turso_db.clone();
    let pref_for_query = prefecture.clone();
    let muni_for_query = municipality.clone();

    tokio::task::spawn_blocking(move || {
        use crate::handlers::analysis::fetch as analysis_fetch;
        use crate::handlers::helpers::{get_f64_opt, get_i64_opt, get_str};

        let labor = analysis_fetch::fetch_labor_force(
            &db,
            turso.as_ref(),
            &pref_for_query,
            &muni_for_query,
        );
        let daytime = analysis_fetch::fetch_daytime_population(
            &db,
            turso.as_ref(),
            &pref_for_query,
            &muni_for_query,
        );
        let rental = analysis_fetch::fetch_rental_housing(
            &db,
            turso.as_ref(),
            &pref_for_query,
            &muni_for_query,
        );
        let minimum_wage = analysis_fetch::fetch_minimum_wage(&db, &pref_for_query);
        let inflow = analysis_fetch::fetch_commute_inflow(
            &db,
            turso.as_ref(),
            &pref_for_query,
            &muni_for_query,
        );

        let labor_row = labor.first();
        let daytime_row = daytime.first();
        let minimum_wage_row = minimum_wage.first();
        let local_rent = rental
            .iter()
            .find(|row| {
                get_str(row, "prefecture") == pref_for_query
                    && matches!(get_str(row, "structure").as_str(), "" | "総数")
                    && matches!(get_str(row, "area_class").as_str(), "" | "総数")
            })
            .or_else(|| {
                rental
                    .iter()
                    .find(|row| get_str(row, "prefecture") == pref_for_query)
            });
        let area_scope = if muni_for_query.is_empty() {
            pref_for_query.clone()
        } else {
            format!("{pref_for_query} {muni_for_query}")
        };
        let labor_reference_date = labor_row
            .map(|row| get_str(row, "reference_date"))
            .unwrap_or_default();
        let daytime_reference_year =
            daytime_row.and_then(|row| get_i64_opt(row, "reference_year"));
        let housing_reference_date = local_rent
            .map(|row| get_str(row, "as_of"))
            .unwrap_or_default();
        let minimum_wage_effective_date = minimum_wage_row
            .map(|row| get_str(row, "effective_date"))
            .unwrap_or_default();
        let minimum_wage_fiscal_year =
            minimum_wage_row.and_then(|row| get_i64_opt(row, "fiscal_year"));
        let commute_reference_year = inflow
            .iter()
            .map(|row| row.reference_year)
            .filter(|year| *year > 0)
            .max();

        let commute_origins = inflow
            .iter()
            .take(5)
            .map(|row| {
                json!({
                    "prefecture":row.partner_pref,
                    "municipality":row.partner_muni,
                    "commuters":row.total_commuters,
                    "reference_year":row.reference_year
                })
            })
            .collect::<Vec<_>>();

        let available = labor_row.is_some()
            || daytime_row.is_some()
            || local_rent.is_some()
            || minimum_wage_row.is_some()
            || !inflow.is_empty();
        let mut source_details = Vec::new();
        if labor_row.is_some() {
            source_details.push(json!({
                "label":"労働力人口",
                "source":"国勢調査・SSDSE 労働力統計",
                "reference_date":labor_reference_date,
                "scope":area_scope
            }));
        }
        if daytime_row.is_some() {
            source_details.push(json!({
                "label":"昼夜間人口",
                "source":"国勢調査 従業地・通学地集計",
                "reference_year":daytime_reference_year,
                "scope":area_scope
            }));
        }
        if !inflow.is_empty() {
            source_details.push(json!({
                "label":"通勤流入",
                "source":"国勢調査 通勤OD",
                "reference_year":commute_reference_year,
                "scope":area_scope
            }));
        }
        if local_rent.is_some() {
            source_details.push(json!({
                "label":"住宅・家賃",
                "source":"住宅・土地統計調査",
                "reference_date":housing_reference_date,
                "scope":area_scope
            }));
        }
        if minimum_wage_row.is_some() {
            source_details.push(json!({
                "label":"最低賃金",
                "source":"厚生労働省 地域別最低賃金",
                "effective_date":minimum_wage_effective_date,
                "fiscal_year":minimum_wage_fiscal_year,
                "scope":pref_for_query
            }));
        }
        json!({
            "available":available,
            "area":{
                "prefecture":pref_for_query,
                "municipality":muni_for_query
            },
            "labor_force":{
                "employed":labor_row.and_then(|row| get_i64_opt(row, "employed")),
                "unemployed":labor_row.and_then(|row| get_i64_opt(row, "unemployed")),
                "not_in_labor_force":labor_row.and_then(|row| get_i64_opt(row, "not_in_labor_force")),
                "unemployment_rate_percent":labor_row.and_then(|row| get_f64_opt(row, "unemployment_rate")),
                "labor_force_participation_rate_percent":labor_row.and_then(|row| get_f64_opt(row, "labor_force_participation_rate")),
                "reference_date":labor_reference_date
            },
            "daytime_population":{
                "nighttime_population":daytime_row.and_then(|row| get_i64_opt(row, "nighttime_pop")),
                "daytime_population":daytime_row.and_then(|row| get_i64_opt(row, "daytime_pop")),
                "day_night_ratio_percent":daytime_row.and_then(|row| get_f64_opt(row, "day_night_ratio")),
                "inflow_population":daytime_row.and_then(|row| get_i64_opt(row, "inflow_pop")),
                "outflow_population":daytime_row.and_then(|row| get_i64_opt(row, "outflow_pop")),
                "reference_year":daytime_reference_year
            },
            "commute_origins":commute_origins,
            "commute_reference_year":commute_reference_year,
            "housing":{
                "rent_per_tatami_yen":local_rent.and_then(|row| get_i64_opt(row, "median_rent_jpy")),
                "reference_date":housing_reference_date,
                "unit_note":"住宅・土地統計の1畳当たり家賃。月額家賃ではありません。"
            },
            "minimum_wage":{
                "hourly_yen":minimum_wage_row.and_then(|row| get_i64_opt(row, "hourly_min_wage")),
                "effective_date":minimum_wage_effective_date,
                "fiscal_year":minimum_wage_fiscal_year,
                "area_note":"最低賃金は都道府県単位。顧客求人の適法性判定には算入賃金と所定労働時間の確認が必要。"
            },
            "source_details":source_details,
            "sources":[
                "国勢調査・SSDSE 労働力統計",
                "国勢調査 従業地・通学地集計",
                "国勢調査 通勤OD",
                "住宅・土地統計調査",
                "厚生労働省 地域別最低賃金"
            ],
            "caveat":"地域集計は人材母集団を考える補助情報で、個人の応募意向や採用可能人数を示しません。"
        })
    })
    .await
    .unwrap_or_else(|_| {
        json!({
            "available":false,
            "reason":"公的統計の取得処理が完了しませんでした。",
            "prefecture":prefecture,
            "municipality":municipality
        })
    })
}

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
    let b64 = body
        .get("data_base64")
        .and_then(Value::as_str)
        .map(String::from);
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
    let count = body
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(3, 5) as usize;
    let prompt = strategy::build_personas_prompt(&source, &analysis, count);
    let schema = strategy::personas_schema();
    match jobgen_llm(&prompt, &schema, 0.7).await {
        Ok(v) => Json(
            json!({"status":"ok","personas": v.get("personas").cloned().unwrap_or(Value::Null)}),
        ),
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/copy` — 工程④: キャッチコピー (1ペルソナ分)。
///
/// 検証: 法令NG(ng_violations) + 表現レビュー(expression_warnings) + 数値照合(number_violations)。
/// `source_text` (任意) を渡すと数値照合が有効化され、プロンプトにも原文制約が注入される。
pub async fn jobgen_copy(Json(body): Json<Value>) -> Json<Value> {
    let persona = body.get("persona").cloned().unwrap_or(Value::Null);
    let analysis = body.get("analysis").cloned().unwrap_or(Value::Null);
    let source = body_str(&body, "source_text");
    let prompt = strategy::build_copy_prompt(&persona, &analysis, &source);
    let schema = strategy::copy_schema();
    match jobgen_llm(&prompt, &schema, 0.9).await {
        Ok(v) => {
            let copies = v.get("copies").cloned().unwrap_or(Value::Null);
            let texts = strings_at(&copies, "text");
            let (ng, expr, r1) = ng_and_expression_gate(&texts);
            let (num, num_check, r2) = number_gate(&source, &texts);
            Json(json!({
                "status": "ok",
                "copies": copies,
                "ng_violations": ng,
                "expression_warnings": expr,
                "number_violations": num,
                "number_check": num_check,
                "review_required": r1 || r2,
            }))
        }
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/images` — 工程⑤: 画像ディレクション。
///
/// `source_text` (任意) を渡すと direction 文へ数値照合が有効化され、プロンプトにも
/// 原文制約 (身だしなみ規定・勤務条件) が注入される。
pub async fn jobgen_images(Json(body): Json<Value>) -> Json<Value> {
    let personas = body.get("personas").cloned().unwrap_or(Value::Null);
    let source = body_str(&body, "source_text");
    let prompt = strategy::build_images_prompt(&personas, &source);
    let schema = strategy::images_schema();
    match jobgen_llm(&prompt, &schema, 0.7).await {
        Ok(v) => {
            let directions = v.get("directions").cloned().unwrap_or(Value::Null);
            let texts = strings_at(&directions, "direction");
            let (num, num_check, review) = number_gate(&source, &texts);
            Json(json!({
                "status": "ok",
                "directions": directions,
                "number_violations": num,
                "number_check": num_check,
                "review_required": review,
            }))
        }
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
    let source = body_str(&body, "source_text");
    let prompt = strategy::build_image_prompts_prompt(&directions, &personas, &source);
    let schema = strategy::image_prompts_schema();
    match jobgen_llm(&prompt, &schema, 0.4).await {
        Ok(v) => {
            let prompts = v.get("prompts").cloned().unwrap_or(Value::Null);
            let texts = strings_at(&prompts, "prompt");
            let (num, num_check, review) = number_gate(&source, &texts);
            Json(json!({
                "status": "ok",
                "prompts": prompts,
                "number_violations": num,
                "number_check": num_check,
                "review_required": review,
            }))
        }
        Err(e) => Json(json!({"status":"error","message": e.to_string()})),
    }
}

/// `POST /api/jobgen/mobile` — 工程⑥: スマホ原稿 (1ペルソナ分)。
///
/// 検証: 法令NG(ng_violations) + 表現レビュー(expression_warnings) + 数値照合(number_violations)。
/// `source_text` (任意) を渡すと結合本文へ数値照合が有効化される (facts_text とは別枠)。
pub async fn jobgen_mobile(Json(body): Json<Value>) -> Json<Value> {
    let persona = body.get("persona").cloned().unwrap_or(Value::Null);
    let facts_text = body_str(&body, "facts_text");
    let source = body_str(&body, "source_text");
    let prompt = strategy::build_mobile_prompt(&persona, &facts_text);
    let schema = strategy::mobile_schema();
    match jobgen_llm(&prompt, &schema, 0.8).await {
        Ok(v) => {
            let lines: Vec<String> = v
                .get("lines")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let joined = lines.join("\n");
            let texts = vec![joined];
            let (ng, expr, r1) = ng_and_expression_gate(&texts);
            let (num, num_check, r2) = number_gate(&source, &texts);
            Json(json!({
                "status": "ok",
                "lines": lines,
                "ng_violations": ng,
                "expression_warnings": expr,
                "number_violations": num,
                "number_check": num_check,
                "review_required": r1 || r2,
            }))
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
            Err(e) => {
                return Json(json!({"status":"error","message":format!("facts の形式が不正: {e}")}))
            }
        },
        None => return Json(json!({"status":"error","message":"facts が必要です"})),
    };
    let ng = match load_ng_rules() {
        Ok(n) => n,
        Err(e) => {
            return Json(json!({"status":"error","message":format!("NGワードルール読込失敗: {e}")}))
        }
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
            format!(
                "{strategy_hint}\n# 前回生成の問題点(必ず回避すること)\n{}",
                issues.join("\n")
            )
        };
        let prompt = hrhacker::build_generation_prompt(&facts_text, &hint);
        let raw = match jobgen_llm(&prompt, &schema, 0.4).await {
            Ok(v) => v,
            Err(e) => return Json(json!({"status":"error","message": e.to_string()})),
        };
        let generated = hrhacker::validate_generated(&source, &raw, &ng);
        attempts = attempt + 1;
        let review_count = generated
            .values()
            .filter(|g| g.status == "review_required")
            .count();
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
        ordered.insert(
            col.to_string(),
            Value::String(row.get(col).cloned().unwrap_or_default()),
        );
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
    // 転記充足率と未割当ヒント (レビュー指摘[B5]: 生成列の検証と転記の充足を分けて示す)。
    let fill_stats = hrhacker::fill_stats(&row);
    let unassigned_hints = hrhacker::detect_unassigned_hints(&source, &row);
    Json(json!({
        "status":"ok",
        "attempts": attempts,
        "row": Value::Object(ordered),
        "generated_fields": generated,
        "review_required_fields": review,
        "unsupported_numbers": unsupported,
        "fill_stats": fill_stats,
        "unassigned_hints": unassigned_hints,
    }))
}

/// `POST /api/jobgen/ab` — 工程⑧: A/Bテスト助言。
///
/// 検証: 法令NG(ng_violations) + 表現レビュー(expression_warnings) + 数値照合(number_violations)。
/// レビュー指摘: 工程⑧が前工程でNG判定された語(例「主婦」)を再使用しないよう、ここにも
/// NGゲートを掛ける。`source_text` (任意) を渡すと数値照合と原文制約が有効化される。
pub async fn jobgen_ab(Json(body): Json<Value>) -> Json<Value> {
    let summary = body_str(&body, "summary");
    let source = body_str(&body, "source_text");
    let prompt = strategy::build_ab_prompt(&summary, &source);
    let schema = strategy::ab_schema();
    match jobgen_llm(&prompt, &schema, 0.4).await {
        Ok(v) => {
            let steps = v.get("steps").cloned().unwrap_or(Value::Null);
            // metric と action を結合した文を検証対象にする。
            let texts: Vec<String> = steps
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|s| {
                            let m = s.get("metric").and_then(Value::as_str).unwrap_or("");
                            let ac = s.get("action").and_then(Value::as_str).unwrap_or("");
                            format!("{m} {ac}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            let (ng, expr, r1) = ng_and_expression_gate(&texts);
            let (num, num_check, r2) = number_gate(&source, &texts);
            Json(json!({
                "status": "ok",
                "steps": steps,
                "ng_violations": ng,
                "expression_warnings": expr,
                "number_violations": num,
                "number_check": num_check,
                "review_required": r1 || r2,
            }))
        }
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
        Err(e) => {
            return Json(json!({"status":"error","message":format!("NGワードルール読込失敗: {e}")}))
        }
    };
    let items = match body.get("items").and_then(Value::as_array) {
        Some(a) => a,
        None => return Json(json!({"status":"error","message":"items(配列)が必要です"})),
    };
    const MAX_ITEMS: usize = 50_000; // 公開求人×十数列を1リクエストで賄える上限。
    if items.len() > MAX_ITEMS {
        return Json(
            json!({"status":"error","message":format!("items が多すぎます(最大{MAX_ITEMS})")}),
        );
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

/// JSON 配列 (オブジェクト要素) の各要素から `key` の文字列値を集める。
/// `arr` が配列でない、または要素に `key` が無ければその要素は飛ばす。
fn strings_at(arr: &Value, key: &str) -> Vec<String> {
    arr.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|item| item.get(key).and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
