//! 共通ヘルパー関数
//! get_f64/get_i64/get_str等をハンドラー間で統一

use serde_json::Value;
use std::collections::HashMap;

pub type Row = HashMap<String, Value>;

/// HashMap からString値を取得
pub fn get_str(row: &Row, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// HashMap から参照&str取得（コピー不要な場合用）
pub fn get_str_ref<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// HTMLエスケープ済み文字列取得（DB値をHTML埋め込み時に使用）
pub fn get_str_html(row: &Row, key: &str) -> String {
    escape_html(row.get(key).and_then(|v| v.as_str()).unwrap_or(""))
}

/// HashMap からi64値を取得（f64/文字列からの自動変換対応）
pub fn get_i64(row: &Row, key: &str) -> i64 {
    row.get(key)
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_f64().map(|f| f as i64))
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0)
}

/// HashMap からf64値を取得（i64/文字列からの自動変換対応）
pub fn get_f64(row: &Row, key: &str) -> f64 {
    row.get(key)
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|i| i as f64))
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0.0)
}

/// HTMLエスケープ（XSS対策）
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// 数値を3桁区切りフォーマット
pub fn format_number(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 && ch != '-' {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// 文字列を指定文字数で切り詰め
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", chars[..max_chars].iter().collect::<String>())
    }
}

/// パーセント表示（小数1桁）
pub fn pct(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

/// CSSバー（パーセント値のプログレスバー）
pub fn pct_bar(v: f64, color: &str) -> String {
    let w = (v * 100.0).min(100.0).max(0.0);
    format!(
        r#"<div class="w-full bg-slate-700 rounded h-1.5"><div class="rounded h-1.5" style="width:{w:.1}%;background:{color}"></div></div>"#
    )
}

/// テーブル存在確認（パラメータバインド使用、SQLインジェクション対策済み）
pub fn table_exists(db: &crate::db::local_sqlite::LocalDb, name: &str) -> bool {
    db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        &[&name],
    ).unwrap_or(0) > 0
}
