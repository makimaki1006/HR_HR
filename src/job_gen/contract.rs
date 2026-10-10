//! 求人票生成 (`/api/jobgen/*`) の JSON 契約 (React 画面 `/app/jobgen` 向け、2026-09-29 W8)。
//!
//! ハンドラ ([`super::handlers`]) は `serde_json::Value` で JSON を組んでいて、ここに
//! 置くのはその **形を写した型**。用途は 2 つ:
//!
//! 1. ts-rs で `frontend/src/generated/*.ts` に TypeScript 型を書き出す
//!    (`src/handlers/app_api.rs` の `export_ts_bindings` から [`export_ts`] を呼ぶ)。
//! 2. 契約テスト: ハンドラの成功応答 (`*_ok_response`、LLM を呼ばない純粋部分) を
//!    `deny_unknown_fields` 付きでこの型に読み込み、**ハンドラのキーと型がずれたら落ちる**。
//!    同じ応答を `frontend/src/generated/jobgen/fixtures.json` に書き出し、
//!    Vitest と旧新比較 (Playwright) のモック応答に使う (fixture を想像で作らない)。
//!
//! LLM がそのまま通る部分 (`analysis` / `personas` / `copies` / `directions` / `prompts` /
//! `steps`) は、実運用ではスキーマ外のキーが混ざることがある。ハンドラはそれを落とさず
//! 通すので、その部分の型には `deny_unknown_fields` を付けない (画面が読むキーだけを書く)。
//!
//! 生成ロジック・プロンプト・ゲートはこのモジュールでは触らない。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::hrhacker::{FillStats, GeneratedField, UnassignedHint};
use super::inputs::NormalizedJob;
use super::ng_words::NgViolation;
use super::types::FactField;

/// 失敗時の共通応答 `{"status":"error","message":"..."}` (HTTP は 200 のまま)。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct JobgenErrorResponse {
    /// 常に `"error"`。
    pub status: String,
    pub message: String,
}

// ───────── 取り込み (/api/jobgen/normalize) ─────────

/// `POST /api/jobgen/normalize` の要求。`kind` に応じて `text` / `url` / `data_base64` の
/// どれか 1 つだけを付ける (旧 `static/jobgen.html` と同じキー)。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct NormalizeRequest {
    /// `free_text` | `url` | `csv` | `excel` | `pdf` | `html`
    pub kind: String,
    /// free_text / csv / html の本文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    /// url のとき。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    /// excel / pdf のファイル本体 (base64)。multipart ではなく JSON ボディで送る
    /// (ボディ上限は `lib.rs` の `JOBGEN_NORMALIZE_BODY_LIMIT_BYTES` = 24MB)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub data_base64: Option<String>,
}

/// `POST /api/jobgen/normalize` の成功応答。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NormalizeResponse {
    pub status: String,
    pub jobs: Vec<NormalizedJob>,
}

// ───────── 工程① 事実抽出 ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtractRequest {
    pub source_text: String,
}

/// 工程①の成功応答。`facts` のキーは [`super::types::FACT_KEYS`] の 8 つ。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExtractResponse {
    pub status: String,
    pub facts: BTreeMap<String, FactField>,
    /// verified の項目だけを `key: value` 行にしたテキスト (工程⑥の入力)。
    pub facts_text: String,
}

// ───────── 工程② 市場分析 ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AnalyzeRequest {
    pub source_text: String,
    /// 画面の職種名欄 (先頭行候補をユーザーが直したもの)。
    pub job_title: String,
}

/// 工程②の LLM 出力 (そのまま通す)。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct Analysis {
    #[serde(default)]
    pub surface_strengths: Vec<String>,
    #[serde(default)]
    pub hidden_strengths: Vec<String>,
    #[serde(default)]
    pub bottlenecks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeResponse {
    pub status: String,
    /// 職種知識のカテゴリ (該当なしは「その他」)。
    pub category: String,
    /// 該当職種の知識を注入できたか。false なら画面が職種名の見直しを促す。
    pub knowledge_used: bool,
    pub analysis: Analysis,
}

// ───────── 工程③ ペルソナ設計 ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PersonasRequest {
    pub source_text: String,
    pub analysis: Analysis,
    /// 3〜5 (サーバ側で clamp)。ts-rs は u64 を bigint にするので u32。
    pub count: u32,
}

/// 工程③の 1 ペルソナ (LLM 出力そのまま)。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct Persona {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub dissatisfaction: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub pain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PersonasResponse {
    pub status: String,
    /// LLM 出力に `personas` が無ければ null。
    pub personas: Option<Vec<Persona>>,
}

// ───────── 工程④ キャッチコピー (1 ペルソナ分) ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CopyRequest {
    pub persona: Persona,
    pub analysis: Analysis,
    pub source_text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct CopyItem {
    /// [`super::strategy::COPY_STYLES`] のいずれか。
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub text: String,
}

/// 数値照合で「原文にない数値」を含んだ生成文。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NumberViolation {
    pub text: String,
    pub numbers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CopyResponse {
    pub status: String,
    pub copies: Option<Vec<CopyItem>>,
    pub ng_violations: Vec<NgViolation>,
    pub expression_warnings: Vec<NgViolation>,
    pub number_violations: Vec<NumberViolation>,
    /// `"checked"` | `"skipped(source_text未提供)"`
    pub number_check: String,
    pub review_required: bool,
}

// ───────── 工程⑤ 画像ディレクション / ⑤b 生成プロンプト ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ImagesRequest {
    pub personas: Vec<Persona>,
    pub source_text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ImageDirection {
    #[serde(default)]
    pub persona_label: String,
    #[serde(default)]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImagesResponse {
    pub status: String,
    pub directions: Option<Vec<ImageDirection>>,
    pub number_violations: Vec<NumberViolation>,
    pub number_check: String,
    pub review_required: bool,
}

/// 工程⑤の出力を `{"directions":[...]}` の形で包んで送る (旧画面と同じ)。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectionsEnvelope {
    pub directions: Vec<ImageDirection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ImagePromptsRequest {
    pub directions: DirectionsEnvelope,
    pub personas: Vec<Persona>,
    pub source_text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ImagePrompt {
    #[serde(default)]
    pub persona_label: String,
    #[serde(default)]
    pub appeal_core: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: String,
    #[serde(default)]
    pub aspect_ratio: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImagePromptsResponse {
    pub status: String,
    pub prompts: Option<Vec<ImagePrompt>>,
    pub number_violations: Vec<NumberViolation>,
    pub number_check: String,
    pub review_required: bool,
}

// ───────── 工程⑥ スマホ原稿 (1 ペルソナ分) ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct MobileRequest {
    pub persona: Persona,
    /// 工程①の `facts_text`。
    pub facts_text: String,
    pub source_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MobileResponse {
    pub status: String,
    /// 空行は空文字列の要素。
    pub lines: Vec<String>,
    pub ng_violations: Vec<NgViolation>,
    pub expression_warnings: Vec<NgViolation>,
    pub number_violations: Vec<NumberViolation>,
    pub number_check: String,
    pub review_required: bool,
}

// ───────── 工程⑦ 84 列原稿 ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct HrhackerRequest {
    pub source_text: String,
    /// 工程①の `facts` をそのまま。
    pub facts: BTreeMap<String, FactField>,
    /// 工程②の表面の強み + 裏の強みを「、」で結合した文字列 (無ければ空)。
    pub strategy_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct HrhackerResponse {
    pub status: String,
    /// LLM 生成の試行回数 (1 or 2)。
    pub attempts: usize,
    /// 84 列 (列名 → 値)。**キー順が CSV の列順** ([`super::hrhacker::HRHACKER_COLUMNS`])。
    /// serde_json の preserve_order で挿入順のまま届くので、画面は `Object.keys` の順を使う。
    #[ts(type = "Record<string, string>")]
    pub row: serde_json::Map<String, Value>,
    /// 生成 5 列 (キーは内部名 job_title 等、値の `column` が列名)。
    pub generated_fields: BTreeMap<String, GeneratedField>,
    /// review_required になった内部キー。
    pub review_required_fields: Vec<String>,
    /// 生成列の issues を平坦化したもの (数値/文字数/NG の理由文字列)。
    pub unsupported_numbers: Vec<String>,
    pub fill_stats: FillStats,
    pub unassigned_hints: Vec<UnassignedHint>,
}

// ───────── 工程⑧ A/B テスト助言 ─────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AbRequest {
    /// 工程②③の要約 (無ければ原文先頭 400 字)。
    pub summary: String,
    pub source_text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct AbStep {
    #[serde(default)]
    pub metric: String,
    #[serde(default)]
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AbResponse {
    pub status: String,
    pub steps: Option<Vec<AbStep>>,
    pub ng_violations: Vec<NgViolation>,
    pub expression_warnings: Vec<NgViolation>,
    pub number_violations: Vec<NumberViolation>,
    pub number_check: String,
    pub review_required: bool,
}

/// 上の型を全部 `frontend/src/generated/` に書き出す (`export_ts_bindings` から呼ぶ)。
/// 依存型 (FactField / NgViolation / GeneratedField / FillStats / UnassignedHint / NormalizedJob)
/// も `export_all` が一緒に書き出す。
pub fn export_ts(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    JobgenErrorResponse::export_all(cfg)?;
    NormalizeRequest::export_all(cfg)?;
    NormalizeResponse::export_all(cfg)?;
    ExtractRequest::export_all(cfg)?;
    ExtractResponse::export_all(cfg)?;
    AnalyzeRequest::export_all(cfg)?;
    AnalyzeResponse::export_all(cfg)?;
    PersonasRequest::export_all(cfg)?;
    PersonasResponse::export_all(cfg)?;
    CopyRequest::export_all(cfg)?;
    CopyResponse::export_all(cfg)?;
    ImagesRequest::export_all(cfg)?;
    ImagesResponse::export_all(cfg)?;
    ImagePromptsRequest::export_all(cfg)?;
    ImagePromptsResponse::export_all(cfg)?;
    MobileRequest::export_all(cfg)?;
    MobileResponse::export_all(cfg)?;
    HrhackerRequest::export_all(cfg)?;
    HrhackerResponse::export_all(cfg)?;
    AbRequest::export_all(cfg)?;
    AbResponse::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! 契約テスト + fixture 書き出し。
    //!
    //! LLM の生出力 (`raw_*`) だけを固定し、応答はハンドラの純粋部分 (`*_ok_response`) で組む。
    //! だから fixture の `ng_violations` / `number_violations` / 84 列 / 充足率は
    //! **本物のゲート・組み立てコードの出力**。値の期待は具体値で書く。
    use super::*;
    use crate::job_gen::handlers::{
        ab_ok_response, analyze_ok_response, copy_ok_response, extract_ok_response,
        hrhacker_ok_response, image_prompts_ok_response, images_ok_response, mobile_ok_response,
        normalize_ok_response, personas_ok_response,
    };
    use crate::job_gen::{hrhacker, inputs, knowledge, types as job_types};
    use serde_json::json;

    /// fixture の求人原文。引用照合・数値照合はこれに対して行われる。
    pub(crate) const SOURCE_TEXT: &str = "【職種】介護職員（特別養護老人ホーム）
【給与】月給192,000円〜195,000円（資格手当・処遇改善手当を含む）
【勤務時間】8:30〜17:30（休憩60分）
【休日】週休2日制（シフト制）、年間休日110日
【勤務地】東京都八王子市高尾町1-2-3 ○○苑（京王線 高尾駅から徒歩10分）
【雇用形態】正社員
【保険】雇用保険、労災保険、健康保険、厚生年金
【手当】通勤手当（上限20,000円／月）、資格手当、夜勤手当（1回5,000円）
【必須資格】介護福祉士または初任者研修修了者
【教育】入職後1か月の研修あり。未経験の方も先輩職員が同行します。";

    const JOB_TITLE: &str = "介護職";

    fn raw_extract() -> Value {
        json!({
            "salary": {"value": "月給192,000円〜195,000円", "evidence_quote": "月給192,000円〜195,000円"},
            "working_hours": {"value": "8:30〜17:30（休憩60分）", "evidence_quote": "8:30〜17:30（休憩60分）"},
            "holidays": {"value": "週休2日制（シフト制）、年間休日110日", "evidence_quote": "週休2日制（シフト制）、年間休日110日"},
            "work_location": {"value": "東京都八王子市高尾町1-2-3 ○○苑", "evidence_quote": "東京都八王子市高尾町1-2-3 ○○苑"},
            "employment_type": {"value": "正社員", "evidence_quote": "【雇用形態】正社員"},
            // 引用が原文に無い (退職金制度は原文に無い) → rejected
            "insurance": {"value": "雇用保険、労災保険、健康保険、厚生年金、退職金制度", "evidence_quote": "雇用保険、労災保険、健康保険、厚生年金、退職金制度"},
            "allowances": {"value": "通勤手当（上限20,000円／月）、資格手当、夜勤手当（1回5,000円）", "evidence_quote": "通勤手当（上限20,000円／月）、資格手当、夜勤手当（1回5,000円）"},
            // 値なし → missing
            "required_qualifications": {"value": "", "evidence_quote": ""}
        })
    }

    fn raw_analysis() -> Value {
        json!({
            "surface_strengths": [
                "月給192,000円〜195,000円で資格手当・処遇改善手当を含む",
                "年間休日110日・週休2日制",
                "高尾駅から徒歩10分"
            ],
            "hidden_strengths": [
                "入職後1か月の研修と先輩職員の同行で未経験でも始めやすい",
                "夜勤手当が1回ごとに明示されている"
            ],
            "bottlenecks": [
                "夜勤の有無・回数が原文で明確でない",
                "給与レンジが狭く昇給の見通しが書かれていない"
            ]
        })
    }

    fn raw_personas() -> Value {
        json!({"personas": [
            {"label": "子育て中の復職希望者",
             "profile": "38歳女性。介護福祉士。出産を機に離職し、下の子が小学生になったため復職を考えている。八王子市在住。",
             "dissatisfaction": "以前の職場は夜勤が月8回あり家庭と両立できなかった",
             "environment": "自宅から高尾駅まで自転車10分。夫は平日不在がち",
             "pain": "家庭を優先しながら資格を活かして働きたいが、条件の合う職場が見つからない"},
            {"label": "介護未経験の異業種転職者",
             "profile": "29歳男性。飲食店勤務6年。体力に自信があり、手に職をつけたい。",
             "dissatisfaction": "深夜営業で生活が不規則、昇給がない",
             "environment": "一人暮らし。八王子市内に賃貸",
             "pain": "未経験で採用されるか不安。研修の有無を重視"},
            {"label": "処遇改善を求めるベテラン",
             "profile": "45歳男性。介護歴15年、介護福祉士。現在は小規模施設で主任。",
             "dissatisfaction": "処遇改善手当が基本給に反映されず、責任ばかり増える",
             "environment": "妻と子2人。車通勤",
             "pain": "経験に見合う待遇と、腰を据えて働ける法人を探している"}
        ]})
    }

    fn raw_copies() -> Value {
        json!({"copies": [
            {"style": "常識破壊", "text": "夜勤なしでも、介護福祉士の資格はちゃんと評価される。"},
            // 年間休日は原文では 110 日 → 120 は「原文にない数値」
            {"style": "比較・リアルな声", "text": "年間休日120日。前の職場より土日が増えたと先輩が言った。"},
            // 「女性歓迎」は法令 NG (性別差別表現)
            {"style": "感情・共感", "text": "女性歓迎。子育てとの両立、私たちが一緒に考えます。"}
        ]})
    }

    fn raw_directions() -> Value {
        json!({"directions": [
            {"persona_label": "子育て中の復職希望者", "direction": "午後の明るい談話室。利用者と笑顔で話す職員を斜め前から。制服は清潔なポロシャツ。"},
            {"persona_label": "介護未経験の異業種転職者", "direction": "先輩職員が新人に記録の書き方を教えている場面。二人の距離感を近く。"},
            {"persona_label": "処遇改善を求めるベテラン", "direction": "夕方の廊下で落ち着いた表情の職員。背景に施設の掲示板。"}
        ]})
    }

    fn raw_prompts() -> Value {
        json!({"prompts": [
            {"persona_label": "子育て中の復職希望者", "appeal_core": "家庭と両立できる働き方",
             "prompt": "被写体: 30代の女性介護職員1人。場所: 明るい談話室。表情: 穏やかな笑顔。服装: 清潔なポロシャツ。",
             "negative_prompt": "夜間、暗い照明、疲れた表情、文字入れ", "aspect_ratio": "4:5"},
            {"persona_label": "介護未経験の異業種転職者", "appeal_core": "研修と同行で未経験から始められる",
             "prompt": "被写体: 先輩職員と新人職員の2人。場所: 記録用のデスク。構図: 肩越しに手元を見せる。",
             "negative_prompt": "一人きり、無表情、散らかった机", "aspect_ratio": "4:5"},
            {"persona_label": "処遇改善を求めるベテラン", "appeal_core": "経験が待遇に反映される",
             "prompt": "被写体: 40代の男性職員1人。場所: 施設の廊下。表情: 落ち着いた自信。",
             "negative_prompt": "若すぎる人物、派手な色、文字入れ", "aspect_ratio": "4:5"}
        ]})
    }

    fn raw_mobile() -> Value {
        json!({"lines": [
            "高尾駅から徒歩10分。",
            "",
            "週休2日、年間休日110日。",
            "入職後1か月の研修で、未経験でも安心して始められます。",
            "アットホームな職場です。"
        ]})
    }

    fn raw_hrhacker() -> Value {
        json!({
            "job_title": "介護職員（特養）／高尾駅徒歩10分・年間休日110日",
            "job_description": "特別養護老人ホームでの介護業務全般。食事・入浴・排泄の介助、レクリエーションの企画、記録業務。入職後1か月の研修があり、未経験の方も先輩職員が同行します。",
            "catch_copy": "資格手当・処遇改善手当を含む月給192,000円〜。週休2日で家庭と両立。",
            // 120日は原文に無い → review_required (空欄)
            "merit": "年間休日120日。通勤手当（上限20,000円／月）支給。",
            "indeed_job_title": "介護職員（特別養護老人ホーム）"
        })
    }

    fn raw_ab() -> Value {
        json!({"steps": [
            {"metric": "CTR（クリック率）", "action": "キャッチコピーA（夜勤なし訴求）とB（資格評価訴求）を同一画像で並走し、CTRが低い方を差し替える"},
            {"metric": "CVR（応募転換率）", "action": "応募フォームの必須項目を減らし、変更前後で応募完了率を比べる"},
            {"metric": "CPA（応募単価）", "action": "高尾駅周辺への配信と八王子市全域への配信で応募単価を比較する"}
        ]})
    }

    fn facts() -> job_types::ExtractedFacts {
        crate::job_gen::fact_extract::verify(SOURCE_TEXT, &raw_extract())
    }

    fn generated() -> BTreeMap<String, GeneratedField> {
        let ng = crate::job_gen::ng_words::NgRules::load_from_str(include_str!(
            "../../assets/ng_words.json"
        ))
        .expect("埋め込み NG ルールが読めない");
        hrhacker::validate_generated(SOURCE_TEXT, &raw_hrhacker(), &ng)
    }

    /// 全応答を組む (fixture 書き出しと各テストで共有)。
    fn all_responses() -> Vec<(&'static str, Value)> {
        let jobs = vec![inputs::NormalizedJob {
            title_hint: "【職種】介護職員（特別養護老人ホーム）".into(),
            source_text: SOURCE_TEXT.into(),
        }];
        let bundle = knowledge::lookup_default(JOB_TITLE).expect("職種知識の参照に失敗");
        let knowledge_used = !bundle.sections.is_empty();
        vec![
            ("normalize", normalize_ok_response(&jobs)),
            ("extract", extract_ok_response(SOURCE_TEXT, &raw_extract())),
            (
                "analyze",
                analyze_ok_response(&bundle.category, knowledge_used, raw_analysis()),
            ),
            ("personas", personas_ok_response(&raw_personas())),
            ("copy", copy_ok_response(SOURCE_TEXT, &raw_copies())),
            ("images", images_ok_response(SOURCE_TEXT, &raw_directions())),
            (
                "image_prompts",
                image_prompts_ok_response(SOURCE_TEXT, &raw_prompts()),
            ),
            ("mobile", mobile_ok_response(SOURCE_TEXT, &raw_mobile())),
            (
                "hrhacker",
                hrhacker_ok_response(SOURCE_TEXT, &facts(), &generated(), 1),
            ),
            ("ab", ab_ok_response(SOURCE_TEXT, &raw_ab())),
        ]
    }

    fn get<'a>(all: &'a [(&'static str, Value)], key: &str) -> &'a Value {
        &all.iter().find(|(k, _)| *k == key).expect(key).1
    }

    fn parse<T: for<'de> Deserialize<'de>>(v: &Value) -> T {
        serde_json::from_value(v.clone()).unwrap_or_else(|e| {
            panic!(
                "応答が契約型に合わない: {e}\n{}",
                serde_json::to_string_pretty(v).unwrap()
            )
        })
    }

    #[test]
    fn 全応答が契約型に読める_deny_unknown_fields() {
        let all = all_responses();
        let _: NormalizeResponse = parse(get(&all, "normalize"));
        let _: ExtractResponse = parse(get(&all, "extract"));
        let _: AnalyzeResponse = parse(get(&all, "analyze"));
        let _: PersonasResponse = parse(get(&all, "personas"));
        let _: CopyResponse = parse(get(&all, "copy"));
        let _: ImagesResponse = parse(get(&all, "images"));
        let _: ImagePromptsResponse = parse(get(&all, "image_prompts"));
        let _: MobileResponse = parse(get(&all, "mobile"));
        let _: HrhackerResponse = parse(get(&all, "hrhacker"));
        let _: AbResponse = parse(get(&all, "ab"));
    }

    #[test]
    fn 工程一の抽出結果は検証6_リジェクト1_欠落1() {
        let all = all_responses();
        let r: ExtractResponse = parse(get(&all, "extract"));
        assert_eq!(r.status, "ok");
        assert_eq!(r.facts.len(), 8);
        let count = |st: &str| r.facts.values().filter(|f| f.status == st).count();
        assert_eq!(count("verified"), 6, "{:?}", r.facts);
        assert_eq!(count("rejected"), 1, "{:?}", r.facts);
        assert_eq!(count("missing"), 1, "{:?}", r.facts);
        assert_eq!(r.facts["salary"].value, "月給192,000円〜195,000円");
        assert_eq!(r.facts["insurance"].status, "rejected");
        assert_eq!(r.facts["insurance"].value, "", "rejected は値を空にする");
        assert_eq!(r.facts["required_qualifications"].status, "missing");
        assert!(r.facts_text.contains("salary: 月給192,000円〜195,000円"));
        assert!(
            !r.facts_text.contains("退職金"),
            "rejected は facts_text に出ない"
        );
    }

    #[test]
    fn 工程四のコピーは法令ng1件と原文にない数値1件() {
        let all = all_responses();
        let r: CopyResponse = parse(get(&all, "copy"));
        let copies = r.copies.as_deref().expect("copies が null");
        assert_eq!(copies.len(), 3);
        assert_eq!(copies[0].style, "常識破壊");
        assert_eq!(r.ng_violations.len(), 1, "{:?}", r.ng_violations);
        assert_eq!(r.ng_violations[0].matched, "女性歓迎");
        assert_eq!(r.ng_violations[0].severity, "legal");
        assert_eq!(r.number_violations.len(), 1, "{:?}", r.number_violations);
        assert_eq!(r.number_violations[0].numbers, vec!["120", "120日"]);
        // 「夜勤なし」は表現レビュー辞書 (warning)。法令NGとは別リストで届く。
        assert_eq!(
            r.expression_warnings.len(),
            1,
            "{:?}",
            r.expression_warnings
        );
        assert_eq!(r.expression_warnings[0].matched, "夜勤なし");
        assert_eq!(r.number_check, "checked");
        assert!(r.review_required);
    }

    #[test]
    fn 工程七は生成4列verified_メリットreview_充足11_84() {
        let all = all_responses();
        let r: HrhackerResponse = parse(get(&all, "hrhacker"));
        assert_eq!(r.attempts, 1);
        assert_eq!(r.row.len(), 84);
        // 列順は HRHACKER_COLUMNS のまま届く (CSV の列順)。
        let keys: Vec<&str> = r.row.keys().map(String::as_str).collect();
        assert_eq!(keys, hrhacker::HRHACKER_COLUMNS.to_vec());
        assert_eq!(
            r.row["案件名"],
            "介護職員（特養）／高尾駅徒歩10分・年間休日110日"
        );
        assert_eq!(r.row["メリット"], "", "review_required の列は空欄");
        assert_eq!(r.row["雇用形態"], "正社員");
        assert_eq!(r.row["応募資格"], "", "missing の事実は転記しない");
        assert_eq!(r.review_required_fields, vec!["merit".to_string()]);
        assert_eq!(r.generated_fields["merit"].status, "review_required");
        assert_eq!(
            r.unsupported_numbers,
            vec!["unsupported_numbers:120,120日".to_string()]
        );
        assert_eq!(
            r.fill_stats,
            FillStats {
                filled: 11,
                total: 84,
                fact_mapped_filled: 11,
                fact_mapped_total: 13
            }
        );
        let hint_cols: Vec<&str> = r
            .unassigned_hints
            .iter()
            .map(|h| h.column.as_str())
            .collect();
        assert_eq!(hint_cols, vec!["最寄り駅", "試用・研修の有無"]);
    }

    #[test]
    fn 工程六は全ゲート通過_工程八は表現レビュー1件() {
        let all = all_responses();
        let m: MobileResponse = parse(get(&all, "mobile"));
        assert_eq!(m.lines.len(), 5);
        assert_eq!(m.lines[1], "", "空行は空文字列で届く");
        assert!(m.number_violations.is_empty(), "{:?}", m.number_violations);
        assert!(m.ng_violations.is_empty(), "{:?}", m.ng_violations);
        assert!(
            m.expression_warnings.is_empty(),
            "{:?}",
            m.expression_warnings
        );
        assert!(!m.review_required);
        // ⑧: 「夜勤なし」は表現レビュー辞書 (warning) に当たる。法令NG・数値は通過。
        let ab: AbResponse = parse(get(&all, "ab"));
        assert_eq!(ab.steps.as_deref().map(<[AbStep]>::len), Some(3));
        assert!(
            ab.number_violations.is_empty(),
            "{:?}",
            ab.number_violations
        );
        assert!(ab.ng_violations.is_empty(), "{:?}", ab.ng_violations);
        assert_eq!(
            ab.expression_warnings.len(),
            1,
            "{:?}",
            ab.expression_warnings
        );
        assert_eq!(ab.expression_warnings[0].matched, "夜勤なし");
        assert_eq!(ab.expression_warnings[0].severity, "warning");
        assert!(ab.review_required);
    }

    #[test]
    fn normalize要求は指定したキーだけを送る() {
        let v = serde_json::to_value(NormalizeRequest {
            kind: "free_text".into(),
            text: Some("本文".into()),
            url: None,
            data_base64: None,
        })
        .unwrap();
        assert_eq!(v, json!({"kind": "free_text", "text": "本文"}));
    }

    /// 本物のハンドラ (LLM を使わない経路) が契約型どおりの JSON を返すこと。
    #[tokio::test]
    async fn 本物のハンドラの非llm経路が契約型に読める() {
        use crate::job_gen::handlers;
        use axum::Json;

        let req = serde_json::to_value(NormalizeRequest {
            kind: "free_text".into(),
            text: Some(SOURCE_TEXT.into()),
            url: None,
            data_base64: None,
        })
        .unwrap();
        let Json(v) = handlers::jobgen_normalize(Json(req)).await;
        let r: NormalizeResponse = parse(&v);
        assert_eq!(r.status, "ok");
        assert_eq!(r.jobs.len(), 1);
        assert_eq!(r.jobs[0].source_text, SOURCE_TEXT);

        let Json(v) = handlers::jobgen_extract(Json(json!({"source_text": ""}))).await;
        let e: JobgenErrorResponse = parse(&v);
        assert_eq!(e.status, "error");
        assert_eq!(e.message, "source_text が必要です");

        let Json(v) = handlers::jobgen_hrhacker(Json(json!({"source_text": "x"}))).await;
        let e: JobgenErrorResponse = parse(&v);
        assert_eq!(e.message, "facts が必要です");
    }

    /// Vitest と旧新比較 (Playwright) が使う fixture を書き出す。`cargo test --lib` で毎回走る。
    /// 生成物はコミットし、CI (contract-types) が `frontend/src/generated` の差分ゼロを検査する。
    #[tokio::test]
    async fn export_jobgen_fixtures() {
        use crate::job_gen::handlers;
        use axum::Json;

        let all = all_responses();
        let mut responses = serde_json::Map::new();
        for (k, v) in &all {
            responses.insert((*k).to_string(), v.clone());
        }
        // 本物のハンドラの失敗応答も 1 つ入れる (画面のエラー表示テスト用)。
        let Json(err) = handlers::jobgen_extract(Json(json!({"source_text": ""}))).await;
        responses.insert("error".to_string(), err);

        let fixture = json!({
            "_comment": "cargo test --lib (src/job_gen/contract.rs export_jobgen_fixtures) が生成。手で編集しない。",
            "source_text": SOURCE_TEXT,
            "job_title": JOB_TITLE,
            "responses": Value::Object(responses),
        });
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/generated/jobgen");
        std::fs::create_dir_all(&dir).unwrap();
        let text = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
        std::fs::write(dir.join("fixtures.json"), text).expect("fixtures.json を書き出せない");
        // CSV 出力と確認表は応答オブジェクトのキー順ではなく、この正本を参照する。
        let columns = format!(
            "{}\n",
            serde_json::to_string_pretty(&crate::job_gen::hrhacker::HRHACKER_COLUMNS.to_vec())
                .unwrap()
        );
        std::fs::write(dir.join("columns.json"), columns).expect("columns.json を書き出せない");
    }

    #[test]
    fn ts型の宣言() {
        let cfg = ts_rs::Config::default();
        let decl = HrhackerResponse::decl(&cfg);
        assert!(decl.contains("row: Record<string, string>,"), "{decl}");
        assert!(decl.contains("fill_stats: FillStats,"), "{decl}");
        let decl = NormalizeRequest::decl(&cfg);
        assert!(decl.contains("text?: string,"), "{decl}");
        let decl = PersonasResponse::decl(&cfg);
        assert!(decl.contains("personas: Array<Persona> | null,"), "{decl}");
    }
}
