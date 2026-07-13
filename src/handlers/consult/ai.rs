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

/// AI 文章化の結果一式
#[derive(Debug, Clone, Default)]
pub struct AiComposite {
    /// 一文要約の自然文 (検証を通ったときのみ Some)
    pub one_line_summary: Option<String>,
    /// 検証を通った複合考察項目
    pub items: Vec<AiItem>,
}

impl AiComposite {
    pub fn is_empty(&self) -> bool {
        self.one_line_summary.is_none() && self.items.is_empty()
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

/// 複合考察のレスポンススキーマ (構造化出力)。
/// claim_axis / claim_direction は逆証明の道具箱 (T3 反対方向シグナル検索) の入力に使う。
fn composite_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "body": { "type": "string" },
                        "evidence_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "caveat": { "type": "string" },
                        "claim_axis": {
                            "type": "string",
                            "enum": ["demand", "supply", "competition", "offer", "other"]
                        },
                        "claim_direction": {
                            "type": "string",
                            "enum": ["problem", "opportunity", "neutral"]
                        }
                    },
                    "required": ["title", "body", "evidence_ids", "caveat", "claim_axis", "claim_direction"]
                }
            }
        },
        "required": ["items"]
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

/// LLM が返した項目群をサーバ側で検証し、通過した項目だけを返す。
///
/// 破棄条件:
/// - evidence_ids が空 (根拠なし §19.1)
/// - evidence_ids に analysis.evidence へ実在しない id を含む
/// - title / body / caveat のいずれかに禁止表現を含む (§19.2)
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
            true
        })
        .cloned()
        .collect()
}

/// Gemini の複合考察レスポンス Value から AiItem 群をパースする (検証前)
fn parse_items(v: &Value) -> Vec<AiItem> {
    v.get("items")
        .and_then(|i| i.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|it| serde_json::from_value::<AiItem>(it.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
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

/// 複合考察生成の user プロンプトを構築する (テスト可能な純関数)。
/// claim_axis / claim_direction のタグ付け指示は道具箱 T3 (反対方向シグナル検索) の入力になる。
fn composite_user_prompt(pack_json: &str) -> String {
    format!(
        "次の evidence_pack を読み、複数のシグナル (signals) や矛盾 (contradictions) を\
         つなげて解釈した『複合考察』を3〜5項目作成してください。\
         単独の指標の言い換えではなく、複数の根拠を結びつけた気づきにしてください。\
         各項目は body を2〜3文にし、根拠にした evidence の id を evidence_ids に列挙し、\
         不足している情報や留意点を caveat に書いてください。\
         各項目には、主張が主に関わる軸を claim_axis \
         (demand=需要, supply=供給, competition=競争, offer=自社条件, other=その他) で、\
         主張の向きを claim_direction (problem=採用への逆風, opportunity=好機・活用余地, \
         neutral=中立) で必ずタグ付けしてください。\
         すべて可能性の表現にし、断定しないでください。\n\n{}",
        pack_json
    )
}

/// 複合考察項目の生成 (2回目の呼び出し)。返り値は検証前の生項目。
async fn generate_composite_items(client: &GeminiClient, pack_json: &str) -> Vec<AiItem> {
    let user = composite_user_prompt(pack_json);
    let Some(resp) = client
        .generate_json(SYSTEM_CONSTRAINTS, &user, composite_schema())
        .await
    else {
        return Vec::new();
    };
    parse_items(&resp)
}

/// ブリーフ1回あたり最大2回の Gemini 呼び出しで AI 文章化を行う (要約1 + 複合考察1)。
/// 生成後の考察には逆証明の道具箱 (非LLM・決定的) による反証チェックをかける。
///
/// 失敗・未設定・全破棄はすべて空 (または一部) の `AiComposite` として返り、パニックしない。
/// 道具箱チェックは考察を破棄せず、「確認が必要な点」「別の見方」として併記される。
pub async fn generate_ai_composite(
    client: &GeminiClient,
    analysis: &ConsultAnalysis,
) -> AiComposite {
    let pack_json = evidence_pack_input(analysis);

    // 1回目: 一文要約
    let one_line_summary = generate_summary(client, &pack_json).await;
    // 2回目: 複合考察 → サーバ側検証
    let raw_items = generate_composite_items(client, &pack_json).await;
    let validated = validate_items(&raw_items, analysis);

    // 反証ステージ: 逆証明の道具箱 (非LLM)。T1 標本数 / T2 粒度 / T3 反対方向 / T4 逆因果。
    let (items, refuted_count, reviewed_count) =
        super::refute_toolbox::apply_toolbox(validated, analysis);

    // 失敗診断用: 「API が返さなかった」のか「検証で破棄された」のかをログで区別できるようにする
    tracing::info!(
        summary_ok = one_line_summary.is_some(),
        generated = raw_items.len(),
        validated = items.len(),
        reviewed_count,
        refuted_count,
        "consult AI composite finished"
    );

    AiComposite {
        one_line_summary,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence_pack::{analyze, tests::rich_input};

    fn analysis() -> ConsultAnalysis {
        analyze(&rich_input())
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
        let real_id = a.evidence[0].id.clone();
        let items = vec![
            AiItem {
                title: "実在IDのみ".to_string(),
                body: "根拠がそろっている可能性があります".to_string(),
                evidence_ids: vec![real_id.clone()],
                caveat: "要確認".to_string(),
                ..Default::default()
            },
            AiItem {
                title: "架空ID混入".to_string(),
                body: "考察の可能性があります".to_string(),
                evidence_ids: vec![real_id, "E-999".to_string()],
                caveat: "".to_string(),
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
        let real_id = a.evidence[0].id.clone();
        let items = vec![
            AiItem {
                title: "根拠なし".to_string(),
                body: "可能性があります".to_string(),
                evidence_ids: vec![],
                caveat: "".to_string(),
                ..Default::default()
            },
            AiItem {
                title: "禁止表現".to_string(),
                body: "この施策で必ず採用できる".to_string(),
                evidence_ids: vec![real_id.clone()],
                caveat: "".to_string(),
                ..Default::default()
            },
            AiItem {
                title: "サービス名混入".to_string(),
                body: "SalesNow によると".to_string(),
                evidence_ids: vec![real_id],
                caveat: "".to_string(),
                ..Default::default()
            },
        ];
        let out = validate_items(&items, &a);
        assert!(out.is_empty(), "根拠なし・禁止表現・サービス名はすべて破棄");
    }

    #[test]
    fn parse_items_from_gemini_shape() {
        // claim_axis / claim_direction を含む生成コールの JSON 形状
        let v = json!({
            "items": [
                { "title": "t1", "body": "b1", "evidence_ids": ["E-001"], "caveat": "c1",
                  "claim_axis": "supply", "claim_direction": "problem" },
                { "title": "t2", "body": "b2", "evidence_ids": [], "caveat": "" }
            ]
        });
        let items = parse_items(&v);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "t1");
        assert_eq!(items[0].evidence_ids, vec!["E-001"]);
        assert_eq!(items[0].claim_axis, "supply");
        assert_eq!(items[0].claim_direction, "problem");
        // タグ欠落 (旧形状) でもデフォルト空文字でパースできる
        assert_eq!(items[1].claim_axis, "");
    }

    #[test]
    fn schemas_are_objects_with_required() {
        let cs = composite_schema();
        assert_eq!(cs["type"], json!("object"));
        assert!(cs["properties"]["items"].is_object());
        // 道具箱 T3 の入力になる claim タグは必須 + enum 制約つき
        let item_schema = &cs["properties"]["items"]["items"];
        assert!(item_schema["properties"]["claim_axis"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("supply")));
        assert!(item_schema["properties"]["claim_direction"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("problem")));
        let required: Vec<&str> = item_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"claim_axis"));
        assert!(required.contains(&"claim_direction"));
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
    fn generation_prompt_mentions_claim_tagging() {
        // 生成プロンプトのタグ付け指示が退行しないこと (T3 の入力が絶たれると道具箱が弱る)
        let prompt = composite_user_prompt("{}");
        assert!(
            prompt.contains("claim_axis") && prompt.contains("demand=需要"),
            "生成プロンプトに claim_axis のタグ付け指示がある"
        );
        assert!(
            prompt.contains("claim_direction") && prompt.contains("problem=採用への逆風"),
            "生成プロンプトに claim_direction のタグ付け指示がある"
        );
        assert!(
            prompt.ends_with("{}"),
            "evidence_pack JSON が末尾に埋め込まれる"
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
