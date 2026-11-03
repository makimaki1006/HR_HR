//! 「がっかり」の印（名札とは別。名札の本数 `n_flags` には数えない）。
//!
//! 応募が30日無い: 稼働中で、契約開始から30日以上たち、直近30日（今日を含む）の応募が1件も無い案件。
//! 求人が掲載中かどうかは見ない。応募は案件管理が集計した応募日を読むだけで、新しく HubSpot は読まない。
//! 応募のデータが無い案件・応募日が分からない応募がある案件には付けない（0件とは言えないため）。
use crate::handlers::job_copy_live::shared_applications::{DealApplications, Published};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};

pub const KEY_NO_APPLICANTS: &str = "no_applicants_30d";
pub const LABEL_NO_APPLICANTS: &str = "応募が30日無い";
const DAYS: i64 = 30;

pub const RULE: &str = "がっかり（注意の数には入れません）: 契約開始から30日以上で、直近30日に応募が0件。求人の掲載状況は見ません。\
開始前・開始30日未満・応募のデータが無い案件には付けません。\
候補者の辞退・入社後すぐの退職・同じ不満2回は、MTGの記録から拾う予定です。";

/// 応募が30日無いか。`start` は契約開始日。
pub fn no_applicants_30d(
    start: Option<NaiveDate>,
    apps: Option<&DealApplications>,
    today: NaiveDate,
) -> bool {
    let (Some(start), Some(apps)) = (start, apps) else {
        return false;
    };
    if start > today - Duration::days(DAYS) || apps.unknown_dates > 0 {
        return false;
    }
    let from = today - Duration::days(DAYS);
    !apps.days.iter().any(|d| *d > from && *d <= today)
}

/// 最後の応募日（今日まで）。無ければ None。
pub fn last_application(apps: &DealApplications, today: NaiveDate) -> Option<NaiveDate> {
    apps.days.iter().copied().filter(|d| *d <= today).max()
}

/// 行に `gakkari` と `last_application` を足し、meta に件数と応募の時点を足す。
/// 既存の名札・並び・件数には触れない。
pub fn annotate(rows: &mut [Value], meta: &mut Value, today: NaiveDate, facts: Option<&Published>) {
    let mut n = 0usize;
    let mut checked = 0usize;
    for r in rows.iter_mut() {
        let apps = facts.and_then(|f| r["deal_id"].as_str().and_then(|id| f.deals.get(id)));
        let start = r["start"].as_str().and_then(super::date10);
        let hit = no_applicants_30d(start, apps, today);
        if let (Some(s), Some(a)) = (start, apps) {
            if s <= today - Duration::days(DAYS) && a.unknown_dates == 0 {
                checked += 1;
            }
        }
        n += usize::from(hit);
        r["gakkari"] = if hit {
            json!([KEY_NO_APPLICANTS])
        } else {
            json!([])
        };
        r["last_application"] = apps
            .and_then(|a| last_application(a, today))
            .map_or(Value::Null, |d| json!(d.to_string()));
    }
    if let Some(m) = meta.as_object_mut() {
        m.insert(
            "gakkari_counts".into(),
            json!([{"key": KEY_NO_APPLICANTS, "label": LABEL_NO_APPLICANTS, "n": n}]),
        );
        m.insert("gakkari_checked".into(), json!(checked));
        m.insert("gakkari_rule".into(), json!(RULE));
        m.insert(
            "applications_as_of".into(),
            facts.map_or(Value::Null, |f| json!(f.built_at.to_rfc3339())),
        );
    }
}

/// いま公開されている応募日を使って付ける。
pub fn annotate_current(rows: &mut [Value], meta: &mut Value, today: NaiveDate) {
    let facts = crate::handlers::job_copy_live::shared_applications::current();
    annotate(rows, meta, today, facts.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn apps(days: &[&str], unknown: usize) -> DealApplications {
        DealApplications {
            days: days.iter().map(|s| d(s)).collect(),
            unknown_dates: unknown,
        }
    }
    const TODAY: &str = "2026-10-27";

    #[test]
    fn started_30_days_ago_is_marked_29_is_not() {
        let a = apps(&[], 0);
        // 10/27 の30日前は 9/27
        assert!(no_applicants_30d(Some(d("2026-09-27")), Some(&a), d(TODAY)));
        assert!(!no_applicants_30d(
            Some(d("2026-09-28")),
            Some(&a),
            d(TODAY)
        ));
        assert!(!no_applicants_30d(
            Some(d("2026-11-01")),
            Some(&a),
            d(TODAY)
        ));
        assert!(!no_applicants_30d(None, Some(&a), d(TODAY)));
    }

    #[test]
    fn last_application_30_days_ago_is_marked_29_is_not() {
        let start = Some(d("2026-01-01"));
        // 9/27 は直近30日（9/28〜10/27）の外、9/28 は中
        assert!(no_applicants_30d(
            start,
            Some(&apps(&["2026-09-27"], 0)),
            d(TODAY)
        ));
        assert!(!no_applicants_30d(
            start,
            Some(&apps(&["2026-09-28"], 0)),
            d(TODAY)
        ));
        assert!(!no_applicants_30d(
            start,
            Some(&apps(&["2026-10-27"], 0)),
            d(TODAY)
        ));
    }

    #[test]
    fn no_data_or_unknown_dates_is_never_marked() {
        let start = Some(d("2026-01-01"));
        assert!(!no_applicants_30d(start, None, d(TODAY)));
        assert!(!no_applicants_30d(start, Some(&apps(&[], 1)), d(TODAY)));
    }

    #[test]
    fn annotate_adds_fields_counts_and_as_of() {
        let built: chrono::DateTime<chrono::Utc> = "2026-10-27T03:00:00Z".parse().unwrap();
        let mut deals = BTreeMap::new();
        deals.insert("1".to_string(), apps(&["2026-09-01"], 0)); // 最後の応募が古い → 印
        deals.insert("2".to_string(), apps(&["2026-10-20"], 0)); // 直近にある
        deals.insert("3".to_string(), apps(&[], 0)); // 開始が最近 → 印なし
        let facts = Published {
            built_at: built,
            deals,
        };
        let mut rows = vec![
            json!({"deal_id": "1", "start": "2026-06-01", "flags": ["a"], "n_flags": 1}),
            json!({"deal_id": "2", "start": "2026-06-01", "flags": [], "n_flags": 0}),
            json!({"deal_id": "3", "start": "2026-10-10", "flags": [], "n_flags": 0}),
            json!({"deal_id": "4", "start": "2026-06-01", "flags": [], "n_flags": 0}),
        ];
        let mut meta = json!({});
        annotate(&mut rows, &mut meta, d(TODAY), Some(&facts));
        assert_eq!(rows[0]["gakkari"], json!([KEY_NO_APPLICANTS]));
        assert_eq!(rows[0]["last_application"], "2026-09-01");
        assert_eq!(rows[1]["gakkari"], json!([]));
        assert_eq!(rows[1]["last_application"], "2026-10-20");
        assert_eq!(rows[2]["gakkari"], json!([]));
        assert_eq!(rows[2]["last_application"], Value::Null);
        assert_eq!(rows[3]["gakkari"], json!([]));
        // 名札には触れない
        assert_eq!(rows[0]["n_flags"], 1);
        assert_eq!(rows[0]["flags"], json!(["a"]));
        assert_eq!(meta["gakkari_counts"][0]["n"], 1);
        assert_eq!(meta["gakkari_checked"], 2); // 1 と 2（3 は開始が最近、4 はデータ無し）
        assert_eq!(meta["applications_as_of"], built.to_rfc3339());
    }

    #[test]
    fn annotate_without_facts_marks_nothing_and_says_so() {
        let mut rows = vec![json!({"deal_id": "1", "start": "2026-06-01"})];
        let mut meta = json!({});
        annotate(&mut rows, &mut meta, d(TODAY), None);
        assert_eq!(rows[0]["gakkari"], json!([]));
        assert_eq!(meta["gakkari_counts"][0]["n"], 0);
        assert_eq!(meta["applications_as_of"], Value::Null);
    }
}
