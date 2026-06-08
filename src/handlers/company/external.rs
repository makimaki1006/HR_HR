//! 企業検索タブの外部統計ドリルダウン (2026-06-03 追加)
//!
//! 設計方針:
//! - 企業検索タブの下部 (`#company-external-area`) に accordion で 3 パネルを並べる
//!     1. 産業構造   : `v2_external_industry_structure` (都道府県 × 産業 × 従業者)
//!     2. 事業所構造 : `v2_external_establishments` (都道府県 × 産業 × 事業所/従業者)
//!     3. 企業セグメント : 既存 `v2_salesnow_companies` を 4 セグメント (大手 / 中堅 / 急成長 /
//!        採用活発) で代表企業を提示
//! - 3 つとも `?pref=...&muni=...` で HTMX 取得 (GET) する純粋なパーシャル
//! - 結果は HTML テーブル + caption (出典明記)
//!
//! 厳守ルール:
//! - HTML / UI 出力に「SalesNow」という固有名は **絶対に書かない**
//!   (UI では「外部企業データベース」と表記)。
//! - silent fallback 禁止: DB 未接続 / 0 行 / エラー はそれぞれ別 HTML を返す。
//! - 中立表現: 「劣位」「集中」「縮小」などの評価語を使わない。
//! - DISPLAY_SPEC v1.0 §2: 求職者「人数」推定を生成しない。本ファイルは事業所側集計のみ。

use crate::db::turso_http::{ToSqlTurso, TursoDb};
use crate::handlers::helpers::{
    escape_html, format_number, get_f64, get_i64, get_str, strip_county_prefix, Row,
};
use std::fmt::Write as _;

// ============================================================
// 1. 産業構造 (v2_external_industry_structure)
// ============================================================

/// 産業構造 (国勢調査ベース) を都道府県粒度で取得する。
///
/// `muni` が空でない場合は、当該都道府県内の市区町村粒度
/// (`city_name = ?` で完全一致) で集計する。
/// `muni` が空の場合は、`prefecture_code` で都道府県集計を取る。
/// pref → prefecture_code 変換は `crate::geo::pref_name_to_code()` を使用。
///
/// 集計コード除外規則は既存実装と同じ:
/// `AS` (分類不能), `AR` (分類不能の産業), `CR` (分類不能), `AB`, `D` を除く。
pub fn fetch_industry_structure(turso: &TursoDb, pref: &str, muni: &str) -> Vec<Row> {
    if pref.is_empty() {
        // 全国版は行数が多すぎてドリルダウン用途に不適のため、
        // pref 未指定なら空を返す (UI 側で「都道府県を選択してください」表示)。
        return Vec::new();
    }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        // 市区町村粒度: city_name 完全一致 (誤マッチ防止)
        (
            "SELECT industry_code, industry_name, \
              SUM(employees_total) AS employees_total, \
              SUM(employees_male) AS employees_male, \
              SUM(employees_female) AS employees_female \
              FROM v2_external_industry_structure \
              WHERE city_name = ?1 \
                AND industry_code NOT IN ('AS','AR','CR','AB','D') \
              GROUP BY industry_code, industry_name \
              ORDER BY employees_total DESC LIMIT 25"
                .to_string(),
            vec![muni.to_string()],
        )
    } else {
        // 都道府県粒度: pref 名 → prefecture_code 変換して SUM
        // pref_name_to_code は HashMap<&str, &str>
        let map = crate::geo::pref_name_to_code();
        let pref_code = match map.get(pref) {
            Some(code) => code.to_string(),
            None => {
                // 未知の pref 名は空を返す (silent fallback 禁止: UI 側で no-data 表示)
                return Vec::new();
            }
        };
        (
            "SELECT industry_code, industry_name, \
              SUM(employees_total) AS employees_total, \
              SUM(employees_male) AS employees_male, \
              SUM(employees_female) AS employees_female \
              FROM v2_external_industry_structure \
              WHERE prefecture_code = ?1 \
                AND industry_code NOT IN ('AS','AR','CR','AB','D') \
              GROUP BY industry_code, industry_name \
              ORDER BY employees_total DESC LIMIT 25"
                .to_string(),
            vec![pref_code],
        )
    };

    let param_refs: Vec<&dyn ToSqlTurso> = params.iter().map(|s| s as &dyn ToSqlTurso).collect();
    turso.query(&sql, &param_refs).unwrap_or_default()
}

/// 産業構造パネルの HTML をレンダリング。
///
/// 表示要素:
/// - 表 (産業 / 従業者 (千人) / シェア / 男女比)
/// - 出典 caption (国勢調査)
pub fn render_industry_structure_panel(pref: &str, muni: &str, rows: &[Row]) -> String {
    let title_scope = if muni.is_empty() {
        escape_html(pref)
    } else {
        format!("{} {}", escape_html(pref), escape_html(muni))
    };

    if pref.is_empty() {
        return panel_message_pref_required("産業構造");
    }
    if rows.is_empty() {
        return panel_message_no_data(
            "産業構造",
            &title_scope,
            "国勢調査 (v2_external_industry_structure)",
        );
    }

    let total: i64 = rows.iter().map(|r| get_i64(r, "employees_total")).sum();
    let mut html = String::with_capacity(4096);
    let _ = write!(
        html,
        r#"<div class="stat-card" id="company-ext-industry-card">
  <h3 class="text-base font-semibold text-white mb-2">産業構造 <span class="text-blue-300 text-sm">({title_scope})</span></h3>
  <div class="overflow-x-auto">
    <table class="min-w-full text-sm">
      <thead>
        <tr class="text-slate-400 border-b border-slate-700">
          <th class="text-left py-1 pr-2">産業</th>
          <th class="text-right py-1 pr-2">従業者 (千人)</th>
          <th class="text-right py-1 pr-2">シェア</th>
          <th class="text-right py-1 pr-2">女性比率</th>
        </tr>
      </thead>
      <tbody>"#
    );
    for row in rows.iter().take(20) {
        let name = escape_html(&get_str(row, "industry_name"));
        let emp = get_i64(row, "employees_total");
        let male = get_i64(row, "employees_male");
        let female = get_i64(row, "employees_female");
        let share = if total > 0 {
            (emp as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let mf_total = male + female;
        let female_pct = if mf_total > 0 {
            (female as f64 / mf_total as f64) * 100.0
        } else {
            0.0
        };
        let _ = write!(
            html,
            r#"<tr class="border-b border-slate-800"><td class="py-1 pr-2 text-slate-200">{name}</td>
<td class="py-1 pr-2 text-right text-slate-200">{emp_k}</td>
<td class="py-1 pr-2 text-right text-slate-300">{share:.1}%</td>
<td class="py-1 pr-2 text-right text-slate-300">{female_pct:.1}%</td></tr>"#,
            emp_k = format_number(emp / 1000)
        );
    }
    html.push_str("</tbody></table></div>\n");
    html.push_str(
        r#"<p class="text-xs text-slate-500 mt-2">出典: 総務省統計局 国勢調査 (v2_external_industry_structure)。集計不能コード (AS/AR/CR/AB/D) は除外。</p>
</div>"#,
    );
    html
}

// ============================================================
// 2. 事業所数 × 規模帯 (v2_external_establishments)
// ============================================================

/// 事業所数 × 規模帯 (経済センサス) を都道府県粒度で取得。
///
/// 既存実装 (analysis/fetch/subtab5_phase4.rs の `fetch_establishments`) を踏襲し、
/// `prefecture` カラムベースで集計する。市区町村粒度の集計は本テーブルに
/// city_code が未収録のため、`muni` 指定時も都道府県集計を返す
/// (UI 側で caption に「市区町村粒度未対応」と注記)。
pub fn fetch_establishments(turso: &TursoDb, pref: &str) -> Vec<Row> {
    if pref.is_empty() {
        return Vec::new();
    }
    let sql = "SELECT industry_code AS industry, industry_name, \
               SUM(establishments) AS establishment_count, \
               SUM(employees) AS employees, \
               MAX(reference_year) AS reference_year \
               FROM v2_external_establishments \
               WHERE prefecture = ?1 AND industry_code <> 'ALL' \
               GROUP BY industry_code, industry_name \
               ORDER BY establishment_count DESC LIMIT 25";
    let pref_owned = pref.to_string();
    let params: Vec<&dyn ToSqlTurso> = vec![&pref_owned];
    turso.query(sql, &params).unwrap_or_default()
}

/// 事業所数パネルの HTML をレンダリング。
pub fn render_establishments_panel(pref: &str, muni: &str, rows: &[Row]) -> String {
    let title_scope = if muni.is_empty() {
        escape_html(pref)
    } else {
        format!("{} {}", escape_html(pref), escape_html(muni))
    };

    if pref.is_empty() {
        return panel_message_pref_required("事業所構造");
    }
    if rows.is_empty() {
        return panel_message_no_data(
            "事業所構造",
            &title_scope,
            "経済センサス (v2_external_establishments)",
        );
    }

    let total_est: i64 = rows.iter().map(|r| get_i64(r, "establishment_count")).sum();
    let max_year = rows
        .iter()
        .map(|r| get_i64(r, "reference_year"))
        .max()
        .unwrap_or(0);

    let mut html = String::with_capacity(4096);
    let _ = write!(
        html,
        r#"<div class="stat-card" id="company-ext-establishments-card">
  <h3 class="text-base font-semibold text-white mb-2">事業所構造 <span class="text-blue-300 text-sm">({title_scope})</span></h3>
  <div class="overflow-x-auto">
    <table class="min-w-full text-sm">
      <thead>
        <tr class="text-slate-400 border-b border-slate-700">
          <th class="text-left py-1 pr-2">産業</th>
          <th class="text-right py-1 pr-2">事業所数</th>
          <th class="text-right py-1 pr-2">シェア</th>
          <th class="text-right py-1 pr-2">従業者 (千人)</th>
          <th class="text-right py-1 pr-2">1事業所平均 (人)</th>
        </tr>
      </thead>
      <tbody>"#
    );
    for row in rows.iter().take(20) {
        let name = escape_html(&get_str(row, "industry_name"));
        let est = get_i64(row, "establishment_count");
        let emp = get_i64(row, "employees");
        let share = if total_est > 0 {
            (est as f64 / total_est as f64) * 100.0
        } else {
            0.0
        };
        let avg = if est > 0 {
            emp as f64 / est as f64
        } else {
            0.0
        };
        let _ = write!(
            html,
            r#"<tr class="border-b border-slate-800"><td class="py-1 pr-2 text-slate-200">{name}</td>
<td class="py-1 pr-2 text-right text-slate-200">{est_str}</td>
<td class="py-1 pr-2 text-right text-slate-300">{share:.1}%</td>
<td class="py-1 pr-2 text-right text-slate-300">{emp_k}</td>
<td class="py-1 pr-2 text-right text-slate-300">{avg:.1}</td></tr>"#,
            est_str = format_number(est),
            emp_k = format_number(emp / 1000)
        );
    }
    html.push_str("</tbody></table></div>\n");
    let muni_note = if !muni.is_empty() {
        " 市区町村粒度は本データセット未収録のため都道府県集計を表示。"
    } else {
        ""
    };
    let _ = write!(
        html,
        r#"<p class="text-xs text-slate-500 mt-2">出典: 総務省 経済センサス (v2_external_establishments、参照年 {year}){muni_note}</p>
</div>"#,
        year = if max_year > 0 {
            max_year.to_string()
        } else {
            "未定".to_string()
        }
    );
    html
}

// ============================================================
// 3. 企業セグメント (v2_salesnow_companies の region 集計)
// ============================================================

/// 企業データベースから、地域 (都道府県 + 市区町村) で 4 セグメントを集計。
///
/// セグメント定義 (既存 fetch_company_segments_by_region と同じ):
/// - 大手   : employee_count 降順 上位 10
/// - 中堅   : 50 ≤ employee_count ≤ 300、上位 10
/// - 急成長 : employee_delta_1y > +10.0%、降順 上位 10
/// - 採用活発 : sales_range が空でない (代理指標、HW 結合は別 endpoint で実施)
///
/// 引数 `muni` は v2_salesnow_companies の `address LIKE '%muni%'` で絞り込む。
/// 既存 fetch.rs と同じパターン。
pub fn fetch_company_segments(turso: &TursoDb, pref: &str, muni: &str) -> Vec<SegmentRow> {
    if pref.is_empty() {
        return Vec::new();
    }
    // 2026-06-08 Team H-Fix: 「郡」プレフィックスを strip。6市町
    // (郡山市/郡上市/蒲郡市/上郡町/大和郡山市/小郡市) は COUNTY_PREFIX_KEEP で identity 保持。
    let muni_key = strip_county_prefix(muni);
    let muni_pattern = format!("%{}%", muni_key);

    let mut segments: Vec<SegmentRow> = Vec::new();

    // 大手: employee_count 降順 Top 10
    segments.extend(fetch_segment(
        turso,
        pref,
        muni,
        &muni_pattern,
        "大手",
        "employee_count DESC",
        "employee_count IS NOT NULL AND employee_count > 0",
    ));

    // 中堅: 50-300 名
    segments.extend(fetch_segment(
        turso,
        pref,
        muni,
        &muni_pattern,
        "中堅",
        "employee_count DESC",
        "employee_count BETWEEN 50 AND 300",
    ));

    // 急成長: employee_delta_1y > 10.0 (= 10%)
    segments.extend(fetch_segment(
        turso,
        pref,
        muni,
        &muni_pattern,
        "急成長",
        "employee_delta_1y DESC",
        "employee_delta_1y > 10.0 AND employee_delta_1y < 200.0",
    ));

    // 採用シグナル候補: credit_score 50 以上で listing_category が記載されているもの
    // (HW 求人結合は重い処理なので別 endpoint。本セグメントは「採用基盤が整っている候補」)
    segments.extend(fetch_segment(
        turso,
        pref,
        muni,
        &muni_pattern,
        "採用基盤候補",
        "credit_score DESC",
        "credit_score >= 50.0 AND listing_category IS NOT NULL AND listing_category != ''",
    ));

    segments
}

/// 単一セグメントの内部取得関数。
fn fetch_segment(
    turso: &TursoDb,
    pref: &str,
    muni: &str,
    muni_pattern: &str,
    segment_label: &str,
    order_by: &str,
    extra_where: &str,
) -> Vec<SegmentRow> {
    let muni_clause = if muni.is_empty() {
        ""
    } else {
        " AND address LIKE ?2"
    };
    let sql = format!(
        "SELECT corporate_number, company_name, sn_industry, \
         employee_count, employee_delta_1y, credit_score, listing_category, sales_range \
         FROM v2_salesnow_companies \
         WHERE prefecture = ?1{muni_clause} AND {extra_where} \
         ORDER BY {order_by} LIMIT 10"
    );

    let pref_owned = pref.to_string();
    let muni_pattern_owned = muni_pattern.to_string();
    let mut params: Vec<&dyn ToSqlTurso> = vec![&pref_owned];
    if !muni.is_empty() {
        params.push(&muni_pattern_owned);
    }
    let rows = turso.query(&sql, &params).unwrap_or_default();
    rows.into_iter()
        .map(|r| SegmentRow {
            segment: segment_label.to_string(),
            corporate_number: get_str(&r, "corporate_number"),
            company_name: get_str(&r, "company_name"),
            sn_industry: get_str(&r, "sn_industry"),
            employee_count: get_i64(&r, "employee_count"),
            employee_delta_1y: get_f64(&r, "employee_delta_1y"),
            credit_score: get_f64(&r, "credit_score"),
            listing_category: get_str(&r, "listing_category"),
        })
        .collect()
}

/// 単一セグメント行 (HTML rendering 用 DTO)
#[derive(Debug, Clone, Default)]
pub struct SegmentRow {
    pub segment: String,
    pub corporate_number: String,
    pub company_name: String,
    pub sn_industry: String,
    pub employee_count: i64,
    pub employee_delta_1y: f64,
    pub credit_score: f64,
    pub listing_category: String,
}

/// 企業セグメント パネルの HTML をレンダリング。
pub fn render_segments_panel(pref: &str, muni: &str, rows: &[SegmentRow]) -> String {
    let title_scope = if muni.is_empty() {
        escape_html(pref)
    } else {
        format!("{} {}", escape_html(pref), escape_html(muni))
    };

    if pref.is_empty() {
        return panel_message_pref_required("企業セグメント");
    }
    if rows.is_empty() {
        return panel_message_no_data("企業セグメント", &title_scope, "外部企業データベース");
    }

    // セグメント毎にグループ化 (Vec の順序を保つため Vec<(String, Vec<&SegmentRow>)>)
    let segment_order = ["大手", "中堅", "急成長", "採用基盤候補"];
    let mut grouped: Vec<(&str, Vec<&SegmentRow>)> =
        segment_order.iter().map(|s| (*s, Vec::new())).collect();
    for row in rows {
        for (key, vec) in grouped.iter_mut() {
            if row.segment == *key {
                vec.push(row);
                break;
            }
        }
    }

    let mut html = String::with_capacity(8192);
    let _ = write!(
        html,
        r#"<div class="stat-card" id="company-ext-segments-card">
  <h3 class="text-base font-semibold text-white mb-2">企業セグメント <span class="text-blue-300 text-sm">({title_scope})</span></h3>"#
    );

    for (segment_name, segment_rows) in &grouped {
        let label = match *segment_name {
            "大手" => "大手 (従業員数 上位)",
            "中堅" => "中堅 (従業員 50〜300名)",
            "急成長" => "急成長 (従業員 1年+10%超)",
            "採用基盤候補" => "採用基盤候補 (上場 + 信用50+)",
            _ => *segment_name,
        };
        let _ = write!(
            html,
            r#"<div class="mt-3"><h4 class="text-sm font-semibold text-blue-300 mb-1">{label}</h4>"#
        );
        if segment_rows.is_empty() {
            html.push_str(
                r#"<p class="text-xs text-slate-500 ml-2">該当企業はありません (条件に合致するレコードなし)。</p></div>"#,
            );
            continue;
        }
        html.push_str(
            r#"<div class="overflow-x-auto"><table class="min-w-full text-xs">
<thead><tr class="text-slate-400 border-b border-slate-700">
<th class="text-left py-1 pr-2">企業名</th>
<th class="text-left py-1 pr-2">業種</th>
<th class="text-right py-1 pr-2">従業員</th>
<th class="text-right py-1 pr-2">1年推移</th>
<th class="text-right py-1 pr-2">信用</th>
</tr></thead><tbody>"#,
        );
        for r in segment_rows.iter() {
            let name = escape_html(&r.company_name);
            let ind = escape_html(&r.sn_industry);
            let emp = if r.employee_count > 0 {
                format_number(r.employee_count)
            } else {
                "-".to_string()
            };
            let delta = if r.employee_delta_1y.abs() > 0.001 {
                format!("{:+.1}%", r.employee_delta_1y)
            } else {
                "-".to_string()
            };
            let credit = if r.credit_score > 0.0 {
                format!("{:.0}", r.credit_score)
            } else {
                "-".to_string()
            };
            let cn = escape_html(&r.corporate_number);
            let _ = write!(
                html,
                r##"<tr class="border-b border-slate-800">
<td class="py-1 pr-2"><a class="text-blue-400 hover:text-blue-300 underline cursor-pointer"
   hx-get="/api/company/profile/{cn}"
   hx-target="#company-profile-area"
   hx-swap="innerHTML">{name}</a></td>
<td class="py-1 pr-2 text-slate-300">{ind}</td>
<td class="py-1 pr-2 text-right text-slate-200">{emp}</td>
<td class="py-1 pr-2 text-right text-slate-300">{delta}</td>
<td class="py-1 pr-2 text-right text-slate-300">{credit}</td>
</tr>"##
            );
        }
        html.push_str("</tbody></table></div></div>");
    }

    html.push_str(
        r#"<p class="text-xs text-slate-500 mt-3">出典: 外部企業データベース。各セグメント上位 10 社まで表示。市区町村は住所部分一致で絞り込み。</p>
</div>"#,
    );
    html
}

// ============================================================
// 4-8. 地域経済・環境補足 5 テーブル (Wave1-D 未活用テーブルの移植)
// ============================================================
//
// 2026-06-05 追加:
// 地域カルテ (非表示タブ) 専用だった未活用 5 テーブルを、表示中の企業検索タブへ移植する。
// fetch + DTO 変換は既存 `analysis::fetch::external_extra` を再利用し、本ファイルでは
// 企業検索タブ向けの fetch ラッパ + 単独パネル render (HTMX 個別ロード) を提供する。
//
// 5 テーブル:
//   - business_dynamics (採用市場動態: 開廃業率)   ─ 都道府県粒度
//   - car_ownership     (通勤圏: 車保有率)         ─ 都道府県粒度
//   - land_price        (生活コスト: 地価)         ─ 都道府県粒度
//   - boj_tankan        (全国景況: 業況DI ※全国値)  ─ 全国粒度 (pref 無関係)
//   - climate           (環境補足: 降雪日数等)      ─ 都道府県粒度
//
// 厳守ルール:
//   - silent fallback 禁止: pref 空 / データ無しは明示メッセージ
//     (panel_message_pref_required / panel_message_no_data 再利用)
//   - 中立表現 (劣位/集中/縮小を使わない)、相関≠因果 (傾向/可能性表現)、出典明記
//   - boj_tankan = 全国値、climate = 環境補足 と粒度明記 (地域別誤認防止)
//   - これら 5 テーブルに市区町村粒度はないため muni は無視 (タイトル表示にも使わない)

use crate::db::local_sqlite::LocalDb;
use crate::handlers::analysis::fetch as af;
use crate::handlers::analysis::fetch::external_extra::{
    to_boj_tankan_latest, to_business_dynamics_latest, to_car_ownership_latest, to_climate_latest,
    to_land_price_items, BojTankanLatest, BusinessDynamicsLatest, CarOwnershipLatest,
    ClimateLatest, LandPriceItem,
};

// ---- fetch ラッパ (analysis::fetch の既存 fetch を呼び、DTO へ変換) ----

/// 採用市場動態 (開廃業率) を都道府県粒度で取得。空 pref → None。
pub fn fetch_company_business_dynamics(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref: &str,
) -> Option<BusinessDynamicsLatest> {
    if pref.is_empty() {
        return None;
    }
    to_business_dynamics_latest(&af::fetch_business_dynamics(db, turso, pref))
}

/// 通勤圏 (車保有率) を都道府県粒度で取得。空 pref → None。
pub fn fetch_company_car_ownership(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref: &str,
) -> Option<CarOwnershipLatest> {
    if pref.is_empty() {
        return None;
    }
    to_car_ownership_latest(&af::fetch_car_ownership(db, turso, pref))
}

/// 生活コスト (地価) を都道府県粒度で取得。空 pref → 空 Vec。
pub fn fetch_company_land_price(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref: &str,
) -> Vec<LandPriceItem> {
    if pref.is_empty() {
        return vec![];
    }
    to_land_price_items(&af::fetch_land_price(db, turso, pref))
}

/// 全国景況 (業況DI・全国値) を取得。全国粒度のため pref 無関係。
pub fn fetch_company_boj_tankan(db: &LocalDb, turso: Option<&TursoDb>) -> Vec<BojTankanLatest> {
    to_boj_tankan_latest(&af::fetch_boj_tankan(db, turso))
}

/// 環境補足 (気候) を都道府県粒度で取得。空 pref → None。
pub fn fetch_company_climate(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref: &str,
) -> Option<ClimateLatest> {
    if pref.is_empty() {
        return None;
    }
    to_climate_latest(&af::fetch_climate(db, turso, pref))
}

// ---- render パネル (HTMX 個別ロード単位) ----

/// 採用市場動態パネル (開廃業率)。
///
/// So What: 開業率 > 廃業率なら新規事業所による採用需要が生じる「可能性」。相関≠因果。
pub fn render_business_dynamics_panel(pref: &str, d: Option<&BusinessDynamicsLatest>) -> String {
    if pref.is_empty() {
        return panel_message_pref_required("採用市場動態 (開廃業)");
    }
    let d = match d {
        Some(d) => d,
        None => {
            return panel_message_no_data(
                "採用市場動態 (開廃業)",
                &escape_html(pref),
                "経済センサス (v2_external_business_dynamics)",
            )
        }
    };
    let opening = fmt_pct_opt(d.opening_rate);
    let closure = fmt_pct_opt(d.closure_rate);
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
        r##"<div class="stat-card" id="company-ext-bizdyn-card">
  <h3 class="text-base font-semibold text-white mb-2">採用市場動態 (開廃業) <span class="text-blue-300 text-sm">({scope})</span></h3>
  <div class="text-sm text-slate-200 space-y-1">
    <div>開業率 <span class="text-emerald-400 font-mono">{opening}</span></div>
    <div>廃業率 <span class="text-rose-400 font-mono">{closure}</span></div>
  </div>
  <p class="text-xs text-slate-400 mt-2">{so_what}</p>
  <p class="text-xs text-slate-500 mt-2">出典: 総務省 経済センサス (v2_external_business_dynamics、{year}年度・都道府県粒度)</p>
</div>"##,
        scope = escape_html(pref),
        opening = opening,
        closure = closure,
        so_what = escape_html(so_what),
        year = d.fiscal_year,
    )
}

/// 通勤圏パネル (車保有率)。
///
/// So What: 車保有率が高いと車通勤前提で採用リーチ圏が広がる「可能性」。相関≠因果。
pub fn render_car_ownership_panel(pref: &str, d: Option<&CarOwnershipLatest>) -> String {
    if pref.is_empty() {
        return panel_message_pref_required("通勤圏 (車保有率)");
    }
    let d = match d {
        Some(d) => d,
        None => {
            return panel_message_no_data(
                "通勤圏 (車保有率)",
                &escape_html(pref),
                "自動車検査登録情報協会 (v2_external_car_ownership)",
            )
        }
    };
    let cars = match d.cars_per_100people {
        Some(v) if v.is_finite() => format!("{:.1} 台/100人", v),
        _ => "—".to_string(),
    };
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
        r##"<div class="stat-card" id="company-ext-car-card">
  <h3 class="text-base font-semibold text-white mb-2">通勤圏 (車保有率) <span class="text-blue-300 text-sm">({scope})</span></h3>
  <div class="text-sm text-slate-200"><span class="text-sky-400 font-mono text-lg">{cars}</span></div>
  <p class="text-xs text-slate-400 mt-2">{so_what}</p>
  <p class="text-xs text-slate-500 mt-2">出典: 自動車検査登録情報協会 (v2_external_car_ownership、{year}年・都道府県粒度)</p>
</div>"##,
        scope = escape_html(pref),
        cars = cars,
        so_what = escape_html(so_what),
        year = d.year,
    )
}

/// 生活コストパネル (地価)。
///
/// So What: 地価は生活コストの代理指標。給与の実質購買力評価の参考になる「可能性」。相関≠因果。
pub fn render_land_price_panel(pref: &str, items: &[LandPriceItem]) -> String {
    if pref.is_empty() {
        return panel_message_pref_required("生活コスト (地価)");
    }
    if items.is_empty() {
        return panel_message_no_data(
            "生活コスト (地価)",
            &escape_html(pref),
            "地価公示 (v2_external_land_price)",
        );
    }
    let mut rows_html = String::new();
    for it in items.iter().take(6) {
        let price = match it.avg_price_per_sqm {
            Some(v) if v.is_finite() && v > 0.0 => format!("{} 円/m²", format_number(v as i64)),
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
        let _ = write!(
            rows_html,
            r##"<div>{use_label}: <span class="text-amber-300 font-mono">{price}</span> <span class="text-slate-500">(前年比 {yoy})</span></div>"##,
            use_label = use_label,
            price = price,
            yoy = yoy,
        );
    }
    let year = items.iter().map(|i| i.year).max().unwrap_or(0);
    let so_what =
        "地価は生活コストの代理指標です。給与水準の実質的な購買力評価の参考になる可能性があります。";
    format!(
        r##"<div class="stat-card" id="company-ext-land-card">
  <h3 class="text-base font-semibold text-white mb-2">生活コスト (地価) <span class="text-blue-300 text-sm">({scope})</span></h3>
  <div class="text-sm text-slate-200 space-y-1">{rows}</div>
  <p class="text-xs text-slate-400 mt-2">{so_what}</p>
  <p class="text-xs text-slate-500 mt-2">出典: 国土交通省 地価公示 (v2_external_land_price、{year}年・都道府県粒度)</p>
</div>"##,
        scope = escape_html(pref),
        rows = rows_html,
        so_what = escape_html(so_what),
        year = year,
    )
}

/// 全国景況パネル (業況DI・全国値)。
///
/// **全国粒度**。当該地域の動向と一致しないことを明記する。
/// So What: 業況DIは採用意欲の先行指標となる「可能性」(全国値)。相関≠因果。
pub fn render_boj_tankan_panel(items: &[BojTankanLatest]) -> String {
    // boj_tankan は全国値のため pref 不要。データ無しは no_data メッセージ。
    if items.is_empty() {
        return panel_message_no_data(
            "全国景況 (業況DI・全国値)",
            "全国",
            "日本銀行 短観 (v2_external_boj_tankan)",
        );
    }
    let mut rows_html = String::new();
    for it in items.iter().take(6) {
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
        let _ = write!(
            rows_html,
            r##"<div>{label}{size}: <span class="font-mono text-indigo-300">DI {di}</span></div>"##,
            label = label,
            size = size,
            di = di,
        );
    }
    let survey = items
        .first()
        .map(|i| escape_html(&i.survey_date))
        .unwrap_or_default();
    let so_what =
        "業況DIは採用意欲の先行指標となる可能性があります（全国値のため当該地域の動向とは一致しないことがあります）。";
    format!(
        r##"<div class="stat-card" id="company-ext-tankan-card">
  <h3 class="text-base font-semibold text-white mb-2">全国景況 (業況DI・全国値) <span class="text-blue-300 text-sm">(全国)</span></h3>
  <div class="text-sm text-slate-200 space-y-1">{rows}</div>
  <p class="text-xs text-slate-400 mt-2">{so_what}</p>
  <p class="text-xs text-slate-500 mt-2">出典: 日本銀行 短観 (v2_external_boj_tankan、{survey} 調査)。※全国値であり市区町村別ではありません。</p>
</div>"##,
        rows = rows_html,
        so_what = escape_html(so_what),
        survey = survey,
    )
}

/// 環境補足パネル (気候)。
///
/// **環境補足情報**。採用条件検討の背景情報として表示する。
/// So What: 降雪日数が多いと冬季の通勤環境が採用条件の検討要素になる「可能性」。相関≠因果。
pub fn render_climate_panel(pref: &str, d: Option<&ClimateLatest>) -> String {
    if pref.is_empty() {
        return panel_message_pref_required("環境補足 (気候)");
    }
    let d = match d {
        Some(d) => d,
        None => {
            return panel_message_no_data(
                "環境補足 (気候)",
                &escape_html(pref),
                "気象庁 (v2_external_climate)",
            )
        }
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
        r##"<div class="stat-card" id="company-ext-climate-card">
  <h3 class="text-base font-semibold text-white mb-2">環境補足 (気候) <span class="text-blue-300 text-sm">({scope})</span></h3>
  <div class="text-sm text-slate-200 space-y-1">
    <div>年平均気温 <span class="font-mono text-cyan-300">{avg_t}</span></div>
    <div>降雪日数 <span class="font-mono text-blue-300">{snow}</span></div>
  </div>
  <p class="text-xs text-slate-400 mt-2">{so_what}</p>
  <p class="text-xs text-slate-500 mt-2">出典: 気象庁 (v2_external_climate、{year}年度)。※環境補足情報です。</p>
</div>"##,
        scope = escape_html(pref),
        avg_t = avg_t,
        snow = snow,
        so_what = escape_html(so_what),
        year = d.fiscal_year,
    )
}

/// Option<f64> を「%」付き文字列に。None / 非有限 → "—"。
fn fmt_pct_opt(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{:.2}%", x),
        _ => "—".to_string(),
    }
}

// ============================================================
// 共通: パネル用メッセージ HTML
// ============================================================

fn panel_message_pref_required(panel_label: &str) -> String {
    format!(
        r#"<div class="stat-card"><h3 class="text-base font-semibold text-white mb-2">{label}</h3>
<p class="text-sm text-slate-400">都道府県を選択するとパネルが表示されます。</p></div>"#,
        label = escape_html(panel_label)
    )
}

fn panel_message_no_data(panel_label: &str, scope: &str, source: &str) -> String {
    format!(
        r#"<div class="stat-card"><h3 class="text-base font-semibold text-white mb-2">{label} <span class="text-blue-300 text-sm">({scope})</span></h3>
<p class="text-sm text-slate-400">該当する集計データが見つかりません。</p>
<p class="text-xs text-slate-500 mt-2">出典: {source}</p></div>"#,
        label = escape_html(panel_label),
        scope = scope,
        source = escape_html(source)
    )
}

/// 検索ページ下部に差し込む外部パネル ドリルダウン UI のスケルトン。
///
/// 都道府県セレクト + 市区町村セレクト + 3 つのパネル領域 (HTMX で遅延ロード) を返す。
/// `pref_options` には render_search_page 上位で組み立てた `<option>` 文字列を渡す。
pub fn render_external_drilldown_skeleton(pref_options: &str) -> String {
    format!(
        r##"<div class="stat-card" id="company-ext-drilldown">
  <details open>
    <summary class="cursor-pointer text-base font-semibold text-white mb-2">外部統計ドリルダウン</summary>
    <p class="text-xs text-slate-500 mb-3">企業検索結果と独立に、地域選択で外部統計 (国勢調査 / 経済センサス / 外部企業データベース) を個別確認できます。</p>
    <div class="grid grid-cols-1 md:grid-cols-2 gap-3 mb-3">
      <div>
        <label class="block text-xs text-slate-400 mb-1">都道府県</label>
        <select id="company-ext-pref" class="w-full bg-slate-700 text-white text-sm rounded px-2 py-1 border border-slate-600 focus:border-blue-500 focus:outline-none"
                onchange="companyExtRunPref()">
          <option value="">-- 選択 --</option>
{pref_options}
        </select>
      </div>
      <div>
        <label class="block text-xs text-slate-400 mb-1">市区町村 (任意)</label>
        <input type="text" id="company-ext-muni" name="muni" placeholder="例: 札幌市中央区"
               class="w-full bg-slate-700 text-white text-sm rounded px-2 py-1 border border-slate-600 focus:border-blue-500 focus:outline-none"
               onkeyup="if(event.key==='Enter') companyExtRunMuni();" />
      </div>
    </div>
    <div id="company-ext-industry" class="mb-3"></div>
    <div id="company-ext-establishments" class="mb-3"></div>
    <div id="company-ext-segments" class="mb-3"></div>
    <p class="text-xs text-slate-500 mt-4 mb-2 border-t border-slate-800 pt-3">地域経済・環境補足 (都道府県粒度。採用市場・通勤圏・生活コスト・全国景況・環境の背景把握用)</p>
    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
      <div id="company-ext-bizdyn"></div>
      <div id="company-ext-car"></div>
      <div id="company-ext-land"></div>
      <div id="company-ext-tankan"></div>
      <div id="company-ext-climate"></div>
    </div>
    <script>
    (function(){{
      // 🔴 htmx.ajax を一斉発火すると htmx が同時リクエストを取りこぼし一部パネルしか
      //    描画されない (本番実測 8本中2本)。promise チェーンで1本ずつ逐次実行する。
      function seq(items){{
        (function next(i){{
          if(i>=items.length) return;
          htmx.ajax('GET',items[i][0],{{target:items[i][1],swap:'innerHTML'}})
            .then(function(){{next(i+1);}}).catch(function(){{next(i+1);}});
        }})(0);
      }}
      function q(o){{ return new URLSearchParams(o).toString(); }}
      window.companyExtRunPref=function(){{
        var pref=document.getElementById('company-ext-pref').value;
        var muni=document.getElementById('company-ext-muni').value;
        if(!pref) return;
        seq([
          ['/api/company/external/industry_structure?'+q({{pref:pref,muni:muni}}),'#company-ext-industry'],
          ['/api/company/external/establishments?'+q({{pref:pref,muni:muni}}),'#company-ext-establishments'],
          ['/api/company/external/segments?'+q({{pref:pref,muni:muni}}),'#company-ext-segments'],
          ['/api/company/external/business_dynamics?'+q({{pref:pref}}),'#company-ext-bizdyn'],
          ['/api/company/external/car_ownership?'+q({{pref:pref}}),'#company-ext-car'],
          ['/api/company/external/land_price?'+q({{pref:pref}}),'#company-ext-land'],
          ['/api/company/external/boj_tankan?'+q({{pref:pref}}),'#company-ext-tankan'],
          ['/api/company/external/climate?'+q({{pref:pref}}),'#company-ext-climate']
        ]);
      }};
      window.companyExtRunMuni=function(){{
        var pref=document.getElementById('company-ext-pref').value;
        var muni=document.getElementById('company-ext-muni').value;
        if(!pref) return;
        seq([
          ['/api/company/external/industry_structure?'+q({{pref:pref,muni:muni}}),'#company-ext-industry'],
          ['/api/company/external/establishments?'+q({{pref:pref,muni:muni}}),'#company-ext-establishments'],
          ['/api/company/external/segments?'+q({{pref:pref,muni:muni}}),'#company-ext-segments']
        ]);
      }};
    }})();
    </script>
  </details>
</div>"##
    )
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mk_row(pairs: &[(&str, serde_json::Value)]) -> Row {
        let mut row = Row::new();
        for (k, v) in pairs {
            row.insert((*k).to_string(), v.clone());
        }
        row
    }

    // ---------------- 産業構造パネル ----------------

    #[test]
    fn render_industry_panel_pref_empty_returns_pref_required_message() {
        let html = render_industry_structure_panel("", "", &[]);
        assert!(
            html.contains("都道府県を選択"),
            "pref 未指定時は選択促進メッセージ"
        );
        // SalesNow 表記禁止
        assert!(!html.contains("SalesNow"), "UI 出力に SalesNow を含まない");
    }

    #[test]
    fn render_industry_panel_empty_rows_returns_no_data_message() {
        let html = render_industry_structure_panel("北海道", "", &[]);
        assert!(
            html.contains("該当する集計データが見つかりません"),
            "空行時のメッセージ"
        );
        assert!(html.contains("国勢調査"), "出典 caption が含まれる");
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_industry_panel_with_rows_renders_table_with_share() {
        let rows = vec![
            mk_row(&[
                ("industry_name", json!("医療，福祉")),
                ("employees_total", json!(120000)),
                ("employees_male", json!(30000)),
                ("employees_female", json!(90000)),
            ]),
            mk_row(&[
                ("industry_name", json!("製造業")),
                ("employees_total", json!(80000)),
                ("employees_male", json!(60000)),
                ("employees_female", json!(20000)),
            ]),
        ];
        let html = render_industry_structure_panel("北海道", "", &rows);
        assert!(html.contains("医療，福祉"));
        assert!(html.contains("製造業"));
        // シェア: 120k / 200k = 60.0%
        assert!(html.contains("60.0%"), "上位産業のシェアが描画される");
        // 女性比率: 90k / 120k = 75.0%
        assert!(html.contains("75.0%"));
        // 出典
        assert!(html.contains("国勢調査"));
        // 禁止語
        assert!(!html.contains("SalesNow"));
        assert!(!html.contains("劣位"));
        assert!(!html.contains("推定人数"));
    }

    // ---------------- 事業所パネル ----------------

    #[test]
    fn render_establishments_panel_empty_returns_no_data_message() {
        let html = render_establishments_panel("北海道", "", &[]);
        assert!(html.contains("該当する集計データが見つかりません"));
        assert!(html.contains("経済センサス"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_establishments_panel_renders_avg_and_share() {
        let rows = vec![
            mk_row(&[
                ("industry_name", json!("卸売業，小売業")),
                ("establishment_count", json!(10000)),
                ("employees", json!(50000)),
                ("reference_year", json!(2021)),
            ]),
            mk_row(&[
                ("industry_name", json!("医療，福祉")),
                ("establishment_count", json!(10000)),
                ("employees", json!(120000)),
                ("reference_year", json!(2021)),
            ]),
        ];
        let html = render_establishments_panel("北海道", "", &rows);
        // シェア: 10000 / 20000 = 50.0%
        assert!(html.contains("50.0%"));
        // 1事業所平均: 50000 / 10000 = 5.0、120000 / 10000 = 12.0
        assert!(html.contains("5.0") && html.contains("12.0"));
        assert!(html.contains("2021"));
        assert!(html.contains("経済センサス"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_establishments_panel_with_muni_notes_pref_aggregate() {
        let rows = vec![mk_row(&[
            ("industry_name", json!("製造業")),
            ("establishment_count", json!(100)),
            ("employees", json!(1000)),
            ("reference_year", json!(2021)),
        ])];
        let html = render_establishments_panel("北海道", "札幌市中央区", &rows);
        assert!(
            html.contains("市区町村粒度は本データセット未収録"),
            "市区町村指定時の注記が描画される"
        );
        assert!(html.contains("札幌市中央区"));
    }

    // ---------------- セグメント パネル ----------------

    #[test]
    fn render_segments_panel_empty_returns_no_data_message() {
        let html = render_segments_panel("北海道", "", &[]);
        assert!(html.contains("該当する集計データが見つかりません"));
        // 外部企業データベース表記 (SalesNow ではない)
        assert!(html.contains("外部企業データベース"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_segments_panel_renders_all_four_segments() {
        let rows = vec![
            SegmentRow {
                segment: "大手".to_string(),
                corporate_number: "1000000000001".to_string(),
                company_name: "A社".to_string(),
                sn_industry: "製造業".to_string(),
                employee_count: 5000,
                employee_delta_1y: 0.0,
                credit_score: 65.0,
                listing_category: "東P".to_string(),
            },
            SegmentRow {
                segment: "中堅".to_string(),
                corporate_number: "1000000000002".to_string(),
                company_name: "B社".to_string(),
                sn_industry: "卸売".to_string(),
                employee_count: 150,
                employee_delta_1y: 0.0,
                credit_score: 55.0,
                listing_category: "".to_string(),
            },
            SegmentRow {
                segment: "急成長".to_string(),
                corporate_number: "1000000000003".to_string(),
                company_name: "C社".to_string(),
                sn_industry: "情報サービス".to_string(),
                employee_count: 80,
                employee_delta_1y: 35.5,
                credit_score: 50.0,
                listing_category: "".to_string(),
            },
            SegmentRow {
                segment: "採用基盤候補".to_string(),
                corporate_number: "1000000000004".to_string(),
                company_name: "D社".to_string(),
                sn_industry: "金融".to_string(),
                employee_count: 1200,
                employee_delta_1y: 2.0,
                credit_score: 75.0,
                listing_category: "東P".to_string(),
            },
        ];
        let html = render_segments_panel("北海道", "", &rows);
        // 4 セグメント全て描画される
        assert!(html.contains("A社") && html.contains("B社"));
        assert!(html.contains("C社") && html.contains("D社"));
        // 急成長で +35.5%
        assert!(html.contains("+35.5%"));
        // 企業プロフィール遷移リンク
        assert!(html.contains("/api/company/profile/1000000000001"));
        // 出典 + 禁止語
        assert!(html.contains("外部企業データベース"));
        assert!(!html.contains("SalesNow"));
        assert!(!html.contains("劣位"));
    }

    // ---------------- ドリルダウン スケルトン ----------------

    #[test]
    fn render_external_drilldown_skeleton_includes_targets() {
        let html = render_external_drilldown_skeleton(r#"<option value="北海道">北海道</option>"#);
        // 3 つのパネル領域 id
        assert!(html.contains("company-ext-industry"));
        assert!(html.contains("company-ext-establishments"));
        assert!(html.contains("company-ext-segments"));
        // 都道府県セレクタ
        assert!(html.contains("北海道"));
        // 禁止語
        assert!(!html.contains("SalesNow"));
        assert!(!html.contains("劣位"));
    }

    // ============================================================
    // 追加テスト (silent fallback 境界 / データ妥当性 / 逆証明)
    // ============================================================

    // ---- 産業構造: ゼロ合計時の division guard (silent fallback 境界) ----

    #[test]
    fn render_industry_panel_zero_total_does_not_divide_by_zero() {
        // employees_total が全行 0 の不正データ: パニックせず 0.0% を出す。
        // (NaN や "inf%" を出さないこと)
        let rows = vec![mk_row(&[
            ("industry_name", json!("医療，福祉")),
            ("employees_total", json!(0)),
            ("employees_male", json!(0)),
            ("employees_female", json!(0)),
        ])];
        let html = render_industry_structure_panel("北海道", "", &rows);
        assert!(html.contains("0.0%"), "ゼロ合計でも 0.0% で安全描画");
        assert!(!html.contains("NaN"), "NaN を表示しない");
        assert!(!html.contains("inf"), "inf を表示しない");
    }

    // ---- 産業構造: 企業/産業名の XSS エスケープ (データ妥当性) ----

    #[test]
    fn render_industry_panel_escapes_industry_name() {
        // 公的統計テーブルにヘッダー混入/不正値が入った場合の XSS 防御
        let rows = vec![mk_row(&[
            ("industry_name", json!("<script>alert(1)</script>")),
            ("employees_total", json!(100)),
            ("employees_male", json!(50)),
            ("employees_female", json!(50)),
        ])];
        let html = render_industry_structure_panel("北海道", "", &rows);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    // ---- 産業構造: muni に XSS が来てもタイトルでエスケープ ----

    #[test]
    fn render_industry_panel_escapes_muni_in_title() {
        let rows = vec![mk_row(&[
            ("industry_name", json!("製造業")),
            ("employees_total", json!(100)),
            ("employees_male", json!(50)),
            ("employees_female", json!(50)),
        ])];
        let html = render_industry_structure_panel("北海道", "<img src=x>", &rows);
        assert!(!html.contains("<img src=x>"));
        assert!(html.contains("&lt;img"));
    }

    // ---- 事業所: establishment_count=0 で平均が 0.0 (NaN/inf 回避) ----

    #[test]
    fn render_establishments_panel_zero_count_avoids_nan() {
        let rows = vec![mk_row(&[
            ("industry_name", json!("製造業")),
            ("establishment_count", json!(0)),
            ("employees", json!(1000)),
            ("reference_year", json!(2021)),
        ])];
        let html = render_establishments_panel("北海道", "", &rows);
        // total_est=0 → share 0.0%、est=0 → avg 0.0
        assert!(html.contains("0.0%"));
        assert!(!html.contains("NaN"));
        assert!(!html.contains("inf"));
    }

    // ---- 事業所: reference_year 欠落時は「未定」(silent fallback 禁止) ----

    #[test]
    fn render_establishments_panel_missing_year_shows_placeholder() {
        let rows = vec![mk_row(&[
            ("industry_name", json!("製造業")),
            ("establishment_count", json!(100)),
            ("employees", json!(1000)),
            // reference_year なし → get_i64 で 0 → max_year 0 → "未定"
        ])];
        let html = render_establishments_panel("北海道", "", &rows);
        assert!(html.contains("未定"), "参照年欠落時は『未定』を明示");
    }

    // ---- セグメント: pref 未指定で選択促進 (逆証明: 空でなく明示メッセージ) ----

    #[test]
    fn render_segments_panel_pref_empty_returns_message() {
        let html = render_segments_panel("", "", &[]);
        assert!(html.contains("都道府県を選択"));
        assert!(!html.trim().is_empty());
        assert!(!html.contains("SalesNow"));
    }

    // ---- セグメント: employee_count<=0 / delta≈0 / credit<=0 は "-" 表示 ----

    #[test]
    fn render_segments_panel_invalid_metrics_show_dash() {
        let rows = vec![SegmentRow {
            segment: "大手".to_string(),
            corporate_number: "1000000000099".to_string(),
            company_name: "E社".to_string(),
            sn_industry: "製造業".to_string(),
            employee_count: 0,      // → "-"
            employee_delta_1y: 0.0, // → "-"
            credit_score: 0.0,      // → "-"
            listing_category: "".to_string(),
        }];
        let html = render_segments_panel("北海道", "", &rows);
        assert!(html.contains("E社"));
        // 不正/欠損メトリクス (employee=0, delta=0, credit=0) は 0 表示ではなく
        // セル内容 ">-<" で "-" になる (silent fallback 禁止)。
        assert!(
            html.contains(">-<"),
            "欠損メトリクスは 0 ではなく - で明示する"
        );
        // 0 を従業員数として誤表示していないこと (E社 行に >0< の数値セルがない)
        assert!(
            !html.contains(">0<"),
            "欠損値を 0 として誤表示してはならない"
        );
        // 該当しないセグメントは「該当企業はありません」を明示
        assert!(html.contains("該当企業はありません"));
    }

    // ---- セグメント: company_name の XSS エスケープ + HTMX 属性検証 ----

    #[test]
    fn render_segments_panel_escapes_name_and_has_htmx() {
        let rows = vec![SegmentRow {
            segment: "大手".to_string(),
            corporate_number: "1000000000001".to_string(),
            company_name: "<b>X社</b>".to_string(),
            sn_industry: "製造業".to_string(),
            employee_count: 100,
            employee_delta_1y: -5.5, // 負の推移も -5.5% で透過
            credit_score: 60.0,
            listing_category: "東P".to_string(),
        }];
        let html = render_segments_panel("北海道", "", &rows);
        // XSS 防御
        assert!(!html.contains("<b>X社</b>"));
        assert!(html.contains("&lt;b&gt;X社"));
        // HTMX 属性 (データ妥当性: ドリルダウン遷移が機能する)
        assert!(html.contains("hx-get=\"/api/company/profile/1000000000001\""));
        assert!(html.contains("hx-target=\"#company-profile-area\""));
        assert!(html.contains("hx-swap=\"innerHTML\""));
        // 負の推移が正しく符号付きで描画
        assert!(html.contains("-5.5%"));
    }

    // ---- ドリルダウン スケルトン: HTMX 属性とフォーム field 名 (データ妥当性) ----

    #[test]
    fn render_skeleton_has_htmx_and_form_fields() {
        let html = render_external_drilldown_skeleton("");
        // form field 名: muni は name="muni"、pref は id 経由
        assert!(html.contains(r#"name="muni""#));
        assert!(html.contains(r#"id="company-ext-pref""#));
        assert!(html.contains(r#"id="company-ext-muni""#));
        // HTMX endpoint
        assert!(html.contains("/api/company/external/industry_structure"));
        // pref 変更で逐次ローダ (companyExtRunPref) を起動する
        // (旧: hx-trigger 宣言。htmx.ajax 一斉発火は同時リクエスト取りこぼしのため
        //  逐次 promise チェーンに変更済み)
        assert!(html.contains("companyExtRunPref"));
        assert!(html.contains("htmx.ajax"));
    }

    #[test]
    fn render_skeleton_includes_external_extra_panels_and_endpoints() {
        // Wave1-D 移植: 5 パネルの target div と onchange ajax endpoint が含まれること
        let html = render_external_drilldown_skeleton("");
        for id in [
            "company-ext-bizdyn",
            "company-ext-car",
            "company-ext-land",
            "company-ext-tankan",
            "company-ext-climate",
        ] {
            assert!(html.contains(id), "skeleton に {id} の領域が含まれる");
        }
        for ep in [
            "/api/company/external/business_dynamics",
            "/api/company/external/car_ownership",
            "/api/company/external/land_price",
            "/api/company/external/boj_tankan",
            "/api/company/external/climate",
        ] {
            assert!(
                html.contains(ep),
                "skeleton に {ep} の ajax 呼び出しが含まれる"
            );
        }
        // 粒度明記 (都道府県粒度) と禁止語チェック
        assert!(html.contains("都道府県粒度"));
        assert!(!html.contains("SalesNow"));
    }

    // ============================================================
    // Wave1-D 移植: 地域経済・環境補足 5 パネルのテスト
    // ============================================================

    // ---- 採用市場動態 (business_dynamics) ----

    #[test]
    fn render_bizdyn_pref_empty_returns_pref_required() {
        let html = render_business_dynamics_panel("", None);
        assert!(
            html.contains("都道府県を選択"),
            "pref 空は選択促進 (silent fallback 禁止)"
        );
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_bizdyn_none_returns_no_data_with_source() {
        let html = render_business_dynamics_panel("北海道", None);
        assert!(html.contains("該当する集計データが見つかりません"));
        assert!(html.contains("経済センサス"), "出典明記");
    }

    #[test]
    fn render_bizdyn_opening_gt_closure_shows_active_market_so_what() {
        let d = BusinessDynamicsLatest {
            prefecture: "北海道".into(),
            fiscal_year: 2021,
            opening_rate: Some(5.5),
            closure_rate: Some(3.2),
            new_establishments: Some(120),
            closed_establishments: Some(70),
        };
        let html = render_business_dynamics_panel("北海道", Some(&d));
        // データ妥当性: 開業率/廃業率が描画される
        assert!(html.contains("5.50%") && html.contains("3.20%"));
        assert!(html.contains("2021"), "年度が描画される");
        // So What: 相関≠因果 (傾向/可能性表現)
        assert!(html.contains("傾向") && html.contains("可能性"));
        assert!(!html.contains("劣位") && !html.contains("集中") && !html.contains("縮小"));
    }

    #[test]
    fn render_bizdyn_null_rate_shows_dash_not_zero() {
        // NULL は "—" 表示 (0% と誤表示しない: silent fallback 禁止)
        let d = BusinessDynamicsLatest {
            prefecture: "沖縄県".into(),
            fiscal_year: 2021,
            opening_rate: None,
            closure_rate: Some(2.0),
            new_establishments: None,
            closed_establishments: Some(0),
        };
        let html = render_business_dynamics_panel("沖縄県", Some(&d));
        assert!(html.contains("—"), "NULL の開業率は — で明示");
        assert!(html.contains("2.00%"));
    }

    #[test]
    fn render_bizdyn_no_salesnow_name() {
        let d = BusinessDynamicsLatest {
            prefecture: "東京都".into(),
            fiscal_year: 2021,
            opening_rate: Some(4.0),
            closure_rate: Some(4.0),
            new_establishments: Some(100),
            closed_establishments: Some(100),
        };
        let html = render_business_dynamics_panel("東京都", Some(&d));
        assert!(!html.contains("SalesNow"));
        // 拮抗ケースの So What
        assert!(html.contains("拮抗"));
    }

    // ---- 通勤圏 (car_ownership) ----

    #[test]
    fn render_car_pref_empty_returns_pref_required() {
        let html = render_car_ownership_panel("", None);
        assert!(html.contains("都道府県を選択"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_car_none_returns_no_data() {
        let html = render_car_ownership_panel("富山県", None);
        assert!(html.contains("該当する集計データが見つかりません"));
        assert!(html.contains("自動車検査登録情報協会"), "出典明記");
    }

    #[test]
    fn render_car_high_ownership_shows_reach_so_what_and_unit() {
        let d = CarOwnershipLatest {
            prefecture: "富山県".into(),
            year: 2023,
            cars_per_100people: Some(62.5),
        };
        let html = render_car_ownership_panel("富山県", Some(&d));
        // 単位検証: 台/100人 (% ではない)
        assert!(html.contains("62.5 台/100人"));
        assert!(html.contains("2023"));
        // So What: 相関≠因果
        assert!(html.contains("可能性"));
        assert!(!html.contains("劣位"));
    }

    #[test]
    fn render_car_none_value_shows_dash() {
        let d = CarOwnershipLatest {
            prefecture: "東京都".into(),
            year: 2023,
            cars_per_100people: None,
        };
        let html = render_car_ownership_panel("東京都", Some(&d));
        assert!(html.contains("—"), "値 None は — で明示");
        assert!(html.contains("欠損"));
    }

    #[test]
    fn render_car_no_salesnow_name() {
        let d = CarOwnershipLatest {
            prefecture: "大阪府".into(),
            year: 2022,
            cars_per_100people: Some(30.0),
        };
        let html = render_car_ownership_panel("大阪府", Some(&d));
        assert!(!html.contains("SalesNow"));
        // 中〜低水準ケース
        assert!(html.contains("公共交通"));
    }

    // ---- 生活コスト (land_price) ----

    #[test]
    fn render_land_pref_empty_returns_pref_required() {
        let html = render_land_price_panel("", &[]);
        assert!(html.contains("都道府県を選択"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_land_empty_returns_no_data() {
        let html = render_land_price_panel("東京都", &[]);
        assert!(html.contains("該当する集計データが見つかりません"));
        assert!(html.contains("地価公示"), "出典明記");
    }

    #[test]
    fn render_land_renders_items_with_yoy_percent() {
        let items = vec![
            LandPriceItem {
                prefecture: "東京都".into(),
                land_use: "商業地".into(),
                avg_price_per_sqm: Some(5_000_000.0),
                yoy_change_pct: Some(2.3),
                year: 2024,
                point_count: Some(500),
            },
            LandPriceItem {
                prefecture: "東京都".into(),
                land_use: "住宅地".into(),
                avg_price_per_sqm: Some(600_000.0),
                yoy_change_pct: Some(-1.1),
                year: 2024,
                point_count: Some(1200),
            },
        ];
        let html = render_land_price_panel("東京都", &items);
        assert!(html.contains("商業地") && html.contains("住宅地"));
        // 単位検証: yoy は % のまま、3 桁区切り価格
        assert!(html.contains("+2.3%") && html.contains("-1.1%"));
        assert!(html.contains("5,000,000 円/m²"));
        assert!(html.contains("2024"));
        // So What: 相関≠因果
        assert!(html.contains("可能性"));
    }

    #[test]
    fn render_land_zero_price_shows_dash() {
        let items = vec![LandPriceItem {
            prefecture: "島根県".into(),
            land_use: "工業地".into(),
            avg_price_per_sqm: None,
            yoy_change_pct: None,
            year: 2024,
            point_count: None,
        }];
        let html = render_land_price_panel("島根県", &items);
        assert!(html.contains("—"), "価格/前年比 None は — で明示");
    }

    #[test]
    fn render_land_escapes_land_use_xss() {
        let items = vec![LandPriceItem {
            prefecture: "東京都".into(),
            land_use: "<script>alert(1)</script>".into(),
            avg_price_per_sqm: Some(100.0),
            yoy_change_pct: Some(0.0),
            year: 2024,
            point_count: Some(1),
        }];
        let html = render_land_price_panel("東京都", &items);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("SalesNow"));
    }

    // ---- 全国景況 (boj_tankan) ----

    #[test]
    fn render_tankan_empty_returns_no_data() {
        let html = render_boj_tankan_panel(&[]);
        assert!(html.contains("該当する集計データが見つかりません"));
        assert!(html.contains("日本銀行 短観"), "出典明記");
    }

    #[test]
    fn render_tankan_marks_national_granularity() {
        let items = vec![BojTankanLatest {
            survey_date: "2025-03-01".into(),
            industry_j: "製造業".into(),
            enterprise_size: "大企業".into(),
            di_value: Some(12.0),
        }];
        let html = render_boj_tankan_panel(&items);
        // 粒度明記: 全国値 (地域別誤認防止)
        assert!(html.contains("全国値"), "業況DIは全国値と明記");
        assert!(html.contains("市区町村別ではありません"));
    }

    #[test]
    fn render_tankan_renders_di_value_and_so_what() {
        let items = vec![BojTankanLatest {
            survey_date: "2025-03-01".into(),
            industry_j: "非製造業".into(),
            enterprise_size: "中小企業".into(),
            di_value: Some(-8.0),
        }];
        let html = render_boj_tankan_panel(&items);
        // データ妥当性: 負の DI も符号付きで描画
        assert!(html.contains("DI -8"));
        assert!(html.contains("非製造業") && html.contains("中小企業"));
        // So What: 先行指標 (可能性表現)
        assert!(html.contains("先行指標") && html.contains("可能性"));
    }

    #[test]
    fn render_tankan_di_none_shows_dash() {
        let items = vec![BojTankanLatest {
            survey_date: "2025-03-01".into(),
            industry_j: "製造業".into(),
            enterprise_size: "大企業".into(),
            di_value: None,
        }];
        let html = render_boj_tankan_panel(&items);
        assert!(html.contains("DI —"), "DI None は — で明示");
    }

    #[test]
    fn render_tankan_no_salesnow_and_no_forbidden_words() {
        let items = vec![BojTankanLatest {
            survey_date: "2025-03-01".into(),
            industry_j: "製造業".into(),
            enterprise_size: "大企業".into(),
            di_value: Some(5.0),
        }];
        let html = render_boj_tankan_panel(&items);
        assert!(!html.contains("SalesNow"));
        assert!(!html.contains("劣位") && !html.contains("集中") && !html.contains("縮小"));
    }

    // ---- 環境補足 (climate) ----

    #[test]
    fn render_climate_pref_empty_returns_pref_required() {
        let html = render_climate_panel("", None);
        assert!(html.contains("都道府県を選択"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn render_climate_none_returns_no_data() {
        let html = render_climate_panel("新潟県", None);
        assert!(html.contains("該当する集計データが見つかりません"));
        assert!(html.contains("気象庁"), "出典明記");
    }

    #[test]
    fn render_climate_marks_supplement_granularity() {
        let d = ClimateLatest {
            prefecture: "新潟県".into(),
            fiscal_year: 2023,
            avg_temperature: Some(14.0),
            max_temperature: Some(35.0),
            min_temperature: Some(-5.0),
            snow_days: Some(60.0),
            sunshine_hours: Some(1700.0),
        };
        let html = render_climate_panel("新潟県", Some(&d));
        // 粒度明記: 環境補足情報
        assert!(html.contains("環境補足"), "気候は環境補足と明記");
        // データ妥当性: 降雪日数/平均気温が描画
        assert!(html.contains("60 日/年") && html.contains("14.0℃"));
        assert!(html.contains("2023"));
        // So What: 多雪ケース (可能性表現)
        assert!(html.contains("可能性"));
    }

    #[test]
    fn render_climate_snow_none_shows_dash() {
        let d = ClimateLatest {
            prefecture: "沖縄県".into(),
            fiscal_year: 2023,
            avg_temperature: None,
            max_temperature: None,
            min_temperature: None,
            snow_days: None,
            sunshine_hours: None,
        };
        let html = render_climate_panel("沖縄県", Some(&d));
        assert!(html.contains("—"), "None 値は — で明示");
        assert!(html.contains("欠損"));
    }

    #[test]
    fn render_climate_no_salesnow_name() {
        let d = ClimateLatest {
            prefecture: "東京都".into(),
            fiscal_year: 2023,
            avg_temperature: Some(16.0),
            max_temperature: Some(38.0),
            min_temperature: Some(2.0),
            snow_days: Some(3.0),
            sunshine_hours: Some(1900.0),
        };
        let html = render_climate_panel("東京都", Some(&d));
        assert!(!html.contains("SalesNow"));
        // 少雪ケース
        assert!(html.contains("少なめ"));
    }

    // ---- fmt_pct_opt ----

    #[test]
    fn fmt_pct_opt_handles_none_and_nan() {
        assert_eq!(fmt_pct_opt(None), "—");
        assert_eq!(fmt_pct_opt(Some(f64::NAN)), "—");
        assert_eq!(fmt_pct_opt(Some(5.5)), "5.50%");
    }

    // ============================================================
    // Team H-Fix (2026-06-08):
    // fetch_company_segments の LIKE pattern が
    // strip_county_prefix 経由で生成されることを確認。
    // 6市町 (郡山市/郡上市/蒲郡市/上郡町/大和郡山市/小郡市) は identity preserved。
    // ============================================================

    fn segments_like_pattern(muni: &str) -> String {
        format!("%{}%", strip_county_prefix(muni))
    }

    #[test]
    fn segments_strip_gun_prefix_for_minamimatsuura() {
        // 南松浦郡新上五島町 → 新上五島町
        assert_eq!(segments_like_pattern("南松浦郡新上五島町"), "%新上五島町%");
    }

    #[test]
    fn segments_identity_for_yamatokoriyama_city() {
        // 大和郡山市 は地名の一部に「郡」を含むが市名そのもの → strip しない
        assert_eq!(segments_like_pattern("大和郡山市"), "%大和郡山市%");
    }
}
