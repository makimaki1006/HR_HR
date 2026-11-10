//! 初回契約の関門。Ver1・Ver2・今日の候補で同じ判定を使う。
use super::{date10, Deal, Sheets};
use chrono::{Duration, NaiveDate};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

#[derive(Debug, Serialize)]
pub struct Gate {
    label: &'static str,
    achieved: Option<bool>,
    deadline: Option<NaiveDate>,
    overdue: bool,
    text: &'static str,
}

/// 空の材料は未達と決めず不明。日数指定の期限日は当日まで猶予がある。
/// 段階3「に入るまで」は境目当日から期限超過とする。
fn gate(
    label: &'static str,
    achieved: Option<bool>,
    deadline: Option<NaiveDate>,
    today: NaiveDate,
    before: bool,
) -> Gate {
    let overdue = achieved == Some(false)
        && deadline.is_some_and(|d| if before { today >= d } else { today > d });
    Gate {
        label,
        achieved,
        deadline,
        overdue,
        text: match achieved {
            None => "判定できない",
            Some(true) => "達成",
            Some(false) if overdue => "期限超過",
            Some(false) => "未達",
        },
    }
}

/// 材料: 初回MTG=CS_MTGのdeal_id・開催日と実施済Zoom（factual_mtg）。
/// メール由来の推定は達成に使わない。契約開始〜今日の開催記録を確認する。
/// 掲載=CS_取引.keisaisu>0、応募=同oubo>=1、面接=同mensetu>=1（履歴の持越値と混ぜない）。
/// 満足度=CS_プロパティ履歴のnps〜nps11（NPS_PROPS）の最新回答。履歴の列が揃っていて回答が無ければ未回答。
/// 数値欠損・MTG/履歴の列不足は判定不能。期限内に達成したかの履歴は揃わないため、現在の達成を表示する。
pub fn gates(d: &Deal, mtg: Option<bool>, nps: Option<bool>, today: NaiveDate) -> Vec<Gate> {
    let start = date10(&d.contract_start_date);
    let expiry = date10(&d.contract_expiration_date);
    let line = start
        .zip(expiry)
        .filter(|(s, e)| e >= s)
        .map(|(s, e)| super::phase::renewal_decision_date(s, e));
    let due = |n| start.map(|s| s + Duration::days(n));
    vec![
        gate("初回MTGの記録", mtg, due(7), today, false),
        gate(
            "求人が掲載中",
            d.keisaisu.map(|v| v > 0.0),
            due(14),
            today,
            false,
        ),
        gate(
            "応募1件以上",
            d.oubo.map(|v| v >= 1.0),
            due(30),
            today,
            false,
        ),
        gate(
            "面接1件以上",
            d.mensetu.map(|v| v >= 1.0),
            line,
            today,
            true,
        ),
        gate("満足度の回答", nps, line, today, true),
    ]
}

pub(super) struct Evidence {
    dates: HashMap<String, Vec<NaiveDate>>,
    mtg_known: bool,
    nps: HashMap<String, (String, f64)>,
    nps_known: bool,
}
impl Evidence {
    pub fn new(sheets: &Sheets) -> Self {
        let mtg = sheets.factual_mtg();
        Self {
            dates: super::contacts_by_deal(&super::empty_history_sheet(), &mtg).0,
            mtg_known: mtg.col("deal_id").is_some() && mtg.col("開催日").is_some(),
            nps: super::latest_nps(&sheets.history),
            nps_known: ["deal_id", "prop", "month", "v"]
                .iter()
                .all(|c| sheets.history.col(c).is_some()),
        }
    }
    pub fn of(&self, d: &Deal, today: NaiveDate) -> Value {
        if d.renewal_no != Some(0)
            || !d.is_active
            || date10(&d.contract_expiration_date).is_some_and(|e| e < today)
        {
            return json!({"gates": [], "overdue_count": 0});
        }
        let start = date10(&d.contract_start_date);
        let mtg = start.and_then(|s| {
            self.mtg_known.then(|| {
                self.dates
                    .get(&d.id)
                    .is_some_and(|ds| ds.iter().any(|dt| *dt >= s && *dt <= today))
            })
        });
        let nps = self.nps_known.then(|| {
            self.nps.get(&d.id).is_some_and(|(month, _)| {
                start.is_some_and(|s| {
                    month.as_str() >= s.format("%Y-%m").to_string().as_str()
                        && month.as_str() <= today.format("%Y-%m").to_string().as_str()
                })
            })
        });
        let gs = gates(d, mtg, nps, today);
        let count = if start.is_some_and(|s| s <= today) {
            gs.iter().filter(|g| g.overdue).count()
        } else {
            0
        };
        json!({"gates": gs, "overdue_count": count})
    }
}

pub fn build_first_contract(sheets: &Sheets, today: NaiveDate) -> Value {
    let (rows, meta) = super::routes::deal_rows(sheets, today);
    let mut rows: Vec<Value> = rows
        .into_iter()
        .filter(|r| r["renewal_no"] == 0 && r["past_expiry"] != true)
        .collect();
    rows.sort_by(|a, b| {
        b["first_contract"]["overdue_count"]
            .as_u64()
            .cmp(&a["first_contract"]["overdue_count"].as_u64())
            .then_with(|| {
                a["start"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("9999")
                    .cmp(
                        b["start"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or("9999"),
                    )
            })
            .then_with(|| a["deal_id"].as_str().cmp(&b["deal_id"].as_str()))
    });
    let consultants: BTreeSet<_> = rows
        .iter()
        .filter_map(|r| r["consultant"].as_str())
        .filter(|s| !s.is_empty())
        .collect();
    json!({"meta": meta, "consultants": consultants, "not_started": rows.iter().filter(|r| r["not_started"] == true).collect::<Vec<_>>(),
        "rows": rows.iter().filter(|r| r["not_started"] != true).collect::<Vec<_>>()})
}

const PROPS: [&str; 3] = [
    "shokai_mtg_mokuhyou_risou",
    "shokai_mtg_mokuhyou_zenshin",
    "shokai_mtg_mokuhyou_data",
];
fn summarize(props: &std::collections::BTreeMap<String, Option<String>>) -> Value {
    let value = |key: &str| props.get(key).and_then(|s| s.as_deref()).unwrap_or("");
    let short = |key: &str| {
        value(key)
            .lines()
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(|s| {
                let mut t: String = s.chars().take(60).collect();
                if s.chars().count() > 60 {
                    t.push('…');
                }
                t
            })
            .unwrap_or_else(|| "未入力".into())
    };
    let draft = serde_json::from_str::<Value>(value(PROPS[2]))
        .ok()
        .is_some_and(|v| v["status"] == "draft");
    json!({"ideal": short(PROPS[0]), "minimum": short(PROPS[1]), "draft": draft, "failed": false})
}

/// 目標だけ5分のプロセス内キャッシュ（最大2000件）。取得失敗は保存しない。
/// 同時要求はロック内で再確認し、関所の背景優先度で100件ずつ読む。全体の待ちは15秒まで。
/// 欠落ID・部分エラー・未設定も目標欄だけ失敗にし、一覧の判定はそのまま返す。
pub async fn enrich_goals(body: &mut Value, client: Option<&crate::hubspot::HubSpotClient>) {
    static CACHE: OnceLock<crate::db::cache::AppCache> = OnceLock::new();
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| crate::db::cache::AppCache::new(300, 2000));
    let ids: Vec<String> = ["rows", "not_started"]
        .iter()
        .flat_map(|key| body[*key].as_array().into_iter().flatten())
        .filter_map(|r| r["deal_id"].as_str().map(str::to_owned))
        .collect();
    let fetch = async {
        let _lock = LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let missing: Vec<String> = ids
            .iter()
            .filter(|id| cache.get(id).is_none())
            .cloned()
            .collect();
        if missing.is_empty() {
            crate::hubspot::gateway::cache_hit("consulting_first_goals");
            return;
        }
        crate::hubspot::gateway::cache_miss("consulting_first_goals");
        let Some(client) = client else {
            return;
        };
        let client = client.with_priority(crate::hubspot::gateway::Priority::Background);
        for chunk in missing.chunks(100) {
            match client.batch_read("deals", chunk, &PROPS).await {
                Ok(records) => {
                    for record in records {
                        // AppCacheの通常上限に加え、期限内の項目だけで上限に達した場合も抑える。
                        if cache.len() >= 2000 {
                            cache.clear();
                        }
                        cache.set(record.id, summarize(&record.properties));
                    }
                }
                Err(_) => {
                    tracing::warn!("初回契約の目標をまとめて読めませんでした");
                    break;
                }
            }
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(15), fetch).await;
    for key in ["rows", "not_started"] {
        if let Some(rows) = body[key].as_array_mut() {
            for row in rows {
                row["goals"] = row["deal_id"]
                    .as_str()
                    .and_then(|id| cache.get(id))
                    .unwrap_or_else(|| json!({"failed": true}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::call_quality::sheets::SheetData;
    use std::sync::Arc;
    fn day(s: &str) -> NaiveDate {
        date10(s).unwrap()
    }
    fn deal() -> Deal {
        Deal {
            id: "80000000001".into(),
            name: "架空の初回契約".into(),
            is_active: true,
            renewal_no: Some(0),
            contract_start_date: "2026-01-01".into(),
            contract_expiration_date: "2026-07-01".into(),
            keisaisu: Some(0.0),
            oubo: Some(0.0),
            mensetu: Some(0.0),
            ..Deal::default()
        }
    }
    fn sheet(cols: &[&str], rows: &[Vec<&str>]) -> Arc<SheetData> {
        Arc::new(SheetData {
            header: cols.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| Arc::from(*s)).collect())
                .collect(),
            fetched_at: std::time::Instant::now(),
        })
    }
    fn sheets() -> Sheets {
        let empty = super::super::empty_history_sheet();
        Sheets {
            pending_recordings: None,
            deal: empty.clone(),
            call: empty.clone(),
            mtg: sheet(&["deal_id", "開催日"], &[]),
            history: sheet(&["deal_id", "prop", "month", "v"], &[]),
            customer: empty.clone(),
            mail_mtg: empty.clone(),
            mail_history: empty.clone(),
            zoom_mtg: empty.clone(),
            handover: empty.clone(),
            owner_hist: empty.clone(),
            meta: empty,
            all_cached: true,
        }
    }
    #[test]
    fn 各関門の達成と未達と期限前と不明() {
        let mut d = deal();
        let gs = gates(&d, Some(false), Some(false), day("2026-01-05"));
        assert_eq!(
            gs.iter()
                .map(|g| g.deadline.unwrap().to_string())
                .collect::<Vec<_>>(),
            [
                "2026-01-08",
                "2026-01-15",
                "2026-01-31",
                "2026-05-02",
                "2026-05-02"
            ]
        );
        assert!(gs.iter().all(|g| g.achieved == Some(false) && !g.overdue));
        let gs = gates(&d, Some(false), Some(false), day("2026-05-02"));
        assert!(gs.iter().all(|g| g.overdue && g.text == "期限超過"));
        d.keisaisu = Some(1.0);
        d.oubo = Some(1.0);
        d.mensetu = Some(1.0);
        assert!(gates(&d, Some(true), Some(true), day("2026-06-01"))
            .iter()
            .all(|g| g.achieved == Some(true) && !g.overdue));
        d.keisaisu = None;
        d.oubo = None;
        d.mensetu = None;
        assert!(gates(&d, None, None, day("2026-06-01"))
            .iter()
            .all(|g| g.achieved.is_none() && !g.overdue && g.text == "判定できない"));
    }
    #[test]
    fn 日数の期限当日と翌日および段階3の境目() {
        let d = deal();
        for (i, on, after) in [
            (0, "2026-01-08", "2026-01-09"),
            (1, "2026-01-15", "2026-01-16"),
            (2, "2026-01-31", "2026-02-01"),
        ] {
            assert!(!gates(&d, Some(false), Some(false), day(on))[i].overdue);
            assert!(gates(&d, Some(false), Some(false), day(after))[i].overdue);
        }
        for i in [3, 4] {
            assert!(!gates(&d, Some(false), Some(false), day("2026-05-01"))[i].overdue);
            assert!(gates(&d, Some(false), Some(false), day("2026-05-02"))[i].overdue);
        }
        let mut d = d;
        d.contract_expiration_date = "2027-01-01".into();
        assert_eq!(
            gates(&d, None, None, day("2026-01-01"))[3].deadline,
            Some(day("2026-09-14"))
        );
        d.contract_start_date.clear();
        assert!(gates(&d, Some(false), Some(false), day("2026-06-01"))
            .iter()
            .all(|g| g.deadline.is_none() && !g.overdue));
    }
    #[test]
    fn 開催記録は契約前と未来を除き材料が無いときは不明() {
        let mut sh = sheets();
        let d = deal();
        sh.mtg = sheet(
            &["deal_id", "開催日"],
            &[vec![&d.id, "2025-12-31"], vec![&d.id, "2026-02-01"]],
        );
        assert_eq!(
            Evidence::new(&sh).of(&d, day("2026-01-10"))["gates"][0]["achieved"],
            false
        );
        sh.mtg = sheet(&["deal_id", "開催日"], &[vec![&d.id, "2026-01-03"]]);
        assert_eq!(
            Evidence::new(&sh).of(&d, day("2026-01-10"))["gates"][0]["achieved"],
            true
        );
        sh.mtg = super::super::empty_history_sheet();
        sh.history = sh.mtg.clone();
        let v = Evidence::new(&sh).of(&d, day("2026-06-01"));
        assert!(v["gates"][0]["achieved"].is_null());
        assert!(v["gates"][4]["achieved"].is_null());
        assert_eq!(v["gates"][0]["overdue"], false);
    }
    #[test]
    fn 初回だけを整列し期限超過は今日の候補と表示にも入る() {
        let mut sh = sheets();
        let mut rows = vec![];
        // 25件より多くして「今日」の24件上限から漏れた分も確かめる。
        let ids: Vec<String> = (1..=27).map(|i| format!("8000000{i:04}")).collect();
        for id in &ids {
            rows.push(vec![
                id.as_str(),
                "架空の契約",
                "TRUE",
                "0",
                "2026-01-01",
                "2026-07-01",
                "0",
                "0",
                "0",
            ]);
        }
        rows.extend([
            vec![
                "80000000901",
                "架空の継続",
                "TRUE",
                "1",
                "2026-01-01",
                "2026-07-01",
                "0",
                "0",
                "0",
            ],
            vec![
                "80000000902",
                "架空の開始前",
                "TRUE",
                "0",
                "2026-07-01",
                "2027-01-01",
                "0",
                "0",
                "0",
            ],
            vec![
                "80000000903",
                "架空の満了後",
                "TRUE",
                "0",
                "2025-01-01",
                "2026-01-01",
                "0",
                "0",
                "0",
            ],
            vec![
                "80000000904",
                "架空の終了",
                "FALSE",
                "0",
                "2026-01-01",
                "2026-07-01",
                "0",
                "0",
                "0",
            ],
            vec![
                "80000000905",
                "架空の遅い開始",
                "TRUE",
                "0",
                "2026-02-01",
                "2026-08-01",
                "0",
                "0",
                "0",
            ],
        ]);
        sh.deal = sheet(
            &[
                "deal_id",
                "dealname",
                "is_active",
                "renewal_no",
                "contract_start_date",
                "contract_expiration_date",
                "keisaisu",
                "oubo",
                "mensetu",
            ],
            &rows,
        );
        let today = day("2026-06-01");
        let first = build_first_contract(&sh, today);
        assert_eq!(first["rows"].as_array().unwrap().len(), 28);
        assert_eq!(first["not_started"].as_array().unwrap().len(), 1);
        assert_eq!(first["rows"][0]["deal_id"], ids[0]);
        assert_eq!(first["rows"][27]["deal_id"], "80000000905");
        for r in first["rows"].as_array().unwrap().iter().take(27) {
            assert_eq!(r["first_contract"]["overdue_count"], 5);
        }
        assert_eq!(first["rows"][27]["first_contract"]["overdue_count"], 3);
        let board = super::super::routes::build_today_board(&sh, today);
        for id in ids {
            assert!(board["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["deal_id"] == id));
            assert!(board["rows"]
                .as_array()
                .unwrap()
                .iter()
                .chain(board["extra_low_satisfaction"].as_array().unwrap())
                .any(|r| r["deal_id"] == id));
        }
        let only = json!({"n_flags":0,"flags":[],"first_contract":{"overdue_count":1}});
        assert!(super::super::routes::is_today_candidate(&only));
        assert!(!super::super::routes::is_today_candidate(
            &json!({"n_flags":0,"first_contract":{"overdue_count":0}})
        ));
    }
    #[test]
    fn 目標は先頭行を短くし案と空欄を返す() {
        let p = std::collections::BTreeMap::from([
            (PROPS[0].into(), Some("\n 採用を2名増やす\n次の目標".into())),
            (PROPS[2].into(), Some(r#"{"status":"draft"}"#.into())),
        ]);
        assert_eq!(
            summarize(&p),
            json!({"ideal":"採用を2名増やす","minimum":"未入力","draft":true,"failed":false})
        );
        let p = std::collections::BTreeMap::from([(PROPS[0].into(), Some("あ".repeat(80)))]);
        assert_eq!(summarize(&p)["ideal"].as_str().unwrap().chars().count(), 61);
    }
    #[tokio::test]
    async fn 目標を読めなくても案件は残る() {
        let mut v =
            json!({"rows":[{"deal_id":"89999999999","name":"架空の契約"}],"not_started":[]});
        enrich_goals(&mut v, None).await;
        assert_eq!(v["rows"][0]["name"], "架空の契約");
        assert_eq!(v["rows"][0]["goals"]["failed"], true);
    }
}
