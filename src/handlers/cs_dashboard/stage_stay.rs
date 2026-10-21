//! 同じステージに長く止まっている案件と、満了後も稼働中の案件。
//!
//! 直すのは HubSpot 側のステージ。この画面は見せるだけで、HubSpot には書かない。
//!
//! 定義（2026-10-20 ユーザー決定）
//! - 滞留 = 同じ稼働中のステージに `STAY_DAYS`（45）日を超えている（45 日ちょうどは含めない）。
//! - 入った日 = `CS_プロパティ履歴` の `prop=dealstage` で、いまのステージに変わった最後の時刻（日本時間の日付）。
//!   同じ値が続く行は変化ではないので、いまの値が続いている先頭の行を採る。
//! - エスカレーション = 満了日を過ぎたのに、まだ稼働中のどれかのステージにいる案件。
//!   滞留日数が 45 日未満でも、例外ステージにいても出す。
//! - 継続済・解約済・充足などの決着したステージ（`is_active` が偽）は出さない。
//! - 紹介料の取引・オプション契約は `deals_of` で外れている。
//! - 履歴にステージの変更が無い案件は入った日が分からない。黙って落とさず件数を返す。

use std::collections::HashMap;

use chrono::NaiveDate;
use serde_json::{json, Value};

use super::{call_date_jst, consultant_of, date10, deals_of, outcome_of, Outcome, Sheets};
use crate::handlers::call_quality::sheets::SheetData;

/// 滞留とみなす日数。これを超えたら滞留（ちょうど 45 日は含めない）。
pub const STAY_DAYS: i64 = 45;

/// 滞留に数えないステージの ID。**あとで足せるよう 1 か所に置く。今は空。**
/// 満了後も稼働中のエスカレーションは、ここに入れたステージでも出す。
pub const STAY_EXEMPT_STAGES: &[&str] = &[];

/// いまのステージに入った日。履歴に変更が無い、または最後の値がいまのステージと合わないときは `None`。
fn entered_dates(history: &SheetData, current: &HashMap<&str, &str>) -> HashMap<String, NaiveDate> {
    let (Some(c_deal), Some(c_prop), Some(c_v), Some(c_ts)) = (
        history.col("deal_id"),
        history.col("prop"),
        history.col("v"),
        history.col("ts"),
    ) else {
        return HashMap::new();
    };
    let mut by_deal: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for row in &history.rows {
        let cell = |i: usize| row.get(i).map(|s| s.as_ref()).unwrap_or("");
        if cell(c_prop) != "dealstage" {
            continue;
        }
        let deal = cell(c_deal);
        if !current.contains_key(deal) || cell(c_ts).is_empty() {
            continue;
        }
        by_deal
            .entry(deal)
            .or_default()
            .push((cell(c_ts), cell(c_v).trim()));
    }
    let mut out = HashMap::new();
    for (deal, mut evs) in by_deal {
        // 時刻は ISO 8601（同じ書式）なので文字の順で並ぶ。同時刻は表の並びのまま
        evs.sort_by(|a, b| a.0.cmp(b.0));
        let cur = current[deal];
        if evs.last().map(|e| e.1) != Some(cur) {
            continue;
        }
        let start = evs.iter().rposition(|e| e.1 != cur).map_or(0, |i| i + 1);
        if let Some(d) = call_date_jst(evs[start].0) {
            out.insert(deal.to_string(), d);
        }
    }
    out
}

/// 滞留か。例外ステージは数えない（満了後のエスカレーションは別に判定する）
fn is_stay(stage: &str, days: Option<i64>, exempt: &[&str]) -> bool {
    !exempt.contains(&stage) && matches!(days, Some(x) if x > STAY_DAYS)
}

pub fn build_stage_stay(sheets: &Sheets, today: NaiveDate) -> Value {
    stage_stay_of(&sheets.deal, &sheets.history, &sheets.owner_hist, today)
}

pub(super) fn stage_stay_of(
    deal: &SheetData,
    history: &SheetData,
    owner_hist: &SheetData,
    today: NaiveDate,
) -> Value {
    let deals = deals_of(deal);
    let act: Vec<_> = deals
        .iter()
        .filter(|d| d.is_active && outcome_of(&d.stage) == Outcome::Pending)
        .collect();
    let current: HashMap<&str, &str> = act
        .iter()
        .map(|d| (d.id.as_str(), d.stage.as_str()))
        .collect();
    let entered = entered_dates(history, &current);
    let who = consultant_of(owner_hist);

    let mut rows: Vec<(bool, i64, i64, Value)> = Vec::new();
    let (mut n_stay, mut n_esc, mut n_unknown) = (0usize, 0usize, 0usize);
    for d in &act {
        let since = entered.get(&d.id).copied();
        let days = since.map(|s| (today - s).num_days());
        let expiry = date10(&d.contract_expiration_date);
        let over = expiry.map(|e| (today - e).num_days()).filter(|x| *x > 0);
        let escalation = over.is_some();
        let stay = is_stay(&d.stage, days, STAY_EXEMPT_STAGES);
        if since.is_none() {
            n_unknown += 1;
        }
        if !escalation && !stay {
            continue;
        }
        if escalation {
            n_esc += 1;
        }
        if stay {
            n_stay += 1;
        }
        rows.push((
            escalation,
            over.unwrap_or(0),
            days.unwrap_or(-1),
            json!({
                "deal_id": d.id,
                "name": d.name,
                "consultant": who.get(&d.id).map(|(n, _)| n.as_str()).unwrap_or(""),
                "stage": d.stage,
                "stage_label": d.stage_label,
                "entered": since.map(|s| s.to_string()),
                "days": days,
                "expiry": expiry.map(|e| e.to_string()),
                "days_past_expiry": over,
                "escalation": escalation,
                "stay": stay,
            }),
        ));
    }
    // エスカレーションを先に（満了を過ぎた日数の多い順）、その後に滞留日数の長い順
    rows.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(b.2.cmp(&a.2))
            .then(a.3["deal_id"].as_str().cmp(&b.3["deal_id"].as_str()))
    });
    json!({
        "meta": {
            "today": today.to_string(),
            "stay_days": STAY_DAYS,
            "n_active": act.len(),
            "n_stay": n_stay,
            "n_escalation": n_esc,
            "n_unknown_entered": n_unknown,
            "n_exempt_stages": STAY_EXEMPT_STAGES.len(),
        },
        "rows": rows.into_iter().map(|r| r.3).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    fn sheet(h: &[&str], rows: &[&[&str]]) -> SheetData {
        SheetData {
            header: h.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|c| Arc::<str>::from(*c)).collect())
                .collect(),
            fetched_at: Instant::now(),
        }
    }

    const DH: &[&str] = &[
        "deal_id",
        "dealname",
        "dealstage",
        "dealstage_label",
        "contract_kind",
        "contract_expiration_date",
        "is_active",
    ];
    fn ev(deal: &'static str, v: &'static str, ts: &'static str) -> [&'static str; 5] {
        [deal, "dealstage", "2026-01", v, ts]
    }

    // 基準日 2026-10-20。45 日前は 2026-09-05
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
    }

    fn run(deals: &[&[&str]], hist: &[[&str; 5]]) -> Value {
        let h: Vec<&[&str]> = hist.iter().map(|r| &r[..]).collect();
        stage_stay_of(
            &sheet(DH, deals),
            &sheet(&["deal_id", "prop", "month", "v", "ts"], &h),
            &sheet(&["date", "owner", "retired", "deal_id"], &[]),
            today(),
        )
    }

    fn ids(v: &Value) -> Vec<&str> {
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["deal_id"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn 滞留は45日ちょうどが出ず46日が出る() {
        let v = run(
            &[
                &[
                    "a45",
                    "架空A",
                    "52016156",
                    "定期1",
                    "(新規)",
                    "2027-01-01",
                    "TRUE",
                ],
                &[
                    "a46",
                    "架空B",
                    "52016156",
                    "定期1",
                    "(新規)",
                    "2027-01-01",
                    "TRUE",
                ],
            ],
            &[
                // 日本時間の日付: UTC 9/4 15:00 = JST 9/5（45 日前）
                ev("a45", "52016156", "2026-09-04T15:00:00.000Z"),
                ev("a46", "52016156", "2026-09-04T14:59:00.000Z"),
            ],
        );
        assert_eq!(ids(&v), vec!["a46"]);
        assert_eq!(v["rows"][0]["days"], 46);
        assert_eq!(v["rows"][0]["entered"], "2026-09-04");
        assert_eq!(v["meta"]["n_stay"], 1);
        assert_eq!(v["meta"]["n_escalation"], 0);
    }

    #[test]
    fn 入った日はいまの値が続く先頭の行() {
        let v = run(
            &[&[
                "x",
                "架空",
                "52016156",
                "定期1",
                "(新規)",
                "2027-01-01",
                "TRUE",
            ]],
            &[
                ev("x", "52016153", "2026-05-01T00:00:00Z"),
                ev("x", "52016156", "2026-07-01T00:00:00Z"),
                // 同じ値の書き直しは変化ではない
                ev("x", "52016156", "2026-09-30T00:00:00Z"),
            ],
        );
        assert_eq!(v["rows"][0]["entered"], "2026-07-01");
        assert_eq!(v["rows"][0]["days"], 111);
    }

    #[test]
    fn 決着と紹介料とオプションは出さない() {
        let v = run(
            &[
                &[
                    "keep",
                    "架空継続",
                    "66848546",
                    "継続済",
                    "(新規)",
                    "2026-01-01",
                    "FALSE",
                ],
                &[
                    "ref",
                    "架空9月分紹介料",
                    "52016155",
                    "求人出稿完了",
                    "(新規)",
                    "2026-01-01",
                    "TRUE",
                ],
                &[
                    "opt",
                    "架空オプション",
                    "1049738304",
                    "オプション",
                    "求人追加",
                    "2026-01-01",
                    "TRUE",
                ],
            ],
            &[
                ev("keep", "66848546", "2026-01-01T00:00:00Z"),
                ev("ref", "52016155", "2026-01-01T00:00:00Z"),
                ev("opt", "1049738304", "2026-01-01T00:00:00Z"),
            ],
        );
        assert!(ids(&v).is_empty());
        assert_eq!(v["meta"]["n_active"], 0);
    }

    #[test]
    fn 満了後も稼働中は45日未満でも先頭に出す() {
        let v = run(
            &[
                &[
                    "old",
                    "架空長期",
                    "52016156",
                    "定期1",
                    "(新規)",
                    "2027-01-01",
                    "TRUE",
                ],
                &[
                    "exp",
                    "架空満了後",
                    "52016155",
                    "求人出稿完了",
                    "(新規)",
                    "2026-10-10",
                    "TRUE",
                ],
                &[
                    "today",
                    "架空満了日当日",
                    "52016155",
                    "求人出稿完了",
                    "(新規)",
                    "2026-10-20",
                    "TRUE",
                ],
            ],
            &[
                ev("old", "52016156", "2026-06-01T00:00:00Z"),
                ev("exp", "52016155", "2026-10-15T00:00:00Z"),
                ev("today", "52016155", "2026-10-15T00:00:00Z"),
            ],
        );
        assert_eq!(ids(&v), vec!["exp", "old"]);
        assert_eq!(v["rows"][0]["escalation"], true);
        assert_eq!(v["rows"][0]["days"], 5);
        assert_eq!(v["rows"][0]["days_past_expiry"], 10);
        assert_eq!(v["rows"][0]["stay"], false);
        assert_eq!(v["meta"]["n_escalation"], 1);
        assert_eq!(v["meta"]["n_stay"], 1);
    }

    #[test]
    fn 入った日が分からない案件は件数に出し満了後なら一覧に残す() {
        let v = run(
            &[
                &[
                    "none",
                    "架空履歴なし",
                    "52016156",
                    "定期1",
                    "(新規)",
                    "2027-01-01",
                    "TRUE",
                ],
                &[
                    "mismatch",
                    "架空不一致",
                    "52016156",
                    "定期1",
                    "(新規)",
                    "2027-01-01",
                    "TRUE",
                ],
                &[
                    "expnone",
                    "架空満了後履歴なし",
                    "52016156",
                    "定期1",
                    "(新規)",
                    "2026-09-01",
                    "TRUE",
                ],
            ],
            &[ev("mismatch", "52016153", "2026-01-01T00:00:00Z")],
        );
        assert_eq!(v["meta"]["n_unknown_entered"], 3);
        assert_eq!(ids(&v), vec!["expnone"]);
        assert!(v["rows"][0]["days"].is_null());
        assert!(v["rows"][0]["entered"].is_null());
    }

    #[test]
    fn 例外ステージは滞留に出さない() {
        let exempt: &[&str] = &["52016156"];
        assert!(!is_stay("52016156", Some(100), exempt));
        assert!(is_stay("52016155", Some(100), exempt));
        assert!(!is_stay("52016155", Some(45), exempt));
        assert!(!is_stay("52016155", None, exempt));
        assert!(is_stay("52016156", Some(100), STAY_EXEMPT_STAGES));
    }

    #[test]
    fn 満了後は滞留でなくても出る() {
        let v = run(
            &[&[
                "e",
                "架空",
                "52016156",
                "定期1",
                "(新規)",
                "2026-10-01",
                "TRUE",
            ]],
            &[ev("e", "52016156", "2026-10-10T00:00:00Z")],
        );
        assert_eq!(ids(&v), vec!["e"]);
        assert_eq!(v["rows"][0]["stay"], false);
        assert_eq!(v["rows"][0]["escalation"], true);
    }
}
