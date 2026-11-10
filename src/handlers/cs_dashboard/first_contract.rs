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
    /// 間に合う最後の日。日数の関門は期限日の当日、段階3の関門は境目の前日。
    /// 画面の「期限前（M/Dまで）」「期限超過（M/D）」はこの日を使う。
    last_day: Option<NaiveDate>,
    overdue: bool,
    /// 契約がまだ始まっていない（期限超過にしない）。
    not_started: bool,
    /// 開始前の MTG だけがある（商談の可能性が高いので達成にせず、確かめてもらう）。
    check: bool,
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
        last_day: deadline.map(|d| if before { d - Duration::days(1) } else { d }),
        overdue,
        not_started: false,
        check: false,
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
/// 満足度=CS_プロパティ履歴のnps〜nps11（NPS_PROPS）の契約開始〜今日の回答。
/// 数値欠損・MTG/履歴の列不足は判定不能。期限内に達成したかの履歴は揃わないため、現在の達成を表示する。
pub fn gates(d: &Deal, mtg: Option<bool>, nps: Option<bool>, today: NaiveDate) -> Vec<Gate> {
    gates_with(d, mtg, false, nps, today)
}

/// `mtg_before_start`: 契約開始の30日前〜前日に MTG の記録がある（2026-11-10 ユーザー決定:
/// 対象にするが、契約前の商談も同じ取引に付くので多くは誤検知。達成にせず「確かめてください」）。
pub fn gates_with(
    d: &Deal,
    mtg: Option<bool>,
    mtg_before_start: bool,
    nps: Option<bool>,
    today: NaiveDate,
) -> Vec<Gate> {
    let start = date10(&d.contract_start_date);
    let expiry = date10(&d.contract_expiration_date);
    let line = start
        .zip(expiry)
        .filter(|(s, e)| e >= s)
        .map(|(s, e)| super::phase::renewal_decision_date(s, e));
    let due = |n| start.map(|s| s + Duration::days(n));
    let mut gates = vec![
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
    ];
    if mtg == Some(false) && mtg_before_start {
        let g = &mut gates[0];
        g.overdue = false;
        g.check = true;
        g.text = "確かめてください（開始前のMTGあり）";
    }
    // 短期契約では段階3の境目が開始前に来る。契約が始まるまでは期限超過にしない。
    if start.is_some_and(|s| today < s) {
        for g in &mut gates {
            g.overdue = false;
            g.not_started = true;
            if g.achieved == Some(false) {
                g.text = "開始前";
            }
        }
    }
    gates
}

pub(super) struct Evidence {
    dates: HashMap<String, Vec<NaiveDate>>,
    mtg_known: bool,
    nps: HashMap<String, Vec<NaiveDate>>,
    nps_undated: BTreeSet<String>,
    nps_known: bool,
}
impl Evidence {
    pub fn new(sheets: &Sheets) -> Self {
        let mtg = sheets.factual_mtg();
        let mut nps: HashMap<String, Vec<NaiveDate>> = HashMap::new();
        let mut nps_undated = BTreeSet::new();
        for row in &sheets.history.rows {
            let id = sheets.history.get(row, "deal_id");
            if id.is_empty()
                || !super::NPS_PROPS.contains(&sheets.history.get(row, "prop"))
                || super::opt_num(sheets.history.get(row, "v")).is_none()
            {
                continue;
            }
            // ts を JST 日付に変換する（UTC の履歴もある）。month は日を失うため代用しない。
            // 日付欠損・不正の回答は期間内と証明できないので達成にせず、他に
            // 期間内の回答がなければ不明とする（未回答として赤くするのも避ける）。
            if let Some(date) = super::call_date_jst(sheets.history.get(row, "ts")) {
                nps.entry(id.to_string()).or_default().push(date);
            } else {
                nps_undated.insert(id.to_string());
            }
        }
        Self {
            dates: super::contacts_by_deal(&super::empty_history_sheet(), &mtg).0,
            mtg_known: mtg.col("deal_id").is_some() && mtg.col("開催日").is_some(),
            nps,
            nps_undated,
            nps_known: ["deal_id", "prop", "ts", "v"]
                .iter()
                .all(|c| sheets.history.col(c).is_some()),
        }
    }
    pub fn of(&self, d: &Deal, today: NaiveDate) -> Value {
        if !d.is_active || date10(&d.contract_expiration_date).is_some_and(|e| e < today) {
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
        let nps = start.and_then(|s| {
            if !self.nps_known {
                return None;
            }
            if self
                .nps
                .get(&d.id)
                .is_some_and(|dates| dates.iter().any(|date| *date >= s && *date <= today))
            {
                Some(true)
            } else if self.nps_undated.contains(&d.id) {
                None
            } else {
                Some(false)
            }
        });
        let mtg_before_start = mtg == Some(false)
            && start.is_some_and(|s| {
                self.dates
                    .get(&d.id)
                    .is_some_and(|ds| ds.iter().any(|dt| *dt >= s - Duration::days(30) && *dt < s))
            });
        let gs = gates_with(d, mtg, mtg_before_start, nps, today);
        let count = if start.is_some_and(|s| s <= today) {
            gs.iter().filter(|g| g.overdue).count()
        } else {
            0
        };
        // 「今日」に入った理由（2026-11-10 レビュー: 名札が無く、なぜ出ているか分からなかった）。
        let late: Vec<&str> = if count > 0 {
            gs.iter()
                .filter(|g| g.overdue)
                .map(|g| short_label(g.label))
                .collect()
        } else {
            Vec::new()
        };
        let reason = if late.is_empty() {
            String::new()
        } else {
            format!("初回契約: {}が期限超過", late.join("・"))
        };
        json!({"gates": gs, "overdue_count": count, "reason": reason})
    }
}

/// 「今日」の理由に使う短い呼び名。
fn short_label(label: &str) -> &'static str {
    match label {
        "初回MTGの記録" => "初回MTG",
        "求人が掲載中" => "求人掲載",
        "応募1件以上" => "応募",
        "面接1件以上" => "面接",
        "満足度の回答" => "満足度",
        _ => "関門",
    }
}

pub fn build_first_contract(sheets: &Sheets, today: NaiveDate, sales: &SalesSnapshot) -> Value {
    let (mut rows, meta) = super::routes::deal_rows(sheets, today);
    attach_gates(&mut rows, sheets, today, sales);
    let eligible = rows.iter().filter(|r| r["past_expiry"] != true);
    let mut missing = 0;
    let mut failed = 0;
    let mut population_size = 0;
    for row in eligible {
        population_size += 1;
        match sales.values.get(row["deal_id"].as_str().unwrap_or("")) {
            Some(None) => missing += 1,
            None => failed += 1,
            _ => {}
        }
    }
    let mut rows: Vec<Value> = rows
        .into_iter()
        .filter(|r| sales.is_first(r["deal_id"].as_str().unwrap_or("")) && r["past_expiry"] != true)
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
    json!({"meta": meta, "sales_unavailable": failed > 0 && failed == population_size,
        "sales_failed_count": failed, "sales_empty_count": missing, "consultants": consultants, "not_started": rows.iter().filter(|r| r["not_started"] == true).collect::<Vec<_>>(),
        "rows": rows.iter().filter(|r| r["not_started"] != true).collect::<Vec<_>>()})
}

/// None は取得済みの空欄。キーなしは未取得・部分失敗。
#[derive(Default, Clone)]
pub struct SalesSnapshot {
    values: HashMap<String, Option<f64>>,
}
impl SalesSnapshot {
    /// 初回契約 = HubSpot の取引の「売上回数」（number_of_sales）が 1。コンサルKPIの「初回」はすべてこれで決める。
    pub(super) fn is_first(&self, id: &str) -> bool {
        self.values.get(id) == Some(&Some(1.0))
    }
    /// 売上回数を読めなかった（未取得・部分失敗）。空欄は読めた扱い（初回ではない）。
    pub(super) fn is_unread(&self, id: &str) -> bool {
        !self.values.contains_key(id)
    }
    #[cfg(test)]
    pub(super) fn from_values(values: HashMap<String, Option<f64>>) -> Self {
        Self { values }
    }
}

/// 関門をシート由来の行キャッシュの外で適用する。売上回数の更新を即座に反映する。
pub(super) fn attach_gates(
    rows: &mut [Value],
    sheets: &Sheets,
    today: NaiveDate,
    sales: &SalesSnapshot,
) {
    let evidence = Evidence::new(sheets);
    let deals: HashMap<_, _> = super::deals_of(&sheets.deal)
        .into_iter()
        .map(|d| (d.id.clone(), d))
        .collect();
    for row in rows {
        let id = row["deal_id"].as_str().unwrap_or("");
        row["first_contract"] = if sales.is_first(id) {
            deals
                .get(id)
                .map(|d| evidence.of(d, today))
                .unwrap_or_else(|| json!({"gates":[],"overdue_count":0}))
        } else {
            json!({"gates":[],"overdue_count":0})
        };
    }
}

#[derive(Default)]
struct SalesCache {
    // TTL は再取得の期限。失敗時用の直近成功値は期限後も残す。
    values: std::sync::Mutex<HashMap<String, (Option<f64>, std::time::Instant)>>,
    // 読めなかった・返ってこなかった ID。60 秒は読み直さない（混雑中に毎回待たせない）。
    misses: std::sync::Mutex<HashMap<String, std::time::Instant>>,
    refresh: tokio::sync::Mutex<()>,
}

const SALES_TTL: std::time::Duration = std::time::Duration::from_secs(300);
const SALES_MISS_TTL: std::time::Duration = std::time::Duration::from_secs(60);

impl SalesCache {
    /// 読み直しが要る ID。数字でない ID は HubSpot に送ると束ごと失敗するので外す。
    fn due(&self, ids: &[String]) -> Vec<String> {
        let values = self.values.lock().unwrap();
        let misses = self.misses.lock().unwrap();
        ids.iter()
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            .filter(|id| {
                values
                    .get(*id)
                    .is_none_or(|(_, at)| at.elapsed() >= SALES_TTL)
            })
            .filter(|id| {
                misses
                    .get(*id)
                    .is_none_or(|at| at.elapsed() >= SALES_MISS_TTL)
            })
            .cloned()
            .collect()
    }

    async fn fetch(&self, due: Vec<String>, client: crate::hubspot::HubSpotClient) {
        let _guard = self.refresh.lock().await;
        // 待っている間に別の要求が読み終えていれば、その分は読まない。
        let due = self.due(&due);
        if due.is_empty() {
            return;
        }
        let client = client.with_priority(crate::hubspot::gateway::Priority::Background);
        for chunk in due.chunks(100) {
            let mut got = std::collections::HashSet::new();
            match client
                .batch_read("deals", chunk, &["number_of_sales"])
                .await
            {
                Ok(records) => {
                    let mut values = self.values.lock().unwrap();
                    for record in records {
                        if !chunk.contains(&record.id) {
                            continue;
                        }
                        // プロパティ自体の欠落・不正値は空欄と混同せず、直近成功値を残す。
                        let Some(raw) = record.properties.get("number_of_sales") else {
                            continue;
                        };
                        let value = match raw.as_deref().map(str::trim) {
                            None | Some("") => None,
                            Some(raw) => match raw.parse::<f64>() {
                                Ok(n) if n.is_finite() => Some(n),
                                _ => continue,
                            },
                        };
                        got.insert(record.id.clone());
                        values.insert(record.id, (value, std::time::Instant::now()));
                    }
                }
                Err(_) => {
                    tracing::warn!("初回契約の売上回数をまとめて読めませんでした");
                }
            }
            let now = std::time::Instant::now();
            let mut misses = self.misses.lock().unwrap();
            for id in chunk {
                if got.contains(id) {
                    misses.remove(id);
                } else {
                    misses.insert(id.clone(), now);
                }
            }
        }
    }

    /// 前の値があれば待たずに返し、裏で読み直す（同時に 1 本）。一度も読めていないときだけ最大 15 秒待つ。
    async fn read(
        &'static self,
        ids: &[String],
        client: Option<&crate::hubspot::HubSpotClient>,
    ) -> SalesSnapshot {
        let due = self.due(ids);
        if due.is_empty() {
            crate::hubspot::gateway::cache_hit("consulting_first_sales");
        } else if let Some(client) = client.cloned() {
            crate::hubspot::gateway::cache_miss("consulting_first_sales");
            let warm = {
                let values = self.values.lock().unwrap();
                ids.iter().any(|id| values.contains_key(id))
            };
            if warm {
                if self.refresh.try_lock().is_ok() {
                    tokio::spawn(async move { self.fetch(due, client).await });
                }
            } else {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    self.fetch(due, client),
                )
                .await;
            }
        }
        let mut values = self.values.lock().unwrap();
        // 現在の稼働母集団だけ保持し、終了した取引による無制限の増加を防ぐ。
        let active: std::collections::HashSet<_> = ids.iter().collect();
        values.retain(|id, _| active.contains(id));
        self.misses
            .lock()
            .unwrap()
            .retain(|id, _| active.contains(id));
        SalesSnapshot {
            values: values.iter().map(|(id, (n, _))| (id.clone(), *n)).collect(),
        }
    }
}

/// CRM の目標認可とは独立して、既存母集団の稼働中全件を読む（満了超過も取得対象）。
/// 立ち上がりの図（開始月ごとの応募の推移）の対象月に始まった取引も、終了済みを含めて読む
/// （稼働中だけにすると、解約した取引が図から抜けて残った取引だけの推移になるため）。
pub(super) async fn read_sales(
    sheets: &Sheets,
    client: Option<&crate::hubspot::HubSpotClient>,
) -> SalesSnapshot {
    static CACHE: OnceLock<SalesCache> = OnceLock::new();
    let starts = super::routes::launch_starts(super::routes::today_jst());
    let ids: Vec<_> = super::deals_of(&sheets.deal)
        .into_iter()
        .filter(|d| {
            d.is_active
                || d.contract_start_date
                    .get(..7)
                    .is_some_and(|m| starts.iter().any(|s| s == m))
        })
        .map(|d| d.id)
        .collect();
    CACHE
        .get_or_init(SalesCache::default)
        .read(&ids, client)
        .await
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

fn goals_cache() -> &'static crate::db::cache::AppCache {
    static CACHE: OnceLock<crate::db::cache::AppCache> = OnceLock::new();
    CACHE.get_or_init(|| crate::db::cache::AppCache::new(300, 2000))
}

/// 目標だけ5分のプロセス内キャッシュ（最大2000件）。取得失敗は保存しない。
/// 同時要求はロック内で再確認し、関所の背景優先度で100件ずつ読む。全体の待ちは15秒まで。
/// 欠落ID・部分エラー・未設定も目標欄だけ失敗にし、一覧の判定はそのまま返す。
pub async fn enrich_goals(
    body: &mut Value,
    authorization: Result<(), crate::crm::rbac::Denied>,
    client: Option<&crate::hubspot::HubSpotClient>,
) {
    // キャッシュの初期化・参照も HubSpot の読み取りも、CRM 認可の後だけ行う。
    body["goals_available"] = json!(authorization.is_ok());
    if authorization.is_err() {
        return;
    }
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    let cache = goals_cache();
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
            history: sheet(&["deal_id", "prop", "month", "v", "ts"], &[]),
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
    fn 売上回数だけで対象と空欄と未取得を分ける() {
        let mut sh = sheets();
        sh.deal = sheet(
            &[
                "deal_id",
                "is_active",
                "renewal_no",
                "contract_start_date",
                "contract_expiration_date",
            ],
            &[
                vec!["1", "TRUE", "5", "2026-01-01", "2027-01-01"],
                vec!["2", "TRUE", "0", "2026-01-01", "2027-01-01"],
                vec!["3", "TRUE", "0", "2026-01-01", "2027-01-01"],
                vec!["4", "TRUE", "0", "2026-01-01", "2027-01-01"],
                vec!["5", "TRUE", "0", "2026-08-01", "2027-01-01"],
                vec!["6", "TRUE", "0", "2025-01-01", "2026-01-01"],
                vec!["7", "FALSE", "0", "2026-01-01", "2027-01-01"],
            ],
        );
        let sales = SalesSnapshot {
            values: HashMap::from([
                ("1".into(), Some(1.0)),
                ("2".into(), Some(2.0)),
                ("3".into(), None),
                ("5".into(), Some(1.0)),
                ("6".into(), Some(1.0)),
                ("7".into(), Some(1.0)),
            ]),
        };
        let today = day("2026-06-01");
        let body = build_first_contract(&sh, today, &sales);
        assert_eq!(
            body["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["deal_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["1"]
        );
        assert_eq!(body["not_started"][0]["deal_id"], "5");
        assert_eq!(body["not_started"].as_array().unwrap().len(), 1);
        assert_eq!(body["sales_empty_count"], 1);
        assert_eq!(body["sales_failed_count"], 1);
        assert_eq!(body["sales_unavailable"], false);
        let (mut rows, _) = super::super::routes::deal_rows(&sh, today);
        attach_gates(&mut rows, &sh, today, &sales);
        assert!(
            rows.iter().find(|r| r["deal_id"] == "1").unwrap()["first_contract"]["overdue_count"]
                .as_u64()
                .unwrap()
                > 0
        );
        for id in ["2", "3", "4", "5", "6"] {
            assert_eq!(
                rows.iter().find(|r| r["deal_id"] == id).unwrap()["first_contract"]
                    ["overdue_count"],
                0
            );
        }
        let empty = build_first_contract(&sh, today, &SalesSnapshot::default());
        assert_eq!(empty["rows"], json!([]));
        assert_eq!(empty["not_started"], json!([]));
        assert_eq!(empty["sales_failed_count"], 5);
        assert_eq!(empty["sales_unavailable"], true);
    }

    #[tokio::test]
    async fn クライアント未接続でも期限切れの前回値を使い未取得だけを外す() {
        let cache: &'static SalesCache = Box::leak(Box::default());
        let old = std::time::Instant::now() - std::time::Duration::from_secs(301);
        cache.values.lock().unwrap().extend([
            ("1".into(), (Some(1.0), old)),
            ("2".into(), (Some(2.0), old)),
            ("3".into(), (None, old)),
        ]);
        let ids = ["1", "2", "3", "4"].map(str::to_string);
        let sales = cache.read(&ids, None).await;
        assert!(sales.is_first("1"));
        assert!(!sales.is_first("2"));
        assert_eq!(sales.values.get("3"), Some(&None));
        assert!(!sales.values.contains_key("4"));
        let cold = Box::leak(Box::<SalesCache>::default())
            .read(&ids, None)
            .await;
        assert!(cold.values.is_empty());
    }

    #[tokio::test]
    async fn 偽httpで百件分割と五分キャッシュと部分欠落と失敗時の前回値を確認() {
        use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        };
        #[derive(Default)]
        struct Fake {
            fail: AtomicBool,
            calls: Mutex<Vec<Value>>,
        }
        async fn respond(
            State(fake): State<Arc<Fake>>,
            Json(body): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            fake.calls.lock().unwrap().push(body.clone());
            if fake.fail.load(Ordering::SeqCst) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"message":"test failure"})),
                );
            }
            let results: Vec<_> = body["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["id"] != "102")
                .map(|r| {
                    let n = match r["id"].as_str().unwrap() {
                        "2" => json!("2"),
                        "3" => Value::Null,
                        _ => json!("1.0"),
                    };
                    json!({"id":r["id"], "properties":{"number_of_sales": n}})
                })
                .collect();
            (StatusCode::OK, Json(json!({"results":results})))
        }
        let fake = Arc::new(Fake::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/crm/v3/objects/deals/batch/read", post(respond))
            .with_state(fake.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client =
            crate::hubspot::HubSpotClient::new("test-token".into(), &base, Default::default())
                .unwrap();
        let cache: &'static SalesCache = Box::leak(Box::default());
        let ids: Vec<_> = (1..=102).map(|i| i.to_string()).collect();
        let sales = cache.read(&ids, Some(&client)).await;
        assert!(sales.is_first("1"));
        assert!(!sales.is_first("2"));
        assert_eq!(sales.values.get("3"), Some(&None));
        assert!(!sales.values.contains_key("102"));
        {
            let calls = fake.calls.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0]["inputs"].as_array().unwrap().len(), 100);
            assert_eq!(calls[1]["inputs"].as_array().unwrap().len(), 2);
            assert_eq!(calls[0]["properties"], json!(["number_of_sales"]));
        }
        // 取得済み101件は5分以内ならHTTPを再送しない。
        cache.read(&ids[..101], Some(&client)).await;
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
        for (_, at) in cache.values.lock().unwrap().values_mut() {
            *at = std::time::Instant::now() - std::time::Duration::from_secs(301);
        }
        fake.fail.store(true, Ordering::SeqCst);
        let stale = cache.read(&ids, Some(&client)).await;
        assert!(stale.is_first("1"));
        assert_eq!(stale.values.get("2"), Some(&Some(2.0)));
        assert_eq!(stale.values.get("3"), Some(&None));
        assert!(!stale.values.contains_key("102"));
        let cold = Box::leak(Box::<SalesCache>::default())
            .read(&ids, Some(&client))
            .await;
        assert!(cold.values.is_empty());
        assert_eq!(
            client
                .gateway()
                .snapshot()
                .granted_by_priority
                .get("background"),
            Some(&(fake.calls.lock().unwrap().len() as u64))
        );
        assert_eq!(
            client.gateway().snapshot().total_calls,
            fake.calls.lock().unwrap().len() as u64
        );
        server.abort();
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
    fn 満足度は開始日当日から今日までの日付だけを数える() {
        let mut sh = sheets();
        let mut d = deal();
        d.contract_start_date = "2026-01-15".into();
        for (ts, achieved) in [
            ("2026-01-14", json!(false)),
            ("2026-01-15", json!(true)),
            ("2026-01-14T14:59:59Z", json!(false)),
            ("2026-01-14T15:00:00Z", json!(true)),
            ("2026-01-20", json!(true)),
            ("2026-01-21", json!(false)),
            ("2026-01-20T14:59:59Z", json!(true)),
            ("2026-01-20T15:00:00Z", json!(false)),
            ("", Value::Null),
            ("不正", Value::Null),
        ] {
            sh.history = sheet(
                &["deal_id", "prop", "month", "v", "ts"],
                &[vec![&d.id, "nps", "2026-01", "8", ts]],
            );
            assert_eq!(
                Evidence::new(&sh).of(&d, day("2026-01-20"))["gates"][4]["achieved"],
                achieved,
                "ts={ts}"
            );
        }
        // 最新行が未来でも、期間内の別の回答を取り落とさない。0点も有効な回答。
        sh.history = sheet(
            &["deal_id", "prop", "v", "ts"],
            &[
                vec![&d.id, "nps", "0", "2026-01-15"],
                vec![&d.id, "nps2", "9", "2026-02-01"],
                vec![&d.id, "nps3", "7", ""],
            ],
        );
        assert_eq!(
            Evidence::new(&sh).of(&d, day("2026-01-20"))["gates"][4]["achieved"],
            true
        );
        sh.history = sheet(
            &["deal_id", "prop", "month", "v"],
            &[vec![&d.id, "nps", "2026-01", "8"]],
        );
        assert!(Evidence::new(&sh).of(&d, day("2026-01-20"))["gates"][4]["achieved"].is_null());
    }

    #[test]
    fn 開始前の短期契約は段階3を過ぎても赤と件数がともにゼロ() {
        let mut d = deal();
        d.contract_start_date = "2026-06-01".into();
        d.contract_expiration_date = "2026-07-01".into();
        let v = Evidence::new(&sheets()).of(&d, day("2026-05-31"));
        assert_eq!(v["gates"][3]["deadline"], "2026-05-02");
        assert_eq!(v["overdue_count"], 0);
        for g in v["gates"].as_array().unwrap() {
            assert_eq!(g["overdue"], false);
            assert_eq!(g["not_started"], true);
            assert_eq!(g["text"], "開始前");
        }
        assert_eq!(v["reason"], "");
        let v = Evidence::new(&sheets()).of(&d, day("2026-06-01"));
        assert_eq!(v["overdue_count"], 2);
        assert_eq!(v["gates"][3]["overdue"], true);
        assert_eq!(v["gates"][4]["overdue"], true);
    }

    #[tokio::test]
    async fn 認可拒否は目標キャッシュとhubspotを読まず一覧と関門を残す() {
        use crate::crm::rbac::{check_session_and_domain, CrmAccess, Principal};
        // ポートを開かずに、実クライアントの関所で HubSpot 呼び出しがゼロか検証する。
        let client = crate::hubspot::HubSpotClient::new(
            "test-token".into(),
            "http://127.0.0.1:1",
            Default::default(),
        )
        .unwrap();
        let body = json!({"rows":[{"deal_id":"89999999998","first_contract":{"gates":[{"achieved":true}]}}],"not_started":[{"deal_id":"89999999998"}]});
        let access = CrmAccess::from_list("staff@f-a-c.co.jp,external@example.com");
        let domains = vec!["f-a-c.co.jp".to_string()];
        let check = |email: Option<&str>, method: Option<&str>| {
            check_session_and_domain(
                &Principal {
                    email: email.map(str::to_owned),
                    login_method: method.map(str::to_owned),
                    role: None,
                },
                Some(crate::hubspot::RecordType::Deal),
                &access,
                &domains,
            )
        };
        let google = Some(crate::auth::LOGIN_METHOD_GOOGLE_OIDC);
        // 未認可を先に確認し、その後キャッシュが温まった状態でも確認する。
        for warm in [false, true] {
            if warm {
                goals_cache().set(
                    "89999999998".into(),
                    json!({"ideal":"社内限定目標","failed":false}),
                );
                let mut authorized = body.clone();
                enrich_goals(
                    &mut authorized,
                    check(Some("staff@f-a-c.co.jp"), google),
                    Some(&client),
                )
                .await;
                assert_eq!(authorized["rows"][0]["goals"]["ideal"], "社内限定目標");
                assert_eq!(authorized["goals_available"], true);
            }
            for (email, method) in [
                (None, None),
                (
                    Some("staff@f-a-c.co.jp"),
                    Some(crate::auth::LOGIN_METHOD_PASSWORD_INTERNAL),
                ),
                (
                    Some("staff@f-a-c.co.jp"),
                    Some(crate::auth::LOGIN_METHOD_PASSWORD_EXTERNAL),
                ),
                (Some("external@example.com"), google),
                (Some("other@f-a-c.co.jp"), google),
            ] {
                let authorization = check(email, method);
                assert!(authorization.is_err());
                let mut denied = body.clone();
                enrich_goals(&mut denied, authorization, Some(&client)).await;
                assert_eq!(denied["rows"], body["rows"]);
                assert_eq!(denied["not_started"], body["not_started"]);
                assert_eq!(denied["goals_available"], false);
                assert!(!denied.to_string().contains("社内限定目標"));
                assert_eq!(client.gateway().snapshot().total_calls, 0);
                assert_eq!(client.gateway().snapshot().granted, 0);
            }
        }
        goals_cache().remove_prefix("89999999998");
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
        let sales = SalesSnapshot {
            values: super::super::deals_of(&sh.deal)
                .iter()
                .map(|d| {
                    (
                        d.id.clone(),
                        Some(if d.id == "80000000901" { 2.0 } else { 1.0 }),
                    )
                })
                .collect(),
        };
        let first = build_first_contract(&sh, today, &sales);
        assert_eq!(first["rows"].as_array().unwrap().len(), 28);
        assert_eq!(first["not_started"].as_array().unwrap().len(), 1);
        assert_eq!(first["rows"][0]["deal_id"], ids[0]);
        assert_eq!(first["rows"][27]["deal_id"], "80000000905");
        for r in first["rows"].as_array().unwrap().iter().take(27) {
            assert_eq!(r["first_contract"]["overdue_count"], 5);
        }
        assert_eq!(first["rows"][27]["first_contract"]["overdue_count"], 3);
        let board = super::super::routes::build_today_board_with_sales(&sh, today, &sales);
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
        enrich_goals(&mut v, Ok(()), None).await;
        assert_eq!(v["rows"][0]["name"], "架空の契約");
        assert_eq!(v["rows"][0]["goals"]["failed"], true);
    }

    #[test]
    fn 間に合う最後の日は日数の関門は当日_段階3は境目の前日() {
        let mut d = deal();
        d.contract_start_date = "2026-01-01".into();
        d.contract_expiration_date = "2026-06-30".into();
        let gs = gates(&d, Some(false), Some(false), day("2026-01-05"));
        assert_eq!(gs[0].last_day, gs[0].deadline);
        assert_eq!(gs[2].last_day, gs[2].deadline);
        assert_eq!(
            gs[3].last_day.unwrap() + Duration::days(1),
            gs[3].deadline.unwrap()
        );
        // 最後の日の当日は期限前、翌日（＝境目）に期限超過
        let last = gs[3].last_day.unwrap();
        assert!(!gates(&d, Some(false), Some(false), last)[3].overdue);
        assert!(gates(&d, Some(false), Some(false), last + Duration::days(1))[3].overdue);
    }

    #[test]
    fn 開始前のmtgだけなら赤にせず確かめてもらう() {
        let mut d = deal();
        d.contract_start_date = "2026-01-01".into();
        d.contract_expiration_date = "2026-06-30".into();
        let gs = gates_with(&d, Some(false), true, Some(true), day("2026-02-01"));
        assert!(!gs[0].overdue && gs[0].check);
        assert_eq!(gs[0].text, "確かめてください（開始前のMTGあり）");
        // 開始後の MTG があれば達成のまま（開始前の記録は見ない）
        let gs = gates_with(&d, Some(true), true, Some(true), day("2026-02-01"));
        assert_eq!(gs[0].text, "達成");
        assert!(!gs[0].check);
        // 開始前の MTG も無ければ従来どおり期限超過
        let gs = gates_with(&d, Some(false), false, Some(true), day("2026-02-01"));
        assert!(gs[0].overdue && !gs[0].check);
    }

    #[test]
    fn 今日の理由は期限超過の関門の短い名前() {
        let mut d = deal();
        d.contract_start_date = "2026-01-01".into();
        d.contract_expiration_date = "2026-12-31".into();
        d.keisaisu = Some(0.0);
        d.oubo = Some(0.0);
        d.mensetu = Some(1.0);
        let v = Evidence::new(&sheets()).of(&d, day("2026-03-01"));
        let reason = v["reason"].as_str().unwrap();
        assert!(reason.starts_with("初回契約: "), "{reason}");
        assert!(reason.contains("求人掲載・応募"), "{reason}");
        assert!(reason.ends_with("が期限超過"), "{reason}");
    }

    #[test]
    fn 数字でないidと直近に読めなかったidは読み直さない() {
        let cache: &'static SalesCache = Box::leak(Box::default());
        let now = std::time::Instant::now();
        cache.misses.lock().unwrap().extend([
            ("10".to_string(), now),
            ("11".to_string(), now - std::time::Duration::from_secs(61)),
        ]);
        cache
            .values
            .lock()
            .unwrap()
            .insert("12".into(), (Some(1.0), now));
        let ids = ["abc", "", "10", "11", "12", "13"].map(str::to_string);
        // abc・空は送らない。10 は 60 秒以内に読めなかった。12 は 5 分以内に読んだ。
        assert_eq!(cache.due(&ids), vec!["11".to_string(), "13".to_string()]);
    }
}
