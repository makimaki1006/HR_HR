//! Official prefectural rates, selected by effective date in Japan.
use chrono::{FixedOffset, NaiveDate, Utc};
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Debug, Deserialize)]
pub struct Rate {
    pub fiscal_year: i64,
    pub prefecture: String,
    pub hourly_min_wage: i64,
    pub effective_date: NaiveDate,
    pub source_url: String,
    #[serde(default)]
    pub source: String,
}

pub fn japan_today() -> NaiveDate {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap())
        .date_naive()
}

fn rates() -> &'static [Rate] {
    static RATES: OnceLock<Vec<Rate>> = OnceLock::new();
    RATES.get_or_init(|| {
        csv::Reader::from_reader(include_str!("../data/minimum_wage_rates.csv").as_bytes())
            .deserialize::<Rate>()
            .map(|row| {
                let mut rate = row.expect("checked official minimum wage CSV");
                assert!(rate.hourly_min_wage > 0);
                rate.source = "official_csv".to_string();
                rate
            })
            .collect()
    })
}

pub fn official_at(prefecture: &str, as_of: NaiveDate) -> Option<Rate> {
    rates()
        .iter()
        .filter(|r| r.prefecture == prefecture && r.effective_date <= as_of)
        .max_by_key(|r| (r.effective_date, r.fiscal_year))
        .cloned()
}

/// A DB record needs date and year metadata to supersede a known official rate.
/// Undated or future DB amounts are never used as a current legal rate.
pub fn resolve_at(
    prefecture: &str,
    row: Option<&HashMap<String, serde_json::Value>>,
    as_of: NaiveDate,
) -> Option<Rate> {
    let official = official_at(prefecture, as_of);
    let db = row.and_then(|r| {
        let integer = |key: &str| {
            r.get(key)
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        };
        let fiscal_year = integer("fiscal_year")?;
        let hourly_min_wage = integer("hourly_min_wage")?;
        let effective_date =
            NaiveDate::parse_from_str(r.get("effective_date")?.as_str()?, "%Y-%m-%d").ok()?;
        if hourly_min_wage <= 0
            || effective_date > as_of
            || fiscal_year > chrono::Datelike::year(&effective_date) as i64
        {
            return None;
        }
        Some(Rate {
            fiscal_year,
            prefecture: prefecture.to_string(),
            hourly_min_wage,
            effective_date,
            source_url: r
                .get("source_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: "database".to_string(),
        })
    });
    match (official, db) {
        (Some(known), Some(db))
            if db.fiscal_year > known.fiscal_year && db.effective_date > known.effective_date =>
        {
            Some(db)
        }
        (Some(known), _) => Some(known),
        (None, db) => db,
    }
}

pub fn resolve(prefecture: &str, row: Option<&HashMap<String, serde_json::Value>>) -> Option<Rate> {
    resolve_at(prefecture, row, japan_today())
}

pub fn resolved_rows(
    rows: &[HashMap<String, serde_json::Value>],
    prefecture: &str,
) -> Vec<HashMap<String, serde_json::Value>> {
    let mut prefectures: std::collections::BTreeSet<String> =
        rates().iter().map(|r| r.prefecture.clone()).collect();
    prefectures.extend(
        rows.iter()
            .filter_map(|r| r.get("prefecture")?.as_str().map(str::to_string)),
    );
    let as_of = japan_today();
    let mut result: Vec<_> = prefectures
        .into_iter()
        .filter(|p| prefecture.is_empty() || p == prefecture)
        .filter_map(|p| {
            let db = rows
                .iter()
                .find(|r| r.get("prefecture").and_then(|v| v.as_str()) == Some(p.as_str()));
            let rate = resolve_at(&p, db, as_of)?;
            Some(HashMap::from([
                ("prefecture".to_string(), serde_json::json!(p)),
                (
                    "hourly_min_wage".to_string(),
                    serde_json::json!(rate.hourly_min_wage),
                ),
                (
                    "fiscal_year".to_string(),
                    serde_json::json!(rate.fiscal_year),
                ),
                (
                    "effective_date".to_string(),
                    serde_json::json!(rate.effective_date.to_string()),
                ),
                ("source_url".to_string(), serde_json::json!(rate.source_url)),
                ("source".to_string(), serde_json::json!(rate.source)),
                ("as_of".to_string(), serde_json::json!(as_of.to_string())),
            ]))
        })
        .collect();
    result.sort_by_key(|r| std::cmp::Reverse(r["hourly_min_wage"].as_i64().unwrap_or(0)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(wage: i64, year: i64, date: &str) -> HashMap<String, serde_json::Value> {
        serde_json::from_value(
            serde_json::json!({"hourly_min_wage":wage,"fiscal_year":year,"effective_date":date}),
        )
        .unwrap()
    }
    #[test]
    fn official_csv_has_unique_prefecture_year_and_valid_dates() {
        let mut keys = std::collections::HashSet::new();
        for r in rates() {
            assert!(keys.insert((&r.prefecture, r.fiscal_year)));
            assert!(r.source_url.starts_with("https://"));
        }
        assert_eq!(rates().iter().filter(|r| r.fiscal_year == 2025).count(), 47);
        assert_eq!(rates().iter().filter(|r| r.fiscal_year == 2026).count(), 47);
    }
    #[test]
    fn future_rate_is_not_applied_and_stale_db_is_overridden() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let known = official_at("東京都", date).unwrap();
        let stale = row(1, 2024, "2024-10-01");
        assert_eq!(
            resolve_at("東京都", Some(&stale), date)
                .unwrap()
                .hourly_min_wage,
            known.hourly_min_wage
        );
        let future = row(9999, 2027, "2027-10-01");
        assert_eq!(
            resolve_at("東京都", Some(&future), date)
                .unwrap()
                .hourly_min_wage,
            known.hourly_min_wage
        );
        assert!(official_at("東京都", NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()).is_none());
    }
    #[test]
    fn newer_effective_db_year_is_preserved() {
        let date = NaiveDate::from_ymd_opt(2030, 10, 1).unwrap();
        let newer = row(1600, 2030, "2030-10-01");
        assert_eq!(
            resolve_at("東京都", Some(&newer), date)
                .unwrap()
                .hourly_min_wage,
            1600
        );
        let undated = serde_json::from_value(serde_json::json!({"hourly_min_wage":9999})).unwrap();
        assert_eq!(
            resolve_at("東京都", Some(&undated), date).unwrap().source,
            "official_csv"
        );
    }
    #[test]
    fn prefectural_effective_dates_apply_on_the_day() {
        let before = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert_eq!(official_at("大阪府", before).unwrap().hourly_min_wage, 1231);
        assert_eq!(official_at("東京都", before).unwrap().hourly_min_wage, 1280);
        assert_eq!(official_at("京都府", before).unwrap().hourly_min_wage, 1122);
        assert_eq!(
            official_at("京都府", NaiveDate::from_ymd_opt(2026, 11, 16).unwrap())
                .unwrap()
                .hourly_min_wage,
            1180
        );
        assert_eq!(
            official_at("沖縄県", NaiveDate::from_ymd_opt(2026, 12, 1).unwrap())
                .unwrap()
                .hourly_min_wage,
            1023
        );
        assert_eq!(
            official_at("沖縄県", NaiveDate::from_ymd_opt(2026, 12, 2).unwrap())
                .unwrap()
                .hourly_min_wage,
            1086
        );
    }
}
