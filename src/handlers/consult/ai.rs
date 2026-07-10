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
//! ## graceful degradation
//! GEMINI_API_KEY 未設定・API失敗・全項目破棄のいずれでもパニックせず、
//! 空の `AiComposite` を返す。呼び出し側 (brief_html) はセクションを省略し1行の注記を出す。
//!
//! ブリーフ生成1回あたり Gemini 呼び出しは最大2回 (要約1 + 複合考察1)。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::evidence_pack::{to_evidence_pack_json, ConsultAnalysis};
use crate::gemini::GeminiClient;

/// 複合考察の1項目 (§18.3: 根拠ID保持・可能性口調)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiItem {
    pub title: String,
    pub body: String,
    pub evidence_ids: Vec<String>,
    /// 不足データ・留意点 (§18.3-5)
    pub caveat: String,
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

/// 複合考察のレスポンススキーマ (構造化出力)
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
                        "caveat": { "type": "string" }
                    },
                    "required": ["title", "body", "evidence_ids", "caveat"]
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

/// 複合考察項目の生成 (2回目の呼び出し)。返り値は検証前の生項目。
async fn generate_composite_items(client: &GeminiClient, pack_json: &str) -> Vec<AiItem> {
    let user = format!(
        "次の evidence_pack を読み、複数のシグナル (signals) や矛盾 (contradictions) を\
         つなげて解釈した『複合考察』を3〜5項目作成してください。\
         単独の指標の言い換えではなく、複数の根拠を結びつけた気づきにしてください。\
         各項目は body を2〜3文にし、根拠にした evidence の id を evidence_ids に列挙し、\
         不足している情報や留意点を caveat に書いてください。\
         すべて可能性の表現にし、断定しないでください。\n\n{}",
        pack_json
    );
    let Some(resp) = client
        .generate_json(SYSTEM_CONSTRAINTS, &user, composite_schema())
        .await
    else {
        return Vec::new();
    };
    parse_items(&resp)
}

/// ブリーフ1回あたり最大2回の Gemini 呼び出しで AI 文章化を行う。
///
/// 失敗・未設定・全破棄はすべて空 (または一部) の `AiComposite` として返り、パニックしない。
pub async fn generate_ai_composite(
    client: &GeminiClient,
    analysis: &ConsultAnalysis,
) -> AiComposite {
    let pack_json = evidence_pack_input(analysis);

    // 1回目: 一文要約
    let one_line_summary = generate_summary(client, &pack_json).await;
    // 2回目: 複合考察 → サーバ側検証
    let raw_items = generate_composite_items(client, &pack_json).await;
    let items = validate_items(&raw_items, analysis);

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
            },
            AiItem {
                title: "架空ID混入".to_string(),
                body: "考察の可能性があります".to_string(),
                evidence_ids: vec![real_id, "E-999".to_string()],
                caveat: "".to_string(),
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
            },
            AiItem {
                title: "禁止表現".to_string(),
                body: "この施策で必ず採用できる".to_string(),
                evidence_ids: vec![real_id.clone()],
                caveat: "".to_string(),
            },
            AiItem {
                title: "サービス名混入".to_string(),
                body: "SalesNow によると".to_string(),
                evidence_ids: vec![real_id],
                caveat: "".to_string(),
            },
        ];
        let out = validate_items(&items, &a);
        assert!(out.is_empty(), "根拠なし・禁止表現・サービス名はすべて破棄");
    }

    #[test]
    fn parse_items_from_gemini_shape() {
        let v = json!({
            "items": [
                { "title": "t1", "body": "b1", "evidence_ids": ["E-001"], "caveat": "c1" },
                { "title": "t2", "body": "b2", "evidence_ids": [], "caveat": "" }
            ]
        });
        let items = parse_items(&v);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "t1");
        assert_eq!(items[0].evidence_ids, vec!["E-001"]);
    }

    #[test]
    fn schemas_are_objects_with_required() {
        assert_eq!(composite_schema()["type"], json!("object"));
        assert!(composite_schema()["properties"]["items"].is_object());
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

    /// 実 Gemini 呼び出しテスト (§検証): GEMINI_API_KEY があれば1回だけ実呼び出し。
    /// なければスキップ。キーは env から読む (ハードコードしない)。
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
        for item in &composite.items {
            for id in &item.evidence_ids {
                assert!(
                    a.evidence.iter().any(|e| &e.id == id),
                    "検証を通った項目の evidence_id は実在する: {}",
                    id
                );
            }
            assert!(!contains_forbidden(&item.body));
        }
        eprintln!(
            "live gemini: summary={:?}, items={}",
            composite.one_line_summary.is_some(),
            composite.items.len()
        );
    }
}
