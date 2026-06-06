//! 未活用外部統計テーブルの活用層（地域カルテ「地域経済・環境補足」セクション向け）
//!
//! # 目的
//! 投入済みだが地域カルテで未活用だった外部統計 5 テーブルを、採用営業の文脈で活用する。
//! - `v2_external_business_dynamics`（開廃業率 = 採用市場動態）
//! - `v2_external_car_ownership`（車保有率 = 通勤圏）
//! - `v2_external_land_price`（地価 = 生活コスト指標）
//! - `v2_external_boj_tankan`（業況DI = 全国景況、※全国粒度）
//! - `v2_external_climate`（降雪日数等 = 環境補足、※環境補足情報）
//!
//! # 設計方針
//! - fetch 関数自体は既存 `subtab5_phase4` / `subtab5_phase4_7` に存在するため、本モジュールは
//!   それらを呼び出して Row → 型付き構造体へ変換し、最新値抽出・集計を担う。
//! - **silent fallback 禁止**: テーブル未接続・0 件は `None` / 空 Vec で表現し、
//!   呼び出し側（karte.rs）で「データなし」を明示メッセージ表示する。NULL→0 の誤誘導をしない。
//! - **単位一貫性**: `yoy_change_pct` は % 。`cars_per_100people` は 100 人あたり台数。混同しない。
//! - **相関≠因果**: So What 文言は「傾向」「可能性」表現に留める（render 側で付与）。
//! - **粒度明記**: boj_tankan = 全国値、climate = 環境補足。地域別と誤認させない。

use crate::handlers::helpers::{get_f64_opt, get_i64_opt, get_str, Row};

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;

// ============================================================
// 型付き構造体（Row → DTO 変換）
// ============================================================

/// 開廃業動態（最新年度の 1 スナップショット）。採用市場動態の指標。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BusinessDynamicsLatest {
    pub prefecture: String,
    pub fiscal_year: i64,
    /// 開業率（%）。新設事業所数 / 既存事業所数。
    pub opening_rate: Option<f64>,
    /// 廃業率（%）。
    pub closure_rate: Option<f64>,
    pub new_establishments: Option<i64>,
    pub closed_establishments: Option<i64>,
}

/// 車保有率（最新年）。通勤圏の広さの代理指標。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CarOwnershipLatest {
    pub prefecture: String,
    pub year: i64,
    /// 100 人あたり自家用車台数（台 / 100 人）。
    pub cars_per_100people: Option<f64>,
}

/// 地価（用途別、最新年の一覧）。生活コストの代理指標。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LandPriceItem {
    pub prefecture: String,
    pub land_use: String,
    /// 平均地価（円 / m²）。
    pub avg_price_per_sqm: Option<f64>,
    /// 前年比変化率（%）。
    pub yoy_change_pct: Option<f64>,
    pub year: i64,
    pub point_count: Option<i64>,
}

/// 日銀短観 業況DI（全国粒度・最新調査回の 1 行）。全国景況の先行指標。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BojTankanLatest {
    pub survey_date: String,
    pub industry_j: String,
    pub enterprise_size: String,
    /// 業況判断DI（「良い」− 「悪い」、%ポイント。正負あり）。
    pub di_value: Option<f64>,
}

/// 気候（最新年度の 1 スナップショット）。環境補足情報。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClimateLatest {
    pub prefecture: String,
    pub fiscal_year: i64,
    pub avg_temperature: Option<f64>,
    pub max_temperature: Option<f64>,
    pub min_temperature: Option<f64>,
    /// 降雪日数（日 / 年）。通勤環境の指標。
    pub snow_days: Option<f64>,
    pub sunshine_hours: Option<f64>,
}

/// 「地域経済・環境補足」セクションの取得結果バンドル。
///
/// 各要素は `None` / 空 Vec で「データなし」を表現する（NULL→0 の誤誘導を避ける）。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ExternalExtraBundle {
    pub business_dynamics: Option<BusinessDynamicsLatest>,
    pub car_ownership: Option<CarOwnershipLatest>,
    pub land_price: Vec<LandPriceItem>,
    pub boj_tankan: Vec<BojTankanLatest>,
    pub climate: Option<ClimateLatest>,
}

// ============================================================
// Row → DTO 変換ヘルパー（純粋関数。テスト容易）
// ============================================================

/// 開廃業動態の Row 群（fiscal_year 昇順）から最新年度を抽出して DTO 化。
///
/// 空入力 → `None`（silent fallback 禁止: 呼び出し側で「データなし」表示）。
pub(crate) fn to_business_dynamics_latest(rows: &[Row]) -> Option<BusinessDynamicsLatest> {
    // fiscal_year 最大の行を採用（クエリは昇順だが防御的に max を取る）
    let latest = rows
        .iter()
        .max_by_key(|r| get_i64_opt(r, "fiscal_year").unwrap_or(i64::MIN))?;
    let fiscal_year = get_i64_opt(latest, "fiscal_year")?;
    Some(BusinessDynamicsLatest {
        prefecture: get_str(latest, "prefecture"),
        fiscal_year,
        opening_rate: get_f64_opt(latest, "opening_rate"),
        closure_rate: get_f64_opt(latest, "closure_rate"),
        new_establishments: get_i64_opt(latest, "new_establishments"),
        closed_establishments: get_i64_opt(latest, "closed_establishments"),
    })
}

/// 車保有率の Row 群から最新年を抽出して DTO 化。空入力 → `None`。
pub(crate) fn to_car_ownership_latest(rows: &[Row]) -> Option<CarOwnershipLatest> {
    let latest = rows
        .iter()
        .max_by_key(|r| get_i64_opt(r, "year").unwrap_or(i64::MIN))?;
    let year = get_i64_opt(latest, "year")?;
    Some(CarOwnershipLatest {
        prefecture: get_str(latest, "prefecture"),
        year,
        cars_per_100people: get_f64_opt(latest, "cars_per_100people"),
    })
}

/// 地価の Row 群を用途別 DTO の Vec に変換。空入力 → 空 Vec。
pub(crate) fn to_land_price_items(rows: &[Row]) -> Vec<LandPriceItem> {
    rows.iter()
        .map(|r| LandPriceItem {
            prefecture: get_str(r, "prefecture"),
            land_use: get_str(r, "land_use"),
            avg_price_per_sqm: get_f64_opt(r, "avg_price_per_sqm"),
            yoy_change_pct: get_f64_opt(r, "yoy_change_pct"),
            year: get_i64_opt(r, "year").unwrap_or(0),
            point_count: get_i64_opt(r, "point_count"),
        })
        .collect()
}

/// 日銀短観の Row 群（survey_date 降順）から最新調査回の業況DI（business）を抽出。
///
/// fetch_boj_tankan は製造業/非製造業 × business/employment を返すため、
/// ここでは最新 survey_date の `di_type='business'` 相当（di_value を持つ）行を最大 4 件採用。
/// 入力に di_type 列がある場合は business のみに絞る。空入力 → 空 Vec。
pub(crate) fn to_boj_tankan_latest(rows: &[Row]) -> Vec<BojTankanLatest> {
    if rows.is_empty() {
        return vec![];
    }
    // 最新 survey_date を特定
    let latest_date = rows.iter().map(|r| get_str(r, "survey_date")).max();
    let latest_date = match latest_date {
        Some(d) if !d.is_empty() => d,
        _ => return vec![],
    };
    rows.iter()
        .filter(|r| get_str(r, "survey_date") == latest_date)
        // di_type 列がある場合は business に限定（業況判断DI）。無い場合は通過。
        .filter(|r| {
            let t = get_str(r, "di_type");
            t.is_empty() || t == "business"
        })
        .map(|r| BojTankanLatest {
            survey_date: get_str(r, "survey_date"),
            industry_j: get_str(r, "industry_j"),
            enterprise_size: get_str(r, "enterprise_size"),
            di_value: get_f64_opt(r, "di_value"),
        })
        .collect()
}

/// 気候の Row 群（fiscal_year 昇順）から最新年度を抽出して DTO 化。空入力 → `None`。
pub(crate) fn to_climate_latest(rows: &[Row]) -> Option<ClimateLatest> {
    let latest = rows
        .iter()
        .max_by_key(|r| get_i64_opt(r, "fiscal_year").unwrap_or(i64::MIN))?;
    let fiscal_year = get_i64_opt(latest, "fiscal_year")?;
    Some(ClimateLatest {
        prefecture: get_str(latest, "prefecture"),
        fiscal_year,
        avg_temperature: get_f64_opt(latest, "avg_temperature"),
        max_temperature: get_f64_opt(latest, "max_temperature"),
        min_temperature: get_f64_opt(latest, "min_temperature"),
        snow_days: get_f64_opt(latest, "snow_days"),
        sunshine_hours: get_f64_opt(latest, "sunshine_hours"),
    })
}

// ============================================================
// fetch + 集計（既存 fetch 関数を再利用）
// ============================================================

/// 「地域経済・環境補足」セクションのデータを一括取得。
///
/// - business_dynamics / car_ownership / land_price / climate は `pref` 完全一致（都道府県粒度）。
/// - boj_tankan は全国粒度（prefecture 列なし）のため `pref` を渡さず最新調査回を取得。
///
/// 各 fetch は内部で Turso 優先 → ローカル DB フォールバック（`query_turso_or_local`）。
/// テーブル未接続・0 件は `None` / 空 Vec として返り、render 側で明示メッセージ表示される。
pub(crate) fn fetch_external_extra(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
) -> ExternalExtraBundle {
    // pref が空の場合（都道府県未選択）は地域別データを取らない。
    // ただし boj_tankan は全国値なので取得する。
    let business_dynamics = if pref.is_empty() {
        None
    } else {
        to_business_dynamics_latest(&super::fetch_business_dynamics(db, turso, pref))
    };
    let car_ownership = if pref.is_empty() {
        None
    } else {
        to_car_ownership_latest(&super::fetch_car_ownership(db, turso, pref))
    };
    let land_price = if pref.is_empty() {
        vec![]
    } else {
        to_land_price_items(&super::fetch_land_price(db, turso, pref))
    };
    let climate = if pref.is_empty() {
        None
    } else {
        to_climate_latest(&super::fetch_climate(db, turso, pref))
    };

    // boj_tankan: 全国粒度（pref 無関係）
    let boj_tankan = to_boj_tankan_latest(&super::fetch_boj_tankan(db, turso));

    ExternalExtraBundle {
        business_dynamics,
        car_ownership,
        land_price,
        boj_tankan,
        climate,
    }
}

// ============================================================
// HTML レンダリング（地域カルテ用セクション）
// ============================================================

/// 「地域経済・環境補足」セクションを生成。
///
/// 主表示: business_dynamics（採用市場動態）/ car_ownership（通勤圏）/ land_price（生活コスト）
/// 補足表示: boj_tankan（全国景況）/ climate（環境補足）
///
/// すべてのデータが欠落している場合も、セクション枠と「データなし」メッセージを表示する
/// （silent に消さない）。
pub(crate) fn render_external_extra_section(b: &ExternalExtraBundle, pref: &str) -> String {
    use crate::handlers::helpers::escape_html;

    let all_empty = b.business_dynamics.is_none()
        && b.car_ownership.is_none()
        && b.land_price.is_empty()
        && b.boj_tankan.is_empty()
        && b.climate.is_none();

    let pref_label = if pref.is_empty() {
        "（都道府県未選択）".to_string()
    } else {
        escape_html(pref)
    };

    let mut html = String::with_capacity(4096);
    html.push_str(&format!(
        r##"<section class="karte-section" aria-labelledby="karte-s-ext-title">
    <h3 id="karte-s-ext-title" class="karte-section-title">🌐 地域経済・環境補足</h3>
    <p class="karte-section-hint">都道府県粒度の経済・環境指標（{pref}）。採用市場・通勤圏・生活コストの背景把握用。</p>"##,
        pref = pref_label,
    ));

    if all_empty {
        html.push_str(
            r##"<div class="karte-chart-card text-center py-6">
        <p class="text-slate-400 text-sm">地域経済・環境補足データは未投入か対象外です。</p>
        <p class="text-slate-500 text-xs mt-1">v2_external_business_dynamics / car_ownership / land_price / boj_tankan / climate のいずれも取得できませんでした。</p>
    </div>
</section>"##,
        );
        return html;
    }

    // --- 主表示: 採用市場動態 / 通勤圏 / 生活コスト ---
    html.push_str(r##"<div class="karte-chart-grid-2">"##);
    html.push_str(&render_business_dynamics_card(b.business_dynamics.as_ref()));
    html.push_str(&render_car_ownership_card(b.car_ownership.as_ref()));
    html.push_str(&render_land_price_card(&b.land_price));
    html.push_str("</div>");

    // --- 補足表示: 全国景況 + 環境 ---
    html.push_str(r##"<div class="karte-chart-grid-2">"##);
    html.push_str(&render_boj_tankan_card(&b.boj_tankan));
    html.push_str(&render_climate_card(b.climate.as_ref()));
    html.push_str("</div>");

    // 出典 caption
    html.push_str(
        r##"<p class="text-[10px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
        出典: 経済センサス（開廃業）/ 自動車検査登録情報協会（車保有）/ 地価公示（地価）/ 日本銀行 短観（業況DI・全国値）/ 気象庁（気候）。
        ※ 業況DIは全国値、気候は環境補足情報であり、いずれも市区町村別ではありません。
    </p>
</section>"##,
    );
    html
}

/// 開廃業動態カード（採用市場動態）
fn render_business_dynamics_card(d: Option<&BusinessDynamicsLatest>) -> String {
    use crate::handlers::helpers::escape_html;
    let d = match d {
        Some(d) => d,
        None => return missing_card("📈 採用市場動態（開廃業）", "開廃業データなし"),
    };
    let opening = fmt_pct_opt(d.opening_rate);
    let closure = fmt_pct_opt(d.closure_rate);
    // So What: 相関≠因果 → 「傾向」「可能性」表現
    let so_what = match (d.opening_rate, d.closure_rate) {
        (Some(o), Some(c)) if o > c => {
            "開業率が廃業率を上回る傾向。新規事業所による採用需要が生じる可能性があります。"
        }
        (Some(o), Some(c)) if c > o => {
            "廃業率が開業率を上回る傾向。離職者プールが形成される可能性があります。"
        }
        (Some(_), Some(_)) => "開業率と廃業率が拮抗する傾向です。",
        _ => "開廃業率の一部が欠損しています。",
    };
    format!(
        r##"<div class="karte-chart-card">
        <h4 class="karte-chart-title">📈 採用市場動態（開廃業）</h4>
        <div class="text-sm text-slate-200 space-y-1">
            <div>開業率 <span class="text-emerald-400 font-mono">{opening}</span></div>
            <div>廃業率 <span class="text-rose-400 font-mono">{closure}</span></div>
            <div class="text-xs text-slate-500">{year}年度（出典: 経済センサス）</div>
        </div>
        <p class="text-xs text-slate-400 mt-2">{so_what}</p>
    </div>"##,
        opening = opening,
        closure = closure,
        year = d.fiscal_year,
        so_what = escape_html(so_what),
    )
}

/// 車保有率カード（通勤圏）
fn render_car_ownership_card(d: Option<&CarOwnershipLatest>) -> String {
    use crate::handlers::helpers::escape_html;
    let d = match d {
        Some(d) => d,
        None => return missing_card("🚗 通勤圏（車保有率）", "車保有データなし"),
    };
    let cars = match d.cars_per_100people {
        Some(v) if v.is_finite() => format!("{:.1} 台/100人", v),
        _ => "—".to_string(),
    };
    // So What: 高保有 → 通勤圏拡大の「可能性」
    let so_what = match d.cars_per_100people {
        Some(v) if v >= 50.0 => {
            "車保有率が高い傾向。車通勤を前提とすると採用リーチ圏が広がる可能性があります。"
        }
        Some(_) => {
            "車保有率は中〜低水準の傾向。公共交通アクセスが採用圏に影響する可能性があります。"
        }
        None => "車保有率データが欠損しています。",
    };
    format!(
        r##"<div class="karte-chart-card">
        <h4 class="karte-chart-title">🚗 通勤圏（車保有率）</h4>
        <div class="text-sm text-slate-200 space-y-1">
            <div><span class="text-sky-400 font-mono text-lg">{cars}</span></div>
            <div class="text-xs text-slate-500">{year}年（出典: 自動車検査登録情報協会）</div>
        </div>
        <p class="text-xs text-slate-400 mt-2">{so_what}</p>
    </div>"##,
        cars = cars,
        year = d.year,
        so_what = escape_html(so_what),
    )
}

/// 地価カード（生活コスト指標）
fn render_land_price_card(items: &[LandPriceItem]) -> String {
    use crate::handlers::helpers::escape_html;
    if items.is_empty() {
        return missing_card("🏷 生活コスト（地価）", "地価データなし");
    }
    // 用途別に行を生成（最大 4 件）
    let mut rows_html = String::new();
    for it in items.iter().take(4) {
        let price = match it.avg_price_per_sqm {
            Some(v) if v.is_finite() && v > 0.0 => format!("{} 円/m²", thousands(v as i64)),
            _ => "—".to_string(),
        };
        let yoy = match it.yoy_change_pct {
            Some(v) if v.is_finite() => format!("{:+.1}%", v),
            _ => "—".to_string(),
        };
        let use_label = if it.land_use.is_empty() {
            "用途不明".to_string()
        } else {
            escape_html(&it.land_use)
        };
        rows_html.push_str(&format!(
            r##"<div>{use_label}: <span class="text-amber-300 font-mono">{price}</span> <span class="text-slate-500">(前年比 {yoy})</span></div>"##,
            use_label = use_label,
            price = price,
            yoy = yoy,
        ));
    }
    let year = items.iter().map(|i| i.year).max().unwrap_or(0);
    // So What: 地価 = 生活コスト代理。相関≠因果。
    let so_what =
        "地価は生活コストの代理指標です。給与水準の実質的な購買力評価の参考になる可能性があります。";
    format!(
        r##"<div class="karte-chart-card">
        <h4 class="karte-chart-title">🏷 生活コスト（地価）</h4>
        <div class="text-sm text-slate-200 space-y-1">
            {rows}
            <div class="text-xs text-slate-500">{year}年（出典: 地価公示）</div>
        </div>
        <p class="text-xs text-slate-400 mt-2">{so_what}</p>
    </div>"##,
        rows = rows_html,
        year = year,
        so_what = escape_html(so_what),
    )
}

/// 日銀短観カード（全国景況・補足）
fn render_boj_tankan_card(items: &[BojTankanLatest]) -> String {
    use crate::handlers::helpers::escape_html;
    if items.is_empty() {
        return missing_card("🇯🇵 全国景況（業況DI・全国値）", "短観データなし");
    }
    let mut rows_html = String::new();
    for it in items.iter().take(4) {
        let di = match it.di_value {
            Some(v) if v.is_finite() => format!("{:+.0}", v),
            _ => "—".to_string(),
        };
        let label = if it.industry_j.is_empty() {
            "業種不明".to_string()
        } else {
            escape_html(&it.industry_j)
        };
        let size = if it.enterprise_size.is_empty() {
            String::new()
        } else {
            format!("（{}）", escape_html(&it.enterprise_size))
        };
        rows_html.push_str(&format!(
            r##"<div>{label}{size}: <span class="font-mono text-indigo-300">DI {di}</span></div>"##,
            label = label,
            size = size,
            di = di,
        ));
    }
    let survey = items
        .first()
        .map(|i| escape_html(&i.survey_date))
        .unwrap_or_default();
    let so_what =
        "業況DIは採用意欲の先行指標となる可能性があります（全国値のため当該地域の動向とは一致しないことがあります）。";
    format!(
        r##"<div class="karte-chart-card">
        <h4 class="karte-chart-title">🇯🇵 全国景況（業況DI・全国値）</h4>
        <div class="text-sm text-slate-200 space-y-1">
            {rows}
            <div class="text-xs text-slate-500">{survey} 調査・全国（出典: 日本銀行 短観）</div>
        </div>
        <p class="text-xs text-slate-400 mt-2">{so_what}</p>
    </div>"##,
        rows = rows_html,
        survey = survey,
        so_what = escape_html(so_what),
    )
}

/// 気候カード（環境補足）
fn render_climate_card(d: Option<&ClimateLatest>) -> String {
    use crate::handlers::helpers::escape_html;
    let d = match d {
        Some(d) => d,
        None => return missing_card("🌡 環境補足（気候）", "気候データなし"),
    };
    let snow = match d.snow_days {
        Some(v) if v.is_finite() => format!("{:.0} 日/年", v),
        _ => "—".to_string(),
    };
    let avg_t = match d.avg_temperature {
        Some(v) if v.is_finite() => format!("{:.1}℃", v),
        _ => "—".to_string(),
    };
    let so_what = match d.snow_days {
        Some(v) if v >= 30.0 => {
            "降雪日数が多い傾向。冬季の通勤環境が採用条件の検討要素になる可能性があります。"
        }
        Some(_) => "降雪日数は少なめの傾向です。",
        None => "降雪日数データが欠損しています。",
    };
    format!(
        r##"<div class="karte-chart-card">
        <h4 class="karte-chart-title">🌡 環境補足（気候）</h4>
        <div class="text-sm text-slate-200 space-y-1">
            <div>年平均気温 <span class="font-mono text-cyan-300">{avg_t}</span></div>
            <div>降雪日数 <span class="font-mono text-blue-300">{snow}</span></div>
            <div class="text-xs text-slate-500">{year}年度・環境補足情報（出典: 気象庁）</div>
        </div>
        <p class="text-xs text-slate-400 mt-2">{so_what}</p>
    </div>"##,
        avg_t = avg_t,
        snow = snow,
        year = d.fiscal_year,
        so_what = escape_html(so_what),
    )
}

// ============================================================
// 小ヘルパー
// ============================================================

/// データ欠損カード（silent fallback 禁止: 明示メッセージ）
fn missing_card(title: &str, msg: &str) -> String {
    use crate::handlers::helpers::escape_html;
    format!(
        r##"<div class="karte-chart-card">
        <h4 class="karte-chart-title">{title}</h4>
        <p class="text-slate-500 text-xs mt-2">{msg}</p>
    </div>"##,
        title = escape_html(title),
        msg = escape_html(msg),
    )
}

/// Option<f64> を「%」付き文字列に。None / 非有限 → "—"。
fn fmt_pct_opt(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{:.2}%", x),
        _ => "—".to_string(),
    }
}

/// 整数を 3 桁区切りに（地価表示用、依存追加を避けるためローカル実装）
fn thousands(n: i64) -> String {
    let neg = n < 0;
    let digits = n.abs().to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if neg {
        format!("-{}", out)
    } else {
        out
    }
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn row(pairs: &[(&str, Value)]) -> Row {
        let mut r: Row = Row::new();
        for (k, v) in pairs {
            r.insert((*k).to_string(), v.clone());
        }
        r
    }

    // ---------- 空入力 ----------

    #[test]
    fn test_empty_inputs_return_none_or_empty() {
        assert_eq!(to_business_dynamics_latest(&[]), None);
        assert_eq!(to_car_ownership_latest(&[]), None);
        assert!(to_land_price_items(&[]).is_empty());
        assert!(to_boj_tankan_latest(&[]).is_empty());
        assert_eq!(to_climate_latest(&[]), None);
    }

    // ---------- 正常: 最新年抽出 ----------

    #[test]
    fn test_business_dynamics_picks_latest_fiscal_year() {
        let rows = vec![
            row(&[
                ("prefecture", json!("北海道")),
                ("fiscal_year", json!(2018)),
                ("opening_rate", json!(4.0)),
                ("closure_rate", json!(3.0)),
                ("new_establishments", json!(100)),
                ("closed_establishments", json!(80)),
            ]),
            row(&[
                ("prefecture", json!("北海道")),
                ("fiscal_year", json!(2021)),
                ("opening_rate", json!(5.5)),
                ("closure_rate", json!(3.2)),
                ("new_establishments", json!(120)),
                ("closed_establishments", json!(70)),
            ]),
        ];
        let d = to_business_dynamics_latest(&rows).expect("最新年が取れること");
        assert_eq!(d.fiscal_year, 2021, "最大 fiscal_year を採用");
        assert_eq!(d.opening_rate, Some(5.5));
        assert_eq!(d.closure_rate, Some(3.2));
        assert_eq!(d.prefecture, "北海道");
    }

    #[test]
    fn test_car_ownership_picks_latest_year_and_unit() {
        let rows = vec![
            row(&[
                ("prefecture", json!("富山県")),
                ("year", json!(2020)),
                ("cars_per_100people", json!(60.0)),
            ]),
            row(&[
                ("prefecture", json!("富山県")),
                ("year", json!(2023)),
                ("cars_per_100people", json!(62.5)),
            ]),
        ];
        let d = to_car_ownership_latest(&rows).unwrap();
        assert_eq!(d.year, 2023);
        // 単位検証: cars_per_100people は 100 人あたり台数（% ではない）
        assert_eq!(d.cars_per_100people, Some(62.5));
    }

    #[test]
    fn test_land_price_items_preserve_yoy_as_percent() {
        let rows = vec![
            row(&[
                ("prefecture", json!("東京都")),
                ("land_use", json!("商業地")),
                ("avg_price_per_sqm", json!(5_000_000.0)),
                ("yoy_change_pct", json!(2.3)),
                ("year", json!(2024)),
                ("point_count", json!(500)),
            ]),
            row(&[
                ("prefecture", json!("東京都")),
                ("land_use", json!("住宅地")),
                ("avg_price_per_sqm", json!(600_000.0)),
                ("yoy_change_pct", json!(-1.1)),
                ("year", json!(2024)),
                ("point_count", json!(1200)),
            ]),
        ];
        let items = to_land_price_items(&rows);
        assert_eq!(items.len(), 2);
        // 単位検証: yoy_change_pct は % のまま（比率に変換しない）
        assert_eq!(items[0].yoy_change_pct, Some(2.3));
        assert_eq!(items[1].yoy_change_pct, Some(-1.1));
        assert_eq!(items[0].land_use, "商業地");
    }

    #[test]
    fn test_boj_tankan_filters_latest_date_and_business() {
        let rows = vec![
            // 旧調査回（除外されるべき）
            row(&[
                ("survey_date", json!("2024-12-01")),
                ("industry_j", json!("製造業")),
                ("enterprise_size", json!("大企業")),
                ("di_type", json!("business")),
                ("di_value", json!(10.0)),
            ]),
            // 最新調査回 business（採用）
            row(&[
                ("survey_date", json!("2025-03-01")),
                ("industry_j", json!("製造業")),
                ("enterprise_size", json!("大企業")),
                ("di_type", json!("business")),
                ("di_value", json!(12.0)),
            ]),
            // 最新調査回 employment（除外: business のみ採用）
            row(&[
                ("survey_date", json!("2025-03-01")),
                ("industry_j", json!("製造業")),
                ("enterprise_size", json!("大企業")),
                ("di_type", json!("employment")),
                ("di_value", json!(-20.0)),
            ]),
        ];
        let items = to_boj_tankan_latest(&rows);
        assert_eq!(items.len(), 1, "最新調査回 × business のみ");
        assert_eq!(items[0].survey_date, "2025-03-01");
        assert_eq!(items[0].di_value, Some(12.0));
    }

    #[test]
    fn test_climate_picks_latest_and_keeps_snow_days() {
        let rows = vec![
            row(&[
                ("prefecture", json!("新潟県")),
                ("fiscal_year", json!(2022)),
                ("avg_temperature", json!(13.5)),
                ("snow_days", json!(45.0)),
            ]),
            row(&[
                ("prefecture", json!("新潟県")),
                ("fiscal_year", json!(2023)),
                ("avg_temperature", json!(14.0)),
                ("snow_days", json!(38.0)),
            ]),
        ];
        let d = to_climate_latest(&rows).unwrap();
        assert_eq!(d.fiscal_year, 2023);
        assert_eq!(d.snow_days, Some(38.0));
    }

    // ---------- NULL と 0 の区別（silent fallback 禁止の検証） ----------

    #[test]
    fn test_null_distinguished_from_zero() {
        // opening_rate が NULL の行 → Option::None（0.0 ではない）
        let rows = vec![row(&[
            ("prefecture", json!("沖縄県")),
            ("fiscal_year", json!(2021)),
            ("opening_rate", Value::Null),
            ("closure_rate", json!(0.0)),
            ("new_establishments", Value::Null),
            ("closed_establishments", json!(0)),
        ])];
        let d = to_business_dynamics_latest(&rows).unwrap();
        assert_eq!(d.opening_rate, None, "NULL は None（0.0 と区別）");
        assert_eq!(
            d.closure_rate,
            Some(0.0),
            "明示 0.0 は Some(0.0)（データありの 0）"
        );
        assert_eq!(d.new_establishments, None);
        assert_eq!(d.closed_establishments, Some(0));
    }

    // ---------- render: 全欠損時に明示メッセージ ----------

    #[test]
    fn test_render_all_empty_shows_explicit_message() {
        let bundle = ExternalExtraBundle::default();
        let html = render_external_extra_section(&bundle, "北海道");
        assert!(
            html.contains("未投入か対象外"),
            "全欠損時に明示メッセージを表示すること（silent に消さない）"
        );
        assert!(html.contains("地域経済・環境補足"), "セクション枠は表示");
    }

    #[test]
    fn test_render_marks_boj_as_national_and_climate_as_supplement() {
        let bundle = ExternalExtraBundle {
            business_dynamics: Some(BusinessDynamicsLatest {
                prefecture: "北海道".into(),
                fiscal_year: 2021,
                opening_rate: Some(5.0),
                closure_rate: Some(3.0),
                new_establishments: Some(100),
                closed_establishments: Some(60),
            }),
            boj_tankan: vec![BojTankanLatest {
                survey_date: "2025-03-01".into(),
                industry_j: "非製造業".into(),
                enterprise_size: "中小企業".into(),
                di_value: Some(8.0),
            }],
            climate: Some(ClimateLatest {
                prefecture: "北海道".into(),
                fiscal_year: 2023,
                avg_temperature: Some(9.0),
                max_temperature: Some(30.0),
                min_temperature: Some(-15.0),
                snow_days: Some(60.0),
                sunshine_hours: Some(1800.0),
            }),
            ..Default::default()
        };
        let html = render_external_extra_section(&bundle, "北海道");
        // 粒度明記の検証
        assert!(html.contains("全国値"), "業況DIは全国値と明記");
        assert!(html.contains("環境補足"), "気候は環境補足と明記");
        // So What が「傾向」「可能性」表現（相関≠因果）
        assert!(
            html.contains("傾向") || html.contains("可能性"),
            "So What は傾向・可能性表現"
        );
    }

    // ---------- 小ヘルパー ----------

    #[test]
    fn test_thousands_formatting() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(5_000_000), "5,000,000");
        assert_eq!(thousands(600_000), "600,000");
        assert_eq!(thousands(-1234), "-1,234");
    }

    #[test]
    fn test_fmt_pct_opt() {
        assert_eq!(fmt_pct_opt(None), "—");
        assert_eq!(fmt_pct_opt(Some(f64::NAN)), "—");
        assert_eq!(fmt_pct_opt(Some(5.5)), "5.50%");
    }
}
