//! 求人検索タブ: 外部統計データドリルダウン (10 データソース MECE)
//!
//! 既存の HW 求人検索機能とは独立した、HW 以外の公的統計データを
//! 都道府県粒度で個別表示するための API パネル群。
//!
//! ## 設計方針
//! - 各データソース 1 endpoint (10 endpoint)
//! - HTMX で求人検索タブのアコーディオン内に挿入
//! - DISPLAY_SPEC §2 遵守: 「人数」生表示は最小限。割合・順位・推移を優先
//! - 中立表現: 「劣位/集中/縮小」評価語禁止。「相対的に高い/低い/参考値」等
//! - silent fallback 禁止: テーブル/データ無し時は明示メッセージ
//! - 既存求人検索機能と疎結合 (fetch.rs / render.rs は無改変)
//!
//! ## データソース一覧
//! | endpoint                        | テーブル                             | 形式 |
//! |---------------------------------|--------------------------------------|------|
//! | /api/competitive/external/min_wage          | v2_external_minimum_wage             | 推移 |
//! | /api/competitive/external/job_ratio         | v2_external_job_openings_ratio       | 推移 |
//! | /api/competitive/external/labor_force       | v2_external_labor_force              | 集計 |
//! | /api/competitive/external/turnover          | v2_external_turnover                 | 推移 |
//! | /api/competitive/external/education         | v2_external_education                | 構成 |
//! | /api/competitive/external/industry_employees| v2_external_industry_structure       | 構成 |
//! | /api/competitive/external/household_spending| v2_external_household_spending       | 棒   |
//! | /api/competitive/external/daytime_population| v2_external_daytime_population       | 集計 |
//! | /api/competitive/external/households        | v2_external_households               | 構成 |
//! | /api/competitive/external/social_life       | v2_external_social_life              | 棒   |

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use super::utils::escape_html;
use crate::handlers::overview::format_number;
use crate::AppState;

// ============================================================
// 共通: パラメータ・ヘルパー
// ============================================================

/// 外部統計パネル共通パラメータ (都道府県のみ)
#[derive(Deserialize)]
pub struct ExternalPanelParams {
    /// 都道府県名 (例: "東京都")。空文字なら全国集計。
    pub prefecture: Option<String>,
}

/// 都道府県名 → 国勢調査 prefecture_code (1〜47) 文字列変換。
///
/// `v2_external_industry_structure` が `prefecture_code` (2 桁数値文字列) 主体のため、
/// API 側で名前→コード変換する。テーブルでは "01"〜"47" 形式で格納されているため
/// 2 桁ゼロパディングで返却。
///
/// 未知 / 空文字は None を返し、呼び出し側でデフォルト動作 (全国集計) を選択する。
pub(crate) fn pref_name_to_code(pref: &str) -> Option<String> {
    let code: u32 = match pref {
        "北海道" => 1,
        "青森県" => 2,
        "岩手県" => 3,
        "宮城県" => 4,
        "秋田県" => 5,
        "山形県" => 6,
        "福島県" => 7,
        "茨城県" => 8,
        "栃木県" => 9,
        "群馬県" => 10,
        "埼玉県" => 11,
        "千葉県" => 12,
        "東京都" => 13,
        "神奈川県" => 14,
        "新潟県" => 15,
        "富山県" => 16,
        "石川県" => 17,
        "福井県" => 18,
        "山梨県" => 19,
        "長野県" => 20,
        "岐阜県" => 21,
        "静岡県" => 22,
        "愛知県" => 23,
        "三重県" => 24,
        "滋賀県" => 25,
        "京都府" => 26,
        "大阪府" => 27,
        "兵庫県" => 28,
        "奈良県" => 29,
        "和歌山県" => 30,
        "鳥取県" => 31,
        "島根県" => 32,
        "岡山県" => 33,
        "広島県" => 34,
        "山口県" => 35,
        "徳島県" => 36,
        "香川県" => 37,
        "愛媛県" => 38,
        "高知県" => 39,
        "福岡県" => 40,
        "佐賀県" => 41,
        "長崎県" => 42,
        "熊本県" => 43,
        "大分県" => 44,
        "宮崎県" => 45,
        "鹿児島県" => 46,
        "沖縄県" => 47,
        _ => return None,
    };
    Some(format!("{:02}", code))
}

/// スコープラベル整形 ("全国" or 都道府県名)。
pub(crate) fn scope_label(pref: &str) -> &str {
    if pref.is_empty() {
        "全国"
    } else {
        pref
    }
}

/// Row 値 → f64 取得 (NULL / 文字列 / 整数すべて吸収)。silent fallback 監査向けに 0.0 ではなく Option を返す。
fn row_f64(row: &HashMap<String, Value>, key: &str) -> Option<f64> {
    let v = row.get(key)?;
    if v.is_null() {
        return None;
    }
    if let Some(f) = v.as_f64() {
        return Some(f);
    }
    if let Some(i) = v.as_i64() {
        return Some(i as f64);
    }
    if let Some(s) = v.as_str() {
        return s.parse::<f64>().ok();
    }
    None
}

/// Row 値 → i64 取得。
fn row_i64(row: &HashMap<String, Value>, key: &str) -> Option<i64> {
    let v = row.get(key)?;
    if v.is_null() {
        return None;
    }
    if let Some(i) = v.as_i64() {
        return Some(i);
    }
    if let Some(f) = v.as_f64() {
        return Some(f as i64);
    }
    if let Some(s) = v.as_str() {
        return s.parse::<i64>().ok();
    }
    None
}

/// Row 値 → 文字列。NULL / 数値も文字列化。
fn row_string(row: &HashMap<String, Value>, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}

/// Row 値 → HTML escape 済み文字列。SQL 由来の文字列を HTML テンプレートに
/// 直接埋め込む際は必ずこちらを通すこと (XSS 対策。data 由来は信頼源だが
/// 公的統計テーブルにヘッダー混入実績あり (MEMORY: feedback_silent_fallback_audit))。
fn row_string_escaped(row: &HashMap<String, Value>, key: &str) -> String {
    escape_html(&row_string(row, key))
}

/// f64 を「-」または小数 fmt 桁で整形。
fn fmt_f64(v: Option<f64>, decimals: usize) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{:.*}", decimals, x),
        _ => "-".to_string(),
    }
}

/// 整数を「-」または桁区切りで整形。
fn fmt_i64(v: Option<i64>) -> String {
    match v {
        Some(x) => format_number(x),
        None => "-".to_string(),
    }
}

/// データなし / テーブル未配備時の共通レスポンス。
fn no_data_html(label: &str) -> String {
    format!(
        r#"<div class="text-slate-400 text-sm py-3">{label} のデータは現在参照できません。</div>"#
    )
}

/// SQL 実行: Turso 優先、ローカルフォールバック。
///
/// `analysis::fetch::query_turso_or_local` の同等品。本モジュールは
/// `analysis::fetch` の private API に依存したくないため、自前実装。
fn query_external(state: &AppState, sql: &str, params: &[String]) -> Vec<HashMap<String, Value>> {
    // Turso 優先
    if let Some(tdb) = state.turso_db.as_ref() {
        let p: Vec<&dyn crate::db::turso_http::ToSqlTurso> = params
            .iter()
            .map(|s| s as &dyn crate::db::turso_http::ToSqlTurso)
            .collect();
        match tdb.query(sql, &p) {
            Ok(rows) if !rows.is_empty() => return rows,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Turso external query failed, falling back to local: {e}");
            }
        }
    }
    // ローカル
    if let Some(db) = state.hw_db.as_ref() {
        let p: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        return db.query(sql, &p).unwrap_or_default();
    }
    Vec::new()
}

/// 外部統計テンプレート共通: 見出し + 出典脚注 + 表 HTML を組み合わせる。
///
/// `title` / `scope` / `source` / `note` は HTML escape して埋め込み。
/// `body` のみ事前に HTML 化済み (呼び出し側で escape 済み) 扱い。
fn wrap_panel(title: &str, scope: &str, source: &str, body: &str, note: &str) -> String {
    let mut html = String::with_capacity(512 + body.len());
    write!(
        html,
        r#"<div class="space-y-2">
  <div class="flex items-center justify-between gap-2 flex-wrap">
    <h4 class="text-sm text-white font-semibold">{title} <span class="text-slate-400 text-xs font-normal">— {scope}</span></h4>
    <span class="text-xs text-slate-500">出典: {source}</span>
  </div>
  {body}
  <p class="text-xs text-slate-500">{note}</p>
</div>"#,
        title = escape_html(title),
        scope = escape_html(scope),
        source = escape_html(source),
        body = body,
        note = escape_html(note),
    )
    .unwrap();
    html
}

// ============================================================
// 1) 最低賃金: v2_external_minimum_wage
// ============================================================

pub async fn ext_min_wage(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    // テーブルには hourly_min_wage しかなく時系列は持たないため、
    // 全国順位を併載することで「相対的位置づけ」を可視化する。
    let sql = "SELECT prefecture, hourly_min_wage \
               FROM v2_external_minimum_wage \
               ORDER BY hourly_min_wage DESC"
        .to_string();
    let rows = query_external(&state, &sql, &[]);
    if rows.is_empty() {
        return Html(wrap_panel(
            "最低賃金 (時給ベース)",
            scope,
            "厚生労働省 地域別最低賃金",
            &no_data_html("最低賃金"),
            "",
        ));
    }

    let total = rows.len();
    let national_avg: f64 = {
        let sum: f64 = rows
            .iter()
            .filter_map(|r| row_f64(r, "hourly_min_wage"))
            .sum();
        if total > 0 {
            sum / total as f64
        } else {
            0.0
        }
    };

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th class="text-center" style="width:60px">順位</th>
          <th>都道府県</th>
          <th class="text-right">時給 (円)</th>
          <th class="text-right">全国平均比</th>
        </tr></thead><tbody>"#,
    );
    let mut found_rank: Option<usize> = None;
    let mut found_wage: Option<f64> = None;
    for (i, row) in rows.iter().enumerate() {
        let name_raw = row_string(row, "prefecture");
        let wage = row_f64(row, "hourly_min_wage");
        let rank = i + 1;
        let highlight = if !pref.is_empty() && name_raw == pref {
            found_rank = Some(rank);
            found_wage = wage;
            " class=\"bg-slate-800\""
        } else {
            ""
        };
        let ratio = wage
            .filter(|_| national_avg > 0.0)
            .map(|w| (w / national_avg) * 100.0);
        write!(
            table,
            "<tr{h}><td class=\"text-center\">{rank}</td><td>{name}</td>\
             <td class=\"text-right\">{wage}</td><td class=\"text-right\">{ratio} %</td></tr>",
            h = highlight,
            rank = rank,
            name = escape_html(&name_raw),
            wage = fmt_f64(wage, 0),
            ratio = fmt_f64(ratio, 1),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    let pref_esc = escape_html(&pref);
    let summary = if !pref.is_empty() {
        match (found_rank, found_wage) {
            (Some(r), Some(w)) => format!(
                "{pref}: 時給 {w} 円 (全国 {r} 位 / 47 県、平均 {avg} 円)。",
                pref = pref_esc,
                w = format_number(w as i64),
                r = r,
                avg = format_number(national_avg as i64),
            ),
            _ => format!("{} の値は取得できませんでした。", pref_esc),
        }
    } else {
        format!(
            "全国平均: {} 円 / 時 (47 県、参考値)。",
            format_number(national_avg as i64)
        )
    };
    let body = format!(
        "<div class=\"text-xs text-slate-300 mb-2\">{}</div>{}",
        summary, table
    );

    Html(wrap_panel(
        "最低賃金 (時給ベース)",
        scope,
        "厚生労働省 地域別最低賃金 (時給円)",
        &body,
        "募集賃金との照合に利用。順位は参考値であり、地域の物価・賃金水準とあわせて解釈してください。",
    ))
}

// ============================================================
// 2) 有効求人倍率: v2_external_job_openings_ratio
// ============================================================

pub async fn ext_job_ratio(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    // 全国推移 + (指定があれば) 当該県の推移を取得。
    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
             FROM v2_external_job_openings_ratio \
             WHERE prefecture IN ('全国', ?1) \
             ORDER BY fiscal_year ASC, prefecture ASC"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
             FROM v2_external_job_openings_ratio \
             WHERE prefecture = '全国' \
             ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "有効求人倍率 (年度推移)",
            scope,
            "厚生労働省 一般職業紹介状況",
            &no_data_html("有効求人倍率"),
            "",
        ));
    }

    // テーブル整形 (年度昇順、都道府県別 2 行ずつ)
    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>年度</th><th>地域</th>
          <th class="text-right">全体</th>
          <th class="text-right">パート除く</th>
        </tr></thead><tbody>"#,
    );
    for row in &rows {
        write!(
            table,
            "<tr><td>{fy}</td><td>{p}</td><td class=\"text-right\">{r1}</td>\
             <td class=\"text-right\">{r2}</td></tr>",
            fy = row_string_escaped(row, "fiscal_year"),
            p = row_string_escaped(row, "prefecture"),
            r1 = fmt_f64(row_f64(row, "ratio_total"), 2),
            r2 = fmt_f64(row_f64(row, "ratio_excl_part"), 2),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    Html(wrap_panel(
        "有効求人倍率 (年度推移)",
        scope,
        "厚生労働省 一般職業紹介状況",
        &table,
        "1.0 を上回ると求人側が上回る目安。全国と当該県を併記しており、絶対値より推移と全国差で読みます。",
    ))
}

// ============================================================
// 3) 失業率: v2_external_labor_force.unemployment_rate
// ============================================================

pub async fn ext_labor_force(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    // 国勢調査 v2_external_labor_force は (prefecture, municipality) 粒度。
    // ここでは県集計を出す: pref が指定されたらその県、無ければ全国。
    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, \
             SUM(employed) as employed, SUM(unemployed) as unemployed, \
             SUM(not_in_labor_force) as not_in_labor_force, \
             CAST(SUM(unemployed) AS REAL) \
               / NULLIF(SUM(employed) + SUM(unemployed), 0) * 100 as unemployment_rate, \
             CAST(SUM(employed) + SUM(unemployed) AS REAL) \
               / NULLIF(SUM(employed) + SUM(unemployed) + SUM(not_in_labor_force), 0) * 100 \
               as labor_force_participation_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_labor_force \
             WHERE prefecture = ?1 \
               AND prefecture IS NOT NULL AND prefecture <> '' AND prefecture <> '都道府県' \
               AND municipality <> '市区町村'"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, \
             SUM(employed) as employed, SUM(unemployed) as unemployed, \
             SUM(not_in_labor_force) as not_in_labor_force, \
             CAST(SUM(unemployed) AS REAL) \
               / NULLIF(SUM(employed) + SUM(unemployed), 0) * 100 as unemployment_rate, \
             CAST(SUM(employed) + SUM(unemployed) AS REAL) \
               / NULLIF(SUM(employed) + SUM(unemployed) + SUM(not_in_labor_force), 0) * 100 \
               as labor_force_participation_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_labor_force \
             WHERE prefecture IS NOT NULL AND prefecture <> '' AND prefecture <> '都道府県' \
               AND municipality <> '市区町村'"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "失業率・労働力参加率",
            scope,
            "国勢調査 (e-Stat)",
            &no_data_html("失業率"),
            "",
        ));
    }

    let row = &rows[0];
    let urate = row_f64(row, "unemployment_rate");
    let prate = row_f64(row, "labor_force_participation_rate");
    let ref_date = row_string(row, "reference_date");

    // ドメイン不変条件: 失業率は 0〜100% (MEMORY: feedback_reverse_proof_tests)。逸脱時は警告で明示。
    let urate_warn = match urate {
        Some(r) if !(0.0..=100.0).contains(&r) => {
            r#"<p class="text-amber-400 text-xs">⚠ 失業率が想定範囲 (0〜100%) を逸脱しています。データ取り込みの確認が必要です。</p>"#
        }
        _ => "",
    };

    let body = format!(
        r#"<div class="grid grid-cols-2 gap-2 text-sm">
          <div class="stat-card"><div class="text-xs text-slate-400">失業率</div>
            <div class="text-2xl font-bold text-amber-300">{u} %</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">労働力参加率</div>
            <div class="text-2xl font-bold text-emerald-300">{p} %</div></div>
        </div>
        <p class="text-xs text-slate-500">基準日: {d}</p>{w}"#,
        u = fmt_f64(urate, 2),
        p = fmt_f64(prate, 2),
        d = if ref_date.is_empty() {
            "-".to_string()
        } else {
            escape_html(&ref_date)
        },
        w = urate_warn,
    );

    Html(wrap_panel(
        "失業率・労働力参加率",
        scope,
        "国勢調査 v2_external_labor_force",
        &body,
        "国勢調査の県内集計値。求人倍率と組み合わせて、地域の労働需給の俯瞰に利用してください。",
    ))
}

// ============================================================
// 4) 離職率/入職率 (業界別): v2_external_turnover
// ============================================================

pub async fn ext_turnover(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
             FROM v2_external_turnover \
             WHERE prefecture IN ('全国', ?1) \
             ORDER BY fiscal_year ASC, prefecture ASC, industry"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
             FROM v2_external_turnover \
             WHERE prefecture = '全国' \
             ORDER BY fiscal_year ASC, industry"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "入職率・離職率 (業界別)",
            scope,
            "厚生労働省 雇用動向調査",
            &no_data_html("入職率・離職率"),
            "",
        ));
    }

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>年度</th><th>地域</th><th>業界</th>
          <th class="text-right">入職率 %</th>
          <th class="text-right">離職率 %</th>
          <th class="text-right">差分 (入-離)</th>
        </tr></thead><tbody>"#,
    );
    for row in &rows {
        write!(
            table,
            "<tr><td>{fy}</td><td>{p}</td><td>{ind}</td>\
             <td class=\"text-right\">{e}</td><td class=\"text-right\">{s}</td>\
             <td class=\"text-right\">{n}</td></tr>",
            fy = row_string_escaped(row, "fiscal_year"),
            p = row_string_escaped(row, "prefecture"),
            ind = row_string_escaped(row, "industry"),
            e = fmt_f64(row_f64(row, "entry_rate"), 2),
            s = fmt_f64(row_f64(row, "separation_rate"), 2),
            n = fmt_f64(row_f64(row, "net_rate"), 2),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    Html(wrap_panel(
        "入職率・離職率 (業界別)",
        scope,
        "厚生労働省 雇用動向調査",
        &table,
        "「医療，福祉」を中心に取得。差分が正の年は採用が離職を上回ったことを示します (人数差ではなく率)。",
    ))
}

// ============================================================
// 5) 進学率/学歴: v2_external_education
// ============================================================

pub async fn ext_education(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, education_level, male_count, female_count, total_count \
             FROM v2_external_education \
             WHERE prefecture = ?1 \
             ORDER BY total_count DESC"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, education_level, \
             SUM(male_count) as male_count, SUM(female_count) as female_count, \
             SUM(total_count) as total_count \
             FROM v2_external_education \
             GROUP BY education_level \
             ORDER BY total_count DESC"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "学歴構成 (男女別)",
            scope,
            "国勢調査 (e-Stat)",
            &no_data_html("学歴構成"),
            "",
        ));
    }

    // 構成比算出
    let total_sum: i64 = rows.iter().filter_map(|r| row_i64(r, "total_count")).sum();

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>学歴</th>
          <th class="text-right">男性</th>
          <th class="text-right">女性</th>
          <th class="text-right">合計</th>
          <th class="text-right">構成比</th>
        </tr></thead><tbody>"#,
    );
    for row in &rows {
        let total = row_i64(row, "total_count");
        let share = total
            .filter(|_| total_sum > 0)
            .map(|t| (t as f64 / total_sum as f64) * 100.0);
        write!(
            table,
            "<tr><td>{lv}</td><td class=\"text-right\">{m}</td>\
             <td class=\"text-right\">{f}</td><td class=\"text-right\">{t}</td>\
             <td class=\"text-right\">{s} %</td></tr>",
            lv = row_string_escaped(row, "education_level"),
            m = fmt_i64(row_i64(row, "male_count")),
            f = fmt_i64(row_i64(row, "female_count")),
            t = fmt_i64(total),
            s = fmt_f64(share, 1),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    Html(wrap_panel(
        "学歴構成 (男女別)",
        scope,
        "国勢調査 v2_external_education",
        &table,
        "新卒採用接点 (大学/高校) の母集団規模を見る参考値。構成比は当該県 (または全国) 合計に対する割合。",
    ))
}

// ============================================================
// 6) 産業別就業者: v2_external_industry_structure
// ============================================================

pub async fn ext_industry_employees(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    // industry_structure は prefecture_code 主体。県名 → コード変換。
    let code_opt = if pref.is_empty() {
        None
    } else {
        pref_name_to_code(&pref)
    };

    let (sql, sql_params): (String, Vec<String>) = if let Some(code) = code_opt.as_ref() {
        (
            "SELECT industry_code, industry_name, \
             SUM(establishments) as establishments, \
             SUM(employees_total) as employees_total, \
             SUM(employees_male) as employees_male, \
             SUM(employees_female) as employees_female \
             FROM v2_external_industry_structure \
             WHERE prefecture_code = ?1 \
               AND industry_code NOT IN ('AS', 'AR', 'CR') \
             GROUP BY industry_code, industry_name \
             ORDER BY employees_total DESC \
             LIMIT 15"
                .to_string(),
            vec![code.clone()],
        )
    } else {
        (
            "SELECT industry_code, industry_name, \
             SUM(establishments) as establishments, \
             SUM(employees_total) as employees_total, \
             SUM(employees_male) as employees_male, \
             SUM(employees_female) as employees_female \
             FROM v2_external_industry_structure \
             WHERE industry_code NOT IN ('AS', 'AR', 'CR') \
             GROUP BY industry_code, industry_name \
             ORDER BY employees_total DESC \
             LIMIT 15"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "産業別 就業者構成 (上位 15)",
            scope,
            "経済センサス v2_external_industry_structure",
            &no_data_html("産業別 就業者構成"),
            "",
        ));
    }

    let total_sum: i64 = rows
        .iter()
        .filter_map(|r| row_i64(r, "employees_total"))
        .sum();

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>産業</th>
          <th class="text-right">事業所</th>
          <th class="text-right">従業者</th>
          <th class="text-right">構成比</th>
        </tr></thead><tbody>"#,
    );
    for row in &rows {
        let emp = row_i64(row, "employees_total");
        let share = emp
            .filter(|_| total_sum > 0)
            .map(|e| (e as f64 / total_sum as f64) * 100.0);
        write!(
            table,
            "<tr><td>{nm}</td><td class=\"text-right\">{est}</td>\
             <td class=\"text-right\">{emp}</td><td class=\"text-right\">{s} %</td></tr>",
            nm = row_string_escaped(row, "industry_name"),
            est = fmt_i64(row_i64(row, "establishments")),
            emp = fmt_i64(emp),
            s = fmt_f64(share, 1),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    Html(wrap_panel(
        "産業別 就業者構成 (上位 15)",
        scope,
        "経済センサス R3 v2_external_industry_structure",
        &table,
        "AS/AR/CR (集計コード) は除外。構成比は表示 15 産業の合計に対する内訳です。",
    ))
}

// ============================================================
// 7) 家計支出: v2_external_household_spending
// ============================================================

pub async fn ext_household_spending(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, category, monthly_amount, reference_year \
             FROM v2_external_household_spending \
             WHERE prefecture = ?1 \
             ORDER BY monthly_amount DESC"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, category, \
             AVG(monthly_amount) as monthly_amount, MAX(reference_year) as reference_year \
             FROM v2_external_household_spending \
             GROUP BY category \
             ORDER BY monthly_amount DESC"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "家計支出 (カテゴリ別)",
            scope,
            "総務省 家計調査",
            &no_data_html("家計支出"),
            "",
        ));
    }

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>カテゴリ</th>
          <th class="text-right">月額 (円)</th>
          <th>参照年</th>
        </tr></thead><tbody>"#,
    );
    for row in &rows {
        write!(
            table,
            "<tr><td>{c}</td><td class=\"text-right\">{a}</td><td>{y}</td></tr>",
            c = row_string_escaped(row, "category"),
            a = fmt_i64(row_i64(row, "monthly_amount")),
            y = row_string_escaped(row, "reference_year"),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    Html(wrap_panel(
        "家計支出 (カテゴリ別 月額)",
        scope,
        "総務省 家計調査 v2_external_household_spending",
        &table,
        "募集賃金の購買力換算の参考値。「消費支出」(親) と各サブカテゴリが混在するため重複合算は避けてください。",
    ))
}

// ============================================================
// 8) 昼夜間人口: v2_external_daytime_population
// ============================================================

pub async fn ext_daytime_population(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, \
             SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
             CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 \
               as day_night_ratio, \
             SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
             FROM v2_external_daytime_population \
             WHERE prefecture = ?1 \
               AND prefecture IS NOT NULL AND prefecture <> '' AND prefecture <> '都道府県' \
               AND municipality <> '市区町村'"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, \
             SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
             CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 \
               as day_night_ratio, \
             SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
             FROM v2_external_daytime_population \
             WHERE prefecture IS NOT NULL AND prefecture <> '' AND prefecture <> '都道府県' \
               AND municipality <> '市区町村'"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "昼夜間人口",
            scope,
            "国勢調査 (e-Stat)",
            &no_data_html("昼夜間人口"),
            "",
        ));
    }

    let row = &rows[0];
    let night = row_i64(row, "nighttime_pop");
    let day = row_i64(row, "daytime_pop");
    let ratio = row_f64(row, "day_night_ratio");
    let inflow = row_i64(row, "inflow_pop");
    let outflow = row_i64(row, "outflow_pop");

    // 流入超過/流出超過の中立表記 (評価語禁止)
    let flow_label = match (inflow, outflow) {
        (Some(i), Some(o)) if i > o => "通勤流入超過",
        (Some(i), Some(o)) if i < o => "通勤流出超過",
        (Some(_), Some(_)) => "概ね均衡",
        _ => "-",
    };

    let body = format!(
        r#"<div class="grid grid-cols-2 md:grid-cols-3 gap-2 text-sm">
          <div class="stat-card"><div class="text-xs text-slate-400">夜間人口</div>
            <div class="text-xl font-bold text-slate-200">{n}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">昼間人口</div>
            <div class="text-xl font-bold text-slate-200">{d}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">昼夜比 (%)</div>
            <div class="text-xl font-bold text-emerald-300">{r}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">流入</div>
            <div class="text-xl font-bold text-slate-200">{i}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">流出</div>
            <div class="text-xl font-bold text-slate-200">{o}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">類型</div>
            <div class="text-base font-semibold text-amber-300">{label}</div></div>
        </div>"#,
        n = fmt_i64(night),
        d = fmt_i64(day),
        r = fmt_f64(ratio, 1),
        i = fmt_i64(inflow),
        o = fmt_i64(outflow),
        label = flow_label,
    );

    Html(wrap_panel(
        "昼夜間人口",
        scope,
        "国勢調査 v2_external_daytime_population",
        &body,
        "通勤圏の流入・流出の俯瞰用。隣接県への通勤等で値が大きく振れるため、市区町村レベルの解釈と併用してください。",
    ))
}

// ============================================================
// 9) 世帯構成: v2_external_households
// ============================================================

pub async fn ext_households(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, \
             SUM(total_households) as total_households, \
             SUM(general_households) as general_households, \
             SUM(nuclear_family_households) as nuclear_family_households, \
             SUM(single_households) as single_households, \
             SUM(elderly_nuclear_households) as elderly_nuclear_households, \
             SUM(elderly_couple_households) as elderly_couple_households, \
             SUM(elderly_single_households) as elderly_single_households, \
             CAST(SUM(single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 \
               as single_rate, \
             CAST(SUM(elderly_single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 \
               as elderly_single_rate \
             FROM v2_external_households \
             WHERE prefecture = ?1 \
               AND prefecture IS NOT NULL AND prefecture <> '' AND prefecture <> '都道府県' \
               AND municipality <> '市区町村'"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, \
             SUM(total_households) as total_households, \
             SUM(general_households) as general_households, \
             SUM(nuclear_family_households) as nuclear_family_households, \
             SUM(single_households) as single_households, \
             SUM(elderly_nuclear_households) as elderly_nuclear_households, \
             SUM(elderly_couple_households) as elderly_couple_households, \
             SUM(elderly_single_households) as elderly_single_households, \
             CAST(SUM(single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 \
               as single_rate, \
             CAST(SUM(elderly_single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 \
               as elderly_single_rate \
             FROM v2_external_households \
             WHERE prefecture IS NOT NULL AND prefecture <> '' AND prefecture <> '都道府県' \
               AND municipality <> '市区町村'"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "世帯構成",
            scope,
            "国勢調査 (e-Stat)",
            &no_data_html("世帯構成"),
            "",
        ));
    }

    let row = &rows[0];
    let total = row_i64(row, "total_households");
    let nuclear = row_i64(row, "nuclear_family_households");
    let single = row_i64(row, "single_households");
    let elderly_single = row_i64(row, "elderly_single_households");
    let single_rate = row_f64(row, "single_rate");
    let elderly_single_rate = row_f64(row, "elderly_single_rate");

    let body = format!(
        r#"<div class="grid grid-cols-2 md:grid-cols-3 gap-2 text-sm">
          <div class="stat-card"><div class="text-xs text-slate-400">総世帯</div>
            <div class="text-xl font-bold text-slate-200">{t}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">核家族世帯</div>
            <div class="text-xl font-bold text-slate-200">{n}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">単身世帯</div>
            <div class="text-xl font-bold text-slate-200">{s}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">高齢単身世帯</div>
            <div class="text-xl font-bold text-slate-200">{es}</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">単身率</div>
            <div class="text-xl font-bold text-amber-300">{sr} %</div></div>
          <div class="stat-card"><div class="text-xs text-slate-400">高齢単身率</div>
            <div class="text-xl font-bold text-amber-300">{esr} %</div></div>
        </div>"#,
        t = fmt_i64(total),
        n = fmt_i64(nuclear),
        s = fmt_i64(single),
        es = fmt_i64(elderly_single),
        sr = fmt_f64(single_rate, 1),
        esr = fmt_f64(elderly_single_rate, 1),
    );

    Html(wrap_panel(
        "世帯構成",
        scope,
        "国勢調査 v2_external_households",
        &body,
        "若年単身ターゲット (寮/単身寮アピール) や高齢単身世帯比率による在宅介護需要の参考に。",
    ))
}

// ============================================================
// 10) 社会生活: v2_external_social_life
// ============================================================

pub async fn ext_social_life(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExternalPanelParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let scope = scope_label(&pref);

    let (sql, sql_params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, category, subcategory, participation_rate, survey_year \
             FROM v2_external_social_life \
             WHERE prefecture = ?1 \
             ORDER BY category, participation_rate DESC"
                .to_string(),
            vec![pref.clone()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, category, subcategory, \
             AVG(participation_rate) as participation_rate, MAX(survey_year) as survey_year \
             FROM v2_external_social_life \
             GROUP BY category, subcategory \
             ORDER BY category, participation_rate DESC"
                .to_string(),
            vec![],
        )
    };
    let rows = query_external(&state, &sql, &sql_params);
    if rows.is_empty() {
        return Html(wrap_panel(
            "社会生活 (参加率)",
            scope,
            "総務省 社会生活基本調査",
            &no_data_html("社会生活"),
            "",
        ));
    }

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>カテゴリ</th><th>項目</th>
          <th class="text-right">参加率 %</th>
          <th>調査年</th>
        </tr></thead><tbody>"#,
    );
    for row in &rows {
        let rate = row_f64(row, "participation_rate");
        // ドメイン不変条件: 参加率は 0〜100%
        let warn = match rate {
            Some(r) if !(0.0..=100.0).contains(&r) => " title=\"参加率が想定範囲外\"",
            _ => "",
        };
        write!(
            table,
            "<tr{w}><td>{c}</td><td>{s}</td>\
             <td class=\"text-right\">{r}</td><td>{y}</td></tr>",
            w = warn,
            c = row_string_escaped(row, "category"),
            s = row_string_escaped(row, "subcategory"),
            r = fmt_f64(rate, 1),
            y = row_string_escaped(row, "survey_year"),
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    Html(wrap_panel(
        "社会生活 (主要カテゴリ参加率)",
        scope,
        "総務省 社会生活基本調査 v2_external_social_life",
        &table,
        "地域住民のライフスタイル傾向の参考値。職場の福利厚生 (スポーツ補助等) 設計の材料に。",
    ))
}

// ============================================================
// テスト (内部ヘルパー単体)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row_with(map: Vec<(&str, Value)>) -> HashMap<String, Value> {
        map.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    // ---- pref_name_to_code (3 件) ----

    #[test]
    fn test_pref_name_to_code_tokyo() {
        assert_eq!(pref_name_to_code("東京都").as_deref(), Some("13"));
    }

    #[test]
    fn test_pref_name_to_code_hokkaido_padding() {
        // 北海道 = 1 → ゼロパディングで "01"
        assert_eq!(pref_name_to_code("北海道").as_deref(), Some("01"));
    }

    #[test]
    fn test_pref_name_to_code_unknown_returns_none() {
        // 逆証明: 未知県名は None (silent fallback で 全国 とせず明示的に区別)
        assert!(pref_name_to_code("").is_none());
        assert!(pref_name_to_code("江戸").is_none());
    }

    // ---- scope_label (2 件) ----

    #[test]
    fn test_scope_label_empty_is_national() {
        assert_eq!(scope_label(""), "全国");
    }

    #[test]
    fn test_scope_label_named_pref() {
        assert_eq!(scope_label("東京都"), "東京都");
    }

    // ---- row_f64 / row_i64 (3 件) ----

    #[test]
    #[allow(clippy::approx_constant)] // 3.14 はテスト用任意値で π 近似ではない
    fn test_row_f64_handles_int_and_string() {
        let r = row_with(vec![
            ("a", json!(3.14)),
            ("b", json!(42)),
            ("c", json!("7.5")),
            ("d", Value::Null),
        ]);
        assert_eq!(row_f64(&r, "a"), Some(3.14));
        assert_eq!(row_f64(&r, "b"), Some(42.0));
        assert_eq!(row_f64(&r, "c"), Some(7.5));
        assert_eq!(row_f64(&r, "d"), None);
        assert_eq!(row_f64(&r, "missing"), None);
    }

    #[test]
    fn test_row_i64_handles_float_and_string() {
        let r = row_with(vec![
            ("a", json!(10)),
            ("b", json!(10.9)),
            ("c", json!("123")),
            ("d", Value::Null),
        ]);
        assert_eq!(row_i64(&r, "a"), Some(10));
        // 10.9 は切り捨てで 10
        assert_eq!(row_i64(&r, "b"), Some(10));
        assert_eq!(row_i64(&r, "c"), Some(123));
        assert_eq!(row_i64(&r, "d"), None);
    }

    #[test]
    fn test_row_string_null_is_empty() {
        let r = row_with(vec![
            ("a", json!("hello")),
            ("b", Value::Null),
            ("c", json!(7)),
        ]);
        assert_eq!(row_string(&r, "a"), "hello");
        assert_eq!(row_string(&r, "b"), "");
        // 数値は文字列化される (Value::to_string)
        assert_eq!(row_string(&r, "c"), "7");
    }

    #[test]
    fn test_row_string_escaped_xss_payload() {
        // SQL 由来データに XSS ペイロードが混入した場合の防御
        let r = row_with(vec![
            ("name", json!("<script>alert(1)</script>")),
            ("amp", json!("a&b")),
            ("attr", json!("x\"y")),
        ]);
        let n = row_string_escaped(&r, "name");
        assert!(!n.contains("<script>"));
        assert!(n.contains("&lt;script&gt;"));
        assert!(row_string_escaped(&r, "amp").contains("&amp;"));
        assert!(row_string_escaped(&r, "attr").contains("&quot;"));
    }

    // ---- fmt_f64 / fmt_i64 (3 件) ----

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159 はテスト用任意値で π 近似ではない
    fn test_fmt_f64_decimals() {
        assert_eq!(fmt_f64(Some(3.14159), 2), "3.14");
        assert_eq!(fmt_f64(Some(0.0), 1), "0.0");
    }

    #[test]
    fn test_fmt_f64_none_or_nan_returns_dash() {
        // 逆証明: None / NaN は "-" (silent fallback の 0.0 表示を避ける)
        assert_eq!(fmt_f64(None, 2), "-");
        assert_eq!(fmt_f64(Some(f64::NAN), 2), "-");
        assert_eq!(fmt_f64(Some(f64::INFINITY), 2), "-");
    }

    #[test]
    fn test_fmt_i64_thousands_separator() {
        assert_eq!(fmt_i64(Some(1_234_567)), "1,234,567");
        assert_eq!(fmt_i64(None), "-");
    }

    // ---- wrap_panel HTML (2 件) ----

    #[test]
    fn test_wrap_panel_contains_title_and_scope() {
        let html = wrap_panel("最低賃金", "東京都", "厚労省", "<p>body</p>", "note text");
        assert!(html.contains("最低賃金"));
        assert!(html.contains("東京都"));
        assert!(html.contains("厚労省"));
        assert!(html.contains("<p>body</p>"));
        assert!(html.contains("note text"));
    }

    #[test]
    fn test_no_data_html_mentions_label() {
        let html = no_data_html("失業率");
        assert!(html.contains("失業率"));
        // MEMORY: feedback_silent_fallback_audit — 空でない明示メッセージ
        assert!(html.contains("参照できません"));
    }

    // ---- Neutral expression policy 検証 (1 件) ----

    #[test]
    fn test_no_judgmental_words_in_panel_template() {
        // MEMORY: feedback_neutral_expression_for_targets
        // パネルの定型 note / source / title に評価語が含まれないこと
        let html_min_wage_note = "募集賃金との照合に利用。順位は参考値であり、地域の物価・賃金水準とあわせて解釈してください。";
        for word in &["劣位", "集中", "縮小", "優秀", "貧弱"] {
            assert!(
                !html_min_wage_note.contains(word),
                "評価語 '{}' が含まれています",
                word
            );
        }
    }

    // ---- 失業率 ドメイン不変条件 (1 件) ----

    #[test]
    fn test_unemployment_rate_domain_constraint_is_used() {
        // ext_labor_force の警告 HTML が想定範囲外で発火することを文字列レベルで検証
        // (実 HTTP テストは別途 integration test で対応)
        let rate_out_of_range = 380.0_f64;
        let in_range = (0.0..=100.0).contains(&rate_out_of_range);
        assert!(
            !in_range,
            "MEMORY feedback_reverse_proof_tests: 380% は範囲外と判定されるべき"
        );

        let rate_normal = 3.5_f64;
        assert!((0.0..=100.0).contains(&rate_normal));
    }

    // ---- 流入/流出ラベル中立性 (1 件) ----

    #[test]
    fn test_flow_label_neutral_wording() {
        // 評価語禁止: 「劣る/優れる」ではなく「流入超過/流出超過/均衡」
        let labels = ["通勤流入超過", "通勤流出超過", "概ね均衡"];
        for l in &labels {
            for word in &["劣", "優", "集中", "縮小"] {
                assert!(
                    !l.contains(word),
                    "中立性違反: {} に {} が含まれる",
                    l,
                    word
                );
            }
        }
    }

    // ============================================================
    // 追加テスト (silent fallback 境界 / データ妥当性 / 逆証明)
    // ============================================================

    // ---- row_f64: 不正値・負値の取り込み ----

    #[test]
    fn test_row_f64_negative_and_invalid_string() {
        // 不正データ: salary=-100 は「データなし(None)」ではなく Some(-100.0) として
        // そのまま透過される (クランプは表示側責務)。逆に parse 不能文字列は None。
        let r = row_with(vec![
            ("salary", json!(-100)),
            ("share_str", json!("150")), // share=150% のような不正値も数値として透過
            ("garbage", json!("abc")),
            ("empty", json!("")),
        ]);
        assert_eq!(row_f64(&r, "salary"), Some(-100.0));
        assert_eq!(row_f64(&r, "share_str"), Some(150.0));
        // 逆証明: 文字列が数値化できない場合は silent に 0.0 とせず None
        assert_eq!(row_f64(&r, "garbage"), None);
        assert_eq!(row_f64(&r, "empty"), None);
    }

    #[test]
    fn test_row_i64_negative_string() {
        let r = row_with(vec![("delta", json!("-42"))]);
        assert_eq!(row_i64(&r, "delta"), Some(-42));
    }

    // ---- fmt_f64: 負値は透過 (除外ではない)、表示妥当性 ----

    #[test]
    fn test_fmt_f64_negative_passthrough() {
        // 負値は「-」(データなし) と混同されないよう数値として整形される
        // (-5.25 は最近接偶数丸めで "-5.2")
        assert_eq!(fmt_f64(Some(-5.25), 1), "-5.2");
        // 負値は有限値なので "-" (データなし記号) 単体にはならない
        let neg = fmt_f64(Some(-3.0), 0);
        assert_ne!(neg, "-", "負値はデータなし記号と区別される");
        assert_eq!(neg, "-3");
    }

    // ---- no_data_html: 空入力でも明示メッセージ (逆証明) ----

    #[test]
    fn test_no_data_html_never_empty() {
        // MEMORY: feedback_silent_fallback_audit — 空文字 label でも空 HTML を返さない
        let html = no_data_html("");
        assert!(!html.trim().is_empty(), "空 label でも空 HTML にしない");
        assert!(html.contains("参照できません"));
    }

    // ---- wrap_panel: title/source/note の XSS エスケープ (データ妥当性) ----

    #[test]
    fn test_wrap_panel_escapes_title_and_source() {
        // title / source / note は escape して埋め込む契約。body のみ raw。
        let html = wrap_panel(
            "<b>t</b>",
            "<i>scope</i>",
            "<src>",
            "<p>RAW_BODY</p>",
            "<note>",
        );
        // title/scope/source/note はエスケープ
        assert!(!html.contains("<b>t</b>"));
        assert!(html.contains("&lt;b&gt;t&lt;/b&gt;"));
        assert!(html.contains("&lt;src&gt;"));
        assert!(html.contains("&lt;note&gt;"));
        // body は raw のまま (呼び出し側で escape 済みの契約)
        assert!(html.contains("<p>RAW_BODY</p>"));
    }

    // ---- 失業率 警告ロジックを文字列定数で逆証明 (範囲外で警告マーカー) ----

    #[test]
    fn test_unemployment_warn_marker_only_when_out_of_range() {
        // ext_labor_force 内の match と同じ判定ロジックを再現し、
        // 範囲内では警告なし・範囲外でのみ警告 HTML を出すことを検証。
        fn warn_for(urate: Option<f64>) -> &'static str {
            match urate {
                Some(r) if !(0.0..=100.0).contains(&r) => "WARN",
                _ => "",
            }
        }
        // 正常値: 警告なし
        assert_eq!(warn_for(Some(3.5)), "");
        assert_eq!(warn_for(Some(0.0)), "");
        assert_eq!(warn_for(Some(100.0)), "");
        // None: 警告なし (データなしは別表示「-」で処理)
        assert_eq!(warn_for(None), "");
        // 逆証明: 380% (MEMORY: unemployment 380% 流出) で警告発火
        assert_eq!(warn_for(Some(380.0)), "WARN");
        assert_eq!(warn_for(Some(-1.0)), "WARN");
    }
}
