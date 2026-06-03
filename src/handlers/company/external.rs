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
use crate::handlers::helpers::{escape_html, format_number, get_f64, get_i64, get_str, Row};
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
    let muni_pattern = format!("%{}%", muni);

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
                hx-get="/api/company/external/industry_structure" hx-include="#company-ext-pref,#company-ext-muni" hx-trigger="change" hx-target="#company-ext-industry" hx-swap="innerHTML"
                onchange="
                  htmx.ajax('GET','/api/company/external/establishments?'+new URLSearchParams({{pref:this.value,muni:document.getElementById('company-ext-muni').value}}),{{target:'#company-ext-establishments',swap:'innerHTML'}});
                  htmx.ajax('GET','/api/company/external/segments?'+new URLSearchParams({{pref:this.value,muni:document.getElementById('company-ext-muni').value}}),{{target:'#company-ext-segments',swap:'innerHTML'}});
                ">
          <option value="">-- 選択 --</option>
{pref_options}
        </select>
      </div>
      <div>
        <label class="block text-xs text-slate-400 mb-1">市区町村 (任意)</label>
        <input type="text" id="company-ext-muni" name="muni" placeholder="例: 札幌市中央区"
               class="w-full bg-slate-700 text-white text-sm rounded px-2 py-1 border border-slate-600 focus:border-blue-500 focus:outline-none"
               hx-get="/api/company/external/industry_structure" hx-include="#company-ext-pref,#company-ext-muni" hx-trigger="keyup changed delay:500ms" hx-target="#company-ext-industry" hx-swap="innerHTML"
               onkeyup="
                 if(event.key==='Enter') {{
                   htmx.ajax('GET','/api/company/external/establishments?'+new URLSearchParams({{pref:document.getElementById('company-ext-pref').value,muni:this.value}}),{{target:'#company-ext-establishments',swap:'innerHTML'}});
                   htmx.ajax('GET','/api/company/external/segments?'+new URLSearchParams({{pref:document.getElementById('company-ext-pref').value,muni:this.value}}),{{target:'#company-ext-segments',swap:'innerHTML'}});
                 }}
               " />
      </div>
    </div>
    <div id="company-ext-industry" class="mb-3"></div>
    <div id="company-ext-establishments" class="mb-3"></div>
    <div id="company-ext-segments" class="mb-3"></div>
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
}
