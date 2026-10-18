//! 日別の件数だけを監査DBに保存する。顧客の情報・金額は含めない。
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use serde_json::{json, Value};

use crate::audit::AuditDb;
use crate::db::turso_http::TursoDb;

use super::{date10, Sheets};

const METRICS: [&str; 6] = [
    "n_flags2",
    "mtg_critical",
    "no_contact",
    "expiring60",
    "nps_low",
    "expiring_week",
];
type Counts = BTreeMap<String, BTreeMap<String, i64>>;
#[derive(Clone)]
struct Previous {
    day: String,
    counts: Counts,
}
#[derive(Default)]
struct HistoryCache {
    day: String,
    loaded: Option<Instant>,
    failed: bool,
    previous: Option<Previous>,
}
static CACHE: LazyLock<Mutex<HistoryCache>> = LazyLock::new(|| Mutex::new(HistoryCache::default()));

fn counts(value: &Value) -> Counts {
    let mut result = Counts::new();
    for row in value["status"]["rows"].as_array().into_iter().flatten() {
        result.insert(
            row["consultant"].as_str().unwrap_or_default().to_string(),
            METRICS
                .iter()
                .map(|key| (key.to_string(), row[key].as_i64().unwrap_or(0)))
                .collect(),
        );
    }
    // 空名は担当不明の専用枠。全体のカードにはこの分も含まれる。
    let unknown = &value["status"]["meta"]["unknown"];
    result.insert(
        String::new(),
        METRICS
            .iter()
            .map(|key| (key.to_string(), unknown[key].as_i64().unwrap_or(0)))
            .collect(),
    );
    result
}

impl HistoryCache {
    fn load(
        &mut self,
        db: &TursoDb,
        day: NaiveDate,
        current: impl FnOnce() -> Counts,
    ) -> Result<Option<Previous>, String> {
        let day = day.to_string();
        if self.day == day
            && self
                .loaded
                .is_some_and(|at| !self.failed || at.elapsed() < Duration::from_secs(60))
        {
            return if self.failed {
                Err("history unavailable".into())
            } else {
                Ok(self.previous.clone())
            };
        }
        self.day = day.clone();
        self.loaded = Some(Instant::now());
        self.failed = true;
        self.previous = None;
        // 当日が既存なら書かない。直前の「存在するデータの日」を選ぶ（土日等を飛ばす）。
        let rows = db.query("SELECT data_day, consultant, metric, count FROM consulting_team_counts
            WHERE data_day = ?1 OR data_day = (SELECT MAX(data_day) FROM consulting_team_counts WHERE data_day < ?1)", &[&day])?;
        let mut exists = false;
        let mut previous: Option<Previous> = None;
        for row in rows {
            let row_day = row["data_day"].as_str().ok_or("invalid day")?;
            if row_day == day {
                exists = true;
                continue;
            }
            let prior = previous.get_or_insert_with(|| Previous {
                day: row_day.to_string(),
                counts: Counts::new(),
            });
            prior
                .counts
                .entry(
                    row["consultant"]
                        .as_str()
                        .ok_or("invalid consultant")?
                        .to_string(),
                )
                .or_default()
                .insert(
                    row["metric"].as_str().ok_or("invalid metric")?.to_string(),
                    row["count"].as_i64().ok_or("invalid count")?,
                );
        }
        if !exists {
            let payload: Vec<Value> = current().into_iter().flat_map(|(name, metrics)|
                metrics.into_iter().map(move |(metric, count)| json!({"consultant": name, "metric": metric, "count": count}))).collect();
            // 一つの文で全担当・全項目を確定。再送や複数プロセスでも日ごとに最初の数を保持する。
            db.execute("INSERT OR IGNORE INTO consulting_team_counts (data_day, consultant, metric, count)
                SELECT ?1, json_extract(value, '$.consultant'), json_extract(value, '$.metric'), json_extract(value, '$.count')
                FROM json_each(?2) WHERE NOT EXISTS (SELECT 1 FROM consulting_team_counts WHERE data_day = ?1)", &[&day, &serde_json::to_string(&payload).map_err(|_| "invalid counts")?])?;
        }
        self.failed = false;
        self.previous = previous;
        Ok(self.previous.clone())
    }
}

fn attach(value: &mut Value, previous: &Previous) {
    let current = counts(value);
    let mut totals = BTreeMap::new();
    for key in METRICS {
        let sum =
            |rows: &Counts| -> Option<i64> { rows.values().map(|r| r.get(key).copied()).sum() };
        if let (Some(now), Some(before)) = (sum(&current), sum(&previous.counts)) {
            totals.insert(key, now - before);
        }
    }
    for row in value["status"]["rows"].as_array_mut().into_iter().flatten() {
        let name = row["consultant"].as_str().unwrap_or_default();
        if let Some(before) = previous.counts.get(name) {
            let delta: BTreeMap<_, _> = METRICS
                .iter()
                .filter_map(|key| Some((*key, row[key].as_i64()? - before.get(*key)?)))
                .collect();
            row["delta"] = json!(delta);
        }
    }
    value["comparison"] = json!({"data_day": previous.day, "totals": totals});
}

/// 任意の比較用DBが遅くても、既存の画面を待たせ続けない。
/// 期限後も保存処理は完了させる。進行中の要求は try_lock で重複させない。
pub(super) async fn enrich_response(value: Value, sheets: Sheets, audit: Option<AuditDb>) -> Value {
    if audit.is_none() {
        return value;
    }
    let fallback = value.clone();
    let job = tokio::task::spawn_blocking(move || {
        let mut value = value;
        enrich(&mut value, &sheets, audit.as_ref());
        value
    });
    match tokio::time::timeout(Duration::from_secs(2), job).await {
        Ok(Ok(value)) => value,
        _ => fallback,
    }
}

pub(super) fn enrich(value: &mut Value, sheets: &Sheets, audit: Option<&AuditDb>) {
    let Some(audit) = audit else { return };
    let Some(day) = super::data_as_of(&sheets.meta).and_then(|s| date10(&s)) else {
        return;
    };
    let Ok(mut cache) = CACHE.try_lock() else {
        return;
    };
    // 既存の画面が計算した数だけを保存する。保存済みの日は日付越えでも上書きしない。
    match cache.load(audit.turso(), day, || counts(value)) {
        Ok(Some(previous)) => attach(value, &previous),
        Ok(None) => {}
        Err(_) => tracing::warn!("コンサルKPI: 前回の件数を読み書きできません"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::fake_turso::start_sqlite_audit_with_queries;

    fn sample(n: i64) -> Value {
        json!({"status": {"rows": [{"consultant": "架空の担当", "n_flags2": n,
            "mtg_critical": 3, "no_contact": 0, "expiring60": 2, "nps_low": 1, "expiring_week": 4}],
            "meta": {"unknown": {"n_flags2": 1}}}})
    }

    #[test]
    fn concrete_deltas_and_missing_owner() {
        let before = Previous {
            day: "2026-10-16".into(),
            counts: counts(&sample(2)),
        };
        let mut now = sample(5);
        now["status"]["rows"][0]["mtg_critical"] = json!(1);
        now["status"]["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"consultant": "新しい架空担当", "n_flags2": 2}));
        attach(&mut now, &before);
        assert_eq!(now["status"]["rows"][0]["delta"]["n_flags2"], 3);
        assert_eq!(now["status"]["rows"][0]["delta"]["mtg_critical"], -2);
        assert_eq!(now["status"]["rows"][0]["delta"]["no_contact"], 0);
        assert!(now["status"]["rows"][1]["delta"].is_null());
        assert_eq!(now["comparison"]["totals"]["n_flags2"], 5);
    }

    #[tokio::test]
    async fn one_insert_per_data_day_and_previous_friday() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        tokio::task::spawn_blocking(move || {
            let mut cache = HistoryCache::default();
            let friday = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
            let monday = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
            assert!(cache
                .load(audit.turso(), friday, || counts(&sample(2)))
                .unwrap()
                .is_none());
            cache
                .load(audit.turso(), friday, || panic!("cached"))
                .unwrap();
            // プロセスの再起動でも既存の日は書かない・元の数を保つ。
            cache = HistoryCache::default();
            assert!(cache
                .load(audit.turso(), friday, || panic!("already saved"))
                .unwrap()
                .is_none());
            let previous = cache
                .load(audit.turso(), monday, || counts(&sample(5)))
                .unwrap()
                .unwrap();
            assert_eq!(previous.day, "2026-10-16");
            assert_eq!(previous.counts["架空の担当"]["n_flags2"], 2);
            assert_eq!(
                conn.lock()
                    .unwrap()
                    .query_row("SELECT COUNT(*) FROM consulting_team_counts", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                24
            );
            assert_eq!(
                queries
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|q| q.starts_with("INSERT OR IGNORE INTO consulting_team_counts"))
                    .count(),
                2
            );
            assert_eq!(
                queries
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|q| q.starts_with("SELECT data_day"))
                    .count(),
                3
            );
            conn.lock()
                .unwrap()
                .execute("DROP TABLE consulting_team_counts", [])
                .unwrap();
            assert!(cache
                .load(audit.turso(), monday.succ_opt().unwrap(), || counts(
                    &sample(9)
                ))
                .is_err());
            assert!(cache
                .load(audit.turso(), monday.succ_opt().unwrap(), || panic!(
                    "failed cache"
                ))
                .is_err());
            assert_eq!(
                queries
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|q| q.starts_with("SELECT data_day"))
                    .count(),
                4
            );
        })
        .await
        .unwrap();
    }
}
