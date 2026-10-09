//! ステージの必須項目 (HubSpot の "conditional stage properties") の設定と、HubSpot の定義とのずれの検査。
//!
//! HubSpot はステージを移すときに必須項目を画面で求めるが、**API の書き込みでは強制しない** (2026-10-09 確認)。
//! そのため HR_HR が `PATCH /api/crm/deals/{id}` でステージを移す前に、移動先の必須項目が空でないことを確かめる。
//! ルールの正本は `stage_rules.json` (HubSpot の画面の内部 API を 2026-10-09 に読み取り専用で写したもの。
//! 出典と日付はファイルの先頭に書いてある)。HubSpot 側でルールを変えたら JSON を更新する。
//!
//! ずれの検査 ([`check_drift`]): 公開のパイプライン API (`GET /crm/v3/pipelines/deals`) と JSON を突き合わせて
//! 「JSON にあるステージが HubSpot に無い」「ステージ名が変わった」を管理者に出す。**ルールそのものの変更は公開 API で
//! 読めないため検出できない** (そこは JSON の更新運用に頼る)。HubSpot への書き込みはしない。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::handlers::crm_metadata::CrmPipeline;
use crate::hubspot::HubSpotClient;
use crate::AppState;

const RULES_JSON: &str = include_str!("stage_rules.json");

/// ずれの検査の間隔 (低頻度。HubSpot の呼び出しは 1 回)
pub const DRIFT_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Deserialize)]
pub struct RuleProp {
    pub name: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StageRule {
    pub stage_id: String,
    pub pipeline_id: Option<String>,
    pub label: Option<String>,
    pub props: Vec<RuleProp>,
}

#[derive(Debug, Deserialize)]
struct RulesFile {
    source: String,
    read_at: String,
    rules: Vec<StageRule>,
}

/// 読み込んだルール (ステージ ID で引く)
#[derive(Debug)]
pub struct StageRules {
    pub source: String,
    pub read_at: String,
    by_stage: HashMap<String, StageRule>,
}

impl StageRules {
    pub fn parse(json: &str) -> Result<Self, String> {
        let f: RulesFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut by_stage = HashMap::new();
        for r in f.rules {
            if by_stage.insert(r.stage_id.clone(), r.clone()).is_some() {
                return Err(format!("stage {} が重複", r.stage_id));
            }
        }
        Ok(Self {
            source: f.source,
            read_at: f.read_at,
            by_stage,
        })
    }

    pub fn get(&self, stage_id: &str) -> Option<&StageRule> {
        self.by_stage.get(stage_id)
    }

    /// 移動先で必須の項目 (JSON の並び)
    pub fn required(&self, stage_id: &str) -> Vec<String> {
        self.get(stage_id)
            .map(|r| {
                r.props
                    .iter()
                    .filter(|p| p.required)
                    .map(|p| p.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 移動先で表示される項目 (必須を含む)
    pub fn shown(&self, stage_id: &str) -> Vec<String> {
        self.get(stage_id)
            .map(|r| r.props.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.by_stage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_stage.is_empty()
    }

    pub fn all(&self) -> impl Iterator<Item = &StageRule> {
        self.by_stage.values()
    }
}

static RULES: OnceLock<StageRules> = OnceLock::new();

/// 起動時に 1 回だけ読む。JSON が壊れていればここで panic する (テストで必ず通るのでビルド済みなら起きない)
pub fn rules() -> &'static StageRules {
    RULES.get_or_init(|| StageRules::parse(RULES_JSON).expect("stage_rules.json を読めない"))
}

// ---------------------------------------------------------------------------
// ずれの検査
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
pub struct StageRuleMismatch {
    /// `stage_missing_in_hubspot` (JSON のステージが HubSpot に無い) /
    /// `stage_moved_pipeline` (別のパイプラインに移っている) /
    /// `label_changed` (ステージ名が違う。参考情報) /
    /// `hubspot_stage_not_in_queue_table` (キューの表に無い新しいステージ)
    pub kind: String,
    pub stage_id: String,
    pub pipeline_id: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct StageRulesDriftReport {
    /// 検査したことがあるか (起動直後・HubSpot 未設定は false)
    pub checked: bool,
    pub checked_at: Option<String>,
    /// 検査に失敗したときの `error_kind`
    pub error_kind: Option<String>,
    pub rules_source: String,
    pub rules_read_at: String,
    #[ts(type = "number")]
    pub rules_count: u32,
    pub mismatches: Vec<StageRuleMismatch>,
}

fn empty_report() -> StageRulesDriftReport {
    let r = rules();
    StageRulesDriftReport {
        checked: false,
        checked_at: None,
        error_kind: None,
        rules_source: r.source.clone(),
        rules_read_at: r.read_at.clone(),
        rules_count: r.len() as u32,
        mismatches: Vec::new(),
    }
}

static LAST_REPORT: Mutex<Option<StageRulesDriftReport>> = Mutex::new(None);

/// 直近の検査結果 (検査前は `checked: false`)
pub fn last_report() -> StageRulesDriftReport {
    LAST_REPORT
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(empty_report)
}

/// パイプライン定義と JSON を突き合わせる (純粋関数。テストで固定の入力を流す)
pub fn compare(rules: &StageRules, defs: &[CrmPipeline]) -> Vec<StageRuleMismatch> {
    let mut stage_pipeline: HashMap<&str, (&str, &str)> = HashMap::new();
    for p in defs {
        for s in &p.stages {
            stage_pipeline.insert(s.id.as_str(), (p.id.as_str(), s.label.as_str()));
        }
    }
    let mut out = Vec::new();
    let mut ids: Vec<&StageRule> = rules.all().collect();
    ids.sort_by(|a, b| a.stage_id.cmp(&b.stage_id));
    for r in ids {
        match stage_pipeline.get(r.stage_id.as_str()) {
            None => out.push(StageRuleMismatch {
                kind: "stage_missing_in_hubspot".into(),
                stage_id: r.stage_id.clone(),
                pipeline_id: r.pipeline_id.clone(),
                detail: format!(
                    "ルールのあるステージ {} ({}) が HubSpot のパイプライン定義に無い",
                    r.stage_id,
                    r.label.as_deref().unwrap_or("-")
                ),
            }),
            Some((pid, label)) => {
                if r.pipeline_id.as_deref().is_some_and(|c| c != *pid) {
                    out.push(StageRuleMismatch {
                        kind: "stage_moved_pipeline".into(),
                        stage_id: r.stage_id.clone(),
                        pipeline_id: Some((*pid).to_string()),
                        detail: format!(
                            "設定のパイプライン {} と HubSpot の {} が違う",
                            r.pipeline_id.as_deref().unwrap_or("-"),
                            pid
                        ),
                    });
                }
                if r.label.as_deref().is_some_and(|l| l != *label) {
                    out.push(StageRuleMismatch {
                        kind: "label_changed".into(),
                        stage_id: r.stage_id.clone(),
                        pipeline_id: Some((*pid).to_string()),
                        detail: format!(
                            "設定のステージ名「{}」と HubSpot の「{}」が違う (参考情報)",
                            r.label.as_deref().unwrap_or(""),
                            label
                        ),
                    });
                }
            }
        }
    }
    // キューの表に無い新しいステージ (ルールが付いたかもしれないので見てもらう)
    let known: HashSet<&str> = super::queue_pipelines::QUEUE_PIPELINES
        .iter()
        .flat_map(|p| p.stages.iter().map(|(id, _)| *id))
        .collect();
    for p in defs {
        if super::queue_pipelines::find_pipeline(&p.id).is_none() {
            continue;
        }
        for s in &p.stages {
            if !known.contains(s.id.as_str()) {
                out.push(StageRuleMismatch {
                    kind: "hubspot_stage_not_in_queue_table".into(),
                    stage_id: s.id.clone(),
                    pipeline_id: Some(p.id.clone()),
                    detail: format!(
                        "HubSpot のステージ「{}」が queue_pipelines の表に無い (ルールがあるか確認)",
                        s.label
                    ),
                });
            }
        }
    }
    out
}

/// 1 回検査して結果を保存する (背景の優先度のクライアントを渡す。書き込みはしない)
pub async fn check_drift(client: &HubSpotClient) -> StageRulesDriftReport {
    let mut report = empty_report();
    report.checked_at = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let read = async {
        let v = client.deal_pipelines().await?;
        crate::handlers::crm_metadata::parse_pipelines(&v)
    };
    match read.await {
        Ok(defs) => {
            report.checked = true;
            report.mismatches = compare(rules(), &defs);
        }
        Err(e) => {
            report.error_kind = Some(e.error_kind().to_string());
            tracing::warn!(
                error_kind = e.error_kind(),
                "crm stage rules drift check failed"
            );
        }
    }
    if let Ok(mut g) = LAST_REPORT.lock() {
        *g = Some(report.clone());
    }
    report
}

/// 低頻度の背景検査を始める (起動 2 分後に 1 回、その後 [`DRIFT_INTERVAL`] ごと)
pub fn spawn_drift_check(state: Arc<AppState>) {
    let Some(client) = state.hubspot.clone() else {
        return;
    };
    let client = client.background();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            let r = check_drift(&client).await;
            if !r.mismatches.is_empty() {
                tracing::warn!(
                    count = r.mismatches.len(),
                    "crm stage rules: HubSpot の定義とずれがあります"
                );
            }
            tokio::time::sleep(DRIFT_INTERVAL).await;
        }
    });
}

/// `GET /api/admin/crm-stage-rules-drift` (管理者のみ)。直近の検査結果を返す。HubSpot は呼ばない
pub async fn api_stage_rules_drift(State(_state): State<Arc<AppState>>) -> Response {
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(last_report()),
    )
        .into_response()
}

/// ステージ ID → ルールの項目 (パイプラインごとの「ルールに出る全項目」)
pub fn props_in_pipeline(pipeline_id: &str) -> BTreeMap<String, bool> {
    let mut m = BTreeMap::new();
    for r in rules().all() {
        if r.pipeline_id.as_deref() == Some(pipeline_id) {
            for p in &r.props {
                let e = m.entry(p.name.clone()).or_insert(false);
                *e |= p.required;
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::crm_metadata::CrmStage;

    #[test]
    fn 設定は読めて_不通は_bpo_10_を必須にする() {
        let r = rules();
        assert_eq!(r.len(), 34);
        assert_eq!(r.required("1095387443"), vec!["bpo_10"]);
        assert_eq!(r.shown("1095387443"), vec!["bpo_10", "bpo_31"]);
        // アポ日確定: 表示 25 項目のうち必須 22 項目 (bpo_28 / charge_impression / risuto_tanntouyakusyoku は任意)
        assert_eq!(r.shown("1095457875").len(), 25);
        let req = r.required("1095457875");
        assert_eq!(req.len(), 22);
        assert!(req.contains(&"bpo_51".to_string()));
        assert!(!req.contains(&"bpo_28".to_string()));
        assert!(r.required("999999").is_empty());
        assert!(r.source.contains("ConditionalPropertiesV2Rpc"));
        assert_eq!(r.read_at, "2026-10-09");
    }

    #[test]
    fn 設定の全ステージはキューの表にある() {
        for rule in rules().all() {
            let pid = rule.pipeline_id.as_deref().expect("pipeline_id");
            let p = super::super::queue_pipelines::find_pipeline(pid).expect("pipeline");
            assert!(p.rule(&rule.stage_id).is_some(), "{}", rule.stage_id);
        }
    }

    fn defs(stages: &[(&str, &str)], pid: &str) -> Vec<CrmPipeline> {
        vec![CrmPipeline {
            id: pid.into(),
            label: "p".into(),
            stages: stages
                .iter()
                .map(|(i, l)| CrmStage {
                    id: (*i).into(),
                    label: (*l).into(),
                })
                .collect(),
        }]
    }

    #[test]
    fn ずれの検出() {
        let rules = StageRules::parse(
            r#"{"source":"s","read_at":"d","rules":[
              {"stage_id":"1","pipeline_id":"753186575","label":"不通","props":[{"name":"a","required":true}]},
              {"stage_id":"2","pipeline_id":"753186575","label":"不在","props":[]}]}"#,
        )
        .unwrap();
        let m = compare(&rules, &defs(&[("2", "不在改")], "753186575"));
        let kinds: Vec<_> = m
            .iter()
            .map(|x| (x.kind.as_str(), x.stage_id.as_str()))
            .collect();
        assert!(
            kinds.contains(&("stage_missing_in_hubspot", "1")),
            "{kinds:?}"
        );
        assert!(kinds.contains(&("label_changed", "2")), "{kinds:?}");
        // 表に無い新ステージ
        assert!(
            kinds.contains(&("hubspot_stage_not_in_queue_table", "2")),
            "{kinds:?}"
        );
        // ずれなし
        let ok = compare(&rules, &defs(&[("1", "不通"), ("2", "不在")], "753186575"));
        assert!(ok
            .iter()
            .all(|x| x.kind == "hubspot_stage_not_in_queue_table"));
    }
}
