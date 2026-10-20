//! 架電数は営業KPI と同じ値を使う。Zoom の通話ログの `result == "Auto Recorded"` を日次同期が
//! シート `KPI営業_架電日次` の「架電数」に書いたもので、営業KPI の画面の「架電数」と同じ。
//! 数え方は営業KPI の `kaden_of` / `kaden_period` をそのまま使う (ここでは数え直さない)。
//! 今日の分はシートの途中経過 (朝・夕の同期の時点)。読めないときは数を出さず、HubSpot では数えない。

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use crate::handlers::call_quality::routes::cq_state;
use crate::handlers::call_quality::sheets::SheetData;
use crate::handlers::sales_kpi::{kaden_last_day, kaden_of, kaden_period, SHEET_KADEN, SHEET_META};

/// 期間の人ごとの架電数 (つながった通話)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dials {
    pub by_owner: HashMap<String, u32>,
    /// 期間の最終日がシートの途中経過か (その日の数は同期の時点のもの)
    pub partial: bool,
    /// 同期の取得時刻 (`yyyy-MM-dd HH:mm`)。途中のときだけ。無ければ空
    pub fetched_at: Option<String>,
}

fn meta_of(sheet: &SheetData) -> BTreeMap<String, String> {
    sheet
        .rows
        .iter()
        .map(|r| {
            (
                sheet.get(r, "項目").to_string(),
                sheet.get(r, "値").to_string(),
            )
        })
        .collect()
}

/// `[from, to]` (JST の日) の人ごとの架電数。`today` は JST の今日
pub fn dials_from_sheets(
    kaden: &SheetData,
    meta: &SheetData,
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
) -> Dials {
    let rows = kaden_of(kaden);
    let meta = meta_of(meta);
    let days: Vec<String> = from
        .iter_days()
        .take_while(|d| *d <= to)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .collect();
    let period = kaden_period(&rows, &days, &HashMap::new());
    let by_owner = period
        .by_person
        .iter()
        .map(|(o, c)| {
            (
                o.clone(),
                c.get("connected").copied().unwrap_or(0).max(0) as u32,
            )
        })
        .collect();
    let (last_day, last_partial) = kaden_last_day(&rows, &meta, today);
    let partial = last_partial && days.contains(&last_day);
    Dials {
        by_owner,
        partial,
        fetched_at: meta
            .get("架電の取得時刻")
            .filter(|t| partial && !t.is_empty())
            .cloned(),
    }
}

/// 今日より前の日はシート、今日は Zoom から数えた値 (`live`) を足す。
/// `sheet_before_today` は `[from, today の前日]` のシートの値 (期間が今日だけなら空)。
pub fn merge_live(
    sheet_before_today: Dials,
    live: &HashMap<String, u32>,
    live_fetched_at: &str,
) -> Dials {
    let mut by_owner = sheet_before_today.by_owner;
    for (o, n) in live {
        *by_owner.entry(o.clone()).or_insert(0) += n;
    }
    Dials {
        by_owner,
        partial: true,
        fetched_at: Some(live_fetched_at.to_string()),
    }
}

/// 営業KPI と同じシート (常駐キャッシュ) から読む。読めなければ `None`
pub async fn load(from: NaiveDate, to: NaiveDate, today: NaiveDate) -> Option<Dials> {
    let (kaden, meta) = load_sheets().await?;
    Some(dials_from_sheets(&kaden, &meta, from, to, today))
}

async fn load_sheets() -> Option<(std::sync::Arc<SheetData>, std::sync::Arc<SheetData>)> {
    let state = cq_state().ok()?;
    let (kaden, _) = state.store.get(&state.client, SHEET_KADEN).await.ok()?;
    let (meta, _) = state.store.get(&state.client, SHEET_META).await.ok()?;
    Some((kaden, meta))
}

/// 期間に今日が含まれ、Zoom から今日を数えられたときは「シートの前日まで + Zoom の今日」。
/// 数えられないとき (設定なし・失敗で前の値もない) は従来どおりシートだけ。
pub async fn load_with_live(
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
    live: Option<&super::live_dials::LiveToday>,
) -> Option<Dials> {
    let sheets = load_sheets().await;
    let Some(live) = live.filter(|_| from <= today && today <= to) else {
        let (kaden, meta) = sheets?;
        return Some(dials_from_sheets(&kaden, &meta, from, to, today));
    };
    let prior = if from < today {
        let (kaden, meta) = sheets?;
        let yesterday = today.pred_opt()?;
        dials_from_sheets(&kaden, &meta, from, yesterday, today)
    } else {
        Dials::default()
    };
    Some(merge_live(prior, &live.by_owner, &live.fetched_at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::sales_kpi::fixture::sheet_from_tsv;

    fn d(n: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, n).unwrap()
    }

    const KADEN: &str = "日付\townerId\t部署\t発信\t架電数\t5分超\n\
        2026-10-19\t11\tBPO\t500\t120\t3\n\
        2026-10-19\t22\t営業\t300\t80\t4\n\
        2026-10-20\t11\tBPO\t200\t41\t1\n\
        2026-10-18\t11\tBPO\t999\t999\t9\n";

    #[test]
    fn sheet_connected_calls_come_out_per_person_for_the_period() {
        let meta = sheet_from_tsv("項目\t値\n架電の最終日\t2026-10-20\n架電の最終日は途中\tはい\n架電の取得時刻\t2026-10-20 06:30\n");
        let r = dials_from_sheets(&sheet_from_tsv(KADEN), &meta, d(19), d(20), d(20));
        // 「発信」ではなく「架電数」(つながった通話) を人ごとに足す。範囲外の 10/18 は入れない
        assert_eq!(r.by_owner.get("11"), Some(&161));
        assert_eq!(r.by_owner.get("22"), Some(&80));
        assert_eq!(r.by_owner.len(), 2);
        // 最終日の今日は同期の途中経過
        assert!(r.partial);
        assert_eq!(r.fetched_at.as_deref(), Some("2026-10-20 06:30"));
    }

    #[test]
    fn live_today_is_added_to_the_sheet_days_before_it() {
        // シートの 10/19 (11 は 120、22 は 80) に、Zoom の今日 (11 は 30、33 は 5) を足す
        let meta = sheet_from_tsv("項目\t値\n");
        let prior = dials_from_sheets(&sheet_from_tsv(KADEN), &meta, d(19), d(19), d(20));
        let live = HashMap::from([("11".to_string(), 30u32), ("33".to_string(), 5u32)]);
        let r = merge_live(prior, &live, "2026-10-20 09:55");
        assert_eq!(r.by_owner.get("11"), Some(&150));
        assert_eq!(r.by_owner.get("22"), Some(&80));
        assert_eq!(r.by_owner.get("33"), Some(&5));
        assert!(r.partial);
        assert_eq!(r.fetched_at.as_deref(), Some("2026-10-20 09:55"));
        // シートの今日の朝の値 (41) は使わない
        let only_today = merge_live(Dials::default(), &live, "2026-10-20 09:55");
        assert_eq!(only_today.by_owner.get("11"), Some(&30));
    }

    #[test]
    fn a_finished_last_day_or_a_period_before_it_is_not_partial() {
        let meta = sheet_from_tsv("項目\t値\n架電の最終日\t2026-10-20\n架電の最終日は途中\tはい\n架電の取得時刻\t2026-10-20 06:30\n");
        let r = dials_from_sheets(&sheet_from_tsv(KADEN), &meta, d(19), d(19), d(20));
        assert_eq!(r.by_owner.get("11"), Some(&120));
        assert!(!r.partial);
        assert_eq!(r.fetched_at, None);
        let done =
            sheet_from_tsv("項目\t値\n架電の最終日\t2026-10-20\n架電の最終日は途中\tいいえ\n");
        assert!(!dials_from_sheets(&sheet_from_tsv(KADEN), &done, d(19), d(20), d(21)).partial);
        // 取得条件が無くても、最終日が今日なら途中と見なす (営業KPI と同じ)
        let none = sheet_from_tsv("項目\t値\n");
        assert!(dials_from_sheets(&sheet_from_tsv(KADEN), &none, d(19), d(20), d(20)).partial);
    }
}
