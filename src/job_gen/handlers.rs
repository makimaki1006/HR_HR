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

use axum::extract::{Query, State};
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
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

/// ダッシュボードのタブ「求人票作成」断片 (2026-08-04)。
///
/// それまで求人票生成・競合比較・ジャーニー診断は別ブラウザタブへ飛ばしていたが、
/// アプリ内で完結するよう同一オリジン iframe で切り替え表示する。
/// 各ツールの画面・API・認証は一切変更しない (iframe はセッション Cookie を共有)。
/// 全画面で使いたい場合のために「別ウィンドウで開く」も残す。
pub async fn tab_jobgen_tools() -> axum::response::Html<&'static str> {
    // iframe は3枚を hidden で切り替える (2026-08-04 レビュー指摘の修正)。
    // 単一 iframe の src 差し替えだと、切り替えのたびにページが再ロードされ
    // 入力途中の求人票や診断結果が消える (旧・別ブラウザタブ時代からの機能後退)。
    // src は初回表示時にだけ data-src から設定する遅延ロード。
    axum::response::Html(
        r#"<div class="space-y-4">
  <h2 class="text-lg font-bold text-slate-100">求人票作成</h2>
  <div class="flex flex-wrap items-center gap-2" id="jobtools-nav" role="tablist" aria-label="求人票作成ツール">
    <button type="button" class="tab-btn active" role="tab" aria-selected="true" data-frame="jt-frame-0" data-src="/jobgen" onclick="jobtoolsSwitch(this)">求人票生成</button>
    <button type="button" class="tab-btn" role="tab" aria-selected="false" data-frame="jt-frame-1" data-src="/jobgen/competitive-beta" onclick="jobtoolsSwitch(this)">競合比較から求人作成</button>
    <button type="button" class="tab-btn" role="tab" aria-selected="false" data-frame="jt-frame-2" data-src="/jobgen/applicant-journey-beta" onclick="jobtoolsSwitch(this)">応募者ジャーニー診断</button>
    <a id="jobtools-open" href="/jobgen" target="_blank" rel="noopener" class="text-[11px] text-slate-400 underline ml-auto">別ウィンドウで開く ↗</a>
  </div>
  <div id="jobtools-frames">
    <iframe id="jt-frame-0" src="/jobgen" title="求人票生成"
            style="width:100%;height:calc(100vh - 230px);min-height:560px;border:0;border-radius:12px;background:#fff"></iframe>
    <iframe id="jt-frame-1" data-src="/jobgen/competitive-beta" title="競合比較から求人作成" hidden
            style="width:100%;height:calc(100vh - 230px);min-height:560px;border:0;border-radius:12px;background:#fff"></iframe>
    <iframe id="jt-frame-2" data-src="/jobgen/applicant-journey-beta" title="応募者ジャーニー診断" hidden
            style="width:100%;height:calc(100vh - 230px);min-height:560px;border:0;border-radius:12px;background:#fff"></iframe>
  </div>
  <script>
    function jobtoolsSwitch(btn){
      var nav=document.getElementById("jobtools-nav");
      nav.querySelectorAll("button").forEach(function(b){b.classList.remove("active");b.setAttribute("aria-selected","false");});
      btn.classList.add("active");btn.setAttribute("aria-selected","true");
      var frames=document.getElementById("jobtools-frames");
      frames.querySelectorAll("iframe").forEach(function(f){f.hidden=true;});
      var frame=document.getElementById(btn.getAttribute("data-frame"));
      if(frame){
        // 初回だけロード。以降は hidden 切替のみで状態を保つ。
        if(!frame.getAttribute("src")&&frame.getAttribute("data-src")){frame.src=frame.getAttribute("data-src");}
        frame.hidden=false;
      }
      document.getElementById("jobtools-open").href=btn.getAttribute("data-src");
    }
  </script>
</div>"#,
    )
}

/// 埋め込みNGワードルール (コンパイル時同梱。正本= Sheets「求人系」NGワードタブ)。
const EMBEDDED_NG_WORDS_JSON: &str = include_str!("../../assets/ng_words.json");

/// 埋め込み表現レビュー辞書 (警告レベル。法令NGとは別系統。severity=warning)。
const EMBEDDED_EXPRESSION_RULES_JSON: &str =
    include_str!("../../assets/expression_review_rules.json");

const JOURNEY_CASE_TTL: Duration = Duration::from_secs(30 * 60);
const JOURNEY_GOOGLE_ADS_WINDOW: Duration = Duration::from_secs(60);
const JOURNEY_GOOGLE_ADS_REQUEST_LIMIT: usize = 15;
// 地域解決1回 + 履歴指標1回 + 429時の最大3再試行を保守的に予約する。
const JOURNEY_GOOGLE_ADS_RESERVED_REQUESTS: usize = 5;

#[derive(Clone)]
struct PreparedJourneyCase {
    created_at: Instant,
    case_profile: Value,
    job_facts: Vec<Value>,
    customer_statements: Vec<Value>,
    competitor: journey::CompetitorSummary,
    reviews: journey::ReviewSummary,
    /// 人気求人オプションのP番号根拠 (未入力なら空)。詳細プロンプトにも渡す。
    popular_jobs: Vec<Value>,
    public_stats: Value,
    prepare_result: Value,
    /// ゲート通過済みの8段階診断結果 (persona_id → result)。note記事案の生成が
    /// クライアント送信値でなくサーバー保存値に接地するために持つ (2026-08-07)。
    persona_details: HashMap<String, Value>,
    allowed_evidence_refs: HashSet<String>,
    keyword_metrics_by_persona: HashMap<String, Value>,
    keyword_metrics_by_query: HashMap<String, Value>,
    keyword_completed_personas: HashSet<String>,
    keyword_measurement_status: Option<String>,
    /// UI が読む形 (`{keyword, avg_monthly}`) の関連語候補。ケース単位で1回だけ取得する。
    keyword_suggestions: Vec<Value>,
    /// 関連語候補の取得を試行済みか。資格情報未設定のときは false のままにして、
    /// 設定後の再実行で取得できるようにする。
    keyword_suggestions_fetched: bool,
}

fn journey_case_store() -> &'static AsyncMutex<HashMap<String, PreparedJourneyCase>> {
    static STORE: OnceLock<AsyncMutex<HashMap<String, PreparedJourneyCase>>> = OnceLock::new();
    STORE.get_or_init(|| AsyncMutex::new(HashMap::new()))
}

fn journey_keyword_fetch_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn journey_google_ads_budget() -> &'static AsyncMutex<VecDeque<Instant>> {
    static BUDGET: OnceLock<AsyncMutex<VecDeque<Instant>>> = OnceLock::new();
    BUDGET.get_or_init(|| AsyncMutex::new(VecDeque::new()))
}

pub(crate) fn reserve_journey_google_ads_budget(
    requests: &mut VecDeque<Instant>,
    now: Instant,
    cost: usize,
) -> Option<Duration> {
    while requests
        .front()
        .is_some_and(|started| now.duration_since(*started) >= JOURNEY_GOOGLE_ADS_WINDOW)
    {
        requests.pop_front();
    }
    if requests.len() + cost <= JOURNEY_GOOGLE_ADS_REQUEST_LIMIT {
        requests.extend((0..cost).map(|_| now));
        return None;
    }
    requests.front().map(|started| {
        JOURNEY_GOOGLE_ADS_WINDOW
            .saturating_sub(now.duration_since(*started))
            .saturating_add(Duration::from_millis(25))
    })
}

pub(crate) fn refresh_latest_journey_google_ads_reservation(
    requests: &mut VecDeque<Instant>,
    completed_at: Instant,
    cost: usize,
) {
    let reservation_start = requests.len().saturating_sub(cost);
    for request in requests.iter_mut().skip(reservation_start) {
        *request = completed_at;
    }
}

async fn acquire_journey_google_ads_budget() {
    loop {
        let wait = {
            let mut requests = journey_google_ads_budget().lock().await;
            reserve_journey_google_ads_budget(
                &mut requests,
                Instant::now(),
                JOURNEY_GOOGLE_ADS_RESERVED_REQUESTS,
            )
        };
        match wait {
            Some(duration) => tokio::time::sleep(duration).await,
            None => return,
        }
    }
}

async fn finish_journey_google_ads_budget() {
    let mut requests = journey_google_ads_budget().lock().await;
    refresh_latest_journey_google_ads_reservation(
        &mut requests,
        Instant::now(),
        JOURNEY_GOOGLE_ADS_RESERVED_REQUESTS,
    );
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
/// 6ペルソナと検索仮説を作る。品質基準を満たすまで後工程用の case_id は発行しない。
pub async fn jobgen_journey_diagnose(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    // 顧客求人はテキスト/HTML 貼り付けのほか PDF 添付でも受ける (2026-08-05)。
    // PDF が付いていれば client_kind (html/text) は無視し、PDF のテキスト抽出結果を
    // そのまま既存フロー (事実抽出 → 引用照合) に流す。
    let client_pdf_base64 = body_str(&body, "client_pdf_base64");
    let has_client_pdf = !client_pdf_base64.trim().is_empty();
    let raw_client = body_str(&body, "client_job");
    if !has_client_pdf {
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
    }
    let normalized = if has_client_pdf {
        // base64 長・復号後サイズ・PDF テキスト抽出の失敗はいずれも inputs 側が
        // 具体的な理由付きのエラーにするため、そのまま文面に載せる。
        match inputs::normalize(inputs::InputKind::Pdf, None, None, Some(client_pdf_base64)).await {
            Ok(mut jobs) if !jobs.is_empty() => jobs.remove(0),
            Ok(_) => {
                return Json(json!({
                    "status":"error",
                    "message":"顧客求人PDFを読み取れません: テキストを1件も抽出できませんでした。"
                }))
            }
            Err(error) => {
                return Json(json!({
                    "status":"error",
                    "message":format!("顧客求人PDFを読み取れません: {error}")
                }))
            }
        }
    } else {
        let input_kind = if body_str(&body, "client_kind") == "html" {
            inputs::InputKind::Html
        } else {
            inputs::InputKind::FreeText
        };
        match inputs::normalize(input_kind, Some(raw_client), None, None).await {
            Ok(mut jobs) if !jobs.is_empty() => jobs.remove(0),
            Ok(_) => {
                return Json(json!({
                    "status":"error",
                    "message":"顧客求人から本文を取得できませんでした。"
                }))
            }
            Err(error) => return Json(json!({"status":"error","message":error.to_string()})),
        }
    };
    if normalized.source_text.trim().chars().count() < 20 {
        // 画像スキャンPDFはテキスト層を持たず、抽出結果がほぼ空になる。
        // 貼り付け入力と原因が違うので、PDF のときは対処が分かる文面にする。
        let message = if has_client_pdf {
            "顧客求人PDFを読み取れません: 抽出できた本文が20文字未満です(文字情報を持たない画像PDFの可能性があります。テキスト貼り付けをお試しください)。"
        } else {
            "顧客求人から分析可能な本文を取得できませんでした。"
        };
        return Json(json!({"status":"error","message":message}));
    }

    let competitor_bytes =
        match journey::decode_csv_base64(&body_str(&body, "competitor_csv_base64"), "競合求人CSV")
        {
            Ok(bytes) => bytes,
            Err(message) => return Json(json!({"status":"error","message":message})),
        };
    // 口コミCSVは任意 (2026-08-04)。顧客が Google ビジネスプロフィール等を
    // 持っていないことは普通にあるため、未提供なら空サマリで診断を続行する
    // (R番号・口コミ件数集計が許可根拠から自動的に消える設計に乗る)。
    let review_base64 = body_str(&body, "review_csv_base64");
    let review_bytes = if review_base64.trim().is_empty() {
        None
    } else {
        match journey::decode_csv_base64(&review_base64, "口コミCSV") {
            Ok(bytes) => Some(bytes),
            Err(message) => return Json(json!({"status":"error","message":message})),
        }
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
    // 人気求人の逆算オプション (2026-08-06)。P番号根拠の組み立ては比較母集団の
    // 確定後に行う (手貼り全文とCSVの人気タグ行を社名照合するため)。
    let popular_raw = body
        .get("popular_jobs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

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
    // 2026-08-04: 口コミCSVは任意入力なので、解析できなくても診断全体は止めない。
    // 添付されたのに使えなかった場合は、その理由を warning として明示したうえで
    // 「未提供」と同じ縮退 (R番号・口コミ件数集計が許可されない) で続行する。
    // 黙って無視はしない — 画面の口コミ欄は「未提供」表示になり、理由が警告に出る。
    let (reviews, review_csv_warning) = match &review_bytes {
        None => (journey::ReviewSummary::not_provided(), None),
        Some(bytes) => {
            match journey::summarize_review_csv(bytes, review_label, review_captured_at) {
                Ok(summary) => (summary, None),
                Err(message) => (
                    journey::ReviewSummary::not_provided(),
                    Some(format!(
                        "口コミCSV「{review_label}」は使用しませんでした。{message}"
                    )),
                ),
            }
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

    // 2026-08-04: 比較母集団が成立しない場合でも診断を停止しない。
    // 以前は blocked で全体を止めていたが、「競合CSVが別地域・別職種だった」は
    // 実運用で普通に起きる (実例: 沖縄の消防設備点検 × 川崎のドライバーCSV)。
    // 無関係な求人と給与比較しない原則は、競合由来の根拠 (C番号・競合集計・給与比較)
    // を許可リストから外すことで守り、その事実を警告として明示して続行する。
    let mut cohort = cohort;
    let competitor = if cohort.status == "blocked" {
        cohort.warning = format!(
            "{} 競合比較の根拠なしで診断を続行します（ペルソナ・対策は顧客求人の事実と職種一般仮説に基づきます）。",
            cohort.warning
        )
        .trim()
        .to_string();
        journey::CompetitorSummary::not_comparable(&competitor_source_summary)
    } else {
        match competitor {
            Some(summary) => summary,
            None => {
                return Json(json!({
                    "status":"error",
                    "message":"比較対象求人を集計できませんでした。"
                }))
            }
        }
    };
    // 手貼り全文をCSVの人気タグ行と社名照合し、残り枠を自動候補で埋める
    // (比較不能CSVは候補が空。根拠なし・照合不能の手貼りは警告して除外)。
    let (popular_jobs, popular_warnings) =
        journey::build_popular_job_evidence(&popular_raw, &competitor);

    let salary_text = verified_fact_value(&facts, "salary");
    let client_salary =
        journey::client_salary_position(&salary_text, &employment_type, &competitor);
    let public_stats = fetch_journey_public_stats(&state, &work_location).await;
    let mut allowed_evidence_refs = journey::allowed_evidence_refs(
        &job_facts,
        &customer_statements,
        &competitor,
        &reviews,
        &popular_jobs,
    );
    if client_salary.is_some() {
        allowed_evidence_refs.insert("給与比較".to_string());
    }
    if public_stats
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        allowed_evidence_refs.insert("公的統計".to_string());
    }

    // 3回目以降: 6ペルソナと検索仮説。構造不備は最大2回まで自動補修する。
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
        &popular_jobs,
        &allowed_evidence_refs,
    );
    let prepare_schema = journey::prepare_schema_with_evidence_refs(&allowed_evidence_refs);
    let mut llm_calls = 3;
    let mut result = match jobgen_llm(&prepare_prompt, &prepare_schema, 0.25).await {
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
        result = match jobgen_llm(&repair_prompt, &prepare_schema, 0.1).await {
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
            "review_csv_warning":review_csv_warning,
            "popular_job_evidence":popular_jobs,
            "popular_jobs_warnings":popular_warnings,
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
                popular_jobs: popular_jobs.clone(),
                public_stats: public_stats.clone(),
                prepare_result: result.clone(),
                persona_details: HashMap::new(),
                allowed_evidence_refs,
                keyword_metrics_by_persona: HashMap::new(),
                keyword_metrics_by_query: HashMap::new(),
                keyword_completed_personas: HashSet::new(),
                keyword_measurement_status: None,
                keyword_suggestions: Vec::new(),
                keyword_suggestions_fetched: false,
            },
        );
    }

    // ready 以外 (limited=小標本 / blocked=縮退続行)・口コミCSVや人気求人の入力が
    // 使えなかった場合はコンサル確認を促す
    let review_required =
        cohort.status != "ready" || review_csv_warning.is_some() || !popular_warnings.is_empty();
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
        "review_csv_warning":review_csv_warning,
        "popular_job_evidence":popular_jobs,
        "popular_jobs_warnings":popular_warnings,
        "client_salary_position":client_salary,
        "public_stats":public_stats,
        "result":result,
        "quality_gate":{"passed":true,"issues":[]},
        "review_required":review_required,
        "llm_calls":llm_calls,
        "notes":{
            "truth_scope":"顧客企業について確定事実として扱うのは、顧客求人から引用照合できた項目と顧客発言だけです。",
            "review_scope":"口コミは求職者が触れ得る外部観測であり、会社の労働実態を断定する根拠にはしません。",
            "persona_scope":"ペルソナと離脱地点は、比較母集団・公的統計・職種一般論を組み合わせた検討仮説です。"
        }
    }))
}

/// 関連語候補の種にする代表検索語の上限。Google広告への問い合わせを1回に抑えるため少数に絞る。
const JOURNEY_SUGGESTION_SEED_LIMIT: usize = 3;
/// 画面に出す関連語候補の上限。
const JOURNEY_SUGGESTION_LIMIT: usize = 12;

/// 関連語候補の種になる代表検索語を選ぶ。
///
/// 準備結果の先頭にある（＝最重要の）ペルソナから importance 高→中→低 の順に数語だけ取り、
/// 全検索語を種にしないことで Google広告への問い合わせを1リクエストに保つ。
pub(crate) fn journey_suggestion_seeds(personas: &[Value], limit: usize) -> Vec<String> {
    fn importance_rank(query: &Value) -> u8 {
        match query
            .get("importance")
            .and_then(Value::as_str)
            .map(str::trim)
        {
            Some("高") => 0,
            Some("中") => 1,
            _ => 2,
        }
    }
    for persona in personas {
        let mut queries = persona
            .get("search_queries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        // sort_by_key は安定ソートなので、同じ重要度の中では元の並び順が保たれる。
        queries.sort_by_key(importance_rank);
        let mut seeds: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for query in &queries {
            let Some(text) = query.get("query").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if text.is_empty() || !seen.insert(text.to_string()) {
                continue;
            }
            seeds.push(text.to_string());
            if seeds.len() >= limit {
                break;
            }
        }
        if !seeds.is_empty() {
            return seeds;
        }
    }
    Vec::new()
}

/// suggest 応答から画面が読む `{keyword, avg_monthly}` だけを取り出す。
///
/// 既に検索需要表へ出している語は候補から除く。status が ok 以外なら空配列。
pub(crate) fn journey_suggestions_from_response(
    response: &Value,
    exclude: &HashSet<String>,
    limit: usize,
) -> Vec<Value> {
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    response
        .get("suggestions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let keyword = item.get("keyword").and_then(Value::as_str).map(str::trim)?;
            if keyword.is_empty() || exclude.contains(keyword) || !seen.insert(keyword.to_string())
            {
                return None;
            }
            Some(json!({
                "keyword":keyword,
                "avg_monthly":item.get("avg_monthly").cloned().unwrap_or(Value::Null),
            }))
        })
        .take(limit)
        .collect()
}

/// 関連語候補を Google広告から取得する。
///
/// 戻り値 `None` は「取得を試していない」（種なし・入力組み立て失敗・資格情報未設定）。
/// 取得に失敗した場合は `Some(空配列)` を返し、診断本体は止めない。
async fn fetch_journey_suggestions(
    seeds: &[String],
    region: &str,
    exclude: &HashSet<String>,
) -> Option<Vec<Value>> {
    if seeds.is_empty() {
        return None;
    }
    let query = serde_json::from_value::<crate::media_engine::handlers::SuggestQuery>(json!({
        "seed":seeds.join("\n"),
        "region":region,
        "limit":JOURNEY_SUGGESTION_LIMIT,
        "noise_floor":0,
        "exclude_brand":false
    }))
    .ok()?;
    // 地域解決1 + キーワードアイデア1 + 再試行分を、検索需要取得と同じ枠で予約する。
    acquire_journey_google_ads_budget().await;
    let Json(response) = crate::media_engine::handlers::suggest_endpoint(Query(query)).await;
    finish_journey_google_ads_budget().await;
    if response.get("status").and_then(Value::as_str) == Some("missing_credentials") {
        return None;
    }
    Some(journey_suggestions_from_response(
        &response,
        exclude,
        JOURNEY_SUGGESTION_LIMIT,
    ))
}

/// `POST /api/jobgen/journey-keywords` — 準備済みペルソナの検索需要をサーバー側で取得する。
///
/// ブラウザから検索量を受け取らず、既存の Google Ads API ハンドラをサーバー内で呼び、
/// case store に保存した値だけを後続の詳細生成へ渡す。
pub async fn jobgen_journey_keywords(Json(body): Json<Value>) -> Json<Value> {
    let case_id = body_str(&body, "case_id");
    let requested_persona_ids = body
        .get("persona_ids")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if case_id.is_empty() || requested_persona_ids.is_empty() {
        return Json(json!({
            "status":"error",
            "message":"準備済みケースと検索需要を確認するペルソナを指定してください。"
        }));
    }
    let requested_persona_ids = requested_persona_ids.into_iter().collect::<HashSet<_>>();
    if requested_persona_ids.len() > journey::REQUIRED_PERSONA_COUNT {
        return Json(json!({
            "status":"error",
            "message":"指定できるペルソナ数を超えています。"
        }));
    }

    // 二重クリックや同時セッションを直列化する。待機後にstoreを読み直すため、
    // 同一case・personaの先行取得結果は外部APIを再実行せず再利用できる。
    let _fetch_guard = journey_keyword_fetch_lock().lock().await;
    let prepared = {
        let mut store = journey_case_store().lock().await;
        store.retain(|_, value| value.created_at.elapsed() < JOURNEY_CASE_TTL);
        store.get(&case_id).cloned()
    };
    let Some(prepared) = prepared else {
        return Json(json!({
            "status":"error",
            "message":"準備データの有効期限が切れました。最初の分析からやり直してください。"
        }));
    };
    let all_personas = prepared
        .prepare_result
        .get("personas")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let personas = all_personas
        .into_iter()
        .filter(|persona| {
            persona
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| requested_persona_ids.contains(id))
        })
        .collect::<Vec<_>>();
    if personas.len() != requested_persona_ids.len() {
        return Json(json!({
            "status":"error",
            "message":"準備結果に存在しないペルソナが含まれています。"
        }));
    }

    let mut query_order = Vec::new();
    let mut seen_queries = HashSet::new();
    for persona in &personas {
        for query in persona
            .get("search_queries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(query) = query.get("query").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if !query.is_empty() && seen_queries.insert(query.to_string()) {
                query_order.push(query.to_string());
            }
        }
    }
    // 上限はペルソナ数×検索語最大数に連動させる (2026-08-06: 32固定のままだと
    // 6ペルソナ×最大8語=48語の全選択で工程4が止まる回帰があった)
    let query_limit = journey::REQUIRED_PERSONA_COUNT * journey::REQUIRED_SEARCH_QUERY_MAX;
    if query_order.is_empty() || query_order.len() > query_limit {
        return Json(json!({
            "status":"error",
            "message":format!("検索需要の確認対象が1〜{query_limit}件になるよう、ペルソナを作り直してください。")
        }));
    }
    if requested_persona_ids
        .iter()
        .all(|persona_id| prepared.keyword_completed_personas.contains(persona_id))
    {
        let measured_query_count = query_order
            .iter()
            .filter(|query| prepared.keyword_metrics_by_query.contains_key(*query))
            .count();
        return Json(json!({
            "status":"ok",
            "measurement_status":prepared
                .keyword_measurement_status
                .as_deref()
                .unwrap_or("measured"),
            "keywords":query_order
                .iter()
                .filter_map(|query| prepared.keyword_metrics_by_query.get(query).cloned())
                .collect::<Vec<_>>(),
            "requested_query_count":query_order.len(),
            "measured_query_count":measured_query_count,
            "suggestions":prepared.keyword_suggestions.clone(),
            "source":"Google広告 Keyword Planner API（サーバー取得）",
            "cache":"case"
        }));
    }
    let personas_to_fetch = personas
        .iter()
        .filter(|persona| {
            persona
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !prepared.keyword_completed_personas.contains(id))
        })
        .collect::<Vec<_>>();
    let mut queries_to_fetch = Vec::new();
    let mut seen_to_fetch = HashSet::new();
    for persona in &personas_to_fetch {
        for query in persona
            .get("search_queries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(query) = query.get("query").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if !query.is_empty() && seen_to_fetch.insert(query.to_string()) {
                queries_to_fetch.push(query.to_string());
            }
        }
    }
    if queries_to_fetch.is_empty() {
        return Json(json!({
            "status":"error",
            "message":"未取得ペルソナの検索語を確認できませんでした。"
        }));
    }

    let region = [
        prepared
            .case_profile
            .get("prefecture")
            .and_then(Value::as_str)
            .unwrap_or(""),
        prepared
            .case_profile
            .get("municipality")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ]
    .into_iter()
    .filter(|value| !value.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    let mut fetched_by_query = prepared.keyword_metrics_by_query.clone();
    let mut measurement_status = "measured";
    let query = serde_json::from_value::<crate::media_engine::handlers::KeywordsQuery>(json!({
        "kw":queries_to_fetch.join("\n"),
        "region":region,
        "noise_floor":0,
        "months":12
    }));
    let query = match query {
        Ok(value) => value,
        Err(_) => {
            return Json(json!({
                "status":"error",
                "message":"検索需要APIの入力を組み立てられませんでした。"
            }))
        }
    };
    // 1回の検索需要取得を、地域解決1 + 履歴指標1 + 最大3再試行の5要求として予約する。
    acquire_journey_google_ads_budget().await;
    let Json(response) = crate::media_engine::handlers::keywords_endpoint(Query(query)).await;
    // 予約枠をAPI完了時刻へ更新し、429再試行が長引いても完了後60秒は枠を保持する。
    finish_journey_google_ads_budget().await;
    match response.get("status").and_then(Value::as_str) {
        Some("ok") => {
            for metric in response
                .get("keywords")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(keyword) = metric
                    .get("keyword")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|keyword| seen_queries.contains(*keyword))
                {
                    fetched_by_query.insert(keyword.to_string(), metric.clone());
                }
            }
        }
        Some("missing_credentials") => {
            measurement_status = "missing_credentials";
        }
        _ => {
            return Json(json!({
                "status":"error",
                "message":"Google広告から検索需要を取得できませんでした。"
            }))
        }
    }

    // 関連語候補はケース単位で1回だけ取得し、再取得時はキャッシュを返す。
    let mut suggestions = prepared.keyword_suggestions.clone();
    let mut suggestions_fetched = prepared.keyword_suggestions_fetched;
    if !suggestions_fetched && measurement_status == "measured" {
        let seeds = journey_suggestion_seeds(&personas, JOURNEY_SUGGESTION_SEED_LIMIT);
        if let Some(fetched) = fetch_journey_suggestions(&seeds, &region, &seen_queries).await {
            suggestions = fetched;
            suggestions_fetched = true;
        }
    }

    let mut metrics_by_persona = HashMap::new();
    for persona in &personas {
        let Some(persona_id) = persona.get("id").and_then(Value::as_str) else {
            continue;
        };
        metrics_by_persona.insert(
            persona_id.to_string(),
            journey::build_trusted_keyword_metrics(persona, &fetched_by_query),
        );
    }
    {
        let mut store = journey_case_store().lock().await;
        let Some(stored) = store.get_mut(&case_id) else {
            return Json(json!({
                "status":"error",
                "message":"検索需要の取得中に準備データの有効期限が切れました。"
            }));
        };
        stored.keyword_metrics_by_query = fetched_by_query.clone();
        stored.keyword_metrics_by_persona.extend(metrics_by_persona);
        stored.keyword_completed_personas.extend(
            personas_to_fetch
                .iter()
                .filter_map(|persona| persona.get("id").and_then(Value::as_str))
                .map(str::to_string),
        );
        stored.keyword_measurement_status = Some(measurement_status.to_string());
        stored.keyword_suggestions = suggestions.clone();
        stored.keyword_suggestions_fetched = suggestions_fetched;
    }

    let measured_query_count = query_order
        .iter()
        .filter(|query| fetched_by_query.contains_key(*query))
        .count();
    Json(json!({
        "status":"ok",
        "measurement_status":measurement_status,
        "keywords":query_order
            .iter()
            .filter_map(|query| fetched_by_query.get(query).cloned())
            .collect::<Vec<_>>(),
        "requested_query_count":query_order.len(),
        "measured_query_count":measured_query_count,
        "suggestions":suggestions,
        "source":"Google広告 Keyword Planner API（サーバー取得）",
        "cache":"fresh"
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
    let keyword_metrics = match prepared.keyword_metrics_by_persona.get(&persona_id) {
        Some(value) => value.clone(),
        None => {
            return Json(json!({
                "status":"error",
                "message":"先に選択ペルソナの検索需要を確認してください。"
            }))
        }
    };

    let popular_analysis = prepared
        .prepare_result
        .get("popular_analysis")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let base_prompt = journey::build_persona_detail_prompt(
        &prepared.case_profile,
        &persona,
        &prepared.job_facts,
        &prepared.customer_statements,
        &prepared.competitor,
        &prepared.reviews,
        &prepared.public_stats,
        &keyword_metrics,
        &prepared.popular_jobs,
        &popular_analysis,
        &prepared.allowed_evidence_refs,
    );
    let detail_schema =
        journey::persona_detail_schema_with_evidence_refs(&prepared.allowed_evidence_refs);
    let mut llm_calls = 1;
    let mut result = match jobgen_llm(&base_prompt, &detail_schema, 0.25).await {
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
        result = match jobgen_llm(&repair_prompt, &detail_schema, 0.1).await {
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
    // note記事案などの後工程がサーバー保存値に接地できるよう、ゲート通過済みの
    // 診断結果をケースに保存する (TTL内の再診断は上書き)。
    {
        let mut store = journey_case_store().lock().await;
        if let Some(case) = store.get_mut(&case_id) {
            case.persona_details
                .insert(persona_id.clone(), result.clone());
        }
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

/// `POST /api/jobgen/journey-note-draft` — ペルソナの離脱要因に応えるnote記事案を作る。
/// 8段階診断 (persona-detail) 完了後のみ。外部公開素材のため、本文の数値は確認済み
/// ソースとの機械照合ゲートを通す。
pub async fn jobgen_journey_note_draft(Json(body): Json<Value>) -> Json<Value> {
    let case_id = body_str(&body, "case_id");
    let persona_id = body_str(&body, "persona_id");
    if case_id.is_empty() || persona_id.is_empty() {
        return Json(json!({
            "status":"error",
            "message":"準備済みケースとペルソナを指定してください。"
        }));
    }
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
    let Some(detail) = prepared.persona_details.get(&persona_id) else {
        return Json(json!({
            "status":"error",
            "message":"このペルソナの8段階診断がまだ完了していません。工程5の診断を先に実行してください。"
        }));
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
    let Some(persona) = persona else {
        return Json(json!({
            "status":"error",
            "message":"指定されたペルソナが見つかりません。"
        }));
    };
    let popular_analysis = prepared
        .prepare_result
        .get("popular_analysis")
        .cloned()
        .unwrap_or_else(|| json!([]));
    // SEO材料: このペルソナの検索クエリ (実測対象) と検索量、Google広告の関連語候補。
    // primary_keyword とtarget_query はこのクエリ一覧の enum に拘束され、
    // 実在しないキーワードでのSEO設計を機械的に封じる。
    let persona_queries: Vec<String> = persona
        .get("search_queries")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|query| query.get("query").and_then(Value::as_str))
                .map(|query| query.trim().to_string())
                .filter(|query| !query.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let suggestion_keywords: Vec<String> = prepared
        .keyword_suggestions
        .iter()
        .filter_map(|item| item.get("keyword").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let keyword_metrics = prepared
        .keyword_metrics_by_persona
        .get(&persona_id)
        .cloned()
        .unwrap_or_else(|| json!([]));
    let base_prompt = journey::build_note_draft_prompt(
        &prepared.case_profile,
        &persona,
        detail,
        &prepared.job_facts,
        &prepared.customer_statements,
        &prepared.popular_jobs,
        &popular_analysis,
        &keyword_metrics,
        &prepared.keyword_suggestions,
        &prepared.allowed_evidence_refs,
    );
    let schema = journey::note_draft_schema_with_evidence_refs(
        &prepared.allowed_evidence_refs,
        &persona_queries,
    );
    let verified_source = journey::note_verified_source_text(
        &prepared.job_facts,
        &prepared.customer_statements,
        &prepared.popular_jobs,
    );
    let mut llm_calls = 1;
    let mut result = match jobgen_llm(&base_prompt, &schema, 0.3).await {
        Ok(value) => value,
        Err(error) => {
            return Json(json!({
                "status":"error",
                "message":format!("note記事案の生成に失敗しました: {error}")
            }))
        }
    };
    journey::normalize_evidence_aliases(&mut result);
    let mut quality_issues = journey::validate_note_draft(
        &result,
        &prepared.allowed_evidence_refs,
        &verified_source,
        &persona_queries,
        &suggestion_keywords,
    );
    for _ in 0..2 {
        if quality_issues.is_empty() {
            break;
        }
        tracing::warn!(
            target: "jobgen_journey",
            persona_id = %persona_id,
            issues = ?quality_issues,
            "note draft quality gate requested repair"
        );
        let repair_prompt =
            journey::build_detail_repair_prompt(&base_prompt, &result, &quality_issues);
        result = match jobgen_llm(&repair_prompt, &schema, 0.1).await {
            Ok(value) => value,
            Err(error) => {
                quality_issues.push(format!("自動補修APIに失敗しました: {error}"));
                break;
            }
        };
        llm_calls += 1;
        journey::normalize_evidence_aliases(&mut result);
        quality_issues = journey::validate_note_draft(
            &result,
            &prepared.allowed_evidence_refs,
            &verified_source,
            &persona_queries,
            &suggestion_keywords,
        );
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
