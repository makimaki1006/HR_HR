//! 競合調査の「キーワードの推移・分析」が使う、スナップショットの生の値。
//! 読むだけで外部 API は呼ばない。表ごとに独立して読み、読めなかった表は項目ごと省く
//! (組み立て側が「データがありません」と 1 文で出す)。
use serde_json::{json, Value};

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_f64_opt, get_i64_opt, get_str};

/// 職種 `title` (と、選んだ都道府県 `pref`。空は全国) の値を JSON にまとめる。
pub fn raw_context(db: &LocalDb, title: &str, pref: &str) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("prefecture".into(), json!(pref));

    if let Ok(rows) = db.query(
        "SELECT variant, months, series, avg_monthly, latest, latest_month, yoy_pct, \
                peak_month, competition \
         FROM insight_search_trend WHERE norm_title = ?1 ORDER BY variant",
        &[&title],
    ) {
        let search: Vec<Value> = rows
            .iter()
            .map(|r| {
                let months: Vec<String> = get_str(r, "months")
                    .split(',')
                    .map(|m| m.trim().to_owned())
                    .collect();
                json!({
                    "variant": get_str(r, "variant"),
                    "months": months,
                    "series": crate::indeed::season::parse_series(&get_str(r, "series")),
                    "avg_monthly": get_i64_opt(r, "avg_monthly"),
                    "latest": get_i64_opt(r, "latest"),
                    "latest_month": get_str(r, "latest_month"),
                    "yoy_pct": get_f64_opt(r, "yoy_pct"),
                    "peak_month": get_i64_opt(r, "peak_month"),
                    "competition": get_str(r, "competition"),
                })
            })
            .collect();
        out.insert("search".into(), Value::Array(search));
    }

    if let Ok(shifts) = crate::indeed::keywords::term_shifts(db, title) {
        let v: Vec<Value> = shifts
            .iter()
            .map(|s| json!({"term": s.term, "before": s.before_pct, "after": s.after_pct, "diff": s.diff_pt}))
            .collect();
        out.insert("shifts".into(), Value::Array(v));
    }
    if let Ok((months, terms)) = crate::indeed::keywords::term_monthly(db, title) {
        let v: Vec<Value> = terms
            .iter()
            .map(|t| json!({"term": t.term, "pct": t.pct}))
            .collect();
        out.insert("term_monthly".into(), json!({"months": months, "terms": v}));
    }
    if let Ok(attr) = crate::indeed::keywords::attr_months(db, title) {
        out.insert(
            "term_count_latest".into(),
            json!(attr.last().and_then(|a| a.term_count)),
        );
    }

    if !pref.is_empty() {
        if let Ok(rows) = db.query(
            "SELECT report_month, kw_available, top_keywords FROM insight_title_pref \
             WHERE norm_title = ?1 AND prefecture = ?2 ORDER BY report_month",
            &[&title, &pref],
        ) {
            let v: Vec<Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "month": get_str(r, "report_month"),
                        "kw_available": get_i64_opt(r, "kw_available").unwrap_or(0),
                        "top_keywords": get_str(r, "top_keywords"),
                    })
                })
                .collect();
            out.insert("pref_months".into(), Value::Array(v));
        }
    }
    Value::Object(out)
}
