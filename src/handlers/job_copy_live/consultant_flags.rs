//! ADR-032: daily flags only; no customer names, amounts, or business history.
use crate::{audit::AuditDb, db::turso_http::TursoDb};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

type Flags = BTreeMap<(String, String), String>;
#[derive(Clone, Default)]
pub(super) struct Comparison {
    pub(super) previous_day: Option<String>,
    previous: Flags,
}
#[derive(Default)]
struct Cache {
    day: String,
    loaded: Option<Instant>,
    result: Option<Comparison>,
}
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::default()));
impl Cache {
    fn load(
        &mut self,
        db: &TursoDb,
        day: &str,
        current: &[(String, String)],
    ) -> Result<Comparison, String> {
        if self.day == day
            && self
                .loaded
                .is_some_and(|at| self.result.is_some() || at.elapsed() < Duration::from_secs(60))
        {
            return self
                .result
                .clone()
                .ok_or_else(|| "flags unavailable".into());
        }
        self.day = day.to_string();
        self.loaded = Some(Instant::now());
        self.result = None;
        let rows = db.query("SELECT data_day, deal_id, flag_key, since_day FROM consulting_deal_flags
            WHERE data_day = ?1 OR data_day = (SELECT MAX(data_day) FROM consulting_deal_flags WHERE data_day < ?1)", &[&day])?;
        let mut result = Comparison::default();
        let mut exists = false;
        for row in rows {
            let get = |k: &str| {
                row.get(k)
                    .and_then(Value::as_str)
                    .ok_or("invalid flags row")
            };
            let row_day = get("data_day")?;
            if row_day == day {
                exists = true;
                continue;
            }
            result.previous_day = Some(row_day.to_string());
            result.previous.insert(
                (get("deal_id")?.to_string(), get("flag_key")?.to_string()),
                get("since_day")?.to_string(),
            );
        }
        if !exists {
            let mut payload: Vec<Value> = current.iter().map(|key| json!({"id":key.0,"key":key.1,"since":result.previous.get(key).map(String::as_str).unwrap_or(day)})).collect();
            // A marker preserves an empty day's snapshot and prevents later additions that day.
            payload.push(json!({"id":"","key":"","since":day}));
            db.execute("INSERT OR IGNORE INTO consulting_deal_flags (data_day, deal_id, flag_key, since_day)
                SELECT ?1, json_extract(value, '$.id'), json_extract(value, '$.key'), json_extract(value, '$.since')
                FROM json_each(?2) WHERE NOT EXISTS (SELECT 1 FROM consulting_deal_flags WHERE data_day = ?1)", &[&day, &serde_json::to_string(&payload).map_err(|_| "invalid flags")?])?;
        }
        self.result = Some(result.clone());
        Ok(result)
    }
}
pub(super) async fn comparison(
    audit: Option<AuditDb>,
    day: Option<String>,
    current: Vec<(String, String)>,
) -> Option<Comparison> {
    let (Some(audit), Some(day)) = (audit, day) else {
        return None;
    };
    let job = tokio::task::spawn_blocking(move || {
        let mut cache = CACHE.try_lock().ok()?;
        cache
            .load(audit.turso(), &day, &current)
            .map_err(|_| tracing::warn!("案件の名札の前回比較を取得できません"))
            .ok()
    });
    tokio::time::timeout(Duration::from_secs(2), job)
        .await
        .ok()?
        .ok()?
}
impl Comparison {
    pub(super) fn flags(&self, id: &str, flags: &[Value], day: Option<&str>) -> Vec<Value> {
        flags.iter().filter_map(Value::as_str).map(|key| {
            let since = self.previous.get(&(id.to_string(), key.to_string()));
            json!({"key":key,"label":key,"new":self.previous_day.is_some() && since.is_none(),"since":since.map(String::as_str).or(day)})
        }).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::fake_turso::start_sqlite_audit_with_queries;
    #[tokio::test]
    async fn daily_snapshot_continuity_restart_and_empty_day() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        tokio::task::spawn_blocking(move || {
            let mut c = Cache::default();
            let a = ("101".into(), "満了まで60日以内".into());
            let b = ("101".into(), "NPSが4以下".into());
            let ongoing = ("103".into(), "満了まで60日以内".into());
            let fresh = ("104".into(), "NPSが4以下".into());
            let first = c
                .load(audit.turso(), "2026-10-16", &[a.clone(), ongoing.clone()])
                .unwrap();
            assert!(first.previous_day.is_none());
            c.load(audit.turso(), "2026-10-16", &[a.clone(), b.clone()])
                .unwrap();
            c = Cache::default();
            c.load(audit.turso(), "2026-10-16", &[]).unwrap();
            let next = c
                .load(audit.turso(), "2026-10-19", &[a.clone(), b.clone(), ongoing.clone(), fresh.clone()])
                .unwrap();
            let flags = next.flags("101", &[json!(a.1), json!(b.1)], Some("2026-10-19"));
            assert_eq!(next.previous_day.as_deref(), Some("2026-10-16"));
            assert_eq!(flags[0]["since"], "2026-10-16");
            assert_eq!(flags[0]["new"], false);
            assert_eq!(flags[1]["since"], "2026-10-19");
            assert_eq!(flags[1]["new"], true);
            assert_eq!(flags, vec![
                json!({"key":"満了まで60日以内","label":"満了まで60日以内","new":false,"since":"2026-10-16"}),
                json!({"key":"NPSが4以下","label":"NPSが4以下","new":true,"since":"2026-10-19"}),
            ]);
            // The API carries every flag on one row per deal. Consumers classify the
            // entire row as new if any flag is new, never splitting mixed rows.
            let rows = [
                ("101", flags),
                ("103", next.flags("103", &[json!(ongoing.1)], Some("2026-10-19"))),
                ("104", next.flags("104", &[json!(fresh.1)], Some("2026-10-19"))),
            ];
            let (new_rows, ongoing_rows): (Vec<_>, Vec<_>) = rows.iter()
                .partition(|(_, flags)| flags.iter().any(|f| f["new"] == true));
            assert_eq!(new_rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec!["101", "104"]);
            assert_eq!(ongoing_rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec!["103"]);
            assert_eq!(new_rows[0].1.len(), 2);
            c.load(audit.turso(), "2026-10-20", &[]).unwrap();
            let after_empty = c.load(audit.turso(), "2026-10-21", &[a]).unwrap();
            assert_eq!(
                after_empty.flags("101", &[json!("満了まで60日以内")], Some("2026-10-21"))[0]["since"],
                "2026-10-21"
            );
            assert_eq!(
                conn.lock()
                    .unwrap()
                    .query_row(
                        "SELECT COUNT(*) FROM consulting_deal_flags WHERE data_day='2026-10-16'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                3
            );
            assert_eq!(
                queries
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|q| q.starts_with("INSERT OR IGNORE INTO consulting_deal_flags"))
                    .count(),
                4
            );
            conn.lock()
                .unwrap()
                .execute("DROP TABLE consulting_deal_flags", [])
                .unwrap();
            assert!(c.load(audit.turso(), "2026-10-22", &[]).is_err());
            let reads = queries.lock().unwrap().len();
            assert!(c.load(audit.turso(), "2026-10-22", &[]).is_err());
            assert_eq!(
                queries.lock().unwrap().len(),
                reads,
                "failed read is cached for 60 seconds"
            );
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn without_audit_no_comparison() {
        assert!(comparison(None, Some("2026-10-19".into()), vec![])
            .await
            .is_none());
    }
}
