//! コンサル支援 (採用仮説ブリーフ) HTTP ハンドラー
//!
//! - GET /consult/brief?session_id=...            → ブリーフHTML (社内用)
//! - GET /consult/evidence_pack.json?session_id=... → §15.2 形式 JSON
//!
//! どちらも既存 survey セッション (アップロード済CSVの集計キャッシュ) を入力とし、
//! 公的統計 (Turso) と企業データベースを読み取り専用で参照する。
//! DB書き込みは一切行わない。
//!
//! V2ルール: 介護データ・HW系テーブル (postings / hw_* / ts_turso_*) は入力に使わない。
//! 使用テーブルは公的統計 cross_* / 国勢調査OD / 企業データベースのみ。

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::evidence_pack::{analyze, to_evidence_pack_json, ConsultAnalysis};
use super::input::{ClientInput, CompanyObservation, ConsultInput};
use crate::handlers::helpers::{get_f64_opt, get_str};
use crate::AppState;

/// クエリパラメータ。数値項目も一旦文字列で受け、寛容にパースする
/// (不正値で 400 にせず None 扱いにする)。
#[derive(Debug, Deserialize, Default)]
pub struct ConsultQuery {
    pub session_id: Option<String>,
    /// 顧客提示給与 下限 (円)
    pub target_salary_min: Option<String>,
    /// 顧客提示給与 上限 (円)
    pub target_salary_max: Option<String>,
    /// 採用予定人数
    pub hiring_count: Option<String>,
    /// 採用期限 (自由記述)
    pub deadline: Option<String>,
    /// 事前メモ
    pub note: Option<String>,
}

fn parse_i64_opt(s: &Option<String>) -> Option<i64> {
    s.as_ref()
        .and_then(|v| v.trim().replace(',', "").parse::<i64>().ok())
        .filter(|v| *v > 0)
}

fn parse_u32_opt(s: &Option<String>) -> Option<u32> {
    s.as_ref()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|v| *v > 0)
}

/// 企業名の正規化 (法人格・空白除去)。企業データベース名寄せ用。
fn normalize_company_name(name: &str) -> String {
    let mut s = name.to_string();
    for legal in [
        "株式会社",
        "有限会社",
        "合同会社",
        "合資会社",
        "合名会社",
        "医療法人",
        "社会福祉法人",
        "一般社団法人",
        "(株)",
        "（株）",
        "(有)",
        "（有）",
    ] {
        s = s.replace(legal, "");
    }
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '　')
        .collect::<String>()
        .to_lowercase()
}

/// キャッシュとDBから ConsultInput を構築する (読み取りのみ)。
/// 戻り値 Err はユーザー向けエラーメッセージ。
async fn build_consult_input(
    state: &Arc<AppState>,
    session_id: &str,
    query: &ConsultQuery,
) -> Result<ConsultInput, String> {
    // 1) セッションキャッシュから集計を復元
    let agg_cached = state.cache.get(&format!("survey_agg_{}", session_id));
    let agg: crate::handlers::survey::aggregator::SurveyAggregation = match agg_cached {
        Some(v) => serde_json::from_value(v).unwrap_or_default(),
        None => {
            return Err("分析データが期限切れです。CSVを再アップロードしてください。".to_string())
        }
    };
    let pref = state
        .cache
        .get(&format!("survey_pref_{}", session_id))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let muni = state
        .cache
        .get(&format!("survey_muni_{}", session_id))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    if pref.is_empty() {
        return Err(
            "対象地域を特定できませんでした。CSVを再アップロードしてください。".to_string(),
        );
    }

    // 2) 今回CSV由来の指標
    let (salary_median, salary_q1, salary_q3, salary_n) = match &agg.enhanced_stats {
        Some(st) => {
            let (q1, q3) = st
                .quartiles
                .as_ref()
                .map(|q| (Some(q.q1), Some(q.q3)))
                .unwrap_or((None, None));
            (Some(st.median), q1, q3, st.count)
        }
        None => (None, None, None, 0),
    };
    // 時給モード時: 時給下限の中央値 (参考値)
    let hourly_median_low = if agg.is_hourly && !agg.salary_min_values_native.is_empty() {
        let mut v = agg.salary_min_values_native.clone();
        v.sort();
        Some(v[v.len() / 2])
    } else {
        None
    };

    let client = ClientInput {
        target_salary_min: parse_i64_opt(&query.target_salary_min),
        target_salary_max: parse_i64_opt(&query.target_salary_max),
        hiring_count: parse_u32_opt(&query.hiring_count),
        deadline: query.deadline.clone().filter(|s| !s.trim().is_empty()),
        note: query.note.clone().filter(|s| !s.trim().is_empty()),
    };

    let mut data_sources = vec!["今回の求人CSV集計".to_string()];

    // 掲載件数上位企業 (名寄せ前)
    let mut top_companies: Vec<CompanyObservation> = agg
        .by_company
        .iter()
        .take(super::config::COMPANY_MATCH_TOP_N)
        .map(|c| CompanyObservation {
            name: c.name.clone(),
            posting_count: c.count,
            employee_count: None,
            employee_delta_1y: None,
        })
        .collect();

    // 3) 公的統計 + 企業名寄せ (ブロッキングDB読み取りを spawn_blocking で実行)
    let turso = state.turso_db.clone();
    let salesnow = state.salesnow_db.clone();
    let hw_db = state.hw_db.clone();
    let pref2 = pref.clone();
    let muni2 = muni.clone();
    let companies_for_thread = top_companies.clone();

    #[derive(Default)]
    struct FetchedStats {
        scheduled_earnings_latest: Option<f64>,
        min_wage_hourly: Option<f64>,
        min_wage_monthly_160h: Option<f64>,
        job_openings_ratio: Option<f64>,
        job_change_desire_rate_pref: Option<f64>,
        job_change_desire_rate_national: Option<f64>,
        wa_decline_rate_muni: Option<f64>,
        commute_inflow_total: Option<i64>,
        commute_outflow_total: Option<i64>,
        commute_inflow_top3: Vec<(String, String, i64)>,
        companies: Vec<CompanyObservation>,
        sources: Vec<String>,
    }

    let fetched = tokio::task::spawn_blocking(move || {
        use crate::handlers::analysis::fetch as af;
        use crate::handlers::trend::fetch as tf;
        let mut out = FetchedStats {
            companies: companies_for_thread,
            ..Default::default()
        };

        // --- 公的統計 + 通勤OD の並列フェッチ ---
        // 2026-07-10: 従来は wage → switcher → future_workforce → commute を直列取得していた。
        //   互いに独立した Turso 往復のため std::thread::scope で並列化する。
        //   パース(集計)と data_sources の push は取得後に従来と同じ順序で直列実行し、
        //   出力の並び順を変えない。
        let (wage_rows, switcher_rows, wf_rows, inflow, outflow) = std::thread::scope(|s| {
            let h_wage = s.spawn(|| {
                turso
                    .as_ref()
                    .map(|t| tf::fetch_cross_wage_public(t, &pref2))
                    .unwrap_or_default()
            });
            let h_switcher = s.spawn(|| {
                turso
                    .as_ref()
                    .map(|t| tf::fetch_cross_switcher_supply(t, &pref2))
                    .unwrap_or_default()
            });
            let h_wf = s.spawn(|| {
                if muni2.is_empty() {
                    Vec::new()
                } else {
                    turso
                        .as_ref()
                        .map(|t| tf::fetch_cross_future_workforce(t, &pref2))
                        .unwrap_or_default()
                }
            });
            let h_commute = s.spawn(|| {
                if muni2.is_empty() {
                    (Vec::new(), Vec::new())
                } else if let Some(db) = hw_db.as_ref() {
                    (
                        af::fetch_commute_inflow(db, turso.as_ref(), &pref2, &muni2),
                        af::fetch_commute_outflow(db, turso.as_ref(), &pref2, &muni2),
                    )
                } else {
                    (Vec::new(), Vec::new())
                }
            });
            let wage = h_wage.join().unwrap_or_default();
            let switcher = h_switcher.join().unwrap_or_default();
            let wf = h_wf.join().unwrap_or_default();
            let (inflow, outflow) = h_commute.join().unwrap_or_default();
            (wage, switcher, wf, inflow, outflow)
        });

        if turso.is_some() {
            // 県の所定内給与・最低賃金 (毎月勤労統計 地方調査 / 地域別最低賃金)
            if let Some(latest) = wage_rows.last() {
                out.scheduled_earnings_latest = get_f64_opt(latest, "scheduled_earnings");
                out.min_wage_hourly = get_f64_opt(latest, "min_wage_hourly");
                out.min_wage_monthly_160h = get_f64_opt(latest, "min_wage_monthly_160h");
                out.sources
                    .push("毎月勤労統計 地方調査・地域別最低賃金".to_string());
            }

            // 転職希望率・有効求人倍率 (就業構造基本調査 / 一般職業紹介状況)
            for row in &switcher_rows {
                let code = get_str(row, "region_code");
                let name = get_str(row, "region_name");
                if code == "00000" {
                    out.job_change_desire_rate_national =
                        get_f64_opt(row, "job_change_desire_rate");
                } else if name == pref2 {
                    out.job_change_desire_rate_pref = get_f64_opt(row, "job_change_desire_rate");
                    out.job_openings_ratio = get_f64_opt(row, "pref_job_openings_ratio");
                }
            }
            if !switcher_rows.is_empty() {
                out.sources
                    .push("就業構造基本調査・一般職業紹介状況".to_string());
            }

            // 働き手の将来増減率 (将来人口推計、市区町村粒度)
            if !muni2.is_empty() {
                if let Some(row) = wf_rows.iter().find(|r| get_str(r, "municipality") == muni2) {
                    out.wa_decline_rate_muni = get_f64_opt(row, "wa_decline_rate");
                    out.sources
                        .push("国立社会保障・人口問題研究所 将来人口推計".to_string());
                }
            }
        }

        // 通勤OD (国勢調査。市区町村が特定できた場合のみ)
        if !muni2.is_empty() && (!inflow.is_empty() || !outflow.is_empty()) {
            out.commute_inflow_total = Some(inflow.iter().map(|f| f.total_commuters).sum());
            out.commute_outflow_total = Some(outflow.iter().map(|f| f.total_commuters).sum());
            out.commute_inflow_top3 = inflow
                .iter()
                .take(3)
                .map(|f| {
                    (
                        f.partner_pref.clone(),
                        f.partner_muni.clone(),
                        f.total_commuters,
                    )
                })
                .collect();
            out.sources.push("国勢調査 通勤・通学OD".to_string());
        }

        // 企業データベース名寄せ (掲載上位企業、各社独立に読み取り専用で並列化)
        // 2026-07-10: 従来は 5 社を直列で search+detail していた。各社は独立した企業DB往復のため
        //   std::thread::scope で並列化する。掲載順 (into_iter → join 順) を維持し、
        //   any_matched / sources push も従来と同一結果になる。
        if let Some(sn) = salesnow.as_ref() {
            use crate::handlers::company::fetch::{fetch_company_detail, search_companies};
            let companies = std::mem::take(&mut out.companies);
            let results: Vec<(CompanyObservation, bool)> = std::thread::scope(|s| {
                // NOTE: handles の collect は必須。全社を spawn してから join しないと
                //   lazy iterator が spawn→join を 1 社ずつ交互に行い直列化してしまう。
                #[allow(clippy::needless_collect)]
                let handles: Vec<_> = companies
                    .into_iter()
                    .map(|mut c| {
                        s.spawn(move || {
                            let normalized_target = normalize_company_name(&c.name);
                            if normalized_target.chars().count() < 2 {
                                return (c, false);
                            }
                            let candidates = search_companies(sn, &normalized_target);
                            let matched = candidates
                                .iter()
                                .find(|row| {
                                    normalize_company_name(&get_str(row, "company_name"))
                                        == normalized_target
                                })
                                .map(|row| get_str(row, "corporate_number"))
                                .filter(|cn| !cn.is_empty());
                            let mut matched_any = false;
                            if let Some(corporate_number) = matched {
                                if let Some(detail) = fetch_company_detail(sn, &corporate_number) {
                                    c.employee_count = get_f64_opt(&detail, "employee_count")
                                        .map(|v| v as i64)
                                        .filter(|v| *v > 0);
                                    c.employee_delta_1y = get_f64_opt(&detail, "employee_delta_1y");
                                    matched_any = true;
                                }
                            }
                            (c, matched_any)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("consult company match thread panicked"))
                    .collect()
            });
            let any_matched = results.iter().any(|(_, m)| *m);
            out.companies = results.into_iter().map(|(c, _)| c).collect();
            if any_matched {
                out.sources.push("企業データベース".to_string());
            }
        }

        out
    })
    .await
    .map_err(|_| "データ取得中にエラーが発生しました。".to_string())?;

    data_sources.extend(fetched.sources);
    top_companies = fetched.companies;

    let as_of = chrono::Local::now().format("%Y-%m-%d").to_string();

    Ok(ConsultInput {
        pref,
        muni,
        occupation_note: String::new(),
        as_of,
        data_sources,
        total_postings: agg.total_count,
        new_count: agg.new_count,
        is_hourly: agg.is_hourly,
        salary_values: agg.salary_values.clone(),
        salary_median,
        salary_q1,
        salary_q3,
        salary_n,
        hourly_median_low,
        // 掲載経過テキストは現行の集計に保持されていないため未取得 (シグナル側で明示)
        posting_age_30plus_ratio: None,
        company_count: agg.by_company.len(),
        companies: top_companies,
        scheduled_earnings_latest: fetched.scheduled_earnings_latest,
        min_wage_hourly: fetched.min_wage_hourly,
        min_wage_monthly_160h: fetched.min_wage_monthly_160h,
        job_openings_ratio: fetched.job_openings_ratio,
        job_change_desire_rate_pref: fetched.job_change_desire_rate_pref,
        job_change_desire_rate_national: fetched.job_change_desire_rate_national,
        wa_decline_rate_muni: fetched.wa_decline_rate_muni,
        commute_inflow_total: fetched.commute_inflow_total,
        commute_outflow_total: fetched.commute_outflow_total,
        commute_inflow_top3: fetched.commute_inflow_top3,
        client,
    })
}

async fn build_analysis(
    state: &Arc<AppState>,
    session_id: &str,
    query: &ConsultQuery,
) -> Result<ConsultAnalysis, String> {
    let input = build_consult_input(state, session_id, query).await?;
    Ok(analyze(&input))
}

fn error_html(msg: &str) -> Html<String> {
    Html(format!(
        "<html><body><p>{}</p></body></html>",
        crate::handlers::helpers::escape_html(msg)
    ))
}

/// GET /consult/brief — 採用仮説ブリーフHTML (社内用)
pub async fn consult_brief(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return error_html("セッションIDが必要です。CSVをアップロードしてください。");
        }
    };

    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_consult_brief",
        "consult",
        &session_id,
        "",
    )
    .await;

    match build_analysis(&state, &session_id, &query).await {
        Ok(analysis) => Html(super::brief_html::render_consult_brief_html(&analysis)),
        Err(msg) => error_html(&msg),
    }
}

/// GET /consult/evidence_pack.json — §15.2 形式の証拠データJSON
pub async fn consult_evidence_pack_json(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
) -> axum::response::Json<serde_json::Value> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return axum::response::Json(serde_json::json!({
                "error": "セッションIDが必要です。CSVをアップロードしてください。"
            }));
        }
    };

    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_consult_evidence_pack",
        "consult",
        &session_id,
        "",
    )
    .await;

    match build_analysis(&state, &session_id, &query).await {
        Ok(analysis) => axum::response::Json(to_evidence_pack_json(&analysis)),
        Err(msg) => axum::response::Json(serde_json::json!({ "error": msg })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_helpers_are_lenient() {
        assert_eq!(parse_i64_opt(&Some("300,000".to_string())), Some(300_000));
        assert_eq!(parse_i64_opt(&Some(" 250000 ".to_string())), Some(250_000));
        assert_eq!(parse_i64_opt(&Some("abc".to_string())), None);
        assert_eq!(parse_i64_opt(&Some("-100".to_string())), None);
        assert_eq!(parse_i64_opt(&None), None);
        assert_eq!(parse_u32_opt(&Some("3".to_string())), Some(3));
        assert_eq!(parse_u32_opt(&Some("0".to_string())), None);
    }

    #[test]
    fn normalize_company_name_strips_legal_forms() {
        assert_eq!(
            normalize_company_name("株式会社サンプル運輸"),
            normalize_company_name("サンプル運輸")
        );
        assert_eq!(
            normalize_company_name("（株）サンプル 運輸"),
            normalize_company_name("サンプル運輸")
        );
        assert_ne!(
            normalize_company_name("サンプル運輸"),
            normalize_company_name("サンプル物流")
        );
    }
}
