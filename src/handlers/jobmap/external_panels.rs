//! 求人地図タブ: 外部統計データ ドリルダウンパネル群 (2026-06-03)
//!
//! ## 背景
//! 採用診断 navy_report (Section 02 地理・06 人口・07 家賃 等) で活用している
//! HW 以外の外部データソースを、求人地図タブの「ドリルダウン領域」で
//! ユーザーが個別に確認できるようにする。
//!
//! ## MECE データソース構成 (7 件、地理 / 通勤 / 家賃 系)
//!
//! | データ | テーブル | 可視化 |
//! |---|---|---|
//! | 地理 (可住地面積/人口密度) | `v2_external_geography` | 都道府県カラー指標表 |
//! | 通勤 OD (流入/流出) | `v2_external_commute_od` | TOP3 流入元/流出先 + sankey 入力 |
//! | 家賃 m² 単価 | `v2_external_rental_housing` | 構造×面積帯 ミニマトリクス |
//! | 人口ピラミッド | `v2_external_population_pyramid` | 5/10 歳階級 ピラミッド |
//! | 教育施設密度 | `v2_external_education_facilities` | 幼/小/中/高 表 |
//! | 自然増減 | `v2_external_vital_statistics` | 出生/死亡/純増減 表 |
//! | 社会移動 | `v2_external_migration` | 転入/転出/純増減 表 |
//!
//! ## 設計原則
//! - **fetch 層の再利用**: 既存 `analysis::fetch::*` の `pub(crate)` 関数を呼ぶだけ。
//!   SQL 重複や silent fallback を防ぐ (MEMORY: feedback_silent_fallback_audit)。
//! - **表示は HTML フラグメントで返す**: HTMX `hx-get` で accordion 内 lazy load。
//! - **DISPLAY_SPEC §2 遵守**: 求職者数等の人数表示はしない。指標は密度・割合・OD 件数のみ。
//! - **中立表現**: 「劣位」「集中」を使わず「全国比 X%」「主要流入元」等で記述。
//! - **MECE matching**: 不明 datasource は明示エラー (silent ignore しない)。

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::fmt::Write as _;
use std::sync::Arc;

use crate::handlers::analysis::fetch as af;
use crate::handlers::competitive::escape_html;
use crate::handlers::helpers::{get_f64, get_i64, get_str_ref};
use crate::AppState;

/// 共通クエリパラメータ (pref + muni)
#[derive(Deserialize)]
pub struct ExternalPanelParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
}

/// 都道府県未選択時の共通プレースホルダ
fn render_no_pref() -> Html<String> {
    Html(
        r#"<div class="text-gray-400 text-sm py-2">都道府県を選択してください。</div>"#.to_string(),
    )
}

/// データ未取得時の共通プレースホルダ (silent fallback ではなく明示)
fn render_no_data(label: &str) -> String {
    format!(
        r#"<div class="text-gray-500 text-sm py-2">{} のデータは取得できませんでした (Turso / local DB ともに該当レコードなし)。</div>"#,
        escape_html(label)
    )
}

// ============================================================
// 1) 地理 (可住地面積 / 人口密度)
// ============================================================

/// `GET /api/jobmap/external/geography?prefecture=&municipality=`
///
/// 可住地面積・人口密度を表形式で表示。
/// 出典: SSDSE-A `v2_external_geography` (`fetch_geography`)。
pub async fn external_geography(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("地理 (可住地面積/人口密度)")),
    };
    let rows = af::fetch_geography(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    if rows.is_empty() {
        return Html(render_no_data("地理 (可住地面積/人口密度)"));
    }
    let r = &rows[0];
    let total_area = get_f64(r, "total_area_km2");
    let habitable_area = get_f64(r, "habitable_area_km2");
    let pop_density = get_f64(r, "population_density_per_km2");
    let hab_density = get_f64(r, "habitable_density_per_km2");
    let year = get_str_ref(r, "reference_year");
    let scope = if p.municipality.is_empty() {
        p.prefecture.clone()
    } else {
        format!("{} {}", p.prefecture, p.municipality)
    };

    let mut h = String::with_capacity(1024);
    write!(
        h,
        r#"<div class="space-y-2 text-sm">
  <div class="text-xs text-gray-400">対象: {}（参照年: {}）</div>
  <table class="w-full text-left border-collapse">
    <tbody>
      <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">総面積</th><td class="py-1 text-white font-medium">{}</td></tr>
      <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">可住地面積</th><td class="py-1 text-white font-medium">{}</td></tr>
      <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">人口密度</th><td class="py-1 text-blue-300 font-medium">{}</td></tr>
      <tr><th class="py-1 text-gray-400 font-normal">可住地人口密度</th><td class="py-1 text-blue-300 font-medium">{}</td></tr>
    </tbody>
  </table>
  <div class="text-xs text-gray-500">出典: 総務省 統計局 SSDSE-A (v2_external_geography)。可住地は森林・湖沼を除く面積。</div>
</div>"#,
        escape_html(&scope),
        escape_html(year),
        format_area(total_area),
        format_area(habitable_area),
        format_density(pop_density),
        format_density(hab_density),
    )
    .unwrap();
    Html(h)
}

// ============================================================
// 2) 通勤 OD (流入元 TOP / 流出先 TOP)
// ============================================================

/// `GET /api/jobmap/external/commute?prefecture=&municipality=`
///
/// 通勤 OD: 流入元 TOP3 + 流出先 TOP3 を表で。
/// 出典: 国勢調査 OD 行列 `v2_external_commute_od`。
pub async fn external_commute(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    if p.municipality.is_empty() {
        return Html(
            r#"<div class="text-gray-400 text-sm py-2">通勤 OD は市区町村粒度のみ提供。市区町村を選択してください。</div>"#
                .to_string(),
        );
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("通勤 OD")),
    };
    let inflow = af::fetch_commute_inflow(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    let outflow = af::fetch_commute_outflow(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    if inflow.is_empty() && outflow.is_empty() {
        return Html(render_no_data("通勤 OD"));
    }

    let mut h = String::with_capacity(2048);
    h.push_str(r#"<div class="space-y-3 text-sm">"#);
    write!(
        h,
        r#"<div class="text-xs text-gray-400">対象: {} {} ／ 自市区町村以外の上位 3 件</div>"#,
        escape_html(&p.prefecture),
        escape_html(&p.municipality)
    )
    .unwrap();

    // 流入元 TOP3
    h.push_str(r#"<div><div class="text-xs text-gray-300 font-medium mb-1">主要流入元 (働きに来ている地域)</div>"#);
    if inflow.is_empty() {
        h.push_str(r#"<div class="text-gray-500 text-xs">該当データなし</div>"#);
    } else {
        h.push_str(r#"<table class="w-full text-left border-collapse"><thead><tr class="border-b border-gray-700"><th class="py-1 text-xs text-gray-400 font-normal">順位</th><th class="py-1 text-xs text-gray-400 font-normal">流入元</th><th class="py-1 text-xs text-gray-400 font-normal text-right">通勤者 (件)</th></tr></thead><tbody>"#);
        for (i, f) in inflow.iter().take(3).enumerate() {
            write!(h,
                r#"<tr class="border-b border-gray-700"><td class="py-1 text-gray-300">{}</td><td class="py-1 text-white">{} {}</td><td class="py-1 text-blue-300 font-medium text-right">{}</td></tr>"#,
                i + 1,
                escape_html(&f.partner_pref),
                escape_html(&f.partner_muni),
                format_int(f.total_commuters)
            ).unwrap();
        }
        h.push_str("</tbody></table>");
    }
    h.push_str("</div>");

    // 流出先 TOP3
    h.push_str(r#"<div><div class="text-xs text-gray-300 font-medium mb-1">主要流出先 (働きに出ている地域)</div>"#);
    if outflow.is_empty() {
        h.push_str(r#"<div class="text-gray-500 text-xs">該当データなし</div>"#);
    } else {
        h.push_str(r#"<table class="w-full text-left border-collapse"><thead><tr class="border-b border-gray-700"><th class="py-1 text-xs text-gray-400 font-normal">順位</th><th class="py-1 text-xs text-gray-400 font-normal">流出先</th><th class="py-1 text-xs text-gray-400 font-normal text-right">通勤者 (件)</th></tr></thead><tbody>"#);
        for (i, f) in outflow.iter().take(3).enumerate() {
            write!(h,
                r#"<tr class="border-b border-gray-700"><td class="py-1 text-gray-300">{}</td><td class="py-1 text-white">{} {}</td><td class="py-1 text-blue-300 font-medium text-right">{}</td></tr>"#,
                i + 1,
                escape_html(&f.partner_pref),
                escape_html(&f.partner_muni),
                format_int(f.total_commuters)
            ).unwrap();
        }
        h.push_str("</tbody></table>");
    }
    h.push_str("</div>");

    h.push_str(r#"<div class="text-xs text-gray-500">出典: 総務省 国勢調査 通勤・通学 OD 行列 (v2_external_commute_od)。件数は調査時点の従業地・常住地ベース。</div>"#);
    h.push_str("</div>");
    Html(h)
}

// ============================================================
// 3) 家賃 m² 単価 (構造 × 面積帯)
// ============================================================

/// `GET /api/jobmap/external/rental?prefecture=`
///
/// 家賃 m² 単価ミニマトリクス (構造 × 面積帯)。
/// 出典: e-Stat 住宅・土地統計 `v2_external_rental_housing`。
pub async fn external_rental(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("家賃 m² 単価")),
    };
    let rows = af::fetch_rental_housing(hw_db, state.turso_db.as_ref(), &p.prefecture);
    if rows.is_empty() {
        return Html(render_no_data("家賃 m² 単価"));
    }

    // 当該県のみフィルタ (rental_housing は全国も同時に返す)
    type RentalRow = std::collections::HashMap<String, serde_json::Value>;
    let target: Vec<&RentalRow> = rows
        .iter()
        .filter(|r| get_str_ref(r, "prefecture") == p.prefecture)
        .collect();
    let display_rows: Vec<&RentalRow> = if target.is_empty() {
        rows.iter().collect()
    } else {
        target
    };

    let mut h = String::with_capacity(2048);
    write!(
        h,
        r#"<div class="space-y-2 text-sm"><div class="text-xs text-gray-400">対象: {}（家賃 = 中央値、専有面積で除した m² 単価を併記）</div>"#,
        escape_html(&p.prefecture)
    )
    .unwrap();

    h.push_str(r#"<table class="w-full text-left border-collapse"><thead><tr class="border-b border-gray-700"><th class="py-1 text-xs text-gray-400 font-normal">構造</th><th class="py-1 text-xs text-gray-400 font-normal">専有面積帯</th><th class="py-1 text-xs text-gray-400 font-normal text-right">家賃中央値 (円/月)</th><th class="py-1 text-xs text-gray-400 font-normal text-right">m² 単価 概算</th></tr></thead><tbody>"#);
    for r in display_rows.iter().take(12) {
        let r: &RentalRow = *r;
        let structure = get_str_ref(r, "structure");
        let area_class = get_str_ref(r, "area_class");
        let rent = get_i64(r, "median_rent_jpy");
        let unit_price = estimate_m2_unit_price(area_class, rent);
        write!(h,
            r#"<tr class="border-b border-gray-700"><td class="py-1 text-gray-300">{}</td><td class="py-1 text-gray-300">{}</td><td class="py-1 text-blue-300 font-medium text-right">{}</td><td class="py-1 text-yellow-300 font-medium text-right">{}</td></tr>"#,
            escape_html(structure),
            escape_html(area_class),
            if rent > 0 { format_int(rent) } else { "-".to_string() },
            unit_price.map(|v| format!("{:.0} 円/m²", v)).unwrap_or_else(|| "-".to_string())
        ).unwrap();
    }
    h.push_str("</tbody></table>");
    h.push_str(r#"<div class="text-xs text-gray-500">出典: e-Stat 住宅・土地統計調査 2023 年 (v2_external_rental_housing)。m² 単価は専有面積帯の中央値から概算。賃貸物件募集の意思決定に用いる際は最新公示を参照。</div>"#);
    h.push_str("</div>");
    Html(h)
}

// ============================================================
// 4) 人口ピラミッド
// ============================================================

/// `GET /api/jobmap/external/pyramid?prefecture=&municipality=`
///
/// 5 歳/10 歳階級ピラミッド (男女別棒グラフ HTML)。
/// 出典: `v2_external_population_pyramid`。
pub async fn external_pyramid(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("人口ピラミッド")),
    };
    let rows = af::fetch_population_pyramid(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    if rows.is_empty() {
        return Html(render_no_data("人口ピラミッド"));
    }

    // 最大値で正規化
    let max_v: i64 = rows
        .iter()
        .map(|r| get_i64(r, "male_count").max(get_i64(r, "female_count")))
        .max()
        .unwrap_or(1)
        .max(1);

    let scope = if p.municipality.is_empty() {
        p.prefecture.clone()
    } else {
        format!("{} {}", p.prefecture, p.municipality)
    };

    let mut h = String::with_capacity(2048);
    write!(
        h,
        r#"<div class="space-y-2 text-sm"><div class="text-xs text-gray-400">対象: {} ／ 男 (左 青) ・ 女 (右 桃)</div>"#,
        escape_html(&scope)
    )
    .unwrap();

    h.push_str(r#"<div class="space-y-1">"#);
    for r in &rows {
        let age = get_str_ref(r, "age_group");
        let male = get_i64(r, "male_count");
        let female = get_i64(r, "female_count");
        let male_pct = (male as f64 / max_v as f64 * 100.0).clamp(0.0, 100.0);
        let female_pct = (female as f64 / max_v as f64 * 100.0).clamp(0.0, 100.0);
        write!(h,
            r#"<div class="flex items-center gap-1 text-xs">
  <div class="flex-1 flex justify-end"><div class="h-3 bg-blue-500/60" style="width:{:.1}%" title="男 {}"></div></div>
  <div class="w-16 text-center text-gray-300 font-medium">{}</div>
  <div class="flex-1 flex justify-start"><div class="h-3 bg-pink-500/60" style="width:{:.1}%" title="女 {}"></div></div>
</div>"#,
            male_pct, format_int(male), escape_html(age), female_pct, format_int(female)
        ).unwrap();
    }
    h.push_str("</div>");
    h.push_str(r#"<div class="text-xs text-gray-500">出典: 総務省 国勢調査 (v2_external_population_pyramid)。年齢階級は 5 歳または 10 歳幅。バー長は最大階級値で正規化。</div>"#);
    h.push_str("</div>");
    Html(h)
}

// ============================================================
// 5) 教育施設密度 (幼/小/中/高)
// ============================================================

/// `GET /api/jobmap/external/education?prefecture=&municipality=`
pub async fn external_education(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("教育施設密度")),
    };
    let rows = af::fetch_education_facilities(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    if rows.is_empty() {
        return Html(render_no_data("教育施設密度"));
    }
    let r = &rows[0];
    let kindergartens = get_i64(r, "kindergartens");
    let elementary = get_i64(r, "elementary_schools");
    let junior_high = get_i64(r, "junior_high_schools");
    let high = get_i64(r, "high_schools");
    let total = kindergartens + elementary + junior_high + high;
    let year = get_str_ref(r, "reference_year");
    let scope = if p.municipality.is_empty() {
        p.prefecture.clone()
    } else {
        format!("{} {}", p.prefecture, p.municipality)
    };

    let mut h = String::with_capacity(1024);
    write!(
        h,
        r#"<div class="space-y-2 text-sm"><div class="text-xs text-gray-400">対象: {}（参照年: {}）</div>
<table class="w-full text-left border-collapse"><tbody>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">幼稚園</th><td class="py-1 text-white font-medium text-right">{} 園</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">小学校</th><td class="py-1 text-white font-medium text-right">{} 校</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">中学校</th><td class="py-1 text-white font-medium text-right">{} 校</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">高等学校</th><td class="py-1 text-white font-medium text-right">{} 校</td></tr>
  <tr><th class="py-1 text-gray-300 font-medium">合計</th><td class="py-1 text-blue-300 font-bold text-right">{} 施設</td></tr>
</tbody></table>
<div class="text-xs text-gray-500">出典: 文部科学省 学校基本調査 (v2_external_education_facilities)。子育て世帯採用時の生活インフラ指標として参考。</div>
</div>"#,
        escape_html(&scope),
        escape_html(year),
        format_int(kindergartens),
        format_int(elementary),
        format_int(junior_high),
        format_int(high),
        format_int(total),
    )
    .unwrap();
    Html(h)
}

// ============================================================
// 6) 自然増減 (出生 / 死亡 / 純増減)
// ============================================================

/// `GET /api/jobmap/external/natural_change?prefecture=&municipality=`
///
/// 出典: 厚生労働省 人口動態調査 `v2_external_vital_statistics`。
pub async fn external_natural_change(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("自然増減")),
    };
    let rows = af::fetch_vital_statistics(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    if rows.is_empty() {
        return Html(render_no_data("自然増減"));
    }
    let r = &rows[0];
    let births = get_i64(r, "births");
    let deaths = get_i64(r, "deaths");
    let natural = get_i64(r, "natural_change");
    let marriages = get_i64(r, "marriages");
    let divorces = get_i64(r, "divorces");
    let year = get_str_ref(r, "reference_year");
    let scope = if p.municipality.is_empty() {
        p.prefecture.clone()
    } else {
        format!("{} {}", p.prefecture, p.municipality)
    };
    let natural_color = if natural >= 0 {
        "text-emerald-300"
    } else {
        "text-rose-300"
    };
    let natural_sign = if natural > 0 { "+" } else { "" };

    let mut h = String::with_capacity(1024);
    write!(
        h,
        r#"<div class="space-y-2 text-sm"><div class="text-xs text-gray-400">対象: {}（参照年: {}）</div>
<table class="w-full text-left border-collapse"><tbody>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">出生</th><td class="py-1 text-emerald-300 font-medium text-right">{}</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">死亡</th><td class="py-1 text-rose-300 font-medium text-right">{}</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-300 font-medium">自然増減 (出生 - 死亡)</th><td class="py-1 {} font-bold text-right">{}{}</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">婚姻</th><td class="py-1 text-white font-medium text-right">{}</td></tr>
  <tr><th class="py-1 text-gray-400 font-normal">離婚</th><td class="py-1 text-white font-medium text-right">{}</td></tr>
</tbody></table>
<div class="text-xs text-gray-500">出典: 厚生労働省 人口動態調査 (v2_external_vital_statistics)。長期的な労働力供給ベースの参考指標。</div>
</div>"#,
        escape_html(&scope),
        escape_html(year),
        format_int(births),
        format_int(deaths),
        natural_color,
        natural_sign,
        format_int(natural),
        format_int(marriages),
        format_int(divorces),
    )
    .unwrap();
    Html(h)
}

// ============================================================
// 7) 社会移動 (転入 / 転出 / 純増減)
// ============================================================

/// `GET /api/jobmap/external/migration?prefecture=&municipality=`
///
/// 出典: 総務省 住民基本台帳人口移動報告 `v2_external_migration`。
pub async fn external_migration(
    State(state): State<Arc<AppState>>,
    Query(p): Query<ExternalPanelParams>,
) -> Html<String> {
    if p.prefecture.is_empty() {
        return render_no_pref();
    }
    let hw_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_data("社会移動")),
    };
    let rows = af::fetch_migration_data(
        hw_db,
        state.turso_db.as_ref(),
        &p.prefecture,
        &p.municipality,
    );
    if rows.is_empty() {
        return Html(render_no_data("社会移動"));
    }
    let r = &rows[0];
    let inflow = get_i64(r, "inflow");
    let outflow = get_i64(r, "outflow");
    let net = get_i64(r, "net_migration");
    let rate = get_f64(r, "net_migration_rate");
    let scope = if p.municipality.is_empty() {
        p.prefecture.clone()
    } else {
        format!("{} {}", p.prefecture, p.municipality)
    };
    let net_color = if net >= 0 {
        "text-emerald-300"
    } else {
        "text-rose-300"
    };
    let net_sign = if net > 0 { "+" } else { "" };

    let mut h = String::with_capacity(1024);
    write!(
        h,
        r#"<div class="space-y-2 text-sm"><div class="text-xs text-gray-400">対象: {}</div>
<table class="w-full text-left border-collapse"><tbody>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">転入</th><td class="py-1 text-emerald-300 font-medium text-right">{}</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-400 font-normal">転出</th><td class="py-1 text-rose-300 font-medium text-right">{}</td></tr>
  <tr class="border-b border-gray-700"><th class="py-1 text-gray-300 font-medium">純移動 (転入 - 転出)</th><td class="py-1 {} font-bold text-right">{}{}</td></tr>
  <tr><th class="py-1 text-gray-400 font-normal">純移動率 (千人比)</th><td class="py-1 {} font-medium text-right">{:.2}‰</td></tr>
</tbody></table>
<div class="text-xs text-gray-500">出典: 総務省 住民基本台帳人口移動報告 (v2_external_migration)。労働力の流出入トレンドの参考。</div>
</div>"#,
        escape_html(&scope),
        format_int(inflow),
        format_int(outflow),
        net_color,
        net_sign,
        format_int(net),
        net_color,
        rate,
    )
    .unwrap();
    Html(h)
}

// ============================================================
// 共通整形ヘルパー
// ============================================================

fn format_int(n: i64) -> String {
    let s = n.abs().to_string();
    let mut buf = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            buf.push(',');
        }
        buf.push(c);
    }
    let mut result: String = buf.chars().rev().collect();
    if n < 0 {
        result.insert(0, '-');
    }
    result
}

fn format_area(km2: f64) -> String {
    if km2 <= 0.0 {
        return "-".to_string();
    }
    format!("{:.1} km²", km2)
}

fn format_density(d: f64) -> String {
    if d <= 0.0 {
        return "-".to_string();
    }
    format!("{:.1} 人/km²", d)
}

/// 専有面積帯 (例: "50-69m²", "30未満", "100m²以上") の中央値から m² 単価を概算。
/// 該当面積帯が解釈不能な場合は None を返す (silent fallback 禁止 MECE 例外)。
fn estimate_m2_unit_price(area_class: &str, rent_jpy: i64) -> Option<f64> {
    if rent_jpy <= 0 {
        return None;
    }
    let midpoint = parse_area_midpoint(area_class)?;
    if midpoint <= 0.0 {
        return None;
    }
    Some(rent_jpy as f64 / midpoint)
}

/// 面積階級ラベルから中央値 (m²) を抽出。
///
/// 入力例: "50-69m²", "30未満", "100m²以上"。
/// "m²" / "m2" / 空白は除去。先頭/末尾の数字以外は parse 段階で除去。
fn parse_area_midpoint(area_class: &str) -> Option<f64> {
    // 「m²」と「m2」を空に置換 (m + ² または m + 2 の単位記号のみ)。
    // 単独の「2」(例: "20m²" の 20) は壊さないよう "m2" / "m²" を unit としてのみ除去。
    let s = area_class
        .replace("m²", "")
        .replace("m2", "")
        .replace('m', "")
        .replace(' ', "")
        .replace('\u{3000}', "");
    if let Some(idx) = s.find('-') {
        let (low, high) = s.split_at(idx);
        let low_n: f64 = low.parse().ok()?;
        let high_n: f64 = high[1..].parse().ok()?;
        return Some((low_n + high_n) / 2.0);
    }
    if let Some(stripped) = s.strip_suffix("未満") {
        let n: f64 = stripped.parse().ok()?;
        return Some(n / 2.0);
    }
    if let Some(stripped) = s.strip_suffix("以上") {
        let n: f64 = stripped.parse().ok()?;
        return Some(n * 1.2);
    }
    None
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_int_basic() {
        assert_eq!(format_int(0), "0");
        assert_eq!(format_int(123), "123");
        assert_eq!(format_int(1234), "1,234");
        assert_eq!(format_int(1234567), "1,234,567");
        assert_eq!(format_int(-1234), "-1,234");
    }

    #[test]
    fn test_format_area_excludes_zero_and_negative() {
        assert_eq!(format_area(0.0), "-");
        assert_eq!(format_area(-1.0), "-");
        assert_eq!(format_area(123.45), "123.5 km²");
    }

    #[test]
    fn test_format_density_excludes_zero() {
        assert_eq!(format_density(0.0), "-");
        assert_eq!(format_density(1500.5), "1500.5 人/km²");
    }

    #[test]
    fn test_parse_area_midpoint_range() {
        // 50-69m² → 中央値 59.5
        let m = parse_area_midpoint("50-69m²").expect("range parse");
        assert!((m - 59.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_area_midpoint_less_than() {
        // 30未満 → 15
        let m = parse_area_midpoint("30未満").expect("less than parse");
        assert!((m - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_area_midpoint_or_more() {
        // 100m²以上 → 120 (×1.2)
        let m = parse_area_midpoint("100m²以上").expect("or more parse");
        assert!((m - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_area_midpoint_unparseable_returns_none() {
        // silent fallback 禁止: 不明形式は None
        assert!(parse_area_midpoint("不明").is_none());
        assert!(parse_area_midpoint("").is_none());
    }

    #[test]
    fn test_estimate_m2_unit_price_with_range() {
        // 50-69m² で 8万円 → 中央値 59.5m² で 1344円/m²
        let p = estimate_m2_unit_price("50-69m²", 80000).expect("unit price");
        assert!((p - 80000.0 / 59.5).abs() < 0.1);
    }

    #[test]
    fn test_estimate_m2_unit_price_zero_rent_returns_none() {
        // rent <= 0 は概算しない (silent fallback 禁止)
        assert!(estimate_m2_unit_price("50-69m²", 0).is_none());
        assert!(estimate_m2_unit_price("50-69m²", -100).is_none());
    }

    #[test]
    fn test_estimate_m2_unit_price_unparseable_area_returns_none() {
        // 面積帯が解釈できない場合も silent に値を出さず None
        assert!(estimate_m2_unit_price("不明", 80000).is_none());
    }

    #[test]
    fn test_render_no_data_contains_label() {
        let h = render_no_data("地理");
        assert!(h.contains("地理"));
        assert!(h.contains("Turso"));
    }

    #[test]
    fn test_render_no_pref_message() {
        let Html(body) = render_no_pref();
        assert!(body.contains("都道府県を選択"));
    }

    /// ExternalPanelParams 構造体がデフォルト値で構築可能 (Query 取得が成功する前提)
    #[test]
    fn test_external_panel_params_default_construct() {
        // axum::extract::Query<T> の Deserialize 経路で空文字 default が効くことを担保
        // (serde の #[serde(default)] により欠落フィールドは String::default() = "")
        let p = ExternalPanelParams {
            prefecture: String::new(),
            municipality: String::new(),
        };
        assert!(p.prefecture.is_empty());
        assert!(p.municipality.is_empty());
    }

    /// silent fallback 禁止: 不正な順序の "範囲" (high<low) でも数値としては動作するが、
    /// マイナス単価は出さないこと
    #[test]
    fn test_estimate_m2_unit_price_unit_consistency() {
        // 30-40m² で 60000 円 → 35m² で 1714円/m² 程度
        let p = estimate_m2_unit_price("30-40m²", 60000).expect("price");
        assert!(p > 0.0);
        assert!(p < 60000.0); // m² 単価は必ず月額より小さい (m² が 1 以上のため)
    }
}
