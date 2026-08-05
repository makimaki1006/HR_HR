//! ジャーニーマップ用 辞書カード API（軽量 JSON）。
//!
//! ルート（`build_app()` の `protected_routes` に登録）:
//!   GET /api/dict/license_card?name=中型免許        : 資格辞書カード
//!   GET /api/dict/occupation_card?name=トラックドライバー : 職種辞典カード
//!
//! 用途:
//!   ジャーニーマップ（static/js/journey_map.js）内の資格名・職種名にホバーした際に
//!   出す小さなカード。既存の資格辞書タブ（handlers::license）／職種辞典タブ
//!   （handlers::driver）と同じ Turso テーブルを参照するが、ホバーごとに叩かれる
//!   ため取得列を絞った軽量版として独立実装している。
//!
//! 参照テーブル（**読み取りのみ**）:
//!   v2_external_jobtag_qualifications  (jobtag_id, item_order, name)
//!   v2_external_jobtag_occupation      (jobtag_id, name, category, aliases, wage_census_code)
//!   v2_external_jobtag_description     (jobtag_id, summary, ...)
//!   v2_external_jobtag_wage_age        (wage_census_code, age_range_order, annual_salary_man_yen, avg_age)
//!
//! 失敗時の方針（silent fallback 禁止）:
//!   Turso 未接続・該当なし・クエリ失敗のいずれも HTTP 200 + `{"found": false, "reason": ...}`
//!   を返す。reason は "not_configured" / "not_found" / "empty_name" / "name_too_long" /
//!   "query_failed" のいずれかで、呼び出し側が区別できるようにする。

use axum::{
    extract::{Query, State},
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::error;

use crate::db::turso_http::{ToSqlTurso, TursoDb};
use crate::AppState;

/// 名前の最大長（これを超える入力は LIKE を投げずに弾く）。
const MAX_NAME_CHARS: usize = 100;
/// 職種カードの summary 最大文字数。
const SUMMARY_MAX_CHARS: usize = 120;
/// 共起資格の返却件数。
const CO_OCCURRING_LIMIT: usize = 3;

/// 辞書カード API のルーターを公開する。
///
/// `build_app()` の `protected_routes` チェーンに以下のように組み込む:
///   `.merge(handlers::dict_cards::router())`
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/dict/license_card", get(api_license_card))
        .route("/api/dict/occupation_card", get(api_occupation_card))
}

#[derive(Deserialize, Default)]
pub struct CardQuery {
    /// 引きたい資格名・職種名。省略時は found:false。
    pub name: Option<String>,
}

// =========================================================================
// ハンドラ
// =========================================================================

pub async fn api_license_card(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CardQuery>,
) -> Json<Value> {
    let name = match validate_name(q.name.as_deref()) {
        Ok(n) => n,
        Err(v) => return Json(v),
    };
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => return Json(not_found("not_configured", &name)),
    };

    let name_for_task = name.clone();
    match tokio::task::spawn_blocking(move || build_license_card(&turso, &name_for_task)).await {
        Ok(Ok(v)) => Json(v),
        Ok(Err(e)) => {
            error!("license_card({name}) failed: {e}");
            Json(not_found("query_failed", &name))
        }
        Err(e) => {
            error!("spawn_blocking failed (license_card): {e}");
            Json(not_found("query_failed", &name))
        }
    }
}

pub async fn api_occupation_card(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CardQuery>,
) -> Json<Value> {
    let name = match validate_name(q.name.as_deref()) {
        Ok(n) => n,
        Err(v) => return Json(v),
    };
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => return Json(not_found("not_configured", &name)),
    };

    let name_for_task = name.clone();
    match tokio::task::spawn_blocking(move || build_occupation_card(&turso, &name_for_task)).await {
        Ok(Ok(v)) => Json(v),
        Ok(Err(e)) => {
            error!("occupation_card({name}) failed: {e}");
            Json(not_found("query_failed", &name))
        }
        Err(e) => {
            error!("spawn_blocking failed (occupation_card): {e}");
            Json(not_found("query_failed", &name))
        }
    }
}

// =========================================================================
// カード組み立て
// =========================================================================

/// 資格カード。resolve → 統計 → 共起資格 の最大 (試行回数 + 2) クエリ。
fn build_license_card(turso: &TursoDb, query_name: &str) -> Result<Value, String> {
    let resolved = match resolve_license_name(turso, query_name)? {
        Some(r) => r,
        None => return Ok(not_found("not_found", query_name)),
    };
    let name = resolved.name;

    // 関連職業 1 行 = 1 職業。賃金センサスの総計行 (age_range_order=0) を LEFT JOIN。
    // handlers::license::data::compute_stats と同じ母集団定義。
    let rows = turso.query(
        "SELECT w.annual_salary_man_yen AS sal, w.avg_age AS age \
         FROM v2_external_jobtag_qualifications q \
         JOIN v2_external_jobtag_occupation o ON o.jobtag_id = q.jobtag_id \
         LEFT JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0 \
         WHERE q.name = ?",
        &[&name as &dyn ToSqlTurso],
    )?;

    let occupation_count = rows.len() as i64;
    let salaries: Vec<f64> = rows.iter().filter_map(|r| f(r, "sal")).collect();
    let ages: Vec<f64> = rows.iter().filter_map(|r| f(r, "age")).collect();

    let median_salary_man_yen = median(salaries.clone());
    let avg_age = if ages.is_empty() {
        None
    } else {
        Some(ages.iter().sum::<f64>() / ages.len() as f64)
    };

    // 共起資格 上位 N
    let co_rows = turso.query(
        "SELECT q2.name AS name, COUNT(DISTINCT q2.jobtag_id) AS co_n \
         FROM v2_external_jobtag_qualifications q1 \
         JOIN v2_external_jobtag_qualifications q2 USING (jobtag_id) \
         WHERE q1.name = ? AND q2.name != q1.name \
         GROUP BY q2.name \
         ORDER BY co_n DESC, LENGTH(q2.name) ASC \
         LIMIT ?",
        &[
            &name as &dyn ToSqlTurso,
            &(CO_OCCURRING_LIMIT as i64) as &dyn ToSqlTurso,
        ],
    )?;
    let co_occurring_licenses: Vec<Value> = co_rows
        .iter()
        .map(|r| json!({ "name": s(r, "name"), "count": i(r, "co_n") }))
        .collect();

    Ok(json!({
        "found": true,
        "kind": "license",
        "query": query_name,
        "name": name,
        "matched_by": resolved.matched_by,
        "occupation_count": occupation_count,
        "wage_target_n": salaries.len() as i64,
        "median_salary_man_yen": round1(median_salary_man_yen),
        "avg_age": round1(avg_age),
        "co_occurring_licenses": co_occurring_licenses,
        "tab_url": tab_url(&format!("/tab/license/{}", urlencoding::encode(&name))),
        "source": "職業情報データベース 資格情報 ver.7.01 (JILPT) / 賃金構造基本統計調査 令和7年 表5",
    }))
}

/// 職種カード。resolve → 詳細 1 クエリ。
fn build_occupation_card(turso: &TursoDb, query_name: &str) -> Result<Value, String> {
    let resolved = match resolve_occupation(turso, query_name)? {
        Some(r) => r,
        None => return Ok(not_found("not_found", query_name)),
    };

    let rows = turso.query(
        "SELECT o.jobtag_id AS jobtag_id, o.name AS name, COALESCE(o.category,'') AS category, \
                COALESCE(d.summary,'') AS summary, \
                w.annual_salary_man_yen AS sal, w.avg_age AS age \
         FROM v2_external_jobtag_occupation o \
         LEFT JOIN v2_external_jobtag_description d ON d.jobtag_id = o.jobtag_id \
         LEFT JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0 \
         WHERE o.jobtag_id = ?",
        &[&resolved.jobtag_id as &dyn ToSqlTurso],
    )?;
    let row = match rows.first() {
        Some(r) => r,
        None => return Ok(not_found("not_found", query_name)),
    };

    let category = s(row, "category");
    let summary = truncate_chars(&s(row, "summary"), SUMMARY_MAX_CHARS);

    Ok(json!({
        "found": true,
        "kind": "occupation",
        "query": query_name,
        "name": s(row, "name"),
        "jobtag_id": i(row, "jobtag_id"),
        "matched_by": resolved.matched_by,
        "category": category,
        "category_label": category_label(&category),
        "summary": summary,
        "annual_salary_man_yen": round1(f(row, "sal")),
        "avg_age": round1(f(row, "age")),
        "tab_url": tab_url(&format!("/tab/driver/{}", i(row, "jobtag_id"))),
        "source": "職業情報データベース ver.7.01 (JILPT) / 賃金構造基本統計調査 令和7年 表5",
    }))
}

// =========================================================================
// 曖昧一致（完全一致 → 前方一致 → 部分一致 → 語幹前方一致 → 同義語）
// =========================================================================

struct ResolvedLicense {
    name: String,
    matched_by: &'static str,
}

struct ResolvedOccupation {
    jobtag_id: i64,
    matched_by: &'static str,
}

/// 資格名を辞書上の実名称へ解決する。
///
/// 呼び出し側は「中型免許」のような通称を渡してくるが、JILPT 資格マスタの実名称は
/// 「中型自動車免許」等になっている可能性がある（本コミット時点では Turso 認証情報が
/// 手元に無く、実データでの名称確認は**未実施**）。そのため完全一致で外れた場合は
/// 前方一致 → 部分一致 → 接尾辞（免許/資格/講習…）を落とした語幹の前方一致 → 逆包含
/// の順で 1 件に絞る。候補が複数ある場合は「名称が短い＝より一般的」を優先し、
/// 同長なら関連職業数の多い方を採る。
fn resolve_license_name(turso: &TursoDb, q: &str) -> Result<Option<ResolvedLicense>, String> {
    let base = "SELECT name, COUNT(DISTINCT jobtag_id) AS n \
                FROM v2_external_jobtag_qualifications WHERE ";
    let tail = " GROUP BY name ORDER BY LENGTH(name) ASC, n DESC LIMIT 1";

    for (cond, pattern, matched_by) in license_attempts(q) {
        let sql = format!("{base}{cond}{tail}");
        let rows = turso.query(&sql, &[&pattern as &dyn ToSqlTurso])?;
        if let Some(r) = rows.first() {
            let name = s(r, "name");
            if !name.is_empty() {
                return Ok(Some(ResolvedLicense { name, matched_by }));
            }
        }
    }
    Ok(None)
}

/// 資格名解決の試行リスト (WHERE 条件, バインド値, matched_by)。
fn license_attempts(q: &str) -> Vec<(&'static str, String, &'static str)> {
    let esc = escape_like(q);
    let mut out: Vec<(&'static str, String, &'static str)> = vec![
        ("name = ?", q.to_string(), "exact"),
        ("name LIKE ? ESCAPE '\\'", format!("{esc}%"), "prefix"),
        ("name LIKE ? ESCAPE '\\'", format!("%{esc}%"), "contains"),
    ];

    // 「中型免許」→ 語幹「中型」→ 前方一致で「中型自動車免許」を拾う。
    if let Some(stem) = strip_license_suffix(q) {
        let stem_esc = escape_like(&stem);
        out.push((
            "name LIKE ? ESCAPE '\\'",
            format!("{stem_esc}%"),
            "stem_prefix",
        ));
        out.push((
            "name LIKE ? ESCAPE '\\'",
            format!("%{stem_esc}%"),
            "stem_contains",
        ));
    }

    // 逆包含: 「中型自動車免許(8t限定解除)」のような長い入力に対し、
    // 辞書側の短い名称が入力に含まれるケースを拾う。
    out.push((
        "? LIKE '%' || REPLACE(REPLACE(name,'%','\\%'),'_','\\_') || '%' ESCAPE '\\'",
        q.to_string(),
        "reverse_contains",
    ));
    out
}

/// 職種名を jobtag_id へ解決する。
///
/// 「トラックドライバー」のような呼称に対し、JILPT 側の名称は「トラック運転手」等の
/// 可能性があるため、名称・別名(aliases)・同義語置換（ドライバー⇔運転手）・語幹前方一致
/// の順で 1 件に絞る。実データでの名称確認は**未実施**（Turso 認証情報が手元に無い）。
fn resolve_occupation(turso: &TursoDb, q: &str) -> Result<Option<ResolvedOccupation>, String> {
    let base = "SELECT jobtag_id FROM v2_external_jobtag_occupation WHERE ";
    let tail = " ORDER BY LENGTH(name) ASC, jobtag_id ASC LIMIT 1";

    for (cond, pattern, matched_by) in occupation_attempts(q) {
        let sql = format!("{base}{cond}{tail}");
        let rows = turso.query(&sql, &[&pattern as &dyn ToSqlTurso])?;
        if let Some(r) = rows.first() {
            let jobtag_id = i(r, "jobtag_id");
            if jobtag_id != 0 {
                return Ok(Some(ResolvedOccupation {
                    jobtag_id,
                    matched_by,
                }));
            }
        }
    }
    Ok(None)
}

fn occupation_attempts(q: &str) -> Vec<(&'static str, String, &'static str)> {
    let esc = escape_like(q);
    let mut out: Vec<(&'static str, String, &'static str)> = vec![
        ("name = ?", q.to_string(), "exact"),
        (
            "COALESCE(aliases,'') LIKE ? ESCAPE '\\'",
            format!("%{esc}%"),
            "alias",
        ),
        ("name LIKE ? ESCAPE '\\'", format!("{esc}%"), "prefix"),
        ("name LIKE ? ESCAPE '\\'", format!("%{esc}%"), "contains"),
    ];

    // 同義語置換（ドライバー ⇔ 運転手 など）で作った別表記も候補にする。
    for syn in synonym_variants(q) {
        let syn_esc = escape_like(&syn);
        out.push(("name = ?", syn.clone(), "synonym_exact"));
        out.push((
            "name LIKE ? ESCAPE '\\'",
            format!("%{syn_esc}%"),
            "synonym_contains",
        ));
    }

    // 「トラックドライバー」→ 語幹「トラック」→ 前方一致で「トラック運転手」を拾う。
    if let Some(stem) = strip_occupation_suffix(q) {
        let stem_esc = escape_like(&stem);
        out.push((
            "name LIKE ? ESCAPE '\\'",
            format!("{stem_esc}%"),
            "stem_prefix",
        ));
    }
    out
}

/// 資格名の接尾辞を落として語幹を返す。落とせない／語幹が空なら None。
fn strip_license_suffix(q: &str) -> Option<String> {
    const SUFFIXES: [&str; 7] = [
        "運転免許証",
        "運転免許",
        "免許証",
        "免許",
        "技能講習",
        "講習",
        "資格",
    ];
    for suf in SUFFIXES {
        if let Some(stem) = q.strip_suffix(suf) {
            if !stem.is_empty() {
                return Some(stem.to_string());
            }
        }
    }
    None
}

/// 職種名の接尾辞を落として語幹を返す。
fn strip_occupation_suffix(q: &str) -> Option<String> {
    const SUFFIXES: [&str; 6] = ["ドライバー", "運転手", "運転士", "スタッフ", "作業員", "職"];
    for suf in SUFFIXES {
        if let Some(stem) = q.strip_suffix(suf) {
            if !stem.is_empty() {
                return Some(stem.to_string());
            }
        }
    }
    None
}

/// 職種名の同義語置換で得られる別表記を返す（元と同じものは除外）。
fn synonym_variants(q: &str) -> Vec<String> {
    const PAIRS: [(&str, &str); 3] = [
        ("ドライバー", "運転手"),
        ("運転手", "ドライバー"),
        ("配送員", "配達員"),
    ];
    let mut out = Vec::new();
    for (from, to) in PAIRS {
        if q.contains(from) {
            let v = q.replace(from, to);
            if v != q && !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}

/// LIKE パターンに埋め込むためワイルドカードをエスケープする（ESCAPE '\' 前提）。
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// =========================================================================
// 小物
// =========================================================================

fn validate_name(raw: Option<&str>) -> Result<String, Value> {
    let name = raw.unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err(not_found("empty_name", ""));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(not_found("name_too_long", ""));
    }
    Ok(name)
}

fn not_found(reason: &str, query_name: &str) -> Value {
    json!({ "found": false, "reason": reason, "query": query_name })
}

/// `?tab=` 復元用 URL（templates/dashboard_inline.html が URLSearchParams で復号する）。
fn tab_url(partial_path: &str) -> String {
    format!("/?tab={}", urlencoding::encode(partial_path))
}

fn category_label(key: &str) -> String {
    // handlers::driver::data::fetch_category_counts と同じ対応表（表示ラベル用）。
    let m: HashMap<&str, &str> = [
        ("driver", "ドライバー"),
        ("logistics", "物流・運輸"),
        ("manufacturing", "製造・加工"),
        ("construction", "建築・土木"),
        ("cleaning", "清掃・廃棄物"),
        ("labor", "倉庫・作業員"),
        ("office", "事務"),
        ("sales", "販売・営業"),
        ("service", "サービス"),
        ("professional", "専門・技術"),
        ("legal_culture", "法務・文化芸術"),
        ("education_childcare", "保育・教育"),
        ("security", "警備・保安"),
        ("agriculture", "農林漁業"),
        ("management", "管理職"),
    ]
    .into_iter()
    .collect();
    m.get(key).copied().unwrap_or(key).to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let cleaned = s.replace(['\r', '\n'], " ");
    if cleaned.chars().count() <= max {
        return cleaned;
    }
    let head: String = cleaned.chars().take(max).collect();
    format!("{head}…")
}

fn round1(v: Option<f64>) -> Option<f64> {
    v.map(|x| (x * 10.0).round() / 10.0)
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

fn s(row: &HashMap<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn i(row: &HashMap<String, Value>, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn f(row: &HashMap<String, Value>, key: &str) -> Option<f64> {
    row.get(key).and_then(|v| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    })
}

// =========================================================================
// テスト（DB 非依存部分のみ。曖昧一致の試行順と LIKE エスケープを固定する）
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_attempts_starts_with_exact_then_prefix() {
        let a = license_attempts("中型免許");
        assert_eq!(a[0].2, "exact");
        assert_eq!(a[0].1, "中型免許");
        assert_eq!(a[1].2, "prefix");
        assert_eq!(a[1].1, "中型免許%");
        assert_eq!(a[2].2, "contains");
        assert_eq!(a[2].1, "%中型免許%");
    }

    #[test]
    fn license_attempts_includes_stem_for_common_driving_licenses() {
        // 「中型免許」→ 語幹「中型」の前方一致で「中型自動車免許」を拾えること
        for q in ["中型免許", "準中型免許", "大型免許", "普通免許"] {
            let a = license_attempts(q);
            let stem = q.strip_suffix("免許").unwrap();
            assert!(
                a.iter()
                    .any(|(_, p, by)| *by == "stem_prefix" && p == &format!("{stem}%")),
                "{q} に stem_prefix 候補が無い"
            );
        }
    }

    #[test]
    fn license_stem_not_generated_when_suffix_is_whole_name() {
        // 「免許」だけ渡された場合に空語幹の LIKE '%' を作らないこと
        assert_eq!(strip_license_suffix("免許"), None);
        assert_eq!(strip_license_suffix("資格"), None);
        assert_eq!(strip_license_suffix("フォークリフト"), None);
    }

    #[test]
    fn occupation_attempts_include_synonym_and_stem() {
        let a = occupation_attempts("トラックドライバー");
        assert!(a
            .iter()
            .any(|(_, p, by)| *by == "synonym_exact" && p == "トラック運転手"));
        assert!(a
            .iter()
            .any(|(_, p, by)| *by == "stem_prefix" && p == "トラック%"));
        // 別名列の部分一致が名称の前方一致より先に来る（呼称は aliases に入りやすい）
        let alias_idx = a.iter().position(|(_, _, by)| *by == "alias").unwrap();
        let prefix_idx = a.iter().position(|(_, _, by)| *by == "prefix").unwrap();
        assert!(alias_idx < prefix_idx);
    }

    #[test]
    fn escape_like_neutralizes_wildcards() {
        assert_eq!(escape_like("100%_免許"), "100\\%\\_免許");
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        // ワイルドカード入力が全件一致にならないこと
        let a = license_attempts("%");
        assert_eq!(a[1].1, "\\%%");
    }

    #[test]
    fn tab_url_double_encodes_for_urlsearchparams() {
        let u = tab_url("/tab/driver/123");
        assert_eq!(u, "/?tab=%2Ftab%2Fdriver%2F123");
        // 資格名は 2 重エンコード（?tab= 復号後に /tab/license/%E3%.. が残る）
        let inner = format!("/tab/license/{}", urlencoding::encode("中型自動車免許"));
        assert!(tab_url(&inner).starts_with("/?tab=%2Ftab%2Flicense%2F%25E4"));
    }

    #[test]
    fn truncate_chars_counts_characters_not_bytes() {
        let s: String = "あ".repeat(130);
        let t = truncate_chars(&s, 120);
        assert_eq!(t.chars().count(), 121); // 120 文字 + 省略記号
        assert!(t.ends_with('…'));
        assert_eq!(truncate_chars("短い説明", 120), "短い説明");
        assert_eq!(truncate_chars("改行\nあり", 120), "改行 あり");
    }

    #[test]
    fn validate_name_rejects_empty_and_overlong() {
        assert!(validate_name(None).is_err());
        assert!(validate_name(Some("   ")).is_err());
        assert_eq!(validate_name(Some(" 中型免許 ")).unwrap(), "中型免許");
        let long: String = "あ".repeat(MAX_NAME_CHARS + 1);
        let err = validate_name(Some(&long)).unwrap_err();
        assert_eq!(err["reason"], "name_too_long");
        assert_eq!(err["found"], false);
    }

    #[test]
    fn not_found_payload_shape() {
        let v = not_found("not_configured", "中型免許");
        assert_eq!(v["found"], false);
        assert_eq!(v["reason"], "not_configured");
        assert_eq!(v["query"], "中型免許");
    }

    #[test]
    fn median_and_round1() {
        assert_eq!(median(vec![]), None);
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(round1(Some(421.34)), Some(421.3));
        assert_eq!(round1(None), None);
    }
}
