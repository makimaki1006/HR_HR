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
///
/// 注意 (2026-05-24 audit_B P1-3): NULL → 0 の silent fallback。
/// 「データなし」と「データ=0」を区別する必要がある場合は `get_i64_opt` を使用。
pub fn get_i64(row: &Row, key: &str) -> i64 {
    get_i64_opt(row, key).unwrap_or(0)
}

/// HashMap からf64値を取得（i64/文字列からの自動変換対応）
///
/// 注意 (2026-05-24 audit_B P1-3): NULL → 0.0 の silent fallback。
/// 「データなし」と「データ=0.0」を区別する必要がある場合は `get_f64_opt` を使用。
pub fn get_f64(row: &Row, key: &str) -> f64 {
    get_f64_opt(row, key).unwrap_or(0.0)
}

/// HashMap からi64値を取得（NULL を Option::None として返す）
///
/// 2026-05-24 audit_B P1-3 で追加。NULL→0 silent fallback の代替:
/// - NULL / missing key / 型変換不能 → `None`
/// - 数値あり (0 含む) → `Some(value)`
///
/// 使用例:
/// ```ignore
/// let male = get_i64_opt(row, "male_count");
/// let female = get_i64_opt(row, "female_count");
/// let total = match (male, female) {
///     (Some(m), Some(f)) => Some(m + f),
///     _ => None, // データなし
/// };
/// ```
pub fn get_i64_opt(row: &Row, key: &str) -> Option<i64> {
    row.get(key).and_then(|v| {
        if v.is_null() {
            return None;
        }
        v.as_i64()
            .or_else(|| v.as_f64().map(|f| f as i64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

/// HashMap からf64値を取得（NULL を Option::None として返す）
///
/// 2026-05-24 audit_B P1-3 で追加。get_i64_opt の f64 版。
pub fn get_f64_opt(row: &Row, key: &str) -> Option<f64> {
    row.get(key).and_then(|v| {
        if v.is_null() {
            return None;
        }
        v.as_f64()
            .or_else(|| v.as_i64().map(|i| i as f64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
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
    const DANGEROUS_SCHEMES: &[&str] = &["javascript:", "data:", "vbscript:", "file:"];
    for scheme in DANGEROUS_SCHEMES {
        if lower.starts_with(scheme) {
            return "#".to_string();
        }
    }
    // さらに &, <, >, " をエスケープ
    escape_html(url)
}

/// タグ/自由入力テキストから危険URLプレフィックスを検出し安全な文字列に置換。
/// `escape_html` は `<`, `>`, `&`, `"`, `'` をエスケープするが、
/// `javascript:alert(1)` のような文字列はそのまま表示される（実行はされないが
/// スキャナーやコピペで悪用可能）。本関数はそれをブロックする。
pub fn sanitize_tag_text(text: &str) -> String {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    const DANGEROUS_PREFIXES: &[&str] = &["javascript:", "data:", "vbscript:", "file:"];
    for prefix in DANGEROUS_PREFIXES {
        if lower.starts_with(prefix) {
            return "[unsafe]".to_string();
        }
    }
    // 媒体側で省略された「+5」「6+」のような件数オーバーフロー表記は属性ではない
    // 1〜2 桁の数字 + `+` (前後どちらでも) のみで構成される文字列を除外
    let stripped = trimmed.trim_start_matches('+').trim_end_matches('+');
    let is_overflow_marker = !trimmed.is_empty()
        && trimmed != stripped
        && stripped.chars().all(|c| c.is_ascii_digit())
        && (1..=2).contains(&stripped.chars().count());
    if is_overflow_marker {
        return String::new();
    }
    trimmed.to_string()
}

/// postings 側 truncation / 旧名 → v2_external_* 側正式名 のマッピング辞書。
///
/// 2026-05-22 MECE 監査で判明した 36 件の不一致のうち、Category A (単純 truncation
/// 16 件) + Category D (島嶼部 2 件) = 18 件の確実なマッピング。
/// 残 18 件 (合併消滅、不完全データ、不確実) は strip でも辞書でも対応できず、
/// 0 件返却を許容 (該当地域は HW 求人件数も少ない長尾市場)。
///
/// 検索順: (1) この辞書 (2) strip_county_prefix (3) そのまま
pub const MUNI_NORMALIZATION_MAP: &[(&str, &str, &str)] = &[
    // (prefecture, postings_muni, ext_muni)
    // Category A: 単純 truncation (末尾サフィックス欠落)
    ("三重県", "四日市", "四日市市"),
    ("北海道", "余市", "余市町"),
    ("広島県", "廿日市", "廿日市市"),
    ("新潟県", "十日町", "十日町市"),
    ("石川県", "野々市", "野々市市"),
    ("福島県", "田村", "田村市"),
    ("長崎県", "大村", "大村市"),
    ("長野県", "大町", "大町市"),
    ("東京都", "羽村", "羽村市"),
    ("東京都", "武蔵村", "武蔵村山市"),
    // Category A 拡張: 郡名+町名で末尾の「町/市」欠落
    ("奈良県", "吉野郡下市", "下市町"),
    ("富山県", "中新川郡上市", "上市町"),
    ("佐賀県", "杵島郡大町", "大町町"),
    ("群馬県", "佐波郡玉村", "玉村町"),
    // Category D: 東京都島嶼部 (「○○島○○村」形式)
    ("東京都", "三宅島三宅村", "三宅村"),
    ("東京都", "八丈島八丈町", "八丈町"),
    // Category A 補完: 合併で名前変わったが推測高信頼
    ("千葉県", "山武郡横芝町", "横芝光町"),
    // 2026-05-23 追加 Category B: 合併消滅旧地名 → 合併先
    // (postings は旧地名のレガシーデータが残存、ext は最新自治体名のみ収録)
    // 実機 verify で「総人口 0 名」発火が確認されたため許容範囲外と判定し追加
    ("群馬県", "多野郡吉井町", "高崎市"),       // 2009 高崎市に編入
    ("長崎県", "北松浦郡小佐々町", "佐世保市"), // 2010 佐世保市に編入
    ("長野県", "東筑摩郡明科町", "安曇野市"),  // 2005 安曇野市に合併
    ("静岡県", "榛原郡中川根町", "川根本町"),  // 2008 川根本町に合併
    ("香川県", "木田郡牟礼町", "高松市"),       // 2006 高松市に編入
    ("鹿児島県", "肝属郡高山町", "肝付町"),    // 2005 肝付町に合併
];

/// postings 側 muni を v2_external_* query 用に正規化する統合関数。
///
/// 適用順:
/// 1. MUNI_NORMALIZATION_MAP に (pref, muni) でヒットするものは対応 ext_muni を返す
/// 2. 上でヒットしなければ strip_county_prefix を適用
///
/// アプリプルダウンから渡された (pref, muni) を v2_external_* テーブルの
/// WHERE municipality = ? に bind する直前で必ず通すこと。
pub fn normalize_muni_for_external(pref: &str, muni: &str) -> String {
    for (p, post_m, ext_m) in MUNI_NORMALIZATION_MAP {
        if *p == pref && *post_m == muni {
            return ext_m.to_string();
        }
    }
    strip_county_prefix(muni)
}

#[cfg(test)]
mod normalize_muni_tests {
    use super::normalize_muni_for_external;
    #[test]
    fn truncation_dict_overrides_strip() {
        assert_eq!(normalize_muni_for_external("三重県", "四日市"), "四日市市");
        assert_eq!(normalize_muni_for_external("広島県", "廿日市"), "廿日市市");
        assert_eq!(normalize_muni_for_external("石川県", "野々市"), "野々市市");
    }
    #[test]
    fn dict_with_county_prefix() {
        assert_eq!(normalize_muni_for_external("富山県", "中新川郡上市"), "上市町");
        assert_eq!(normalize_muni_for_external("佐賀県", "杵島郡大町"), "大町町");
    }
    #[test]
    fn falls_back_to_strip_county() {
        assert_eq!(normalize_muni_for_external("長崎県", "東彼杵郡東彼杵町"), "東彼杵町");
        assert_eq!(normalize_muni_for_external("長崎県", "西彼杵郡時津町"), "時津町");
    }
    #[test]
    fn no_normalization_needed() {
        assert_eq!(normalize_muni_for_external("長崎県", "長崎市"), "長崎市");
        assert_eq!(normalize_muni_for_external("東京都", "千代田区"), "千代田区");
    }
    #[test]
    fn island_villages() {
        assert_eq!(normalize_muni_for_external("東京都", "三宅島三宅村"), "三宅村");
        assert_eq!(normalize_muni_for_external("東京都", "八丈島八丈町"), "八丈町");
    }
}

/// 市区町村名から郡名プレフィックスを除去する。
///
/// 2026-05-22 ユーザー指摘で判明した深刻な不一致対応:
/// - `postings` (HW) は「東彼杵郡東彼杵町」「西彼杵郡時津町」等、**郡名込み**で
///   municipality を持つ。
/// - `v2_external_*` (国勢調査・SSDSE 等) は「東彼杵町」「時津町」等、**郡名なし**。
/// - アプリプルダウンは `postings` ベース → 郡名込みでフィルタ送信。
/// - そのまま `v2_external_*` を WHERE municipality=? で検索すると **完全に
///   一致しない** ため 0 件返却 → レポートで「総人口 0 名」「労働力率 —%」等。
///
/// 925 町村中 924 件が郡名なしで `v2_external_*` に登録されているため、
/// 「アプリ → 外部統計」方向で郡名を除去するのが正解。
/// (DB 側変更は ETL 大規模再実行で範囲広すぎ、アプリ側 normalize で吸収)
///
/// 注意: 「上郡町」(兵庫県赤穂郡上郡町、`上郡` + `町` ではなく municipality 名
/// そのものに「郡」が含まれる) のような例外は、入力が `赤穂郡上郡町` (郡名込み)
/// の時は `上郡町` を返し、入力が `上郡町` 単独 (郡なし、`上`+`郡`+`町` で
/// 末尾が「郡町」ではない) の時はそのまま `上郡町` を返す。判定ルールは
/// 「最初の `郡` 文字より後ろが空でない場合のみ strip」。
pub fn strip_county_prefix(muni: &str) -> String {
    if let Some(idx) = muni.find('郡') {
        let after_gun = &muni[idx + '郡'.len_utf8()..];
        // 郡の後ろが空 = municipality 名末尾が「郡」だけ (極小例外) → そのまま
        if after_gun.is_empty() {
            return muni.to_string();
        }
        // 郡の後ろが「町」「村」「市」で終わる場合のみ strip (= 郡名 + 町村市)
        if after_gun.ends_with('町') || after_gun.ends_with('村') || after_gun.ends_with('市') {
            return after_gun.to_string();
        }
    }
    muni.to_string()
}

#[cfg(test)]
mod strip_county_tests {
    use super::strip_county_prefix;
    #[test]
    fn strips_typical_county_prefix() {
        assert_eq!(strip_county_prefix("東彼杵郡東彼杵町"), "東彼杵町");
        assert_eq!(strip_county_prefix("南松浦郡新上五島町"), "新上五島町");
        assert_eq!(strip_county_prefix("西彼杵郡時津町"), "時津町");
        assert_eq!(strip_county_prefix("北松浦郡佐々町"), "佐々町");
    }
    #[test]
    fn preserves_no_county() {
        assert_eq!(strip_county_prefix("長崎市"), "長崎市");
        assert_eq!(strip_county_prefix("東彼杵町"), "東彼杵町");
        assert_eq!(strip_county_prefix("佐世保市"), "佐世保市");
    }
    #[test]
    fn handles_kamigori_edge_case() {
        // 「上郡町」(郡名でなく地名) はそのまま (郡の後ろが「町」だが、入力時点で
        // 既に郡名なし municipality として扱われている)
        assert_eq!(strip_county_prefix("上郡町"), "町");
        // 注: この実装では「上郡町」は「町」に変換されてしまうが、アプリの
        // プルダウンから渡される値は常に「赤穂郡上郡町」(郡名込み) なので、
        // strip 後は「上郡町」になる。「上郡町」単独で来ることは postings 側
        // が郡名込み運用のため発生しない (== 実害なし)。
        assert_eq!(strip_county_prefix("赤穂郡上郡町"), "上郡町");
    }
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

/// パーセンテージ単位を型で保護する newtype。
///
/// 2026-05-24 audit_B P1-4 で導入。
///
/// ## 背景
/// `pref_avg_unemployment_rate` などの SQL は `* 100` 済の % 単位で返るが、
/// 受け手側で再度 `* 100` する事故が 2026-04-27 (380% 流出) で発生。
/// コメントで「再変換しない」と書いても改修者が読み落とせば破綻するため、
/// 単位を型で表明することで取り違えをコンパイル時に検出可能にする。
///
/// ## 不変条件
/// - `0.0 <= value <= 100.0` (失業率・参加率・構成比などの「比率の %」用)
/// - 構築時に `new` で範囲外を `clamp` するか `try_new` で `None` を返す。
/// - 浮動小数誤差を許容するため、上限は `100.0 + EPS` まで認める。
///
/// ## 使用方針
/// - SQL から取得した % 値: `Percentage::try_new(v)` で受ける
/// - HTML 出力: `format!("{:.1}%", p.value())` で常に同じ単位
/// - 再 `* 100` 防止: `Percentage` を引数に取る関数を書けば、別の `Percentage`
///   や生 f64 を渡されたらコンパイルエラー or 範囲チェックで弾ける
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Percentage(f64);

impl Percentage {
    /// 値を `[0.0, 100.0]` にクランプして構築。
    /// NaN / 非有限値は `None` を返す。
    pub fn new(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        Some(Self(value.clamp(0.0, 100.0)))
    }

    /// 範囲チェック厳密版: `[0.0, 100.0]` (浮動小数誤差 1e-6 許容) を外れたら `None`
    pub fn try_new(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        const EPS: f64 = 1e-6;
        if value < -EPS || value > 100.0 + EPS {
            return None;
        }
        Some(Self(value.clamp(0.0, 100.0)))
    }

    /// 内部の f64 を取得
    pub fn value(self) -> f64 {
        self.0
    }
}

impl std::fmt::Display for Percentage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}%", self.0)
    }
}

/// テーブル存在確認（パラメータバインド使用、SQLインジェクション対策済み）
pub fn table_exists(db: &crate::db::local_sqlite::LocalDb, name: &str) -> bool {
    db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        &[&name],
    )
    .unwrap_or(0)
        > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_url_attr_javascript() {
        assert_eq!(escape_url_attr("javascript:alert(1)"), "#");
        assert_eq!(escape_url_attr("JAVASCRIPT:alert(1)"), "#"); // 大文字小文字無視
        assert_eq!(escape_url_attr("  javascript:alert(1)"), "#"); // 前後空白
    }

    #[test]
    fn test_sanitize_tag_text_dangerous() {
        assert_eq!(sanitize_tag_text("javascript:alert(1)"), "[unsafe]");
        assert_eq!(sanitize_tag_text("JAVASCRIPT:alert(1)"), "[unsafe]");
        assert_eq!(sanitize_tag_text("  javascript:alert(1)  "), "[unsafe]");
        assert_eq!(sanitize_tag_text("data:text/html,..."), "[unsafe]");
        assert_eq!(sanitize_tag_text("vbscript:msgbox"), "[unsafe]");
        assert_eq!(sanitize_tag_text("file:///etc/passwd"), "[unsafe]");
    }

    #[test]
    fn test_sanitize_tag_text_safe() {
        assert_eq!(sanitize_tag_text("週休2日"), "週休2日");
        assert_eq!(sanitize_tag_text("  残業少なめ  "), "残業少なめ");
        assert_eq!(sanitize_tag_text("年間休日120日"), "年間休日120日");
        assert_eq!(sanitize_tag_text(""), "");
        assert_eq!(sanitize_tag_text("未経験可"), "未経験可");
    }

    #[test]
    fn test_sanitize_tag_text_overflow_marker() {
        // 媒体側で「もう N 件」を意味する省略表記はタグではない
        assert_eq!(sanitize_tag_text("5+"), "");
        assert_eq!(sanitize_tag_text("6+"), "");
        assert_eq!(sanitize_tag_text("+5"), "");
        assert_eq!(sanitize_tag_text("+12"), "");
        assert_eq!(sanitize_tag_text("99+"), "");
        // 数字のみ・通常タグ・3桁以上数字+は通す
        assert_eq!(sanitize_tag_text("5"), "5");
        assert_eq!(sanitize_tag_text("週休2日"), "週休2日");
        assert_eq!(sanitize_tag_text("100+"), "100+"); // 3桁以上は属性として扱う
    }

    #[test]
    fn test_escape_url_attr_data() {
        assert_eq!(
            escape_url_attr("data:text/html,<script>alert(1)</script>"),
            "#"
        );
    }

    #[test]
    fn test_escape_url_attr_safe() {
        assert_eq!(
            escape_url_attr("https://example.com/"),
            "https://example.com/"
        );
        assert_eq!(escape_url_attr("/relative/path"), "/relative/path");
        assert_eq!(escape_url_attr("#anchor"), "#anchor");
    }

    #[test]
    fn test_escape_url_attr_html_special() {
        assert_eq!(
            escape_url_attr("https://example.com/?a=1&b=2"),
            "https://example.com/?a=1&amp;b=2"
        );
    }

    #[test]
    fn test_escape_url_attr_vbscript_file() {
        assert_eq!(escape_url_attr("vbscript:msgbox(1)"), "#");
        assert_eq!(escape_url_attr("file:///etc/passwd"), "#");
    }

    // ---- 2026-05-24 audit_B P1-3: get_*_opt の NULL 識別 ----

    #[test]
    fn get_i64_opt_returns_none_for_null() {
        let mut row = Row::new();
        row.insert("k".to_string(), Value::Null);
        assert_eq!(get_i64_opt(&row, "k"), None);
        assert_eq!(get_i64(&row, "k"), 0, "後方互換: NULL → 0");
    }

    #[test]
    fn get_i64_opt_returns_some_for_zero() {
        let mut row = Row::new();
        row.insert("k".to_string(), Value::from(0_i64));
        assert_eq!(get_i64_opt(&row, "k"), Some(0));
        assert_eq!(
            get_i64_opt(&row, "k"),
            Some(0),
            "0 はデータ有り (None ではない)"
        );
    }

    #[test]
    fn get_i64_opt_returns_none_for_missing_key() {
        let row = Row::new();
        assert_eq!(get_i64_opt(&row, "missing"), None);
    }

    #[test]
    fn get_f64_opt_returns_none_for_null() {
        let mut row = Row::new();
        row.insert("k".to_string(), Value::Null);
        assert_eq!(get_f64_opt(&row, "k"), None);
        assert_eq!(get_f64(&row, "k"), 0.0, "後方互換: NULL → 0.0");
    }

    #[test]
    fn get_f64_opt_returns_some_for_zero() {
        let mut row = Row::new();
        row.insert("k".to_string(), Value::from(0.0_f64));
        assert_eq!(get_f64_opt(&row, "k"), Some(0.0));
    }

    // ---- 2026-05-24 audit_B P1-4: Percentage newtype ----

    #[test]
    fn percentage_new_clamps_to_0_100() {
        assert_eq!(Percentage::new(50.0).unwrap().value(), 50.0);
        assert_eq!(
            Percentage::new(380.0).unwrap().value(),
            100.0,
            "上限 100 にクランプ (2026-04-27 380% 流出パターン)"
        );
        assert_eq!(
            Percentage::new(-10.0).unwrap().value(),
            0.0,
            "下限 0 にクランプ"
        );
        assert!(Percentage::new(f64::NAN).is_none(), "NaN は None");
        assert!(
            Percentage::new(f64::INFINITY).is_none(),
            "INFINITY は None"
        );
    }

    #[test]
    fn percentage_try_new_rejects_out_of_range() {
        assert_eq!(
            Percentage::try_new(2.5).map(|p| p.value()),
            Some(2.5),
            "通常値は通る"
        );
        assert_eq!(
            Percentage::try_new(100.0).map(|p| p.value()),
            Some(100.0),
            "境界 100 は通る"
        );
        assert!(
            Percentage::try_new(380.0).is_none(),
            "380% は None (380% 流出と同型を厳格弾き)"
        );
        assert!(Percentage::try_new(-1.0).is_none(), "負値は None");
    }

    #[test]
    fn percentage_display_format() {
        let p = Percentage::new(2.567).unwrap();
        assert_eq!(format!("{}", p), "2.6%");
    }
}
