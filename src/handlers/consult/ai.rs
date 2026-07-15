//! Gemini による文章化 (計画書 §18)
//!
//! LLM は「文章化のみ」。数値計算・集計・閾値判定・信頼度計算はすべてコード側で確定済みで、
//! Gemini には確定した構造化データ (evidence_pack JSON) だけを渡す (§15.2 / §18.2)。
//!
//! 用途は2つ、いずれも入力は evidence_pack JSON のみ:
//!   1. 「市場の一文要約」の自然文化 (テンプレ文の言い換え)
//!   2. 新セクション「複合考察 (AI下書き)」: 複数シグナル・矛盾をつないだ考察を数項目
//!
//! ## サーバ側検証 (§18.3 / §19.2)
//! Gemini の出力は信用しない。返ってきた各項目について:
//! - 参照 evidence_id が analysis.evidence に実在しないものを含む → 破棄
//! - 根拠IDが空の項目 → 破棄 (§19.1「仮説に根拠IDが存在するか」)
//! - §19.2 の禁止表現・サービス名を含む → 破棄
//!
//! ## 反証ステージ (逆証明の道具箱)
//! validate_items を通過した考察に対し、LLM ではなく決定的な機械チェック
//! (`refute_toolbox.rs`) をかける。標本数 (T1)・データ粒度 (T2)・反対方向の観測 (T3)・
//! 逆の因果 (T4) の4つの道具で各考察を点検し、結果はコードが裁定して考察カードに併記する
//! (考察は破棄しない)。都度 LLM に逆証明をさせるのではなく、逆証明の道具をコードとして
//! 持ち、コードが実行する方式 (決定的・単体テスト可能・追加のAPIコストなし)。
//! 生成コールには考察の主張軸 (claim_axis) と方向 (claim_direction) のタグ付けを課し、
//! T3 (反対方向シグナル検索) の入力にする。
//!
//! ## graceful degradation
//! GEMINI_API_KEY 未設定・API失敗・全項目破棄のいずれでもパニックせず、
//! 空の `AiComposite` を返す。呼び出し側 (brief_html) はセクションを省略し1行の注記を出す。
//! 道具箱チェックは決定的で、考察が生成された場合は常に実施される (reviewed=true)。
//!
//! ブリーフ生成1回あたり Gemini 呼び出しは最大2回 (要約1 + 複合考察1)。反証は非LLM。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::evidence_pack::{to_evidence_pack_json, ConsultAnalysis};
use super::input::ClientInput;
use crate::gemini::GeminiClient;

/// 複合考察の1項目 (§18.3: 根拠ID保持・可能性口調)
///
/// 反証ステージ由来のフィールド (`refuted` / `refute_reason` / `alt_interpretation` /
/// `reviewed`) は生成コールの JSON には含まれず、逆証明の道具箱 (refute_toolbox.rs) が
/// 裁定時に埋める。デシリアライズ時は既定値になるよう `#[serde(default)]`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiItem {
    pub title: String,
    pub body: String,
    pub evidence_ids: Vec<String>,
    /// 不足データ・留意点 (§18.3-5)
    pub caveat: String,
    /// 主張が主に関わる軸 (demand/supply/competition/offer/other)。生成コールでタグ付け。
    /// 不正値は道具箱側で other として扱う (silent fallback ではなく明示の縮退)。
    #[serde(default)]
    pub claim_axis: String,
    /// 主張の方向 (problem/opportunity/neutral)。生成コールでタグ付け。
    #[serde(default)]
    pub claim_direction: String,
    /// 道具箱チェック (T1 標本数 / T2 粒度) で「確認が必要」と判定されたか (裁定後に確定)。
    #[serde(default)]
    pub refuted: bool,
    /// 確認が必要な点 (T1/T2 の指摘文。refuted=true のときのみ Some)。
    #[serde(default)]
    pub refute_reason: Option<String>,
    /// 逆・別の解釈 (T3 反対方向の観測 / T4 逆因果辞書。無ければ None)。
    #[serde(default)]
    pub alt_interpretation: Option<String>,
    /// この項目に対し道具箱の裁定が実施できたか。
    /// 道具箱は決定的なため通常は常に true。false は「(反証チェック未実施)」注記。
    #[serde(default)]
    pub reviewed: bool,
}

/// 単品層の1項目 (§2層設計: 観測ひとつずつに「だから〜」の着地を付ける)。
///
/// MUST evidence を全網羅する参照層。客の「これはどういう意味?」に即答するための素材。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SingleInsight {
    pub evidence_id: String,
    /// 観測 (その1指標が何を示すか)
    pub observation: String,
    /// その1つで言える結論 (「だから〜」の着地文)
    pub so_what: String,
    /// テーマ (エンジンが付与。表示のグルーピング用。LLM 出力ではなくコードで補完)
    #[serde(default)]
    pub theme: String,
}

/// 除外項目 (今回使わなかったデータと理由)。取りこぼしゼロを明示するため。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExcludedItem {
    pub evidence_id: String,
    pub reason: String,
}

/// AI 文章化の結果一式 (2層: 主役=リード仮説+複合考察 / 参照層=単品)。
#[derive(Debug, Clone, Default)]
pub struct AiComposite {
    /// 一文要約の自然文 (検証を通ったときのみ Some)
    pub one_line_summary: Option<String>,
    /// リード仮説 (この1行だけ持ち帰る。給与確認済/条件未確認の趣旨)
    pub lead_hypothesis: Option<String>,
    /// 単品層: MUST evidence の観測→だから (参照層。全件表示)
    pub single_insights: Vec<SingleInsight>,
    /// 複合考察 (主役)。逆証明の道具箱を引き続き適用するため型は AiItem のまま。
    pub items: Vec<AiItem>,
    /// 除外: 今回使わなかったデータと理由
    pub excluded: Vec<ExcludedItem>,
}

impl AiComposite {
    pub fn is_empty(&self) -> bool {
        self.one_line_summary.is_none()
            && self.lead_hypothesis.is_none()
            && self.single_insights.is_empty()
            && self.items.is_empty()
    }
}

/// §19.2 の禁止表現 + 出力に出してはならないサービス名。
/// これらを含む LLM 出力項目はサーバ側で破棄する。
pub const FORBIDDEN_PHRASES: [&str; 9] = [
    "必ず採用できる",
    "応募が増える",
    "離職率が高い企業",
    "成長企業である",
    "この媒体が最適",
    "SalesNow",
    "salesnow",
    "内定が取れる",
    "確実に",
];

/// テキストに禁止表現が含まれるか
pub fn contains_forbidden(text: &str) -> bool {
    FORBIDDEN_PHRASES.iter().any(|p| text.contains(p))
}

/// §18.3 の必須プロンプト制約 (system プロンプトに明記)
const SYSTEM_CONSTRAINTS: &str = "\
あなたは採用コンサルタントの準備を助けるアシスタントです。以下の制約を厳守してください。\n\
1. 提供された構造化データ (evidence_pack) 以外の事実を追加しない。一般論や推測の数値を足さない。\n\
2. すべて「〜の可能性がある」「〜かもしれない」という可能性の表現にする。断定しない。\n\
3. 因果関係を断定しない (「Aだから応募が増える」等は禁止)。\n\
4. 各項目には、その考察の根拠となる evidence の id を evidence_ids に必ず列挙する。実在しない id を作らない。\n\
5. 提供データで不足している情報は caveat に明記する。\n\
6. 顧客固有の事情は不明なため、最終施策を断定しない。面談で確認すべき論点として書く。\n\
7. 公開統計・媒体データから言えることに限定し、企業名や個社の断定評価をしない。\n\
禁止表現の例: 『必ず採用できる』『応募が増える』『離職率が高い企業』『成長企業である』『この媒体が最適』。\n\
出力は日本語。専門用語や社内用語・略語は避け、平易な言葉で書く。";

/// evidence_pack JSON をプロンプト入力用の文字列にする (原データへは触れさせない §15.2)
fn evidence_pack_input(analysis: &ConsultAnalysis) -> String {
    let pack = to_evidence_pack_json(analysis);
    serde_json::to_string(&pack).unwrap_or_else(|_| "{}".to_string())
}

/// 2層 (単品+複合) のレスポンススキーマ (構造化出力)。
/// - lead_hypothesis: この1行だけ持ち帰る仮説
/// - single_item_insights: 単品層 (MUST evidence を全網羅、so_what は「だから〜」着地)
/// - composite_insights: 複合層 (3〜4本・4根拠以上・2テーマ以上)。
///   claim_axis / claim_direction は逆証明の道具箱 (T3 反対方向シグナル検索) の入力に使う。
/// - excluded: 使わなかった evidence と理由 (取りこぼしゼロ)
fn two_layer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "lead_hypothesis": { "type": "string" },
            "single_item_insights": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "evidence_id": { "type": "string" },
                        "observation": { "type": "string" },
                        "so_what": { "type": "string" }
                    },
                    "required": ["evidence_id", "observation", "so_what"]
                }
            },
            "composite_insights": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "thesis": { "type": "string" },
                        "evidence_ids": { "type": "array", "items": { "type": "string" } },
                        "themes": { "type": "array", "items": { "type": "string" } },
                        "so_what": { "type": "string" },
                        "ask": { "type": "string" },
                        "claim_axis": {
                            "type": "string",
                            "enum": ["demand", "supply", "competition", "offer", "other"]
                        },
                        "claim_direction": {
                            "type": "string",
                            "enum": ["problem", "opportunity", "neutral"]
                        }
                    },
                    "required": ["title", "thesis", "evidence_ids", "themes", "so_what", "ask", "claim_axis", "claim_direction"]
                }
            },
            "excluded": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "evidence_id": { "type": "string" },
                        "reason": { "type": "string" }
                    },
                    "required": ["evidence_id", "reason"]
                }
            }
        },
        "required": ["lead_hypothesis", "single_item_insights", "composite_insights", "excluded"]
    })
}

/// 一文要約のレスポンススキーマ
fn summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "summary": { "type": "string" } },
        "required": ["summary"]
    })
}

/// 複合考察 (主役) をサーバ側で検証し、通過した項目だけを返す。
///
/// 破棄条件:
/// - evidence_ids が空 (根拠なし §19.1)
/// - evidence_ids に analysis.evidence へ実在しない id を含む
/// - title / body / caveat のいずれかに禁止表現を含む (§19.2)
/// - title / body / caveat に言い過ぎ表現 (希少|不足|欠如… × 休日|人材|求職…) を含む (2層設計の強化)
/// - 複合の複合性が成立しない: 2テーマ未満 (単なる言い換え)
/// - 個社が主役: 先頭 evidence が個社、または過半が個社
pub fn validate_items(raw: &[AiItem], analysis: &ConsultAnalysis) -> Vec<AiItem> {
    let exists = |id: &str| analysis.evidence.iter().any(|e| e.id == id);
    raw.iter()
        .filter(|item| {
            if item.evidence_ids.is_empty() {
                return false;
            }
            if item.evidence_ids.iter().any(|id| !exists(id)) {
                return false;
            }
            if item.title.trim().is_empty() || item.body.trim().is_empty() {
                return false;
            }
            if contains_forbidden(&item.title)
                || contains_forbidden(&item.body)
                || contains_forbidden(&item.caveat)
            {
                return false;
            }
            // 言い過ぎ検出 (文レベルの過剰表現)。主役は clean であるべきなので破棄する。
            if super::theme::has_overclaim(&item.title)
                || super::theme::has_overclaim(&item.body)
                || super::theme::has_overclaim(&item.caveat)
            {
                return false;
            }
            // 複合性: 2テーマ以上
            if theme_span(item, analysis) < 2 {
                return false;
            }
            // 個社主役の降格 (先頭が個社 or 過半が個社)
            if company_headed(item, analysis) {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

/// 考察が跨るテーマ数 (個社は除いてカウント)。
fn theme_span(item: &AiItem, analysis: &ConsultAnalysis) -> usize {
    use std::collections::BTreeSet;
    let mut themes: BTreeSet<String> = BTreeSet::new();
    for id in &item.evidence_ids {
        if let Some(t) = super::theme::theme_of(analysis, id) {
            if t != "個社" {
                themes.insert(t);
            }
        }
    }
    themes.len()
}

/// 個社が主役か (先頭 evidence が個社、または過半が個社)。
fn company_headed(item: &AiItem, analysis: &ConsultAnalysis) -> bool {
    let is_company = |id: &str| super::theme::theme_of(analysis, id).as_deref() == Some("個社");
    if let Some(first) = item.evidence_ids.first() {
        if is_company(first) {
            return true;
        }
    }
    let n = item.evidence_ids.len();
    if n == 0 {
        return false;
    }
    let company_n = item.evidence_ids.iter().filter(|id| is_company(id)).count();
    company_n * 2 >= n
}

/// 単品層を検証し、通過した項目を返す。
///
/// 破棄条件: evidence_id が実在しない / so_what が空 (着地なし) / 禁止表現 or 言い過ぎ。
/// テーマはコード側で補完する (表示グルーピング用)。
pub fn validate_singles(raw: &[SingleInsight], analysis: &ConsultAnalysis) -> Vec<SingleInsight> {
    let exists = |id: &str| analysis.evidence.iter().any(|e| e.id == id);
    raw.iter()
        .filter(|s| {
            exists(&s.evidence_id)
                && !s.so_what.trim().is_empty()
                && !s.observation.trim().is_empty()
                && !contains_forbidden(&s.observation)
                && !contains_forbidden(&s.so_what)
                && !super::theme::has_overclaim(&s.observation)
                && !super::theme::has_overclaim(&s.so_what)
        })
        .map(|s| {
            let mut s = s.clone();
            // テーマをコードで補完 (LLM 出力に依存しない)
            s.theme = super::theme::theme_of(analysis, &s.evidence_id).unwrap_or_default();
            s
        })
        .collect()
}

/// 除外項目を検証する (実在 evidence + 理由非空のみ通す)。
pub fn validate_excluded(raw: &[ExcludedItem], analysis: &ConsultAnalysis) -> Vec<ExcludedItem> {
    let exists = |id: &str| analysis.evidence.iter().any(|e| e.id == id);
    raw.iter()
        .filter(|x| exists(&x.evidence_id) && !x.reason.trim().is_empty())
        .cloned()
        .collect()
}

/// 単品層の網羅を担保する: MUST evidence のうち単品にも複合にも登場しないものを、
/// コード側で最小限の単品として補完する (取りこぼしゼロ)。
///
/// 補完項目の observation/so_what は evidence の指標名から機械生成する (可能性表現)。
/// LLM が網羅できなかった分の穴埋めであり、内容の断定はしない。
pub fn backfill_singles(
    singles: &mut Vec<SingleInsight>,
    composites: &[AiItem],
    analysis: &ConsultAnalysis,
) -> usize {
    let plan = super::theme::coverage_plan(analysis);
    let mut covered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for s in singles.iter() {
        covered.insert(s.evidence_id.clone());
    }
    for c in composites {
        for id in &c.evidence_ids {
            covered.insert(id.clone());
        }
    }
    let mut added = 0usize;
    for id in &plan.must_single {
        if covered.contains(id) {
            continue;
        }
        if let Some(ev) = analysis.evidence.iter().find(|e| &e.id == id) {
            singles.push(SingleInsight {
                evidence_id: ev.id.clone(),
                observation: format!("{} は {} です。", ev.metric_name, ev.value_text),
                so_what: "だから、面談でこの点の受け止めを確認する余地がある可能性があります。"
                    .to_string(),
                theme: super::theme::theme_of(analysis, &ev.id).unwrap_or_default(),
            });
            added += 1;
        }
    }
    added
}

/// 一文要約の生成 (1回目の呼び出し)。検証を通れば Some。
async fn generate_summary(client: &GeminiClient, pack_json: &str) -> Option<String> {
    let user = format!(
        "次の evidence_pack (採用市場の構造化データ) を読み、市場環境を1〜2文で要約してください。\
         需要・供給・競争・自社の給与位置の4軸の判定 (metrics.axes) を中心に、\
         面談前の暫定的な観測であることが伝わる表現にしてください。\n\n{}",
        pack_json
    );
    let resp = client
        .generate_json(SYSTEM_CONSTRAINTS, &user, summary_schema())
        .await?;
    let summary = resp.get("summary")?.as_str()?.trim().to_string();
    if summary.is_empty() || contains_forbidden(&summary) {
        return None;
    }
    Some(summary)
}

/// テーマ付きの evidence 一覧をプロンプト入力用に整形する。
/// evidence ID は動的採番なので、その場の analysis から生成する (ハードコードしない)。
fn themed_evidence_block(analysis: &ConsultAnalysis) -> String {
    analysis
        .evidence
        .iter()
        .map(|e| {
            let theme = if e.theme.is_empty() {
                super::theme::classify(e).label_ja().to_string()
            } else {
                e.theme.clone()
            };
            format!(
                "{} [テーマ:{}] {} = {} ({})",
                e.id, theme, e.metric_name, e.value_text, e.granularity
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 2層生成の user プロンプトを構築する (テスト可能な純関数)。
///
/// pack から動的生成した以下を埋め込む:
/// - テーマ付き evidence 一覧 (単品網羅・複合のテーマ横断判定の入力)
/// - 精度ガード (build_accuracy_guards。client_input・発火/未発火で内容が変わる)
/// - 網羅ルール (MUST 単品 / 除外許可)
/// claim_axis / claim_direction のタグ付け指示は道具箱 T3 (反対方向シグナル検索) の入力になる。
fn two_layer_user_prompt(analysis: &ConsultAnalysis, client: &ClientInput) -> String {
    let ev_block = themed_evidence_block(analysis);
    let guards = super::theme::build_accuracy_guards(analysis, client);
    let plan = super::theme::coverage_plan(analysis);
    let fired: Vec<&str> = analysis
        .signals
        .iter()
        .filter(|s| s.fired)
        .map(|s| s.name.as_str())
        .collect();

    format!(
        "採用コンサルタントが商談前に読む『複合診断』を【2層】で作成してください。\n\n\
         ## 発火シグナル (=言えること)\n{fired}\n\n\
         ## 根拠evidence (テーマ付き)\n{ev}\n\n\
         {guards}\n\n\
         ## 網羅ルール (取りこぼしゼロ)\n\
         - single_item_insights は次の evidence を必ず全部取り上げてください (個社・弱信号を除く主要指標): {must}\n\
         - 除外してよいのは個社・弱信号・重複 (同じ趣旨) のみ。除外したものは excluded に理由付きで必ず書く。\n\
         - 単品と複合と除外を合わせて、上記 evidence を1つも取りこぼさないでください。\n\n\
         ## 複合性の絶対ルール\n\
         - composite_insights は 3〜4本。各項目の evidence_ids は4つ以上で、\
           **2つ以上の異なるテーマ** を必ず混ぜてください (同じテーマだけの並列は単なる言い換えで禁止)。\n\
         - 個社テーマの evidence を複合の主役 (先頭・過半) にしないでください。\n\
         - 各複合には claim_axis (demand=需要, supply=供給, competition=競争, offer=自社条件, other=その他) と\
           claim_direction (problem=採用への逆風, opportunity=好機・活用余地, neutral=中立) を必ずタグ付けしてください。\n\n\
         ## 出力する2層 (JSON)\n\
         (1) lead_hypothesis: この1行だけ持ち帰る仮説を1文。給与が確認済みなら\
             「給与では市場で勝てている（確認済）。ただし休日など他条件は未確認」の趣旨を含める。\n\
         (2) single_item_insights: 単品層。主要な観測ひとつずつに observation と\
             so_what (必ず「だから〜」で終わる着地文) を付ける。MUST evidence を網羅。\n\
         (3) composite_insights: 複合層。単品を織った戦略示唆を3〜4本。\
             title / thesis(2-3文) / evidence_ids(4+・2テーマ+) / themes / so_what(だから〜) / ask(面談質問)。\n\
         (4) excluded: 使わなかった evidence を {{evidence_id, reason}} で明示。\n\n\
         すべて可能性の表現にし、断定・因果断定をしないでください。平易な日本語で。",
        fired = if fired.is_empty() { "（発火シグナルなし）".to_string() } else { fired.join("、") },
        ev = ev_block,
        guards = guards,
        must = if plan.must_single.is_empty() { "（該当なし）".to_string() } else { plan.must_single.join(", ") },
    )
}

/// 2層生成レスポンスをパースする (検証前)。
fn parse_two_layer(
    v: &Value,
) -> (
    Option<String>,
    Vec<SingleInsight>,
    Vec<AiItem>,
    Vec<ExcludedItem>,
) {
    let lead = v
        .get("lead_hypothesis")
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let singles = v
        .get("single_item_insights")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|it| serde_json::from_value::<SingleInsight>(it.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    // composite_insights は AiItem 形状 (thesis→body) に写像する。逆証明の道具箱を再利用するため。
    let composites = v
        .get("composite_insights")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(composite_to_ai_item).collect())
        .unwrap_or_default();

    let excluded = v
        .get("excluded")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|it| serde_json::from_value::<ExcludedItem>(it.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    (lead, singles, composites, excluded)
}

/// composite_insights の1要素を AiItem に写像する (thesis+so_what+ask を body/caveat に集約)。
fn composite_to_ai_item(v: &Value) -> Option<AiItem> {
    let title = v.get("title")?.as_str()?.trim().to_string();
    let thesis = v
        .get("thesis")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim();
    let so_what = v
        .get("so_what")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim();
    let ask = v.get("ask").and_then(|s| s.as_str()).unwrap_or("").trim();
    let evidence_ids: Vec<String> = v
        .get("evidence_ids")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    // body = thesis + だから (so_what)。ask は caveat 側 (面談で確認する論点) に置く。
    let mut body = thesis.to_string();
    if !so_what.is_empty() {
        if !body.is_empty() {
            body.push(' ');
        }
        body.push_str(so_what);
    }
    let caveat = if ask.is_empty() {
        String::new()
    } else {
        format!("面談で確認: {}", ask)
    };
    Some(AiItem {
        title,
        body,
        evidence_ids,
        caveat,
        claim_axis: v
            .get("claim_axis")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        claim_direction: v
            .get("claim_direction")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        ..Default::default()
    })
}

/// 2層生成 (複合考察コール)。返り値は検証前の生データ。
async fn generate_two_layer(
    client: &GeminiClient,
    analysis: &ConsultAnalysis,
    client_input: &ClientInput,
) -> (
    Option<String>,
    Vec<SingleInsight>,
    Vec<AiItem>,
    Vec<ExcludedItem>,
) {
    let user = two_layer_user_prompt(analysis, client_input);
    let Some(resp) = client
        .generate_json(SYSTEM_CONSTRAINTS, &user, two_layer_schema())
        .await
    else {
        return (None, Vec::new(), Vec::new(), Vec::new());
    };
    parse_two_layer(&resp)
}

/// ブリーフ1回あたり最大2回の Gemini 呼び出しで AI 文章化を行う (要約1 + 2層生成1)。
/// 2層生成 (リード仮説 + 単品 + 複合 + 除外) を1コールで得て、複合には逆証明の道具箱
/// (非LLM・決定的) をかける。単品の網羅はコード側で担保 (backfill)。
///
/// 失敗・未設定・全破棄はすべて空 (または一部) の `AiComposite` として返り、パニックしない。
/// 道具箱チェックは複合考察を破棄せず、「確認が必要な点」「別の見方」として併記される。
pub async fn generate_ai_composite(
    client: &GeminiClient,
    analysis: &ConsultAnalysis,
) -> AiComposite {
    generate_ai_composite_with_client(client, analysis, &ClientInput::default()).await
}

/// client_input (提示給与など) を渡す版。精度ガードの動的生成に使う。
pub async fn generate_ai_composite_with_client(
    client: &GeminiClient,
    analysis: &ConsultAnalysis,
    client_input: &ClientInput,
) -> AiComposite {
    let pack_json = evidence_pack_input(analysis);

    // 1回目: 一文要約
    let one_line_summary = generate_summary(client, &pack_json).await;
    // 2回目: 2層生成 (リード + 単品 + 複合 + 除外)
    let (lead_raw, singles_raw, composites_raw, excluded_raw) =
        generate_two_layer(client, analysis, client_input).await;

    // サーバ側検証
    let lead_hypothesis =
        lead_raw.filter(|s| !contains_forbidden(s) && !super::theme::has_overclaim(s));
    let mut single_insights = validate_singles(&singles_raw, analysis);
    let validated_composites = validate_items(&composites_raw, analysis);
    let excluded = validate_excluded(&excluded_raw, analysis);

    // 反証ステージ: 逆証明の道具箱 (非LLM)。複合考察 (主役) にのみ適用。
    let (items, refuted_count, reviewed_count) =
        super::refute_toolbox::apply_toolbox(validated_composites, analysis);

    // 単品網羅の担保: MUST evidence の穴をコードで埋める (取りこぼしゼロ)。
    let backfilled = backfill_singles(&mut single_insights, &items, analysis);

    tracing::info!(
        summary_ok = one_line_summary.is_some(),
        lead_ok = lead_hypothesis.is_some(),
        singles_generated = singles_raw.len(),
        singles_validated = single_insights.len(),
        singles_backfilled = backfilled,
        composites_generated = composites_raw.len(),
        composites_validated = items.len(),
        excluded = excluded.len(),
        reviewed_count,
        refuted_count,
        "consult AI 2-layer composite finished"
    );

    AiComposite {
        one_line_summary,
        lead_hypothesis,
        single_insights,
        items,
        excluded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence_pack::{analyze, tests::rich_input};

    fn analysis() -> ConsultAnalysis {
        analyze(&rich_input())
    }

    /// テーマ別に evidence_id を1つ拾う (複合の2テーマ要件を満たすテスト用)。
    fn ev_id_of_theme(a: &ConsultAnalysis, theme: &str) -> String {
        a.evidence
            .iter()
            .find(|e| e.theme == theme)
            .map(|e| e.id.clone())
            .unwrap_or_else(|| panic!("テーマ {} の evidence がない", theme))
    }

    /// 2テーマ以上・非個社主役の妥当な複合考察 (検証を通る前提のベース)。
    fn valid_composite(a: &ConsultAnalysis, title: &str, body: &str) -> AiItem {
        // 供給 + 自社給与 の2テーマで組む (rich_input に両方ある)
        let supply = ev_id_of_theme(a, "供給");
        let salary = ev_id_of_theme(a, "自社給与");
        AiItem {
            title: title.to_string(),
            body: body.to_string(),
            evidence_ids: vec![supply, salary],
            caveat: "面談で確認: 例".to_string(),
            claim_axis: "supply".to_string(),
            claim_direction: "problem".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn forbidden_detection() {
        assert!(contains_forbidden("この施策で応募が増える"));
        assert!(contains_forbidden("SalesNow のデータより"));
        assert!(!contains_forbidden(
            "応募が増える可能性がある"
                .replace("応募が増える", "応募が集まる余地がある")
                .as_str()
        ));
        assert!(!contains_forbidden("露出が課題の可能性があります"));
    }

    #[test]
    fn validate_rejects_nonexistent_evidence_id() {
        let a = analysis();
        let supply = ev_id_of_theme(&a, "供給");
        let salary = ev_id_of_theme(&a, "自社給与");
        let items = vec![
            valid_composite(&a, "実在IDのみ", "根拠がそろっている可能性があります"),
            AiItem {
                title: "架空ID混入".to_string(),
                body: "考察の可能性があります".to_string(),
                evidence_ids: vec![supply, salary, "E-999".to_string()],
                caveat: "".to_string(),
                claim_axis: "supply".to_string(),
                claim_direction: "problem".to_string(),
                ..Default::default()
            },
        ];
        let out = validate_items(&items, &a);
        assert_eq!(out.len(), 1, "架空 evidence_id を含む項目は破棄する");
        assert_eq!(out[0].title, "実在IDのみ");
    }

    #[test]
    fn validate_rejects_empty_evidence_and_forbidden() {
        let a = analysis();
        let mut forbidden = valid_composite(&a, "禁止表現", "この施策で必ず採用できる");
        forbidden.body = "この施策で必ず採用できる".to_string();
        let mut service = valid_composite(&a, "サービス名混入", "SalesNow によると");
        service.body = "SalesNow によると".to_string();
        let items = vec![
            AiItem {
                title: "根拠なし".to_string(),
                body: "可能性があります".to_string(),
                evidence_ids: vec![],
                ..Default::default()
            },
            forbidden,
            service,
        ];
        let out = validate_items(&items, &a);
        assert!(out.is_empty(), "根拠なし・禁止表現・サービス名はすべて破棄");
    }

    #[test]
    fn validate_rejects_single_theme_composite() {
        // 2テーマ未満の複合は「単なる言い換え」として破棄する
        let a = analysis();
        let s1 = ev_id_of_theme(&a, "供給");
        // 供給テーマの evidence をもう1つ探す (自然増減など)
        let supply_ids: Vec<String> = a
            .evidence
            .iter()
            .filter(|e| e.theme == "供給")
            .map(|e| e.id.clone())
            .collect();
        assert!(
            supply_ids.len() >= 2,
            "供給テーマの evidence が2件以上ある前提"
        );
        let item = AiItem {
            title: "供給だけ".to_string(),
            body: "供給が細っている可能性があります".to_string(),
            evidence_ids: vec![s1, supply_ids[1].clone()],
            claim_axis: "supply".to_string(),
            claim_direction: "problem".to_string(),
            ..Default::default()
        };
        let out = validate_items(&[item], &a);
        assert!(out.is_empty(), "同一テーマだけの複合は破棄する");
    }

    #[test]
    fn validate_demotes_company_headed_composite() {
        // 個社が主役 (先頭が個社) の複合は降格 (破棄)
        let a = analysis();
        let company = ev_id_of_theme(&a, "個社");
        let salary = ev_id_of_theme(&a, "自社給与");
        let item = AiItem {
            title: "個社主役".to_string(),
            body: "この企業の動きから市場を語る可能性があります".to_string(),
            evidence_ids: vec![company, salary], // 先頭が個社
            claim_axis: "competition".to_string(),
            claim_direction: "problem".to_string(),
            ..Default::default()
        };
        let out = validate_items(&[item], &a);
        assert!(out.is_empty(), "先頭が個社の複合は降格する");
    }

    #[test]
    fn validate_rejects_overclaim_composite() {
        // 言い過ぎ (希少|不足… × 休日|人材…) を含む複合は破棄する
        let a = analysis();
        let item = valid_composite(
            &a,
            "言い過ぎ",
            "この地域は人材が枯渇している可能性があります",
        );
        let out = validate_items(&[item], &a);
        assert!(out.is_empty(), "言い過ぎ表現の複合は破棄する");
    }

    #[test]
    fn validate_singles_requires_landing_and_real_id() {
        let a = analysis();
        let real = a.evidence[0].id.clone();
        let raw = vec![
            SingleInsight {
                evidence_id: real.clone(),
                observation: "有効求人倍率は中程度です。".to_string(),
                so_what: "だから採用難易度は中程度の可能性があります。".to_string(),
                theme: String::new(),
            },
            // so_what 空 → 破棄
            SingleInsight {
                evidence_id: real.clone(),
                observation: "観測".to_string(),
                so_what: "".to_string(),
                theme: String::new(),
            },
            // 架空ID → 破棄
            SingleInsight {
                evidence_id: "E-999".to_string(),
                observation: "観測".to_string(),
                so_what: "だから〜の可能性。".to_string(),
                theme: String::new(),
            },
            // 言い過ぎ → 破棄
            SingleInsight {
                evidence_id: real,
                observation: "休日が皆無です。".to_string(),
                so_what: "だから厳しい可能性。".to_string(),
                theme: String::new(),
            },
        ];
        let out = validate_singles(&raw, &a);
        assert_eq!(out.len(), 1, "着地あり・実在ID・非過剰のみ通す");
        assert!(!out[0].theme.is_empty(), "テーマがコードで補完される");
    }

    #[test]
    fn backfill_covers_all_must_evidence() {
        // 単品も複合も空のとき、MUST evidence がすべて単品に補完される (取りこぼしゼロ)
        let a = analysis();
        let plan = super::super::theme::coverage_plan(&a);
        let mut singles: Vec<SingleInsight> = Vec::new();
        let added = backfill_singles(&mut singles, &[], &a);
        assert_eq!(added, plan.must_single.len());
        let covered: std::collections::BTreeSet<String> =
            singles.iter().map(|s| s.evidence_id.clone()).collect();
        for id in &plan.must_single {
            assert!(covered.contains(id), "MUST {} が単品に補完される", id);
        }
        // 補完項目も着地文を持つ
        assert!(singles.iter().all(|s| s.so_what.contains("だから")));
    }

    #[test]
    fn two_layer_schema_has_all_layers() {
        let s = two_layer_schema();
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        for k in [
            "lead_hypothesis",
            "single_item_insights",
            "composite_insights",
            "excluded",
        ] {
            assert!(req.contains(&k), "スキーマに {} がない", k);
        }
        // 複合には claim タグが必須 (道具箱 T3 の入力)
        let ci = &s["properties"]["composite_insights"]["items"]["required"];
        let ci_req: Vec<&str> = ci
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(ci_req.contains(&"claim_axis") && ci_req.contains(&"themes"));
        assert_eq!(
            summary_schema()["properties"]["summary"]["type"],
            json!("string")
        );
    }

    #[test]
    fn empty_composite_is_reported_empty() {
        let c = AiComposite::default();
        assert!(c.is_empty());
    }

    #[test]
    fn two_layer_prompt_embeds_guards_and_coverage() {
        // 生成プロンプトに精度ガード・網羅ルール・claim タグ指示が退行なく入る
        let a = analysis();
        let prompt = two_layer_user_prompt(
            &a,
            &ClientInput {
                target_salary_max: Some(300_000),
                ..Default::default()
            },
        );
        assert!(prompt.contains("精度ガード"), "精度ガードが埋め込まれる");
        assert!(
            prompt.contains("給与では勝てている"),
            "給与確認済みガードが入る"
        );
        assert!(prompt.contains("網羅ルール") && prompt.contains("必ず全部"));
        assert!(prompt.contains("claim_axis") && prompt.contains("demand=需要"));
        assert!(
            prompt.contains("2つ以上の異なるテーマ"),
            "複合性ルールが入る"
        );
        // MUST evidence の id が列挙される (動的採番)
        let plan = super::super::theme::coverage_plan(&a);
        assert!(
            prompt.contains(&plan.must_single[0]),
            "MUST evidence id が入る"
        );
    }

    #[test]
    fn ai_item_serializes_refute_fields() {
        // AiItem シリアライズ: 反証フィールド + claim タグが JSON に出る (evidence_pack.json 用)
        let item = AiItem {
            title: "t".to_string(),
            body: "b".to_string(),
            evidence_ids: vec!["E-001".to_string()],
            caveat: "c".to_string(),
            claim_axis: "supply".to_string(),
            claim_direction: "problem".to_string(),
            refuted: true,
            refute_reason: Some("理由".to_string()),
            alt_interpretation: Some("別の見方".to_string()),
            reviewed: true,
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["refuted"], json!(true));
        assert_eq!(v["refute_reason"], json!("理由"));
        assert_eq!(v["alt_interpretation"], json!("別の見方"));
        assert_eq!(v["reviewed"], json!(true));
        assert_eq!(v["claim_axis"], json!("supply"));
        assert_eq!(v["claim_direction"], json!("problem"));
        // 生成コール JSON (反証フィールドなし) からデシリアライズしても既定値で通る
        let gen = json!({
            "title": "g", "body": "gb", "evidence_ids": ["E-002"], "caveat": "",
            "claim_axis": "demand", "claim_direction": "neutral"
        });
        let parsed: AiItem = serde_json::from_value(gen).unwrap();
        assert!(!parsed.refuted);
        assert!(!parsed.reviewed);
        assert!(parsed.refute_reason.is_none());
        assert_eq!(parsed.claim_axis, "demand");
    }

    /// 実 Gemini 呼び出しテスト (§検証): GEMINI_API_KEY があれば1回だけ実呼び出し。
    /// なければスキップ。キーは env から読む (ハードコードしない)。
    /// 生成→道具箱反証→裁定の通しを検証する (道具箱は非LLMなので追加コールなし)。
    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY; run explicitly"]
    async fn live_gemini_composite_schema_conformance() {
        let Some(client) = GeminiClient::from_env() else {
            eprintln!("SKIP: GEMINI_API_KEY 未設定のためスキップ");
            return;
        };
        let a = analysis();
        let composite = generate_ai_composite(&client, &a).await;
        // 実呼び出し結果はネットワーク状況に依存するためパニックしないことを確認し、
        // 返ってきた項目はすべて検証済み (実在 evidence_id・禁止表現なし) であること。
        let mut refuted = 0;
        for item in &composite.items {
            for id in &item.evidence_ids {
                assert!(
                    a.evidence.iter().any(|e| &e.id == id),
                    "検証を通った項目の evidence_id は実在する: {}",
                    id
                );
            }
            assert!(!contains_forbidden(&item.body));
            // 道具箱裁定の後条件: 考察が返った場合は必ず reviewed=true (決定的チェック)。
            assert!(item.reviewed, "道具箱チェックは常に実施される");
            // refuted=true は refute_reason=Some を含意する。
            if item.refuted {
                assert!(item.refute_reason.is_some());
                refuted += 1;
            }
            if let Some(r) = &item.refute_reason {
                assert!(!contains_forbidden(r));
            }
            if let Some(alt) = &item.alt_interpretation {
                assert!(!contains_forbidden(alt));
            }
        }
        eprintln!(
            "live gemini: summary={:?}, items={}, refuted={}",
            composite.one_line_summary.is_some(),
            composite.items.len(),
            refuted
        );
    }
}
