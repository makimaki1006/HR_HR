use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;
use crate::handlers::company::fetch;
use crate::handlers::helpers::{get_f64, get_i64, get_str};

#[derive(Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    20
}

#[derive(Deserialize)]
pub struct MarketParams {
    pub job_type: String,
    pub prefecture: String,
}

/// GET /api/v1/companies?q=xxx&limit=20
pub async fn search_companies(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<Value> {
    if params.q.trim().len() < 2 {
        return Json(json!({"error": "2文字以上入力してください", "results": [], "count": 0}));
    }
    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => return Json(json!({"error": "企業DB未接続", "results": [], "count": 0})),
    };
    let q = params.q.clone();
    let results =
        tokio::task::spawn_blocking(move || fetch::search_companies(&sn_db, &q))
            .await
            .unwrap_or_default();

    let items: Vec<Value> = results
        .iter()
        .take(params.limit)
        .map(|r| {
            json!({
                "corporate_number": get_str(r, "corporate_number"),
                "company_name": get_str(r, "company_name"),
                "prefecture": get_str(r, "prefecture"),
                "sn_industry": get_str(r, "sn_industry"),
                "sn_industry2": get_str(r, "sn_industry2"),
                "employee_count": get_i64(r, "employee_count"),
                "employee_range": get_str(r, "employee_range"),
                "sales_range": get_str(r, "sales_range"),
                "credit_score": get_f64(r, "credit_score"),
            })
        })
        .collect();
    let count = items.len();
    Json(json!({"results": items, "count": count}))
}

/// GET /api/v1/companies/{corporate_number}
pub async fn company_profile(
    State(state): State<Arc<AppState>>,
    Path(corporate_number): Path<String>,
) -> Json<Value> {
    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => return Json(json!({"error": "企業DB未接続"})),
    };
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Json(json!({"error": "求人DB未接続"})),
    };
    let ext_db = state.turso_db.clone();
    let corp = corporate_number.clone();
    let ctx = tokio::task::spawn_blocking(move || {
        fetch::build_company_context(&sn_db, ext_db.as_ref(), &db, &corp)
    })
    .await
    .unwrap_or(None);

    match ctx {
        Some(ctx) => Json(context_to_json(&ctx)),
        None => Json(json!({"error": "企業が見つかりません"})),
    }
}

/// GET /api/v1/companies/{corporate_number}/nearby
pub async fn nearby_companies(
    State(state): State<Arc<AppState>>,
    Path(corporate_number): Path<String>,
) -> Json<Value> {
    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => return Json(json!({"error": "企業DB未接続"})),
    };
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Json(json!({"error": "求人DB未接続"})),
    };
    let corp = corporate_number.clone();
    let result = tokio::task::spawn_blocking(move || {
        let detail = fetch::fetch_company_detail(&sn_db, &corp)?;
        let postal = get_str(&detail, "postal_code");
        let pref = get_str(&detail, "prefecture");
        if postal.is_empty() {
            return None;
        }
        let nearby =
            fetch::fetch_nearby_companies(&sn_db, &db, &postal, &corp, &pref);
        Some((postal, nearby))
    })
    .await
    .unwrap_or(None);

    match result {
        Some((postal, nearby)) => {
            let prefix = if postal.len() >= 3 {
                &postal[..3]
            } else {
                &postal
            };
            let items: Vec<Value> = nearby
                .iter()
                .map(|nc| {
                    json!({
                        "corporate_number": nc.corporate_number,
                        "company_name": nc.company_name,
                        "prefecture": nc.prefecture,
                        "sn_industry": nc.sn_industry,
                        "employee_count": nc.employee_count,
                        "credit_score": nc.credit_score,
                        "postal_code": nc.postal_code,
                        "hw_posting_count": nc.hw_posting_count,
                    })
                })
                .collect();
            let count = items.len();
            Json(json!({"postal_prefix": prefix, "companies": items, "count": count}))
        }
        None => Json(json!({"error": "企業が見つかりません", "companies": []})),
    }
}

/// GET /api/v1/companies/{corporate_number}/postings
pub async fn company_postings(
    State(state): State<Arc<AppState>>,
    Path(corporate_number): Path<String>,
) -> Json<Value> {
    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => return Json(json!({"error": "企業DB未接続"})),
    };
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Json(json!({"error": "求人DB未接続"})),
    };
    let corp = corporate_number.clone();
    let result = tokio::task::spawn_blocking(move || {
        let detail = fetch::fetch_company_detail(&sn_db, &corp)?;
        let name = get_str(&detail, "company_name");
        let pref = get_str(&detail, "prefecture");
        let total_count = fetch::count_hw_postings(&db, &name, &pref);
        let postings = fetch::fetch_hw_postings_for_company(&db, &name, &pref);
        Some((name, total_count, postings))
    })
    .await
    .unwrap_or(None);

    match result {
        Some((name, total_count, postings)) => {
            let items: Vec<Value> = postings
                .iter()
                .map(|r| {
                    json!({
                        "facility_name": get_str(r, "facility_name"),
                        "job_type": get_str(r, "job_type"),
                        "employment_type": get_str(r, "employment_type"),
                        "salary_type": get_str(r, "salary_type"),
                        "salary_min": get_i64(r, "salary_min"),
                        "salary_max": get_i64(r, "salary_max"),
                        "headline": get_str(r, "headline"),
                        "municipality": get_str(r, "municipality"),
                    })
                })
                .collect();
            Json(json!({"company_name": name, "postings": items, "count": total_count, "shown": items.len()}))
        }
        None => Json(json!({"error": "企業が見つかりません", "postings": []})),
    }
}

/// CompanyContext -> JSON変換
fn context_to_json(ctx: &fetch::CompanyContext) -> Value {
    let hw_postings: Vec<Value> = ctx
        .hw_matched_postings
        .iter()
        .map(|r| {
            json!({
                "facility_name": get_str(r, "facility_name"),
                "job_type": get_str(r, "job_type"),
                "employment_type": get_str(r, "employment_type"),
                "salary_min": get_i64(r, "salary_min"),
                "salary_max": get_i64(r, "salary_max"),
                "headline": get_str(r, "headline"),
                "municipality": get_str(r, "municipality"),
            })
        })
        .collect();

    let nearby: Vec<Value> = ctx
        .nearby_companies
        .iter()
        .map(|nc| {
            json!({
                "corporate_number": nc.corporate_number,
                "company_name": nc.company_name,
                "sn_industry": nc.sn_industry,
                "employee_count": nc.employee_count,
                "hw_posting_count": nc.hw_posting_count,
            })
        })
        .collect();

    let pitches: Vec<Value> = ctx.sales_pitches.iter()
        .map(|(h, b)| json!({"headline": h, "body": b}))
        .collect();

    json!({
        "company": {
            "corporate_number": ctx.corporate_number,
            "company_name": ctx.company_name,
            "employee_count": ctx.employee_count,
            "employee_range": ctx.employee_range,
            "employee_delta_1y": ctx.employee_delta_1y,
            "sales_range": ctx.sales_range,
            "sn_industry": ctx.sn_industry,
            "sn_industry2": ctx.sn_industry2,
            "prefecture": ctx.prefecture,
            "address": ctx.address,
            "postal_code": ctx.postal_code,
            "credit_score": ctx.credit_score,
        },
        "industry_mapping": {
            "hw_job_types": ctx.hw_job_types.iter().map(|(jt, conf)| json!({"job_type": jt, "confidence": conf})).collect::<Vec<_>>(),
            "primary_hw_job_type": ctx.primary_hw_job_type,
        },
        "market": {
            "posting_count": ctx.market_posting_count,
            "facility_count": ctx.market_facility_count,
            "avg_salary_min": ctx.market_avg_salary_min,
            "avg_salary_max": ctx.market_avg_salary_max,
            "fulltime_rate": ctx.market_fulltime_rate,
            "vacancy_rate": ctx.market_vacancy_rate,
            "salary_distribution": ctx.salary_distribution.iter().map(|(b,c)| json!({"band": b, "count": c})).collect::<Vec<_>>(),
            "recruitment_reasons": ctx.recruitment_reasons.iter().map(|(r,c)| json!({"reason": r, "count": c})).collect::<Vec<_>>(),
            "benefit_rates": ctx.benefit_rates.iter().map(|(b,r)| json!({"benefit": b, "rate": r})).collect::<Vec<_>>(),
            "national_avg_salary": ctx.national_avg_salary,
        },
        "region": {
            "population": ctx.population,
            "daytime_ratio": ctx.daytime_ratio,
            "aging_rate": ctx.aging_rate,
        },
        "cross_analysis": {
            "region_industry": {
                "total_employees": ctx.region_industry_total_employees,
                "net_change": ctx.region_industry_net_change,
                "avg_delta": ctx.region_industry_avg_delta,
                "company_count": ctx.region_industry_company_count,
            },
            "company_vs_region_gap": ctx.company_vs_region_gap,
            "company_salary": {
                "avg_salary_min": ctx.company_avg_salary_min,
                "salary_count": ctx.company_salary_count,
                "salary_percentile": ctx.salary_percentile,
            },
            "growth_signal": ctx.growth_signal,
            "growth_postings_count": ctx.growth_postings_count,
            "replacement_postings_count": ctx.replacement_postings_count,
            "hiring_risk": {
                "score": ctx.hiring_risk_score,
                "grade": ctx.hiring_risk_grade,
            },
            "sales_pitches": pitches,
        },
        "hw_postings": hw_postings,
        "nearby_companies": nearby,
    })
}
