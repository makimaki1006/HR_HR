//! 初回の契約（継続0回）の立ち上げの関門。
//!
//! 2026-10-27 ユーザー決定。関門の中身と期限の日数は仮置きなので、変えるときは下の `GATES` の表だけを直す
//! （画面の文・件数は表から作る）。
//!
//! 対象: 継続0回（`renewal_no == 0`）で、開始日が今日以前、満了日を過ぎていない稼働中の案件。
//! 開始前の案件・2回目以降・満了後は関門を付けない。
//!
//! 状態は 4 つ。満たしていれば「済」。満たしていなくて期限の日を過ぎていなければ「未」、過ぎていれば「遅れ」。
//! 期限は含む（期限の日までは「未」、翌日から「遅れ」）。データが読めないものは「不明」で、遅れに数えない。
//!
//! 関門ごとの見方:
//! 1. 初回MTGの記録 … 案件一覧「初回契約で MTG の記録が見つからない」と同じ出どころ（Zoom 録画・開催記録・メール由来のどれか）に、
//!    その案件の MTG の記録が1件でもあれば済。日付は問わない（最終MTG日しか持っていないため、開始日以降かは見ない）
//! 2. 求人が掲載中 … 案件管理が数えた、いま掲載中の求人が1件以上
//! 3. 応募 … 契約開始日から今日までの応募が1件以上（応募日が分からない応募があるだけの案件は、0件と言えないので不明）
//! 4. 面接 … HubSpot の取引の項目 mensetu（`phase.rs` と同じ）が1以上。空は不明
//! 5. 満足度の回答 … 定期の満足度（`nps`）の値が1つでもある
use crate::handlers::job_copy_live::shared_applications::{DealApplications, Published};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};

/// 期限の決め方
#[derive(Clone, Copy)]
pub enum Deadline {
    /// 契約開始日から N 日目まで
    DaysAfterStart(i64),
    /// 更新の判断（段階3）に入る日まで（`phase::renewal_decision_date`）
    StageThree,
}

pub struct Gate {
    pub key: &'static str,
    pub label: &'static str,
    /// 点の並びに添える短い名前
    pub short: &'static str,
    pub deadline: Deadline,
    pub deadline_label: &'static str,
}

/// 関門の表（仮置き）。順番が画面の点の順番。
pub const GATES: [Gate; 5] = [
    Gate {
        key: "mtg",
        label: "初回MTGの記録がある",
        short: "MTG",
        deadline: Deadline::DaysAfterStart(7),
        deadline_label: "開始から7日",
    },
    Gate {
        key: "live",
        label: "求人が掲載中",
        short: "掲載",
        deadline: Deadline::DaysAfterStart(14),
        deadline_label: "開始から14日",
    },
    Gate {
        key: "applied",
        label: "応募が1件以上",
        short: "応募",
        deadline: Deadline::DaysAfterStart(30),
        deadline_label: "開始から30日",
    },
    Gate {
        key: "interview",
        label: "面接が1件以上",
        short: "面接",
        deadline: Deadline::StageThree,
        deadline_label: "更新の判断（段階3）に入る日",
    },
    Gate {
        key: "satisfaction",
        label: "満足度の回答がある",
        short: "満足度",
        deadline: Deadline::StageThree,
        deadline_label: "更新の判断（段階3）に入る日",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Done,
    Wait,
    Late,
    Unknown,
}

impl State {
    pub fn key(self) -> &'static str {
        match self {
            State::Done => "ok",
            State::Wait => "wait",
            State::Late => "late",
            State::Unknown => "unknown",
        }
    }
}

pub struct Inputs<'a> {
    pub start: Option<NaiveDate>,
    pub expiry: Option<NaiveDate>,
    pub renewal_no: Option<i64>,
    /// MTG の記録が1件でもあるか。MTG のデータが読めていないときは None
    pub mtg_record: Option<bool>,
    /// 案件管理の応募・掲載のデータ。その案件の分が無ければ None
    pub apps: Option<&'a DealApplications>,
    /// HubSpot の取引の項目 mensetu
    pub interviews: Option<f64>,
    /// 満足度の値があるか。満足度のデータが読めていないときは None
    pub satisfaction: Option<bool>,
    pub today: NaiveDate,
}

/// 関門の対象か（継続0回・開始済み・満了を過ぎていない）
pub fn applies(i: &Inputs) -> bool {
    i.renewal_no == Some(0)
        && i.start.is_some_and(|s| s <= i.today)
        && !i.expiry.is_some_and(|e| e < i.today)
}

fn deadline_date(g: &Gate, start: NaiveDate, expiry: Option<NaiveDate>) -> Option<NaiveDate> {
    match g.deadline {
        Deadline::DaysAfterStart(n) => Some(start + Duration::days(n)),
        Deadline::StageThree => expiry.map(|e| super::phase::renewal_decision_date(start, e)),
    }
}

/// met: Some(true)=満たしている / Some(false)=満たしていない / None=データが無く分からない
fn resolve(met: Option<bool>, deadline: Option<NaiveDate>, today: NaiveDate) -> State {
    match (met, deadline) {
        (Some(true), _) => State::Done,
        (None, _) | (Some(false), None) => State::Unknown,
        (Some(false), Some(d)) => {
            if today > d {
                State::Late
            } else {
                State::Wait
            }
        }
    }
}

/// 5つの関門の状態。対象でなければ None。
pub fn gate_states(i: &Inputs) -> Option<[State; 5]> {
    if !applies(i) {
        return None;
    }
    let start = i.start?;
    let live = i.apps.map(|a| a.live >= 1);
    let applied = i.apps.and_then(|a| {
        let n = a
            .days
            .iter()
            .filter(|d| **d >= start && **d <= i.today)
            .count();
        if n >= 1 {
            Some(true)
        } else if a.unknown_dates > 0 {
            None
        } else {
            Some(false)
        }
    });
    let met = [
        i.mtg_record,
        live,
        applied,
        i.interviews.map(|m| m >= 1.0),
        i.satisfaction,
    ];
    let mut out = [State::Unknown; 5];
    for (k, g) in GATES.iter().enumerate() {
        out[k] = resolve(met[k], deadline_date(g, start, i.expiry), i.today);
    }
    Some(out)
}

pub const RULE_NOTE: &str = "期限の翌日になっても満たしていないものが「遅れ」。データが読めないものは「不明」で、遅れには数えません。";

fn rule_text() -> String {
    let gates = GATES
        .iter()
        .enumerate()
        .map(|(k, g)| {
            format!(
                "{}{}（{}まで）",
                ["①", "②", "③", "④", "⑤"][k],
                g.label,
                g.deadline_label
            )
        })
        .collect::<Vec<_>>()
        .join("、");
    format!(
        "初回の立ち上げの関門（仮置き。関門と期限は変わることがあります）: 初回の契約（継続0回）の開始済み・満了前の案件に、{gates}の5つを置いています。{RULE_NOTE}注意の数には入れません。"
    )
}

/// 行に `onboarding` と `onboarding_late` を足し、meta に関門の定義と件数を足す。
/// 既存の名札・並び・件数には触れない。
/// 行の材料: start / expiry / renewal_no / mtg_record / interviews / nps（`deal_rows` の行）。
/// meta の `onboarding_data` は、MTG と満足度のデータを読めているか（`deal_rows` が入れる）。
pub fn annotate(rows: &mut [Value], meta: &mut Value, today: NaiveDate, facts: Option<&Published>) {
    let mtg_loaded = meta["onboarding_data"]["mtg"].as_bool().unwrap_or(true);
    let sat_loaded = meta["onboarding_data"]["satisfaction"]
        .as_bool()
        .unwrap_or(true);
    let mut late = [0usize; 5];
    let mut unknown = [0usize; 5];
    let mut done = [0usize; 5];
    let mut n_initial = 0usize;
    let mut n_late_any = 0usize;
    for r in rows.iter_mut() {
        let apps = facts.and_then(|f| r["deal_id"].as_str().and_then(|id| f.deals.get(id)));
        let inputs = Inputs {
            start: r["start"].as_str().and_then(super::date10),
            expiry: r["expiry"].as_str().and_then(super::date10),
            renewal_no: r["renewal_no"].as_i64(),
            mtg_record: if mtg_loaded {
                r["mtg_record"].as_bool()
            } else {
                None
            },
            apps,
            interviews: r["interviews"].as_f64(),
            satisfaction: if sat_loaded {
                Some(!r["nps"].is_null())
            } else {
                None
            },
            today,
        };
        let Some(states) = gate_states(&inputs) else {
            r["onboarding"] = Value::Null;
            r["onboarding_late"] = json!(false);
            continue;
        };
        n_initial += 1;
        let n_late = states.iter().filter(|s| **s == State::Late).count();
        n_late_any += usize::from(n_late > 0);
        for (k, s) in states.iter().enumerate() {
            match s {
                State::Late => late[k] += 1,
                State::Unknown => unknown[k] += 1,
                State::Done => done[k] += 1,
                State::Wait => {}
            }
        }
        r["onboarding"] = json!({
            "states": states.iter().map(|s| s.key()).collect::<Vec<_>>(),
            "late": n_late,
        });
        r["onboarding_late"] = json!(n_late > 0);
    }
    if let Some(m) = meta.as_object_mut() {
        m.insert(
            "onboarding".into(),
            json!({
                "provisional": true,
                "n_initial": n_initial,
                "n_late_any": n_late_any,
                "gates": GATES.iter().enumerate().map(|(k, g)| json!({
                    "key": g.key, "label": g.label, "short": g.short,
                    "deadline_label": g.deadline_label,
                    "done": done[k], "late": late[k], "unknown": unknown[k],
                })).collect::<Vec<_>>(),
                "filter_label": "初回の立ち上げ（遅れあり）",
                "rule": rule_text(),
                "has_facts": facts.is_some(),
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn apps(days: &[&str], unknown: usize, live: usize) -> DealApplications {
        DealApplications {
            days: days.iter().map(|s| d(s)).collect(),
            unknown_dates: unknown,
            live,
        }
    }
    /// 3か月契約（2026-01-01〜04-01、90日。段階3に入る日は 2026-01-31）で、何も満たしていない案件
    fn base<'a>(today: &str) -> Inputs<'a> {
        Inputs {
            start: Some(d("2026-01-01")),
            expiry: Some(d("2026-04-01")),
            renewal_no: Some(0),
            mtg_record: Some(false),
            apps: None,
            interviews: Some(0.0),
            satisfaction: Some(false),
            today: d(today),
        }
    }
    fn states(i: &Inputs) -> [State; 5] {
        gate_states(i).unwrap()
    }

    #[test]
    fn mtg_gate_day_7_is_wait_day_8_is_late() {
        // 開始 1/1 の7日後は 1/8（期限の日を含む）
        assert_eq!(states(&base("2026-01-08"))[0], State::Wait);
        assert_eq!(states(&base("2026-01-09"))[0], State::Late);
        let mut i = base("2026-01-20");
        i.mtg_record = Some(true);
        assert_eq!(states(&i)[0], State::Done);
        // MTG のデータが読めていなければ不明
        i.mtg_record = None;
        assert_eq!(states(&i)[0], State::Unknown);
    }

    #[test]
    fn live_gate_day_14_is_wait_day_15_is_late() {
        let a = apps(&[], 0, 0);
        let mut i = base("2026-01-15");
        i.apps = Some(&a);
        assert_eq!(states(&i)[1], State::Wait);
        i.today = d("2026-01-16");
        assert_eq!(states(&i)[1], State::Late);
        let live = apps(&[], 0, 2);
        i.apps = Some(&live);
        assert_eq!(states(&i)[1], State::Done);
        // 案件管理のデータが無ければ不明（未達にしない）
        i.apps = None;
        assert_eq!(states(&i)[1], State::Unknown);
    }

    #[test]
    fn applied_gate_day_30_is_wait_day_31_is_late() {
        let a = apps(&[], 0, 0);
        let mut i = base("2026-01-31");
        i.apps = Some(&a);
        assert_eq!(states(&i)[2], State::Wait);
        i.today = d("2026-02-01");
        assert_eq!(states(&i)[2], State::Late);
        // 契約開始より前の応募は数えない
        let before = apps(&["2025-12-31"], 0, 0);
        i.apps = Some(&before);
        assert_eq!(states(&i)[2], State::Late);
        // 開始日の応募は数える
        let on = apps(&["2026-01-01"], 0, 0);
        i.apps = Some(&on);
        assert_eq!(states(&i)[2], State::Done);
        // 応募日が分からない応募があって、数えられる応募が無いときは 0 件と言えない
        let unk = apps(&[], 1, 0);
        i.apps = Some(&unk);
        assert_eq!(states(&i)[2], State::Unknown);
        // 数えられる応募があれば、日付不明が混じっても済
        let mixed = apps(&["2026-01-10"], 1, 0);
        i.apps = Some(&mixed);
        assert_eq!(states(&i)[2], State::Done);
        // 今日より後の応募日は数えない
        let future = apps(&["2026-02-02"], 0, 0);
        i.apps = Some(&future);
        assert_eq!(states(&i)[2], State::Late);
    }

    #[test]
    fn interview_and_satisfaction_gates_use_stage_three_day() {
        // 3か月契約の段階3は 1/31（30日目）。その日までは未、翌日から遅れ
        let i = base("2026-01-31");
        assert_eq!(states(&i)[3], State::Wait);
        assert_eq!(states(&i)[4], State::Wait);
        let i = base("2026-02-01");
        assert_eq!(states(&i)[3], State::Late);
        assert_eq!(states(&i)[4], State::Late);
        let mut i = base("2026-02-10");
        i.interviews = Some(1.0);
        i.satisfaction = Some(true);
        assert_eq!(states(&i)[3], State::Done);
        assert_eq!(states(&i)[4], State::Done);
        // 面接の項目が空・満足度のデータが読めない → 不明
        i.interviews = None;
        i.satisfaction = None;
        assert_eq!(states(&i)[3], State::Unknown);
        assert_eq!(states(&i)[4], State::Unknown);
        // 12か月契約（365日）の段階3は 256 日目 = 2026-09-14
        let mut i = base("2026-09-14");
        i.expiry = Some(d("2027-01-01"));
        assert_eq!(states(&i)[3], State::Wait);
        i.today = d("2026-09-15");
        assert_eq!(states(&i)[3], State::Late);
        // 満了日が空なら段階3の日が決まらない → 満たしていなければ不明
        let mut i = base("2026-09-15");
        i.expiry = None;
        assert_eq!(states(&i)[3], State::Unknown);
    }

    #[test]
    fn only_first_started_unexpired_contracts_get_gates() {
        assert!(gate_states(&base("2026-01-10")).is_some());
        // 継続1回以上・継続回数が空
        let mut i = base("2026-01-10");
        i.renewal_no = Some(1);
        assert!(gate_states(&i).is_none());
        i.renewal_no = Some(2);
        assert!(gate_states(&i).is_none());
        i.renewal_no = None;
        assert!(gate_states(&i).is_none());
        // 開始前（開始日の前日）と、開始日ちょうど
        assert!(gate_states(&base("2025-12-31")).is_none());
        assert!(gate_states(&base("2026-01-01")).is_some());
        // 開始日が空
        let mut i = base("2026-01-10");
        i.start = None;
        assert!(gate_states(&i).is_none());
        // 満了日を過ぎている（満了日ちょうどは対象）
        assert!(gate_states(&base("2026-04-01")).is_some());
        assert!(gate_states(&base("2026-04-02")).is_none());
    }

    #[test]
    fn annotate_marks_rows_counts_gates_and_leaves_flags_alone() {
        let built: chrono::DateTime<chrono::Utc> = "2026-10-27T03:00:00Z".parse().unwrap();
        let mut deals = BTreeMap::new();
        deals.insert("1".to_string(), apps(&[], 0, 0)); // 開始 40 日後、応募も掲載も無い
        deals.insert("2".to_string(), apps(&["2026-09-25"], 0, 1)); // 満たしている
        let facts = Published {
            built_at: built,
            deals,
        };
        let row = |id: &str, start: &str, renewal: i64| {
            json!({"deal_id": id, "start": start, "expiry": "2027-01-01", "renewal_no": renewal,
                   "mtg_record": true, "interviews": 0.0, "nps": null, "flags": ["a"], "n_flags": 1})
        };
        let mut rows = vec![
            row("1", "2026-09-17", 0),
            row("2", "2026-09-17", 0),
            row("3", "2026-09-17", 1), // 継続1回 → 対象外
            row("4", "2026-09-17", 0), // 案件管理のデータ無し
            row("5", "2026-11-01", 0), // 開始前
        ];
        let mut meta = json!({"onboarding_data": {"mtg": true, "satisfaction": true}});
        annotate(&mut rows, &mut meta, d("2026-10-27"), Some(&facts));
        // 1: 開始 9/17 から 40 日。MTG 済、掲載 遅れ（14日超）、応募 遅れ（30日超）、面接・満足度 未（段階3は 256 日目）
        assert_eq!(
            rows[0]["onboarding"]["states"],
            json!(["ok", "late", "late", "wait", "wait"])
        );
        assert_eq!(rows[0]["onboarding"]["late"], 2);
        assert_eq!(rows[0]["onboarding_late"], true);
        // 2: 掲載・応募は済
        assert_eq!(
            rows[1]["onboarding"]["states"],
            json!(["ok", "ok", "ok", "wait", "wait"])
        );
        assert_eq!(rows[1]["onboarding_late"], false);
        assert_eq!(rows[2]["onboarding"], Value::Null);
        // 4: 案件管理のデータが無い → 掲載・応募は不明（遅れにしない）
        assert_eq!(
            rows[3]["onboarding"]["states"],
            json!(["ok", "unknown", "unknown", "wait", "wait"])
        );
        assert_eq!(rows[3]["onboarding_late"], false);
        assert_eq!(rows[4]["onboarding"], Value::Null);
        // 名札には触れない
        assert_eq!(rows[0]["n_flags"], 1);
        assert_eq!(rows[0]["flags"], json!(["a"]));
        let m = &meta["onboarding"];
        assert_eq!(m["n_initial"], 3);
        assert_eq!(m["n_late_any"], 1);
        assert_eq!(m["gates"][1]["late"], 1);
        assert_eq!(m["gates"][1]["unknown"], 1);
        assert_eq!(m["gates"][1]["done"], 1);
        assert_eq!(m["gates"][0]["done"], 3);
        assert_eq!(m["gates"][0]["label"], "初回MTGの記録がある");
        assert!(m["rule"].as_str().unwrap().contains("開始から30日"));
    }

    #[test]
    fn annotate_without_data_never_says_late() {
        let mut rows = vec![
            json!({"deal_id": "1", "start": "2026-01-01", "expiry": "2027-01-01",
            "renewal_no": 0, "mtg_record": null, "interviews": null, "nps": null}),
        ];
        let mut meta = json!({"onboarding_data": {"mtg": false, "satisfaction": false}});
        annotate(&mut rows, &mut meta, d("2026-10-27"), None);
        assert_eq!(
            rows[0]["onboarding"]["states"],
            json!(["unknown", "unknown", "unknown", "unknown", "unknown"])
        );
        assert_eq!(rows[0]["onboarding_late"], false);
        assert_eq!(meta["onboarding"]["n_late_any"], 0);
        assert_eq!(meta["onboarding"]["has_facts"], false);
    }
}
