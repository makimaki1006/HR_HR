//! Internal market context uses the existing Indeed store and job-copy authorization.
use super::job_copy_live::{authorized_user, Access, ReadError};
use crate::{indeed::detail::PrefSeries, AppState};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MarketQuery {
    title: Option<String>,
    prefecture: Option<String>,
}

fn unavailable() -> ReadError {
    ReadError(StatusCode::SERVICE_UNAVAILABLE, "market_data_unavailable")
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<MarketQuery>,
) -> Result<([(header::HeaderName, &'static str); 1], Json<Value>), ReadError> {
    authorized_user(&state, &access, &session).await?;
    let db = state.indeed_db.as_ref().ok_or_else(unavailable)?;
    let options = db
        .query(
            "SELECT DISTINCT norm_title FROM insight_title_pref ORDER BY norm_title",
            &[],
        )
        .map_err(|_| unavailable())?;
    let titles: Vec<String> = options
        .iter()
        .map(|row| crate::handlers::helpers::get_str(row, "norm_title"))
        .collect();
    let prefs = db
        .query(
            "SELECT DISTINCT prefecture FROM insight_title_pref ORDER BY prefecture",
            &[],
        )
        .map_err(|_| unavailable())?;
    let prefectures: Vec<String> = prefs
        .iter()
        .map(|row| crate::handlers::helpers::get_str(row, "prefecture"))
        .collect();
    let series = match (&query.title, &query.prefecture) {
        (None, None) => Value::Null,
        (Some(title), Some(pref)) if titles.contains(title) && prefectures.contains(pref) => {
            let series =
                crate::indeed::detail::pref_series(db, title, pref).map_err(|_| unavailable())?;
            series
                .map(|series| series_json(&series))
                .unwrap_or(Value::Null)
        }
        _ => return Err(ReadError(StatusCode::BAD_REQUEST, "market_scope_invalid")),
    };
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "source":"Indeed 採用市場レポート（求人企業向け）", "titles":titles,
            "prefectures":prefectures, "series":series, "scope":"internal",
            "ctk_basis":"Indeed上の行動データで労働市場全体ではありません。既存市場レポートのctk_countで、応募者数やHRハッカーのクリック数ではありません。上流の計測定義は別途確認が必要です。"
        })),
    ))
}

fn series_json(series: &PrefSeries) -> Value {
    json!({"prefecture":series.prefecture,"months":series.months,"job_count":series.job,
        "ctk_count":series.ctk,"employer_count":series.employers,"seekers_per_posting":series.spp})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn market_series_preserves_month_scope_missing_and_zero_without_applicant_claims() {
        let series = PrefSeries {
            prefecture: "Synthetic prefecture".into(),
            months: vec!["2026-08".into(), "2026-09".into()],
            job: vec![Some(100.0), None],
            ctk: vec![Some(300.0), Some(0.0)],
            employers: vec![Some(20.0), None],
            spp: vec![Some(3.0), None],
            ..Default::default()
        };
        let value = series_json(&series);
        assert_eq!(value["months"][0], "2026-08");
        assert_eq!(value["ctk_count"][0], 300.0);
        assert_eq!(value["ctk_count"][1], 0.0);
        assert!(value["job_count"][1].is_null());
        assert!(value.get("applications").is_none());
    }
}
