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

/// URL/href/src属性用のエスケープ。
/// javascript:, data:, vbscript: などの危険スキームを拒否して
/// "#" に置換する。安全なURL(http/https/相対/アンカー)はHTMLエスケープのみ適用。
pub fn escape_url_attr(url: &str) -> String {
    let lower = url.trim().to_lowercase();
    const DANGEROUS_SCHEMES: &[&str] = &[
        "javascript:", "data:", "vbscript:", "file:",
    ];
    for scheme in DANGEROUS_SCHEMES {
        if lower.starts_with(scheme) {
            return "#".to_string();
        }
    }
    // さらに &, <, >, " をエスケープ
    escape_html(url)
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

/// クロスナビリンク: 他タブへの誘導リンクを生成
pub fn cross_nav(tab_url: &str, label: &str) -> String {
    format!(
        r#"<a class="inline-flex items-center gap-1 text-xs text-blue-400/80 hover:text-blue-300 cursor-pointer transition-colors" onclick="navigateToTab('{tab_url}')"><svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7l5 5m0 0l-5 5m5-5H6"/></svg>{label}</a>"#
    )
}

/// Haversine距離計算（km単位）
pub fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6371.0; // 地球半径(km)
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// テーブル存在確認（パラメータバインド使用、SQLインジェクション対策済み）
pub fn table_exists(db: &crate::db::local_sqlite::LocalDb, name: &str) -> bool {
    db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        &[&name],
    ).unwrap_or(0) > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_url_attr_javascript() {
        assert_eq!(escape_url_attr("javascript:alert(1)"), "#");
        assert_eq!(escape_url_attr("JAVASCRIPT:alert(1)"), "#");  // 大文字小文字無視
        assert_eq!(escape_url_attr("  javascript:alert(1)"), "#");  // 前後空白
    }

    #[test]
    fn test_escape_url_attr_data() {
        assert_eq!(escape_url_attr("data:text/html,<script>alert(1)</script>"), "#");
    }

    #[test]
    fn test_escape_url_attr_safe() {
        assert_eq!(escape_url_attr("https://example.com/"), "https://example.com/");
        assert_eq!(escape_url_attr("/relative/path"), "/relative/path");
        assert_eq!(escape_url_attr("#anchor"), "#anchor");
    }

    #[test]
    fn test_escape_url_attr_html_special() {
        assert_eq!(escape_url_attr("https://example.com/?a=1&b=2"), "https://example.com/?a=1&amp;b=2");
    }

    #[test]
    fn test_escape_url_attr_vbscript_file() {
        assert_eq!(escape_url_attr("vbscript:msgbox(1)"), "#");
        assert_eq!(escape_url_attr("file:///etc/passwd"), "#");
    }
}
