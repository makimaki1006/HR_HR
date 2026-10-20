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

/// 営業KPI と同じシート (常駐キャッシュ) から読む。読めなければ `None`
pub async fn load(from: NaiveDate, to: NaiveDate, today: NaiveDate) -> Option<Dials> {
    let state = cq_state().ok()?;
    let (kaden, _) = state.store.get(&state.client, SHEET_KADEN).await.ok()?;
    let (meta, _) = state.store.get(&state.client, SHEET_META).await.ok()?;
    Some(dials_from_sheets(&kaden, &meta, from, to, today))
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
