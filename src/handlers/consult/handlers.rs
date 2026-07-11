//! コンサル支援 (商談準備レポート) HTTP ハンドラー (内部ルートは /consult/brief のまま)
//!
//! - GET /consult/brief?session_id=...            → 商談準備レポートHTML (社内用)
//! - GET /consult/evidence_pack.json?session_id=... → §15.2 形式 JSON
//!
//! どちらも既存 survey セッション (アップロード済CSVの集計キャッシュ) を入力とし、
//! 公的統計 (Turso) と企業データベースを読み取り専用で参照する。
//! DB書き込みは一切行わない。
//!
//! V2ルール: 介護データ・HW求人 (求人スクレイピング・時系列) は入力に使わない。
//! 使用テーブルは公的統計 (cross_* / v2_external_* / 国勢調査OD) と企業データベースのみ。

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

    // 2-b) 媒体CSV観測の拡充フィールド (SurveyAggregation。§5.0 に沿い「観測できた記載」として扱う)
    let distinct_tag_count = agg.by_tags.len();
    let top_tags: Vec<(String, usize)> = agg.by_tags.iter().take(8).cloned().collect();
    let popular_ratio = if agg.popularity.indeed_sp_total > 0 {
        Some(agg.popularity.popular_ratio)
    } else {
        None
    };
    let super_popular_count = agg.popularity.super_popular_count;
    let (annual_holidays_median, annual_holidays_n) = {
        let mut hv = agg.jobbox.annual_holidays_values.clone();
        let n = hv.len();
        if n == 0 {
            (None, 0)
        } else {
            hv.sort();
            (Some(hv[n / 2]), n)
        }
    };
    let holiday_pct_ge_120 = if annual_holidays_n > 0 {
        Some(agg.jobbox.holiday_pct_ge_120)
    } else {
        None
    };
    let employment_type_dist: Vec<(String, usize)> = agg.by_employment_type.clone();
    let muni_dist_top: Vec<(String, usize)> = agg
        .by_municipality_salary
        .iter()
        .take(8)
        .map(|m| (m.name.clone(), m.count))
        .collect();

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
        // 拡充 (2026-07-10): 公的統計 (v2_external_*。介護・HW は一切含まない)
        net_migration_rate: Option<f64>,
        daytime_ratio: Option<f64>,
        business_opening_rate: Option<f64>,
        business_closure_rate: Option<f64>,
        // P0-1: 開廃業率の調査間隔 (最新年度 - 前年度)。年換算に使う。
        business_dynamics_interval_years: Option<f64>,
        unemployment_rate_pref: Option<f64>,
        unemployment_rate_national: Option<f64>,
        natural_change: Option<i64>,
        // P0-2: 1畳あたり家賃 (総数/総数 の median_rent_jpy)。月額家賃ではない。
        rent_per_tatami: Option<i64>,
        rent_per_tatami_national: Option<i64>,
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
        type Ext2Bundle = (
            Vec<crate::handlers::helpers::Row>,
            Vec<crate::handlers::helpers::Row>,
            Vec<crate::handlers::helpers::Row>,
            Vec<crate::handlers::helpers::Row>,
            Vec<crate::handlers::helpers::Row>,
            Vec<crate::handlers::helpers::Row>,
            Vec<crate::handlers::helpers::Row>,
        );
        let (wage_rows, switcher_rows, wf_rows, inflow, outflow, ext2) = std::thread::scope(|s| {
            let h_wage = s.spawn(|| {
                turso
                    .as_ref()
                    .map(|t| tf::fetch_cross_wage_public(t, &pref2))
                    .unwrap_or_default()
            });
            // 拡充 (2026-07-10): 公的統計 (人口移動・昼夜間人口・開廃業・失業率・人口動態・家賃)。
            //   すべて v2_external_* (国勢調査・経済センサス・住宅土地統計・人口動態統計等) で、
            //   公的統計のみ。介護需要やHW求人データは一切含まない。
            let h_ext2 = s.spawn(|| -> Ext2Bundle {
                if let Some(db) = hw_db.as_ref() {
                    let t = turso.as_ref();
                    (
                        af::fetch_migration_data(db, t, &pref2, &muni2),
                        af::fetch_daytime_population(db, t, &pref2, &muni2),
                        af::fetch_business_dynamics(db, t, &pref2),
                        af::fetch_labor_force(db, t, &pref2, ""),
                        af::fetch_labor_force(db, t, "", ""),
                        af::fetch_vital_statistics(db, t, &pref2, &muni2),
                        af::fetch_rental_housing(db, t, &pref2),
                    )
                } else {
                    Ext2Bundle::default()
                }
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
            let ext2 = h_ext2.join().unwrap_or_default();
            (wage, switcher, wf, inflow, outflow, ext2)
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

        // 拡充公的統計のパース (v2_external_*。turso-or-local。空なら None のまま)
        {
            let (migration, daytime, biz, lf_pref, lf_nat, vital, rental) = &ext2;

            if let Some(row) = migration.first() {
                out.net_migration_rate = get_f64_opt(row, "net_migration_rate");
                if out.net_migration_rate.is_some() {
                    out.sources.push("住民基本台帳人口移動報告".to_string());
                }
            }
            if let Some(row) = daytime.first() {
                out.daytime_ratio = get_f64_opt(row, "day_night_ratio");
                if out.daytime_ratio.is_some() {
                    out.sources.push("国勢調査 従業地・通学地集計".to_string());
                }
            }
            // 開廃業は最新年度 (fiscal_year 昇順のため last)。
            // 🔴 P0-1: opening_rate/closure_rate は経済センサス調査間の累計。年換算のため
            //   「最新年度 - その1つ前の年度」を調査間隔として算出する (前年度行が無ければ None)。
            if let Some(row) = biz.last() {
                out.business_opening_rate = get_f64_opt(row, "opening_rate");
                out.business_closure_rate = get_f64_opt(row, "closure_rate");
                // fiscal_year は文字列 ("2021" 等) のことがあるため get_str → parse で堅牢に読む
                let year_of = |r: &crate::handlers::helpers::Row| -> Option<f64> {
                    let s = get_str(r, "fiscal_year");
                    s.trim().parse::<f64>().ok()
                };
                if let (Some(latest_y), Some(prev_row)) = (year_of(row), biz.iter().rev().nth(1)) {
                    if let Some(prev_y) = year_of(prev_row) {
                        let interval = latest_y - prev_y;
                        if interval > 0.0 {
                            out.business_dynamics_interval_years = Some(interval);
                        }
                    }
                }
                if out.business_opening_rate.is_some() || out.business_closure_rate.is_some() {
                    out.sources.push("経済センサス 開廃業".to_string());
                }
            }
            if let Some(row) = lf_pref.first() {
                out.unemployment_rate_pref = get_f64_opt(row, "unemployment_rate");
            }
            if let Some(row) = lf_nat.first() {
                out.unemployment_rate_national = get_f64_opt(row, "unemployment_rate");
            }
            if out.unemployment_rate_pref.is_some() || out.unemployment_rate_national.is_some() {
                out.sources.push("国勢調査 労働力状態".to_string());
            }
            if let Some(row) = vital.first() {
                out.natural_change = get_f64_opt(row, "natural_change").map(|v| v as i64);
                if out.natural_change.is_some() {
                    out.sources.push("人口動態統計".to_string());
                }
            }
            // 1畳あたり家賃: 🔴 P0-2 median_rent_jpy の実体は「1畳あたり家賃」であり月額ではない。
            //   従来は全構造・全市区町村 (197行) を平均していたため意味のない値 (721円) になっていた。
            //   代表値は「県全体 (municipality 空) の 総数/総数」1行の median_rent_jpy を使う。
            //   同様に全国 (prefecture=全国) の 総数/総数 を相対位置の基準として取得する。
            let pick_total = |target_pref: &str| -> Option<i64> {
                rental
                    .iter()
                    .find(|r| {
                        get_str(r, "prefecture") == target_pref
                            && get_str(r, "municipality").trim().is_empty()
                            && get_str(r, "structure") == "総数"
                            && get_str(r, "area_class") == "総数"
                    })
                    .and_then(|r| get_f64_opt(r, "median_rent_jpy"))
                    .filter(|v| *v > 0.0)
                    .map(|v| v.round() as i64)
            };
            out.rent_per_tatami = pick_total(&pref2);
            out.rent_per_tatami_national = pick_total("全国");
            if out.rent_per_tatami.is_some() || out.rent_per_tatami_national.is_some() {
                out.sources.push("住宅・土地統計".to_string());
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
        // 拡充公的統計
        net_migration_rate: fetched.net_migration_rate,
        daytime_ratio: fetched.daytime_ratio,
        business_opening_rate: fetched.business_opening_rate,
        business_closure_rate: fetched.business_closure_rate,
        business_dynamics_interval_years: fetched.business_dynamics_interval_years,
        unemployment_rate_pref: fetched.unemployment_rate_pref,
        unemployment_rate_national: fetched.unemployment_rate_national,
        natural_change: fetched.natural_change,
        rent_per_tatami: fetched.rent_per_tatami,
        rent_per_tatami_national: fetched.rent_per_tatami_national,
        // 拡充媒体CSV観測
        distinct_tag_count,
        top_tags,
        popular_ratio,
        super_popular_count,
        annual_holidays_median,
        annual_holidays_n,
        holiday_pct_ge_120,
        employment_type_dist,
        muni_dist_top,
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

/// GET /consult/brief — 商談準備レポートHTML (社内用。内部ルートは /consult/brief のまま)
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
        Ok(analysis) => {
            // AI文章化 (Gemini)。キー未設定・失敗・全破棄は空 AiComposite で graceful degradation。
            // 入力は evidence_pack JSON のみ (原データへは渡さない §15.2)。呼び出しは最大2回。
            let ai = match crate::gemini::GeminiClient::from_env() {
                Some(client) => super::ai::generate_ai_composite(&client, &analysis).await,
                None => super::ai::AiComposite::default(),
            };
            Html(super::brief_html::render_consult_brief_html_with_ai(
                &analysis, &ai,
            ))
        }
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
        Ok(analysis) => {
            let mut pack = to_evidence_pack_json(&analysis);
            // フェーズC: 保存済みヒアリング最新回答を hearing キーとして含める (§13 → フェーズD入力)。
            // 回答が無ければ省略する。ローカル SQLite からの読み取りのみ。
            if let Some(db) = state.hw_db.as_ref() {
                if let Some(hearing) = super::hearing::hearing_json_for_pack(db, &session_id) {
                    if let Some(obj) = pack.as_object_mut() {
                        obj.insert("hearing".to_string(), hearing);
                    }
                }
            }
            axum::response::Json(pack)
        }
        Err(msg) => axum::response::Json(serde_json::json!({ "error": msg })),
    }
}

// =============================================================================
// フェーズC: ヒアリングシート (印刷用) + Web 入力フォーム + 回答保存
// =============================================================================
//
// - GET  /consult/hearing_sheet?session_id=... → 印刷用ヒアリングシート HTML
// - GET  /consult/hearing?session_id=...       → 入力フォーム (最新回答をプリフィル)
// - POST /consult/hearing?session_id=...        → 回答を追記保存 → 保存済み表示
//
// 保存先はローカル SQLite (hellowork.db) の consult_hearing_results テーブルのみ。
// Turso には一切書き込まない。追記オンリー (UPDATE しない)。最新 revision が現在値。

use axum::response::{IntoResponse, Redirect};
use axum::Form;
use std::collections::BTreeMap;

/// セッションキャッシュから対象地域文字列 (「県 市」) を組み立てる。無ければ空。
fn region_from_cache(state: &Arc<AppState>, session_id: &str) -> String {
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
    match (pref.is_empty(), muni.is_empty()) {
        (true, _) => String::new(),
        (false, true) => pref,
        (false, false) => format!("{pref} {muni}"),
    }
}

/// GET /consult/hearing_sheet — 印刷用ヒアリングシート (社内用)
pub async fn consult_hearing_sheet(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return error_html("セッションIDが必要です。CSVをアップロードしてください。"),
    };
    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_consult_hearing_sheet",
        "consult",
        &session_id,
        "",
    )
    .await;

    let region = region_from_cache(&state, &session_id);
    let as_of = chrono::Local::now().format("%Y-%m-%d").to_string();
    Html(super::hearing::hearing_sheet_html(&region, &as_of))
}

/// GET /consult/hearing — 入力フォーム (最新回答をプリフィル)
pub async fn consult_hearing_form(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return error_html("セッションIDが必要です。CSVをアップロードしてください。"),
    };
    crate::audit::record_event(
        &state.audit,
        &session,
        "view_consult_hearing_form",
        "consult",
        &session_id,
        "",
    )
    .await;

    let region = region_from_cache(&state, &session_id);
    let (answers, history) = match state.hw_db.as_ref() {
        Some(db) => {
            let answers = super::hearing::latest_result(db, &session_id)
                .map(|s| super::hearing::answers_from_json(&s.answers_json))
                .unwrap_or_default();
            (answers, super::hearing::revision_history(db, &session_id))
        }
        None => (BTreeMap::new(), Vec::new()),
    };

    Html(super::hearing::hearing_form_html(
        &session_id,
        &region,
        &answers,
        false,
        None,
        &history,
    ))
}

/// POST /consult/hearing — 回答を追記保存し、保存済みフォームを返す
pub async fn consult_hearing_save(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
    Form(form): Form<BTreeMap<String, String>>,
) -> axum::response::Response {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return error_html("セッションIDが必要です。CSVをアップロードしてください。")
                .into_response()
        }
    };

    let Some(db) = state.hw_db.clone() else {
        return error_html("回答を保存できませんでした（ローカルデータベースに接続できません）。")
            .into_response();
    };

    let answers = super::hearing::answers_from_form(&form);
    // 全項目が空なら保存せずフォームへ戻す (空 revision を作らない)
    if answers.is_empty() {
        return Redirect::to(&format!("/consult/hearing?session_id={session_id}")).into_response();
    }
    let answers_json = serde_json::to_string(&answers).unwrap_or_else(|_| "{}".to_string());
    let created_at = chrono::Local::now().to_rfc3339();

    // ブロッキング SQLite 書き込みは spawn_blocking で
    let sid = session_id.clone();
    let saved = tokio::task::spawn_blocking(move || {
        super::hearing::insert_result(&db, &sid, &answers_json, &created_at)
    })
    .await;

    let saved_revision = match saved {
        Ok(Ok(rev)) => Some(rev),
        _ => None,
    };

    crate::audit::record_event(
        &state.audit,
        &session,
        "save_consult_hearing",
        "consult",
        &session_id,
        &saved_revision.map(|r| r.to_string()).unwrap_or_default(),
    )
    .await;

    if saved_revision.is_none() {
        return error_html("回答の保存に失敗しました。時間をおいて再度お試しください。")
            .into_response();
    }

    // 保存後: 最新回答をプリフィルして返す
    let region = region_from_cache(&state, &session_id);
    let (answers, history) = match state.hw_db.as_ref() {
        Some(db) => {
            let answers = super::hearing::latest_result(db, &session_id)
                .map(|s| super::hearing::answers_from_json(&s.answers_json))
                .unwrap_or_default();
            (answers, super::hearing::revision_history(db, &session_id))
        }
        None => (BTreeMap::new(), Vec::new()),
    };

    Html(super::hearing::hearing_form_html(
        &session_id,
        &region,
        &answers,
        true,
        saved_revision,
        &history,
    ))
    .into_response()
}

// =============================================================================
// フェーズD (2026-07-11): ヒアリング後の仮説更新 + 個社別アクションメモ
// =============================================================================
//
// - GET  /consult/hypothesis_review?session_id=... → 仮説更新画面 (支持/否定/保留 + 自動提案)
// - POST /consult/hypothesis_review?session_id=...  → 更新を追記保存 → 保存済み表示
// - GET  /consult/action_memo?session_id=...        → 個社別アクションメモ (顧客共有可)
//
// 保存先はローカル SQLite の consult_hypothesis_reviews のみ (追記オンリー)。
// 仮説一覧は面談前分析 (build_analysis) を再生成して得る (§24-1 決定的)。

/// GET /consult/hypothesis_review — 仮説更新画面 (最新レビューをプリフィル)
pub async fn consult_hypothesis_review_form(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return error_html("セッションIDが必要です。CSVをアップロードしてください。"),
    };
    crate::audit::record_event(
        &state.audit,
        &session,
        "view_consult_hypothesis_review",
        "consult",
        &session_id,
        "",
    )
    .await;

    let analysis = match build_analysis(&state, &session_id, &query).await {
        Ok(a) => a,
        Err(msg) => return error_html(&msg),
    };
    let region = region_from_cache(&state, &session_id);

    // 最新ヒアリング回答 (自動提案の材料) と、既存の仮説更新
    let (_answers, has_hearing, current, history) =
        load_review_state(&state, &session_id, &analysis);

    Html(super::hypothesis_review::hypothesis_review_html(
        &session_id,
        &region,
        &analysis,
        &current,
        has_hearing,
        false,
        None,
        &history,
    ))
}

/// POST /consult/hypothesis_review — 更新を追記保存し、保存済み画面を返す
pub async fn consult_hypothesis_review_save(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
    Form(form): Form<BTreeMap<String, String>>,
) -> axum::response::Response {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return error_html("セッションIDが必要です。CSVをアップロードしてください。")
                .into_response()
        }
    };
    let Some(db) = state.hw_db.clone() else {
        return error_html("更新を保存できませんでした（ローカルデータベースに接続できません）。")
            .into_response();
    };

    let analysis = match build_analysis(&state, &session_id, &query).await {
        Ok(a) => a,
        Err(msg) => return error_html(&msg).into_response(),
    };

    // 自動提案の材料としての最新ヒアリング回答
    let answers = latest_hearing_answers(&state, &session_id);
    let reviews = super::hypothesis_review::reviews_from_form(&analysis, &answers, &form);
    let reviews_json = serde_json::to_string(&reviews).unwrap_or_else(|_| "[]".to_string());
    let created_at = chrono::Local::now().to_rfc3339();

    let sid = session_id.clone();
    let saved = tokio::task::spawn_blocking(move || {
        super::hypothesis_review::insert_reviews(&db, &sid, &reviews_json, &created_at)
    })
    .await;
    let saved_revision = match saved {
        Ok(Ok(rev)) => Some(rev),
        _ => None,
    };

    crate::audit::record_event(
        &state.audit,
        &session,
        "save_consult_hypothesis_review",
        "consult",
        &session_id,
        &saved_revision.map(|r| r.to_string()).unwrap_or_default(),
    )
    .await;

    if saved_revision.is_none() {
        return error_html("更新の保存に失敗しました。時間をおいて再度お試しください。")
            .into_response();
    }

    let region = region_from_cache(&state, &session_id);
    let (_answers, has_hearing, current, history) =
        load_review_state(&state, &session_id, &analysis);

    Html(super::hypothesis_review::hypothesis_review_html(
        &session_id,
        &region,
        &analysis,
        &current,
        has_hearing,
        true,
        saved_revision,
        &history,
    ))
    .into_response()
}

/// GET /consult/action_memo — 個社別アクションメモ (顧客共有可。§14)
pub async fn consult_action_memo(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ConsultQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return error_html("セッションIDが必要です。CSVをアップロードしてください。"),
    };
    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_consult_action_memo",
        "consult",
        &session_id,
        "",
    )
    .await;

    let region = region_from_cache(&state, &session_id);

    // §14.1 作成条件: 最新ヒアリング回答が無ければ案内を返す
    let answers = latest_hearing_answers(&state, &session_id);
    if answers.is_empty() {
        return Html(super::action_memo::action_memo_needs_hearing_html(
            &region,
            &session_id,
        ));
    }

    let analysis = match build_analysis(&state, &session_id, &query).await {
        Ok(a) => a,
        Err(msg) => return error_html(&msg),
    };

    // 仮説更新 (無ければ自動提案を初期値に)
    let reviews = latest_reviews_or_auto(&state, &session_id, &analysis, &answers);

    let as_of = chrono::Local::now().format("%Y-%m-%d").to_string();
    Html(super::action_memo::action_memo_html(
        &analysis, &answers, &reviews, &region, &as_of,
    ))
}

/// 最新ヒアリング回答マップを取得 (無ければ空)。
fn latest_hearing_answers(
    state: &Arc<AppState>,
    session_id: &str,
) -> BTreeMap<String, super::hearing::AnswerValue> {
    match state.hw_db.as_ref() {
        Some(db) => super::hearing::latest_result(db, session_id)
            .map(|s| super::hearing::answers_from_json(&s.answers_json))
            .unwrap_or_default(),
        None => BTreeMap::new(),
    }
}

/// 最新の仮説更新を取得。無ければ自動提案を初期値にする。
fn latest_reviews_or_auto(
    state: &Arc<AppState>,
    session_id: &str,
    analysis: &ConsultAnalysis,
    answers: &BTreeMap<String, super::hearing::AnswerValue>,
) -> Vec<super::hypothesis_review::HypothesisReview> {
    match state.hw_db.as_ref() {
        Some(db) => match super::hypothesis_review::latest_reviews(db, session_id) {
            Some(stored) => super::hypothesis_review::reviews_from_json(&stored.reviews_json),
            None => super::hypothesis_review::auto_suggest_all(analysis, answers),
        },
        None => super::hypothesis_review::auto_suggest_all(analysis, answers),
    }
}

/// 仮説更新画面の状態 (回答・ヒアリング有無・現在のレビュー・履歴) をまとめて取得。
fn load_review_state(
    state: &Arc<AppState>,
    session_id: &str,
    analysis: &ConsultAnalysis,
) -> (
    BTreeMap<String, super::hearing::AnswerValue>,
    bool,
    Vec<super::hypothesis_review::HypothesisReview>,
    Vec<(i64, String)>,
) {
    let answers = latest_hearing_answers(state, session_id);
    let has_hearing = !answers.is_empty();
    let (current, history) = match state.hw_db.as_ref() {
        Some(db) => {
            let current = match super::hypothesis_review::latest_reviews(db, session_id) {
                Some(stored) => super::hypothesis_review::reviews_from_json(&stored.reviews_json),
                None => super::hypothesis_review::auto_suggest_all(analysis, &answers),
            };
            (
                current,
                super::hypothesis_review::revision_history(db, session_id),
            )
        }
        None => (
            super::hypothesis_review::auto_suggest_all(analysis, &answers),
            Vec::new(),
        ),
    };
    (answers, has_hearing, current, history)
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
