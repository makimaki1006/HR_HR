//! 地域カルテ: RESAS的統合ビュー
//!
//! # ルート
//! - `GET /tab/region_karte` → HTMLレンダリング（HTMX swap target）
//! - `GET /api/region/karte/{citycode}` → JSON集約（将来の直接アクセス用、現状は後方互換目的）
//!
//! # データソース
//! 1. HW postings (ローカルSQLite) - 求人数、雇用形態、給与
//! 2. v2_external_* (Turso) - SSDSE-A Phase A 7テーブル
//! 3. v2_flow_* (Turso) - Agoop Phase B 人流
//! 4. insight engine - 22+16 パターンの So What 示唆
//!
//! # セクション構成
//! - S1: 構造KPI 9枚
//! - S2: 人口動態（ピラミッド、世帯、自然動態）
//! - S3: 産業・労働
//! - S4: 医療・教育・福祉
//! - S5: 人流パターン（Agoop）
//! - S6: So What 示唆（insight engine 結果）
//! - S7: 印刷・共有
//!
//! # 安全性
//! - ECharts tooltip formatter: 事前計算文字列埋め込み（関数文字列変換を避ける）
//! - 全HTML出力は escape_html 済み
//! - Turso書込なし（SELECTのみ）

use axum::extract::{Path, Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

use crate::handlers::analysis::fetch as af;
use crate::handlers::helpers::{escape_html, format_number, get_f64, get_i64, get_str, Row};
use crate::handlers::insight::engine::generate_insights;
use crate::handlers::insight::fetch as ifetch;
use crate::handlers::insight::helpers::{Insight, InsightCategory};
use crate::handlers::jobmap::{flow as fflow, fromto as fft};
use crate::handlers::overview::{get_session_filters, make_location_label};
use crate::AppState;
use std::fmt::Write as _;

/// カルテデフォルト年（Agoop最新 = 2021。DB未投入時は None）
const KARTE_DEFAULT_YEAR: i32 = 2021;

// ========== ルート 1: /tab/region_karte ==========

/// GETパラメータ（URL共有対応: ?prefecture=...&municipality=...）
#[derive(Deserialize)]
pub struct KarteTabParams {
    #[serde(default)]
    pub prefecture: Option<String>,
    #[serde(default)]
    pub municipality: Option<String>,
}

/// 地域カルテタブ本体
pub async fn tab_region_karte(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(q): Query<KarteTabParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    // URL パラメータ優先、なければセッション
    let pref = q
        .prefecture
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| filters.prefecture.clone());
    let muni = q
        .municipality
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| filters.municipality.clone());

    // 市区町村未選択時のガイダンス画面
    if pref.is_empty() || muni.is_empty() {
        return Html(render_empty_guide(&pref, &muni));
    }

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db()),
    };
    let db_clone = db.clone();
    let turso_clone = state.turso_db.clone();
    let pref_c = pref.clone();
    let muni_c = muni.clone();

    // 重いデータ取得はブロッキングタスクで
    let (bundle, insights) = tokio::task::spawn_blocking(move || {
        let b = fetch_karte_bundle(&db_clone, turso_clone.as_ref(), &pref_c, &muni_c);
        let ctx = ifetch::build_insight_context(&db_clone, turso_clone.as_ref(), &pref_c, &muni_c);
        let ins = generate_insights(&ctx);
        (b, ins)
    })
    .await
    .unwrap_or_else(|_| (KarteBundle::default(), Vec::new()));

    let html = render_karte(&pref, &muni, &bundle, &insights);
    Html(html)
}

// ========== ルート 2: /api/region/karte/{citycode} ==========

/// 将来の外部アクセス用（JSON APIフォーマット、現状は同一ドメイン内のみ利用想定）
pub async fn api_region_karte(
    State(state): State<Arc<AppState>>,
    Path(citycode): Path<i64>,
) -> axum::Json<Value> {
    // citycode → pref/muni 逆引き（v2_flow_master_prefcity）
    let (pref, muni) = match &state.hw_db {
        Some(db) => lookup_pref_muni(db, citycode).unwrap_or_default(),
        None => (String::new(), String::new()),
    };

    if pref.is_empty() || muni.is_empty() {
        return axum::Json(json!({
            "error": "citycode not found",
            "citycode": citycode,
        }));
    }

    // SAFETY: pref/muni が空でない = lookup_pref_muni が Some を返した = state.hw_db は Some
    // それでも graceful な空応答に置換し panic 経路を除去
    let db = match state.hw_db.as_ref() {
        Some(db) => db.clone(),
        None => {
            return axum::Json(json!({
                "error": "hw_db unavailable",
                "citycode": citycode,
            }));
        }
    };
    let turso = state.turso_db.clone();
    let p = pref.clone();
    let m = muni.clone();
    let bundle = tokio::task::spawn_blocking(move || {
        fetch_karte_bundle(&db, turso.as_ref(), &p, &m)
    })
    .await
    .unwrap_or_default();

    axum::Json(json!({
        "citycode": citycode,
        "prefecture": pref,
        "municipality": muni,
        "kpi": {
            "total_population": bundle.total_population,
            "total_households": bundle.total_households,
            "elderly_rate": bundle.elderly_rate,
            "single_rate": bundle.single_rate,
            "unemployment_rate": bundle.unemployment_rate,
            "physicians_per_10k": bundle.physicians_per_10k,
            "daycare_per_1k": bundle.daycare_per_1k,
            "establishment_count": bundle.establishment_count,
            "habitable_density": bundle.habitable_density,
        },
        "posting_count": bundle.hw_posting_count,
        "daynight_ratio": bundle.daynight_ratio,
        "covid_recovery_ratio": bundle.covid_recovery_ratio,
    }))
}

/// citycode → (prefecture, municipality) 逆引き
fn lookup_pref_muni(db: &crate::db::local_sqlite::LocalDb, citycode: i64) -> Option<(String, String)> {
    if !crate::handlers::helpers::table_exists(db, "v2_flow_master_prefcity") {
        return None;
    }
    let rows = db
        .query(
            "SELECT prefname, cityname FROM v2_flow_master_prefcity WHERE citycode = ?1 LIMIT 1",
            &[&citycode as &dyn rusqlite::types::ToSql],
        )
        .unwrap_or_default();
    rows.first().map(|r| {
        (
            get_str(r, "prefname"),
            get_str(r, "cityname"),
        )
    })
}

// ========== データバンドル ==========

/// カルテ画面に必要な全データの集約
#[derive(Default)]
struct KarteBundle {
    // --- 構造KPI 9枚 ---
    total_population: i64,
    total_households: i64,
    elderly_rate: f64,          // %
    single_rate: f64,           // %
    unemployment_rate: f64,     // %
    physicians_per_10k: f64,
    daycare_per_1k: f64,
    establishment_count: i64,
    habitable_density: f64,     // 人/km²
    total_area_km2: f64,
    habitable_area_km2: f64,

    // --- S2: 人口動態 ---
    pyramid_rows: Vec<Row>,
    households_row: Option<Row>,
    vital_row: Option<Row>,

    // --- S3: 産業・労働 ---
    establishments_top: Vec<Row>,    // industry_name, establishment_count, employees
    primary_employed: i64,
    secondary_employed: i64,
    tertiary_employed: i64,
    unemployed: i64,

    // --- S4: 医療・教育・福祉 ---
    medical_row: Option<Row>,
    education_row: Option<Row>,

    // --- S5: 人流パターン (Agoop) ---
    citycode: Option<i64>,
    karte_profile: Vec<Row>,          // month × dayflag × timezone
    monthly_trend: Vec<Row>,          // year, month, pop_sum
    daynight_ratio: Option<f64>,
    inflow_breakdown: Vec<Row>,       // from_area, total_population
    covid_recovery_ratio: Option<f64>,

    // --- HW求人集計 ---
    hw_posting_count: i64,
    hw_avg_salary_min: f64,
    hw_fulltime_count: i64,
    hw_parttime_count: i64,
}

fn fetch_karte_bundle(
    db: &crate::db::local_sqlite::LocalDb,
    turso: Option<&crate::db::turso_http::TursoDb>,
    pref: &str,
    muni: &str,
) -> KarteBundle {
    let mut b = KarteBundle::default();

    // S1 / S2 / S4: 構造データ
    let pop = af::fetch_population_data(db, turso, pref, muni);
    if let Some(r) = pop.first() {
        b.total_population = get_i64(r, "total_population");
        // aging_rate (v2_external_population の高齢化率 %) を優先、無い場合は pyramid から再計算
        let er = get_f64(r, "aging_rate");
        if er > 0.0 {
            b.elderly_rate = er;
        }
    }

    b.pyramid_rows = af::fetch_population_pyramid(db, turso, pref, muni);
    if b.elderly_rate <= 0.0 {
        b.elderly_rate = calc_elderly_rate_from_pyramid(&b.pyramid_rows);
    }

    let hh = af::fetch_households(db, turso, pref, muni);
    if let Some(r) = hh.first() {
        b.total_households = get_i64(r, "total_households");
        b.single_rate = get_f64(r, "single_rate");
        b.households_row = Some(r.clone());
    }

    let vital = af::fetch_vital_statistics(db, turso, pref, muni);
    b.vital_row = vital.first().cloned();

    let lf = af::fetch_labor_force(db, turso, pref, muni);
    if let Some(r) = lf.first() {
        b.unemployment_rate = get_f64(r, "unemployment_rate");
        b.primary_employed = get_i64(r, "primary_industry_employed");
        b.secondary_employed = get_i64(r, "secondary_industry_employed");
        b.tertiary_employed = get_i64(r, "tertiary_industry_employed");
        b.unemployed = get_i64(r, "unemployed");
    }

    let mw = af::fetch_medical_welfare(db, turso, pref, muni);
    if let Some(r) = mw.first() {
        b.physicians_per_10k = get_f64(r, "physicians_per_10k_pop");
        b.daycare_per_1k = get_f64(r, "daycare_per_1k_children_0_14");
        b.medical_row = Some(r.clone());
    }

    let edu = af::fetch_education_facilities(db, turso, pref, muni);
    b.education_row = edu.first().cloned();

    let geo = af::fetch_geography(db, turso, pref, muni);
    if let Some(r) = geo.first() {
        b.total_area_km2 = get_f64(r, "total_area_km2");
        b.habitable_area_km2 = get_f64(r, "habitable_area_km2");
        b.habitable_density = get_f64(r, "habitable_density_per_km2");
        // habitable_density が NULL の場合は総人口 / 可住地面積 で再計算
        if b.habitable_density <= 0.0 && b.habitable_area_km2 > 0.0 && b.total_population > 0 {
            b.habitable_density = b.total_population as f64 / b.habitable_area_km2;
        }
    }

    // S3: 事業所データ（都道府県粒度のみ入手可能 → 上位10産業）
    let est = af::fetch_establishments(db, turso, pref);
    b.establishments_top = est.into_iter().take(17).collect();
    b.establishment_count = b
        .establishments_top
        .iter()
        .map(|r| get_i64(r, "establishment_count"))
        .sum();

    // S5: Agoop 人流
    if let Some(code) = fetch_citycode_for_karte(db, pref, muni) {
        b.citycode = Some(code);
        b.karte_profile = fflow::get_karte_profile(db, turso, code, KARTE_DEFAULT_YEAR);
        b.monthly_trend = fflow::get_karte_monthly_trend(db, turso, code);
        b.daynight_ratio = fflow::get_karte_daynight_ratio(db, turso, code, KARTE_DEFAULT_YEAR);
        b.inflow_breakdown = fft::get_inflow_breakdown(db, turso, code, KARTE_DEFAULT_YEAR);
        b.covid_recovery_ratio = calc_covid_recovery_for_karte(&b.monthly_trend);
    }

    // HW求人集計（postings）
    let sql = "SELECT COUNT(*) as cnt, AVG(salary_min) as avg_sal, \
               SUM(CASE WHEN employment_type = '正社員' THEN 1 ELSE 0 END) as ft, \
               SUM(CASE WHEN employment_type LIKE '%パート%' OR employment_type LIKE '%アルバイト%' THEN 1 ELSE 0 END) as pt \
               FROM postings WHERE prefecture = ?1 AND municipality = ?2";
    if let Ok(rows) = db.query(
        sql,
        &[
            &pref as &dyn rusqlite::types::ToSql,
            &muni as &dyn rusqlite::types::ToSql,
        ],
    ) {
        if let Some(r) = rows.first() {
            b.hw_posting_count = get_i64(r, "cnt");
            b.hw_avg_salary_min = get_f64(r, "avg_sal");
            b.hw_fulltime_count = get_i64(r, "ft");
            b.hw_parttime_count = get_i64(r, "pt");
        }
    }

    b
}

/// ピラミッド行から高齢化率を再計算（65+ / total）
fn calc_elderly_rate_from_pyramid(pyramid: &[Row]) -> f64 {
    if pyramid.is_empty() {
        return 0.0;
    }
    let mut total = 0i64;
    let mut elderly = 0i64;
    for r in pyramid {
        let grp = get_str(r, "age_group");
        let m = get_i64(r, "male_count");
        let f = get_i64(r, "female_count");
        total += m + f;
        // 65-74, 75+, 60-69(部分), 70-79, 80+
        if grp == "65-74" || grp == "75+" || grp == "70-79" || grp == "80+" {
            elderly += m + f;
        } else if grp == "60-69" {
            elderly += (m + f) / 2; // 概算（65-69のみカウント）
        }
    }
    if total > 0 {
        (elderly as f64 / total as f64) * 100.0
    } else {
        0.0
    }
}

/// monthly_trend (year, month, pop_sum) から 2021/2019 同月比を平均で算出
fn calc_covid_recovery_for_karte(trend: &[Row]) -> Option<f64> {
    use std::collections::HashMap;
    let mut by_year: HashMap<i64, HashMap<i64, f64>> = HashMap::new();
    for r in trend {
        let year = get_i64(r, "year");
        let month = get_i64(r, "month");
        let pop = get_f64(r, "pop_sum");
        by_year.entry(year).or_default().insert(month, pop);
    }
    let m2019 = by_year.get(&2019)?;
    let m2021 = by_year.get(&2021)?;
    let mut ratios: Vec<f64> = Vec::new();
    for (month, p19) in m2019 {
        if let Some(p21) = m2021.get(month) {
            if *p19 > 0.0 {
                ratios.push(p21 / p19);
            }
        }
    }
    if ratios.is_empty() {
        None
    } else {
        Some(ratios.iter().sum::<f64>() / ratios.len() as f64)
    }
}

/// v2_flow_master_prefcity から citycode を引く（存在しなければ None）
fn fetch_citycode_for_karte(
    db: &crate::db::local_sqlite::LocalDb,
    pref: &str,
    muni: &str,
) -> Option<i64> {
    if !crate::handlers::helpers::table_exists(db, "v2_flow_master_prefcity") {
        return None;
    }
    let rows = db
        .query(
            "SELECT citycode FROM v2_flow_master_prefcity \
             WHERE prefname = ?1 AND cityname = ?2 LIMIT 1",
            &[
                &pref as &dyn rusqlite::types::ToSql,
                &muni as &dyn rusqlite::types::ToSql,
            ],
        )
        .ok()?;
    rows.first().and_then(|r| {
        let c = get_i64(r, "citycode");
        if c > 0 {
            Some(c)
        } else {
            None
        }
    })
}

// ========== HTMLレンダリング ==========

fn render_no_db() -> String {
    r##"<div class="p-8 text-center text-slate-400">
        <h2 class="text-2xl mb-4">地域カルテ</h2>
        <p>データベースが読み込まれていません。</p>
        <p class="text-sm mt-2">hellowork.db を配置してください。</p>
    </div>"##
        .to_string()
}

fn render_empty_guide(pref: &str, muni: &str) -> String {
    let hint = if pref.is_empty() {
        "画面上部のドロップダウンで都道府県と市区町村を選択してください。"
    } else if muni.is_empty() {
        "市区町村を選択するとカルテが生成されます。"
    } else {
        ""
    };
    format!(
        r##"<div class="p-8 text-center text-slate-400">
            <h2 class="text-2xl mb-4 text-white">📋 地域カルテ</h2>
            <p class="text-sm mb-2">市区町村単位の人口・産業・労働・医療・教育・人流を1画面で確認できます。</p>
            <p class="text-sm text-slate-500 mt-4">{hint}</p>
        </div>"##,
        hint = escape_html(hint),
    )
}

fn render_karte(pref: &str, muni: &str, b: &KarteBundle, insights: &[Insight]) -> String {
    let location_label = make_location_label(pref, muni);

    let mut html = String::with_capacity(32_000);

    // ========== ヘッダー ==========
    write!(html,
        r##"<div class="space-y-6 karte-container">
<header class="karte-header">
    <div class="flex items-start justify-between flex-wrap gap-3">
        <div>
            <h2 class="text-xl font-bold text-white">📋 地域カルテ <span class="text-blue-400 text-base font-normal">{location}</span></h2>
            <p class="text-xs text-slate-500 mt-1">市区町村構造 + 人流 + HW求人 + So What 示唆の統合ビュー</p>
        </div>
        <div class="flex gap-2 karte-action-bar">
            <button type="button" onclick="window.karteShareUrl && window.karteShareUrl('{pref}', '{muni}')"
                class="karte-btn karte-btn-share" aria-label="この地域カルテのURLをコピー">
                🔗 URL共有
            </button>
            <button type="button" onclick="window.print()"
                class="karte-btn karte-btn-print" aria-label="A4で印刷">
                🖨️ 印刷
            </button>
        </div>
    </div>
    {badges}
</header>"##,
        location = escape_html(&location_label),
        pref = escape_html(pref),
        muni = escape_html(muni),
        badges = render_badges(b),
    ).unwrap();

    // ========== S1: 構造KPI 9枚 ==========
    html.push_str(&render_section_kpi(b));

    // ========== S2: 人口動態 ==========
    html.push_str(&render_section_demographics(b));

    // ========== S3: 産業・労働 ==========
    html.push_str(&render_section_industry(b));

    // ========== S4: 医療・教育・福祉 ==========
    html.push_str(&render_section_welfare(b));

    // ========== S5: 人流パターン（Agoop） ==========
    html.push_str(&render_section_flow(b));

    // ========== S6: So What 示唆 ==========
    html.push_str(&render_section_insights(insights));

    // ========== S7: 印刷・出典 ==========
    html.push_str(&render_section_footer(pref, muni));

    html.push_str("</div>");
    html
}

// ---------- ヘッダー基本属性バッジ ----------
fn render_badges(b: &KarteBundle) -> String {
    let pop = if b.total_population > 0 {
        format!("{}人", format_number(b.total_population))
    } else {
        "—".to_string()
    };
    let area = if b.total_area_km2 > 0.0 {
        format!("{:.1} km²", b.total_area_km2)
    } else {
        "—".to_string()
    };
    let elderly = if b.elderly_rate > 0.0 {
        format!("{:.1}%", b.elderly_rate)
    } else {
        "—".to_string()
    };
    format!(
        r##"<div class="flex flex-wrap gap-2 mt-3 karte-badges">
        <span class="karte-badge karte-badge-pop" title="総人口">👥 {pop}</span>
        <span class="karte-badge karte-badge-area" title="総面積">🗺 {area}</span>
        <span class="karte-badge karte-badge-elderly" title="高齢化率 (65歳以上)">👴 高齢化 {elderly}</span>
    </div>"##,
        pop = escape_html(&pop),
        area = escape_html(&area),
        elderly = escape_html(&elderly),
    )
}

// ---------- S1: 構造KPI 9枚 ----------
fn render_section_kpi(b: &KarteBundle) -> String {
    let cards = [
        ("👥", "総人口", format_i64_or_dash(b.total_population, "人"), "text-blue-400"),
        ("🏠", "世帯数", format_i64_or_dash(b.total_households, "世帯"), "text-emerald-400"),
        ("👴", "高齢化率", format_pct_or_dash(b.elderly_rate), "text-amber-400"),
        ("🚪", "単独世帯率", format_pct_or_dash(b.single_rate), "text-rose-400"),
        ("📉", "失業率", format_pct_or_dash(b.unemployment_rate), "text-orange-400"),
        ("🩺", "医師数 (人/10k)", format_f64_or_dash(b.physicians_per_10k, 1), "text-cyan-400"),
        ("👶", "保育所 (/1k児)", format_f64_or_dash(b.daycare_per_1k, 2), "text-lime-400"),
        ("🏢", "事業所数 (県)", format_i64_or_dash(b.establishment_count, "事業所"), "text-violet-400"),
        ("🏘", "可住地密度", format_density_or_dash(b.habitable_density), "text-pink-400"),
    ];

    let mut html = String::from(
        r##"<section class="karte-section" aria-labelledby="karte-s1-title">
    <h3 id="karte-s1-title" class="karte-section-title">🧩 構造KPI</h3>
    <p class="karte-section-hint">市区町村の基本構造指標（SSDSE-A / e-Stat）</p>
    <div class="karte-kpi-grid">"##,
    );
    for (icon, label, value, color) in cards {
        write!(html,
            r##"<div class="karte-kpi-card">
                <div class="karte-kpi-icon">{icon}</div>
                <div class="karte-kpi-label">{label}</div>
                <div class="karte-kpi-value {color}">{value}</div>
            </div>"##,
            icon = icon,
            label = escape_html(label),
            value = escape_html(&value),
            color = color,
        ).unwrap();
    }
    html.push_str("</div></section>");
    html
}

// ---------- S2: 人口動態 ----------
fn render_section_demographics(b: &KarteBundle) -> String {
    let pyramid_chart = build_population_pyramid_chart(&b.pyramid_rows);
    let household_chart = build_household_stack_chart(b.households_row.as_ref());
    let vital_chart = build_vital_stats_chart(b.vital_row.as_ref());

    format!(
        r##"<section class="karte-section" aria-labelledby="karte-s2-title">
    <h3 id="karte-s2-title" class="karte-section-title">📊 人口動態</h3>
    <p class="karte-section-hint">人口ピラミッド・世帯構成・自然動態（出典: 国勢調査 / 人口動態調査）</p>
    <div class="karte-chart-grid-2">
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">人口ピラミッド</h4>
            {pyramid}
        </div>
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">世帯タイプ構成</h4>
            {household}
        </div>
    </div>
    <div class="karte-chart-card">
        <h4 class="karte-chart-title">自然動態（年間）</h4>
        {vital}
    </div>
</section>"##,
        pyramid = pyramid_chart,
        household = household_chart,
        vital = vital_chart,
    )
}

// ---------- S3: 産業・労働 ----------
fn render_section_industry(b: &KarteBundle) -> String {
    let est_chart = build_establishments_bar(&b.establishments_top);
    let labor_chart = build_labor_tertiary_pie(b);
    let unemp_card = build_unemployment_card(b);

    format!(
        r##"<section class="karte-section" aria-labelledby="karte-s3-title">
    <h3 id="karte-s3-title" class="karte-section-title">🏭 産業・労働</h3>
    <p class="karte-section-hint">17業種事業所・就業者構成・失業率（出典: 経済センサス / 国勢調査）</p>
    <div class="karte-chart-card">
        <h4 class="karte-chart-title">事業所数 TOP10（都道府県集計）</h4>
        <p class="karte-chart-note">※事業所データは都道府県粒度のみ提供。市区町村別内訳は今後拡張予定。</p>
        {est}
    </div>
    <div class="karte-chart-grid-2">
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">就業者構成（1次/2次/3次）</h4>
            {labor}
        </div>
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">失業・労働力</h4>
            {unemp}
        </div>
    </div>
</section>"##,
        est = est_chart,
        labor = labor_chart,
        unemp = unemp_card,
    )
}

// ---------- S4: 医療・教育・福祉 ----------
fn render_section_welfare(b: &KarteBundle) -> String {
    let medical_cards = build_medical_cards(b.medical_row.as_ref());
    let edu_cards = build_education_cards(b.education_row.as_ref());

    format!(
        r##"<section class="karte-section" aria-labelledby="karte-s4-title">
    <h3 id="karte-s4-title" class="karte-section-title">🏥 医療・教育・福祉</h3>
    <p class="karte-section-hint">施設数と相対密度（出典: 医療施設調査 / 学校基本調査 / 社会福祉施設等調査）</p>
    <div class="karte-cards-row">
        {medical}
    </div>
    <div class="karte-cards-row mt-3">
        {edu}
    </div>
</section>"##,
        medical = medical_cards,
        edu = edu_cards,
    )
}

// ---------- S5: 人流パターン（Agoop） ----------
fn render_section_flow(b: &KarteBundle) -> String {
    if b.citycode.is_none() || b.karte_profile.is_empty() {
        return r##"<section class="karte-section" aria-labelledby="karte-s5-title">
    <h3 id="karte-s5-title" class="karte-section-title">🚶 人流パターン</h3>
    <p class="karte-section-hint">出典: Agoop 国土数値情報（1km メッシュ × 時間帯）</p>
    <div class="karte-chart-card">
        <p class="text-slate-400 text-sm">本市区町村の Agoop 人流データは未投入か対象外です。</p>
        <p class="text-slate-500 text-xs mt-1">データ投入後、時間帯プロファイル・昼夜比・36ヶ月トレンド・流入構造を表示します。</p>
    </div>
</section>"##
            .to_string();
    }

    let profile_chart = build_flow_profile_chart(&b.karte_profile);
    let trend_chart = build_flow_monthly_trend_chart(&b.monthly_trend);
    let daynight_gauge = build_daynight_gauge(b.daynight_ratio);
    let inflow_pie = build_inflow_pie(&b.inflow_breakdown);
    let covid_note = b
        .covid_recovery_ratio
        .map(|r| {
            format!(
                r##"<p class="text-xs text-slate-400 mt-2">コロナ回復率 (2021/2019 同月比平均): <span class="text-white font-mono">{:.1}%</span></p>"##,
                r * 100.0
            )
        })
        .unwrap_or_default();

    format!(
        r##"<section class="karte-section" aria-labelledby="karte-s5-title">
    <h3 id="karte-s5-title" class="karte-section-title">🚶 人流パターン</h3>
    <p class="karte-section-hint">出典: Agoop 国土数値情報（1km メッシュ × 時間帯）</p>
    <div class="karte-chart-grid-2">
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">時間帯プロファイル（月別、平日/休日×昼/深夜）</h4>
            {profile}
        </div>
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">36ヶ月 滞在推移（平日昼基準）</h4>
            {trend}
            {covid_note}
        </div>
    </div>
    <div class="karte-chart-grid-2">
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">昼夜比ゲージ</h4>
            {daynight}
        </div>
        <div class="karte-chart-card">
            <h4 class="karte-chart-title">流入構造（4区分）</h4>
            {inflow}
        </div>
    </div>
</section>"##,
        profile = profile_chart,
        trend = trend_chart,
        daynight = daynight_gauge,
        inflow = inflow_pie,
        covid_note = covid_note,
    )
}

// ---------- S6: So What 示唆 ----------
fn render_section_insights(insights: &[Insight]) -> String {
    // StructuralContext (LS/HH/MF/IN/GE + SW-F01〜F10) を優先表示、その他を後続
    let mut primary: Vec<&Insight> = Vec::new();
    let mut secondary: Vec<&Insight> = Vec::new();
    for ins in insights {
        if ins.category == InsightCategory::StructuralContext {
            primary.push(ins);
        } else {
            secondary.push(ins);
        }
    }
    // severity 昇順 (Critical → Warning → Info → Positive)
    primary.sort_by(|a, b| a.severity.cmp(&b.severity));
    secondary.sort_by(|a, b| a.severity.cmp(&b.severity));

    let mut html = String::from(
        r##"<section class="karte-section" aria-labelledby="karte-s6-title">
    <h3 id="karte-s6-title" class="karte-section-title">💡 So What 示唆</h3>
    <p class="karte-section-hint">構造・人流・求人データを掛け合わせた発火済みパターン（傾向・可能性の示唆）</p>"##,
    );

    if primary.is_empty() && secondary.is_empty() {
        html.push_str(
            r##"<div class="karte-chart-card text-center py-8">
            <p class="text-slate-400 text-sm">該当する示唆はありません</p>
            <p class="text-slate-500 text-xs mt-1">データ不足か、特筆すべき構造特徴が検出されませんでした</p>
        </div>"##,
        );
    } else {
        html.push_str(r##"<div class="karte-insight-grid">"##);
        for ins in primary.iter().chain(secondary.iter()) {
            html.push_str(&render_karte_insight_card(ins));
        }
        html.push_str("</div>");
        // 文末表現検証（phrase_validator）のUI側フォールバック: 全cardに軽い注記
        html.push_str(
            r##"<p class="text-[10px] text-slate-600 mt-3">※ 示唆は「傾向」「可能性」の範囲に留めています。因果関係は示していません。</p>"##,
        );
    }

    html.push_str("</section>");
    html
}

/// カルテ用の小さめ示唆カード（insight engine 側の render_insight_card と重複しない独自スタイル）
fn render_karte_insight_card(insight: &Insight) -> String {
    let bg = insight.severity.bg_class();
    let badge = insight.severity.badge_class();
    let label = insight.severity.label();
    let cat_label = insight.category.label();

    let evidence_html = if insight.evidence.is_empty() {
        String::new()
    } else {
        let mut s = String::from(r##"<ul class="karte-evidence-list">"##);
        for ev in insight.evidence.iter().take(3) {
            let formatted = if ev.unit == "円" || ev.unit == "円/月" {
                format!("{:.0}{}", ev.value, ev.unit)
            } else if ev.unit == "%" {
                format!("{:.1}{}", ev.value * 100.0, ev.unit)
            } else {
                format!("{:.2}{}", ev.value, ev.unit)
            };
            s.push_str(&format!(
                r##"<li><span class="text-slate-500">{metric}:</span> <span class="text-white font-mono">{value}</span></li>"##,
                metric = escape_html(&ev.metric),
                value = escape_html(&formatted),
            ));
        }
        s.push_str("</ul>");
        s
    };

    format!(
        r##"<article class="karte-insight-card {bg}" role="listitem" aria-label="{label} {title}">
        <div class="karte-insight-head">
            <span class="karte-insight-badge {badge}">{label}</span>
            <span class="karte-insight-cat">{cat}</span>
        </div>
        <h5 class="karte-insight-title">{title}</h5>
        <p class="karte-insight-body">{body}</p>
        {evidence}
    </article>"##,
        bg = bg,
        badge = badge,
        label = label,
        cat = escape_html(cat_label),
        title = escape_html(&insight.title),
        body = escape_html(&insight.body),
        evidence = evidence_html,
    )
}

// ---------- S7: フッター ----------
fn render_section_footer(_pref: &str, _muni: &str) -> String {
    r##"<section class="karte-section karte-footer" aria-label="出典・注意事項">
    <div class="text-[10px] text-slate-600 border-t border-slate-800 pt-3">
        <div>出典: ハローワーク掲載求人データ / e-Stat API / SSDSE-A（総務省統計局） / Agoop 国土数値情報</div>
        <div class="mt-1">※ HW掲載求人は全求人市場の一部を構成します。IT・通信等HW掲載が少ない産業は参考値。</div>
        <div class="mt-1">※ 事業所データは都道府県粒度、人流データは2021年基準、その他は出典記載年。</div>
    </div>
</section>"##
        .to_string()
}

// ========== ECharts option ビルダー ==========

/// `data-chart-config` 属性付き div を生成（シングルクォート/バックスラッシュ完全エスケープ）
fn echart_div(height: u32, config: &Value, aria_label: &str) -> String {
    // シングルクォート/バックスラッシュを HTML エンティティで escape
    // （data-chart-config は single-quote 属性値、内部に ' が出ると破綻）
    let cfg_str = config
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;");
    format!(
        r##"<div class="echart" role="figure" aria-label="{aria}" style="height:{h}px;" data-chart-config='{cfg}'></div>"##,
        aria = escape_html(aria_label),
        h = height,
        cfg = cfg_str,
    )
}

/// 人口ピラミッド（両側棒）
fn build_population_pyramid_chart(rows: &[Row]) -> String {
    if rows.is_empty() {
        return chart_placeholder("人口ピラミッドデータがありません");
    }
    // 降順表示（上が若年層ではなく高齢層になるよう逆順）
    let mut age_groups: Vec<&str> = Vec::new();
    let mut males: Vec<i64> = Vec::new();
    let mut females: Vec<i64> = Vec::new();
    for r in rows {
        age_groups.push(match r.get("age_group").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => "",
        });
        males.push(-get_i64(r, "male_count")); // 負数化して左側に表示
        females.push(get_i64(r, "female_count"));
    }

    let config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "legend": {"data": ["男性", "女性"], "top": 0, "textStyle": {"color": "#cbd5e1"}},
        "grid": {"left": "12%", "right": "8%", "top": 40, "bottom": 30, "containLabel": true},
        "xAxis": {
            "type": "value",
            "axisLabel": {"color": "#94a3b8"}
        },
        "yAxis": {
            "type": "category",
            "data": age_groups,
            "axisLabel": {"color": "#cbd5e1"}
        },
        "series": [
            {
                "name": "男性",
                "type": "bar",
                "stack": "total",
                "data": males,
                "itemStyle": {"color": "#0072B2"}
            },
            {
                "name": "女性",
                "type": "bar",
                "stack": "total",
                "data": females,
                "itemStyle": {"color": "#E69F00"}
            }
        ]
    });

    let aria_sum: i64 = males.iter().map(|v| v.abs()).sum::<i64>() + females.iter().sum::<i64>();
    let aria = format!(
        "人口ピラミッド 総計 {}人 年齢階層 {}",
        format_number(aria_sum),
        age_groups.len()
    );
    echart_div(360, &config, &aria)
}

/// 世帯タイプ積み上げ棒
fn build_household_stack_chart(row: Option<&Row>) -> String {
    let r = match row {
        Some(r) => r,
        None => return chart_placeholder("世帯データがありません"),
    };
    let nuclear = get_i64(r, "nuclear_family_households");
    let single = get_i64(r, "single_households");
    let elderly_nuclear = get_i64(r, "elderly_nuclear_households");
    let elderly_couple = get_i64(r, "elderly_couple_households");
    let elderly_single = get_i64(r, "elderly_single_households");
    let total = get_i64(r, "total_households");
    let other = (total - nuclear - single - elderly_nuclear - elderly_couple - elderly_single).max(0);

    let config = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}: {c} 世帯 ({d}%)"},
        "legend": {"orient": "vertical", "left": "right", "textStyle": {"color": "#cbd5e1"}},
        "series": [{
            "type": "pie",
            "radius": ["45%", "70%"],
            "center": ["40%", "50%"],
            "avoidLabelOverlap": true,
            "itemStyle": {"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2},
            "label": {"color": "#e2e8f0", "fontSize": 11, "formatter": "{b}\n{d}%"},
            "data": [
                {"name": "核家族", "value": nuclear, "itemStyle": {"color": "#3B82F6"}},
                {"name": "単独", "value": single, "itemStyle": {"color": "#EF4444"}},
                {"name": "高齢核家族", "value": elderly_nuclear, "itemStyle": {"color": "#F59E0B"}},
                {"name": "高齢夫婦", "value": elderly_couple, "itemStyle": {"color": "#10B981"}},
                {"name": "高齢単独", "value": elderly_single, "itemStyle": {"color": "#EC4899"}},
                {"name": "その他", "value": other, "itemStyle": {"color": "#64748B"}},
            ]
        }]
    });

    let aria = format!(
        "世帯構成 総世帯 {} 核家族 {} 高齢単独 {}",
        format_number(total),
        format_number(nuclear),
        format_number(elderly_single)
    );
    echart_div(340, &config, &aria)
}

/// 自然動態（出生/死亡/婚姻/離婚）
fn build_vital_stats_chart(row: Option<&Row>) -> String {
    let r = match row {
        Some(r) => r,
        None => return chart_placeholder("自然動態データがありません"),
    };
    let births = get_i64(r, "births");
    let deaths = get_i64(r, "deaths");
    let marriages = get_i64(r, "marriages");
    let divorces = get_i64(r, "divorces");

    let labels = ["出生", "死亡", "婚姻", "離婚"];
    let values = [births, deaths, marriages, divorces];
    let colors = ["#10B981", "#64748B", "#F59E0B", "#EF4444"];
    let data: Vec<Value> = labels
        .iter()
        .zip(values.iter())
        .zip(colors.iter())
        .map(|((l, v), c)| {
            json!({
                "name": l,
                "value": v,
                "itemStyle": {"color": c}
            })
        })
        .collect();

    let config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "grid": {"left": "3%", "right": "4%", "bottom": "8%", "top": 20, "containLabel": true},
        "xAxis": {"type": "category", "data": labels, "axisLabel": {"color": "#cbd5e1"}},
        "yAxis": {"type": "value", "axisLabel": {"color": "#94a3b8"}},
        "series": [{
            "type": "bar",
            "data": data,
            "label": {"show": true, "position": "top", "color": "#e2e8f0", "fontSize": 11}
        }]
    });
    let aria = format!(
        "自然動態 出生{} 死亡{} 婚姻{} 離婚{}",
        format_number(births),
        format_number(deaths),
        format_number(marriages),
        format_number(divorces)
    );
    echart_div(280, &config, &aria)
}

/// 事業所数 水平棒
fn build_establishments_bar(rows: &[Row]) -> String {
    if rows.is_empty() {
        return chart_placeholder("事業所データがありません");
    }
    // 上位10業種（数が多い順 → 下に最大が来るよう reversed）
    let top: Vec<&Row> = rows.iter().take(10).collect();
    let labels: Vec<String> = top
        .iter()
        .rev()
        .map(|r| get_str(r, "industry_name"))
        .collect();
    let est_values: Vec<i64> = top
        .iter()
        .rev()
        .map(|r| get_i64(r, "establishment_count"))
        .collect();
    let emp_values: Vec<i64> = top.iter().rev().map(|r| get_i64(r, "employees")).collect();

    let config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "legend": {"data": ["事業所数", "従業者数"], "top": 0, "textStyle": {"color": "#cbd5e1"}},
        "grid": {"left": 140, "right": 30, "top": 36, "bottom": 20},
        "xAxis": {"type": "value", "axisLabel": {"color": "#94a3b8"}},
        "yAxis": {"type": "category", "data": labels, "axisLabel": {"color": "#cbd5e1", "fontSize": 11}},
        "series": [
            {
                "name": "事業所数",
                "type": "bar",
                "data": est_values,
                "itemStyle": {"color": "#3B82F6"},
            },
            {
                "name": "従業者数",
                "type": "bar",
                "data": emp_values,
                "itemStyle": {"color": "#10B981"},
            }
        ]
    });

    let aria = format!("事業所数TOP10 {}業種", labels.len());
    echart_div(420, &config, &aria)
}

/// 就業者 3次産業比率 (pie)
fn build_labor_tertiary_pie(b: &KarteBundle) -> String {
    let total = b.primary_employed + b.secondary_employed + b.tertiary_employed;
    if total <= 0 {
        return chart_placeholder("就業者データがありません");
    }

    let config = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}: {c} 人 ({d}%)"},
        "legend": {"orient": "horizontal", "bottom": 0, "textStyle": {"color": "#cbd5e1"}},
        "series": [{
            "type": "pie",
            "radius": ["50%", "75%"],
            "center": ["50%", "45%"],
            "avoidLabelOverlap": true,
            "itemStyle": {"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2},
            "label": {"color": "#e2e8f0", "fontSize": 12, "formatter": "{b}\n{d}%"},
            "data": [
                {"name": "第1次産業", "value": b.primary_employed, "itemStyle": {"color": "#10B981"}},
                {"name": "第2次産業", "value": b.secondary_employed, "itemStyle": {"color": "#F59E0B"}},
                {"name": "第3次産業", "value": b.tertiary_employed, "itemStyle": {"color": "#3B82F6"}},
            ]
        }]
    });
    let aria = format!(
        "就業者構成 1次{} 2次{} 3次{}",
        format_number(b.primary_employed),
        format_number(b.secondary_employed),
        format_number(b.tertiary_employed)
    );
    echart_div(320, &config, &aria)
}

/// 失業率カード（数値ミニ）
fn build_unemployment_card(b: &KarteBundle) -> String {
    let rate = if b.unemployment_rate > 0.0 {
        format!("{:.2}%", b.unemployment_rate)
    } else {
        "—".to_string()
    };
    let unemp_n = if b.unemployed > 0 {
        format!("{}人", format_number(b.unemployed))
    } else {
        "—".to_string()
    };
    let hw_posting = if b.hw_posting_count > 0 {
        format!("{}件", format_number(b.hw_posting_count))
    } else {
        "—".to_string()
    };

    format!(
        r##"<div class="karte-mini-grid">
            <div class="karte-mini-card">
                <div class="karte-mini-label">失業率</div>
                <div class="karte-mini-value text-orange-400">{rate}</div>
            </div>
            <div class="karte-mini-card">
                <div class="karte-mini-label">完全失業者数</div>
                <div class="karte-mini-value text-rose-400">{unemp}</div>
            </div>
            <div class="karte-mini-card">
                <div class="karte-mini-label">HW求人件数</div>
                <div class="karte-mini-value text-blue-400">{posting}</div>
            </div>
        </div>
        <p class="text-[11px] text-slate-500 mt-2">※ 失業者数はHW求人数との比較で有効求人倍率の代替指標として参照可能</p>"##,
        rate = escape_html(&rate),
        unemp = escape_html(&unemp_n),
        posting = escape_html(&hw_posting),
    )
}

/// 医療施設4枚カード
fn build_medical_cards(row: Option<&Row>) -> String {
    let r = match row {
        Some(r) => r,
        None => {
            return r##"<p class="text-slate-500 text-xs">医療データがありません</p>"##.to_string()
        }
    };
    let hospitals = get_i64(r, "general_hospitals");
    let clinics = get_i64(r, "general_clinics");
    let dental = get_i64(r, "dental_clinics");
    let physicians = get_i64(r, "physicians");

    let cards = [
        ("🏥", "一般病院", hospitals, "text-red-400"),
        ("🏥", "一般診療所", clinics, "text-rose-400"),
        ("🦷", "歯科診療所", dental, "text-pink-400"),
        ("👨‍⚕️", "医師数", physicians, "text-orange-400"),
    ];

    let mut html = String::new();
    for (icon, label, val, color) in cards {
        write!(html,
            r##"<div class="karte-mini-card">
                <div class="karte-mini-icon">{icon}</div>
                <div class="karte-mini-label">{label}</div>
                <div class="karte-mini-value {color}">{value}</div>
            </div>"##,
            icon = icon,
            label = escape_html(label),
            value = format_i64_or_dash(val, ""),
            color = color,
        ).unwrap();
    }
    html
}

/// 教育施設4枚カード
fn build_education_cards(row: Option<&Row>) -> String {
    let r = match row {
        Some(r) => r,
        None => {
            return r##"<p class="text-slate-500 text-xs">教育データがありません</p>"##.to_string()
        }
    };
    let kg = get_i64(r, "kindergartens");
    let es = get_i64(r, "elementary_schools");
    let jhs = get_i64(r, "junior_high_schools");
    let hs = get_i64(r, "high_schools");

    let cards = [
        ("🎒", "幼稚園", kg, "text-yellow-400"),
        ("📗", "小学校", es, "text-emerald-400"),
        ("📘", "中学校", jhs, "text-cyan-400"),
        ("📕", "高校", hs, "text-blue-400"),
    ];
    let mut html = String::new();
    for (icon, label, val, color) in cards {
        write!(html,
            r##"<div class="karte-mini-card">
                <div class="karte-mini-icon">{icon}</div>
                <div class="karte-mini-label">{label}</div>
                <div class="karte-mini-value {color}">{value}</div>
            </div>"##,
            icon = icon,
            label = escape_html(label),
            value = format_i64_or_dash(val, ""),
            color = color,
        ).unwrap();
    }
    html
}

/// 時間帯プロファイル (月別 x 平日昼/夜 x 休日昼/夜)
fn build_flow_profile_chart(rows: &[Row]) -> String {
    if rows.is_empty() {
        return chart_placeholder("時間帯プロファイルデータがありません");
    }
    // 4系列構築: (dayflag=1, timezone=0) 平日昼 / (1,1) 平日夜 / (0,0) 休日昼 / (0,1) 休日夜
    let label_months: Vec<String> = (1..=12).map(|m| format!("{}月", m)).collect();
    let mut wd_day = vec![0i64; 12];
    let mut wd_night = vec![0i64; 12];
    let mut hd_day = vec![0i64; 12];
    let mut hd_night = vec![0i64; 12];
    for r in rows {
        let month = get_i64(r, "month");
        let df = get_i64(r, "dayflag");
        let tz = get_i64(r, "timezone");
        let pop = get_i64(r, "pop_sum");
        if !(1..=12).contains(&month) {
            continue;
        }
        let idx = (month - 1) as usize;
        match (df, tz) {
            (1, 0) => wd_day[idx] = pop,
            (1, 1) => wd_night[idx] = pop,
            (0, 0) => hd_day[idx] = pop,
            (0, 1) => hd_night[idx] = pop,
            _ => {}
        }
    }

    let config = json!({
        "tooltip": {"trigger": "axis"},
        "legend": {"data": ["平日昼", "平日夜", "休日昼", "休日夜"], "top": 0, "textStyle": {"color": "#cbd5e1"}},
        "grid": {"left": "3%", "right": "4%", "bottom": "5%", "top": 40, "containLabel": true},
        "xAxis": {"type": "category", "data": label_months, "axisLabel": {"color": "#94a3b8"}},
        "yAxis": {"type": "value", "axisLabel": {"color": "#94a3b8"}},
        "series": [
            {"name": "平日昼", "type": "line", "data": wd_day, "smooth": true, "lineStyle": {"color": "#3B82F6"}, "itemStyle": {"color": "#3B82F6"}},
            {"name": "平日夜", "type": "line", "data": wd_night, "smooth": true, "lineStyle": {"color": "#8B5CF6"}, "itemStyle": {"color": "#8B5CF6"}},
            {"name": "休日昼", "type": "line", "data": hd_day, "smooth": true, "lineStyle": {"color": "#F59E0B"}, "itemStyle": {"color": "#F59E0B"}},
            {"name": "休日夜", "type": "line", "data": hd_night, "smooth": true, "lineStyle": {"color": "#EC4899"}, "itemStyle": {"color": "#EC4899"}},
        ]
    });
    echart_div(320, &config, "時間帯プロファイル 月別 平日休日 昼夜")
}

/// 36ヶ月トレンド (2019-2021 × 平日昼) + コロナ期 markArea
fn build_flow_monthly_trend_chart(rows: &[Row]) -> String {
    if rows.is_empty() {
        return chart_placeholder("月次トレンドデータがありません");
    }
    let mut x_labels: Vec<String> = Vec::with_capacity(36);
    let mut values: Vec<i64> = Vec::with_capacity(36);
    for r in rows {
        let year = get_i64(r, "year");
        let month = get_i64(r, "month");
        x_labels.push(format!("{}-{:02}", year, month));
        values.push(get_i64(r, "pop_sum"));
    }

    let config = json!({
        "tooltip": {"trigger": "axis"},
        "grid": {"left": "3%", "right": "4%", "bottom": "12%", "top": 20, "containLabel": true},
        "xAxis": {
            "type": "category",
            "data": x_labels,
            "axisLabel": {"color": "#94a3b8", "rotate": 45, "fontSize": 10}
        },
        "yAxis": {"type": "value", "axisLabel": {"color": "#94a3b8"}},
        "series": [{
            "name": "平日昼滞在",
            "type": "line",
            "data": values,
            "smooth": true,
            "lineStyle": {"color": "#3B82F6", "width": 2},
            "itemStyle": {"color": "#3B82F6"},
            "markArea": {
                "itemStyle": {"color": "rgba(239, 68, 68, 0.12)"},
                "data": [[
                    {"name": "緊急事態宣言期", "xAxis": "2020-04"},
                    {"xAxis": "2020-05"}
                ], [
                    {"name": "第2回宣言", "xAxis": "2021-01"},
                    {"xAxis": "2021-02"}
                ]]
            }
        }]
    });
    echart_div(300, &config, "36ヶ月滞在推移 コロナ期markArea")
}

/// 昼夜比ゲージ
fn build_daynight_gauge(ratio: Option<f64>) -> String {
    let val = match ratio {
        Some(v) => v,
        None => return chart_placeholder("昼夜比データがありません"),
    };

    // 表示用値: 0.5〜2.0 の範囲で正規化
    let display = (val * 100.0).clamp(30.0, 250.0);

    let config = json!({
        "tooltip": {"formatter": format!("昼夜比: {:.2}", val)},
        "series": [{
            "type": "gauge",
            "center": ["50%", "60%"],
            "startAngle": 210,
            "endAngle": -30,
            "min": 30,
            "max": 250,
            "splitNumber": 4,
            "axisLine": {
                "lineStyle": {
                    "width": 18,
                    "color": [
                        [0.4, "#3B82F6"],
                        [0.56, "#10B981"],
                        [0.72, "#F59E0B"],
                        [1.0, "#EF4444"]
                    ]
                }
            },
            "pointer": {"width": 5, "itemStyle": {"color": "#e2e8f0"}},
            "axisTick": {"distance": -25, "length": 6, "lineStyle": {"color": "#94a3b8"}},
            "splitLine": {"distance": -22, "length": 10, "lineStyle": {"color": "#cbd5e1"}},
            "axisLabel": {"color": "#cbd5e1", "distance": 28, "fontSize": 10},
            "detail": {
                "valueAnimation": true,
                "formatter": format!("{:.2}", val),
                "color": "#e2e8f0",
                "fontSize": 22,
                "offsetCenter": [0, "60%"]
            },
            "data": [{"value": display, "name": "昼/夜"}]
        }]
    });
    echart_div(240, &config, &format!("昼夜比ゲージ {:.2}", val))
}

/// 流入構造 (4区分 pie)
fn build_inflow_pie(rows: &[Row]) -> String {
    if rows.is_empty() {
        return chart_placeholder("流入データがありません");
    }
    let mut data: Vec<Value> = Vec::new();
    let colors = ["#3B82F6", "#10B981", "#F59E0B", "#EF4444"];
    for r in rows {
        let code = get_i64(r, "from_area");
        let pop = get_i64(r, "total_population");
        let name = fft::from_area_label(code);
        let color = colors[code.clamp(0, 3) as usize];
        data.push(json!({
            "name": name,
            "value": pop,
            "itemStyle": {"color": color}
        }));
    }

    let config = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}: {c} 人 ({d}%)"},
        "legend": {"orient": "vertical", "left": "right", "textStyle": {"color": "#cbd5e1", "fontSize": 11}},
        "series": [{
            "type": "pie",
            "radius": ["40%", "70%"],
            "center": ["38%", "50%"],
            "avoidLabelOverlap": true,
            "itemStyle": {"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2},
            "label": {"color": "#e2e8f0", "fontSize": 10, "formatter": "{d}%"},
            "data": data
        }]
    });
    echart_div(260, &config, "流入構造4区分")
}

// ========== 表示補助 ==========

fn chart_placeholder(msg: &str) -> String {
    format!(
        r##"<p class="text-slate-500 text-sm text-center py-10">{}</p>"##,
        escape_html(msg)
    )
}

fn format_i64_or_dash(n: i64, unit: &str) -> String {
    if n <= 0 {
        "—".to_string()
    } else if unit.is_empty() {
        format_number(n)
    } else {
        format!("{}{}", format_number(n), unit)
    }
}

fn format_f64_or_dash(v: f64, precision: usize) -> String {
    if v <= 0.0 || v.is_nan() || v.is_infinite() {
        "—".to_string()
    } else {
        format!("{:.1$}", v, precision)
    }
}

fn format_pct_or_dash(v: f64) -> String {
    if v <= 0.0 || v.is_nan() || v.is_infinite() {
        "—".to_string()
    } else {
        format!("{:.1}%", v)
    }
}

fn format_density_or_dash(v: f64) -> String {
    if v <= 0.0 || v.is_nan() || v.is_infinite() {
        "—".to_string()
    } else if v >= 1000.0 {
        format!("{:.0} 人/km²", v)
    } else {
        format!("{:.1} 人/km²", v)
    }
}

// ========== テスト ==========

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row_of(pairs: &[(&str, Value)]) -> Row {
        let mut r: Row = Row::new();
        for (k, v) in pairs {
            r.insert(k.to_string(), v.clone());
        }
        r
    }

    #[test]
    fn test_format_i64_or_dash() {
        assert_eq!(format_i64_or_dash(0, "人"), "—");
        assert_eq!(format_i64_or_dash(1500, "人"), "1,500人");
        assert_eq!(format_i64_or_dash(100, ""), "100");
    }

    #[test]
    fn test_format_pct_or_dash() {
        assert_eq!(format_pct_or_dash(0.0), "—");
        assert_eq!(format_pct_or_dash(12.345), "12.3%");
        assert_eq!(format_pct_or_dash(f64::NAN), "—");
    }

    #[test]
    fn test_format_density() {
        assert_eq!(format_density_or_dash(0.0), "—");
        assert_eq!(format_density_or_dash(999.9), "999.9 人/km²");
        assert_eq!(format_density_or_dash(1234.56), "1235 人/km²");
    }

    #[test]
    fn test_elderly_rate_from_pyramid() {
        let pyramid = vec![
            row_of(&[
                ("age_group", json!("0-14")),
                ("male_count", json!(100)),
                ("female_count", json!(100)),
            ]),
            row_of(&[
                ("age_group", json!("15-64")),
                ("male_count", json!(300)),
                ("female_count", json!(300)),
            ]),
            row_of(&[
                ("age_group", json!("65-74")),
                ("male_count", json!(100)),
                ("female_count", json!(100)),
            ]),
            row_of(&[
                ("age_group", json!("75+")),
                ("male_count", json!(50)),
                ("female_count", json!(50)),
            ]),
        ];
        // total = 200 (0-14) + 600 (15-64) + 200 (65-74) + 100 (75+) = 1100
        // elderly (65-74 + 75+) = 300 → rate = 300/1100 = 27.27%
        let rate = calc_elderly_rate_from_pyramid(&pyramid);
        let expected = 300.0_f64 / 1100.0_f64 * 100.0;
        assert!(
            (rate - expected).abs() < 0.01,
            "expected {:.4}, got {}",
            expected,
            rate
        );
    }

    #[test]
    fn test_elderly_rate_empty() {
        let rate = calc_elderly_rate_from_pyramid(&[]);
        assert_eq!(rate, 0.0);
    }

    #[test]
    fn test_covid_recovery_empty() {
        let ratio = calc_covid_recovery_for_karte(&[]);
        assert!(ratio.is_none());
    }

    #[test]
    fn test_covid_recovery_basic() {
        let trend = vec![
            row_of(&[
                ("year", json!(2019)),
                ("month", json!(9)),
                ("pop_sum", json!(100.0)),
            ]),
            row_of(&[
                ("year", json!(2021)),
                ("month", json!(9)),
                ("pop_sum", json!(80.0)),
            ]),
        ];
        let ratio = calc_covid_recovery_for_karte(&trend);
        assert_eq!(ratio, Some(0.8));
    }

    #[test]
    fn test_echart_div_escapes_quotes() {
        let cfg = json!({"msg": "it's test"});
        let html = echart_div(200, &cfg, "test chart");
        // シングルクォートはエスケープされているべき（属性値のdelim破壊防止）
        assert!(!html.contains("it's"));
        assert!(html.contains("it&#39;s"));
    }

    #[test]
    fn test_chart_placeholder_escapes() {
        let h = chart_placeholder("<script>alert(1)</script>");
        assert!(!h.contains("<script>"));
        assert!(h.contains("&lt;script&gt;"));
    }
}
