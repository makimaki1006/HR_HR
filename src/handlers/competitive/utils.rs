use serde_json::Value;

/// HTMLエスケープ（XSS対策: シングルクォート含む）
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// 文字列を指定文字数で切り詰め
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars - 1).collect();
        format!("{}\u{2026}", truncated)
    }
}

/// <option>タグを生成（api.rsから参照）
pub fn build_option(value: &str, label: &str) -> String {
    format!(r#"<option value="{}">{}</option>"#, escape_html(value), escape_html(label))
}

/// serde_json::Valueから数値を取得（REAL/INTEGER両対応）
pub(crate) fn value_to_i64(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| v.as_f64().map(|f| f as i64).unwrap_or(0))
}

/// Haversine公式で2点間の距離を計算（km）
pub(crate) fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}
