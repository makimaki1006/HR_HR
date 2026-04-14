//! 給与パーサー（GAS SalaryParser.js移植）
//! 日本語の給与表記を解析し、統一月給に変換

use serde::Serialize;

// ======== 型定義 ========

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SalaryType {
    Hourly,  // 時給
    Daily,   // 日給
    Weekly,  // 週給
    Monthly, // 月給
    Annual,  // 年俸
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedSalary {
    pub original_text: String,
    pub salary_type: SalaryType,
    pub min_value: Option<i64>,
    pub max_value: Option<i64>,
    pub has_range: bool,
    pub unified_monthly: Option<i64>,
    pub unified_annual: Option<i64>,
    pub range_category: Option<String>,
    pub confidence: f64,
}

// ======== 変換レート ========

// GASのSALARY_CONVERSION_RATES相当
const HOURLY_TO_MONTHLY: f64 = 173.8; // 8h × 21.7日
const DAILY_TO_MONTHLY: f64 = 21.7; // 月間勤務日数
const WEEKLY_TO_MONTHLY: f64 = 4.33; // 月間週数

// ======== メインパース関数 ========

/// 給与テキストを解析して構造化データに変換
pub fn parse_salary(text: &str, default_type: SalaryType) -> ParsedSalary {
    if text.is_empty() {
        return empty_result();
    }

    let normalized = normalize_text(text);
    let salary_type = detect_salary_type(&normalized, &default_type);
    let (min_val, max_val, has_range) = extract_salary_values(&normalized);
    let (unified_monthly, unified_annual) = calculate_unified(min_val, max_val, &salary_type);
    let range_category = unified_monthly.map(get_salary_range_category);
    let confidence = calculate_confidence(&normalized, min_val, &salary_type);

    ParsedSalary {
        original_text: text.to_string(),
        salary_type,
        min_value: min_val,
        max_value: max_val,
        has_range,
        unified_monthly,
        unified_annual,
        range_category,
        confidence,
    }
}

fn empty_result() -> ParsedSalary {
    ParsedSalary {
        original_text: String::new(),
        salary_type: SalaryType::Monthly,
        min_value: None,
        max_value: None,
        has_range: false,
        unified_monthly: None,
        unified_annual: None,
        range_category: None,
        confidence: 0.0,
    }
}

// ======== テキスト正規化 ========

fn normalize_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            // 全角数字→半角
            '０'..='９' => result.push((ch as u32 - 0xFEE0) as u8 as char),
            // 全角カンマ→半角
            '，' => result.push(','),
            // 全角ピリオド→半角
            '．' => result.push('.'),
            // チルダ・ダッシュ類→統一
            '～' | '〜' | 'ー' | '―' | '－' => result.push('~'),
            _ => result.push(ch),
        }
    }
    // 複数スペースを1つに
    let mut prev_space = false;
    let mut collapsed = String::with_capacity(result.len());
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(ch);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

// ======== 給与タイプ判定 ========

fn detect_salary_type(text: &str, default: &SalaryType) -> SalaryType {
    if text.contains("時給") {
        return SalaryType::Hourly;
    }
    if text.contains("日給") {
        return SalaryType::Daily;
    }
    if text.contains("週給") {
        return SalaryType::Weekly;
    }
    if text.contains("月給")
        || text.contains("月収")
        || text.contains("基本給")
        || text.contains("固定給")
    {
        return SalaryType::Monthly;
    }
    if text.contains("年俸") || text.contains("年収") {
        return SalaryType::Annual;
    }
    default.clone()
}

// ======== 数値抽出 ========

fn extract_salary_values(text: &str) -> (Option<i64>, Option<i64>, bool) {
    // カンマ除去
    let clean = text.replace(',', "");

    // 範囲表記（~で分割）
    if clean.contains('~') {
        let parts: Vec<&str> = clean.splitn(2, '~').collect();
        if parts.len() == 2 {
            let left = extract_single_value(parts[0]);
            let right = extract_single_value(parts[1]);
            match (left, right) {
                (Some(l), Some(r)) => return (Some(l), Some(r), true),
                (Some(l), None) => return (Some(l), None, true),
                (None, Some(r)) => return (None, Some(r), true),
                _ => {}
            }
        }
    }

    // 単一値
    let val = extract_single_value(&clean);
    (val, None, false)
}

/// 単一の給与値を抽出
fn extract_single_value(text: &str) -> Option<i64> {
    // パターン1: XX.X万円 (例: 25.9万円 → 259,000)
    if let Some(v) = try_parse_decimal_man(text) {
        return Some(v);
    }
    // パターン2: XX万YYYY円 (例: 25万3000円 → 253,000)
    if let Some(v) = try_parse_man_format(text) {
        return Some(v);
    }
    // パターン3: X千円 (例: 5千円 → 5,000)
    if let Some(v) = try_parse_sen_format(text) {
        return Some(v);
    }
    // パターン4: 純粋な数値（4桁以上）
    if let Some(v) = try_parse_plain_number(text) {
        return Some(v);
    }
    None
}

/// XX.X万円パターン
fn try_parse_decimal_man(text: &str) -> Option<i64> {
    let man_pos = text.find('万')?;
    let before = &text[..man_pos];
    // 数字と小数点のみ抽出（末尾から遡る）
    let num_str: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if !num_str.contains('.') {
        return None;
    }
    let val: f64 = num_str.parse().ok()?;
    Some((val * 10_000.0) as i64)
}

/// XX万YYYY円パターン
fn try_parse_man_format(text: &str) -> Option<i64> {
    let man_pos = text.find('万')?;
    let before = &text[..man_pos];
    // 万の前の数字
    let man_str: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let man_val: i64 = man_str.parse().ok()?;

    // 万の後の数字（あれば）
    let after = &text[man_pos + '万'.len_utf8()..];
    let extra: i64 = if after.contains('千') {
        // X千パターン
        let sen_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        sen_str.parse::<i64>().unwrap_or(0) * 1000
    } else {
        let extra_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        extra_str.parse().unwrap_or(0)
    };

    Some(man_val * 10_000 + extra)
}

/// X千Y百Z十円パターン（例: 「1千5百円」→1500、「2千円」→2000）
fn try_parse_sen_format(text: &str) -> Option<i64> {
    let sen_pos = text.find('千')?;
    let before = &text[..sen_pos];
    // 千の前の数字
    let num_str: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let sen_val: i64 = num_str.parse().ok()?;
    let mut total = sen_val * 1_000;

    // 千の後の「百」パターン
    let after_sen = &text[sen_pos + '千'.len_utf8()..];
    if let Some(hyaku_pos) = after_sen.find('百') {
        let before_hyaku = &after_sen[..hyaku_pos];
        let hyaku_str: String = before_hyaku
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(h) = hyaku_str.parse::<i64>() {
            total += h * 100;
        }
        // 百の後の「十」パターン
        let after_hyaku = &after_sen[hyaku_pos + '百'.len_utf8()..];
        if let Some(juu_pos) = after_hyaku.find('十') {
            let before_juu = &after_hyaku[..juu_pos];
            let juu_str: String = before_juu
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(j) = juu_str.parse::<i64>() {
                total += j * 10;
            }
        }
    } else if let Some(juu_pos) = after_sen.find('十') {
        // 千の後に百なしで「十」がある場合
        let before_juu = &after_sen[..juu_pos];
        let juu_str: String = before_juu
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(j) = juu_str.parse::<i64>() {
            total += j * 10;
        }
    }

    Some(total)
}

/// 純粋な数値（4桁以上 = 円単位）
fn try_parse_plain_number(text: &str) -> Option<i64> {
    // テキストから連続する数字列を抽出（4桁以上）
    let mut longest = String::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else {
            if current.len() > longest.len() {
                longest = current.clone();
            }
            current.clear();
        }
    }
    if current.len() > longest.len() {
        longest = current;
    }
    if longest.len() >= 4 {
        longest.parse().ok()
    } else {
        None
    }
}

// ======== 統一給与変換 ========

fn calculate_unified(
    min_val: Option<i64>,
    max_val: Option<i64>,
    salary_type: &SalaryType,
) -> (Option<i64>, Option<i64>) {
    let base = match (min_val, max_val) {
        (Some(min), Some(max)) => Some((min + max) / 2),
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    };

    let base = match base {
        Some(v) => v as f64,
        None => return (None, None),
    };

    let (monthly, annual) = match salary_type {
        SalaryType::Hourly => {
            let m = base * HOURLY_TO_MONTHLY;
            (m, m * 12.0)
        }
        SalaryType::Daily => {
            let m = base * DAILY_TO_MONTHLY;
            (m, m * 12.0)
        }
        SalaryType::Weekly => {
            let m = base * WEEKLY_TO_MONTHLY;
            (m, m * 12.0)
        }
        SalaryType::Monthly => (base, base * 12.0),
        SalaryType::Annual => (base / 12.0, base),
    };

    (Some(monthly as i64), Some(annual as i64))
}

// ======== 給与レンジカテゴリ ========

fn get_salary_range_category(monthly: i64) -> String {
    match monthly {
        m if m < 150_000 => "~15万".to_string(),
        m if m < 200_000 => "15~20万".to_string(),
        m if m < 250_000 => "20~25万".to_string(),
        m if m < 300_000 => "25~30万".to_string(),
        m if m < 350_000 => "30~35万".to_string(),
        m if m < 400_000 => "35~40万".to_string(),
        m if m < 500_000 => "40~50万".to_string(),
        _ => "50万~".to_string(),
    }
}

// ======== 信頼度スコア ========

fn calculate_confidence(text: &str, min_val: Option<i64>, salary_type: &SalaryType) -> f64 {
    let mut conf: f64 = 0.5;

    // 明示的な給与種別キーワード
    if text.contains("時給")
        || text.contains("日給")
        || text.contains("月給")
        || text.contains("月収")
        || text.contains("年俸")
        || text.contains("年収")
    {
        conf += 0.2;
    }

    // 円マーク
    if text.contains('円') {
        conf += 0.1;
    }

    // 妥当な範囲チェック
    if let Some(val) = min_val {
        let reasonable = match salary_type {
            SalaryType::Hourly => (800..=50_000).contains(&val),
            SalaryType::Daily => (5_000..=100_000).contains(&val),
            SalaryType::Monthly => (100_000..=2_000_000).contains(&val),
            SalaryType::Annual => (1_000_000..=50_000_000).contains(&val),
            SalaryType::Weekly => (20_000..=500_000).contains(&val),
        };
        if reasonable {
            conf += 0.2;
        }
    }

    conf.min(1.0)
}

// ======== テスト ========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monthly_man() {
        let r = parse_salary("月給25万円", SalaryType::Monthly);
        assert_eq!(r.salary_type, SalaryType::Monthly);
        assert_eq!(r.min_value, Some(250_000));
        assert_eq!(r.unified_monthly, Some(250_000));
    }

    #[test]
    fn test_monthly_range() {
        let r = parse_salary("月給25万円～30万円", SalaryType::Monthly);
        assert!(r.has_range);
        assert_eq!(r.min_value, Some(250_000));
        assert_eq!(r.max_value, Some(300_000));
        assert_eq!(r.unified_monthly, Some(275_000)); // (25+30)/2
    }

    #[test]
    fn test_hourly() {
        let r = parse_salary("時給1200円", SalaryType::Monthly);
        assert_eq!(r.salary_type, SalaryType::Hourly);
        assert_eq!(r.min_value, Some(1_200));
        // 1200 * 173.8 ≈ 208,560
        assert!(r.unified_monthly.unwrap() > 200_000);
    }

    #[test]
    fn test_annual() {
        let r = parse_salary("年収500万円", SalaryType::Monthly);
        assert_eq!(r.salary_type, SalaryType::Annual);
        assert_eq!(r.min_value, Some(5_000_000));
        assert_eq!(r.unified_monthly, Some(5_000_000 / 12));
    }

    #[test]
    fn test_decimal_man() {
        let r = parse_salary("月給25.9万円", SalaryType::Monthly);
        assert_eq!(r.min_value, Some(259_000));
    }

    #[test]
    fn test_fullwidth() {
        let r = parse_salary("月給２５万円", SalaryType::Monthly);
        assert_eq!(r.min_value, Some(250_000));
    }

    #[test]
    fn test_empty() {
        let r = parse_salary("", SalaryType::Monthly);
        assert_eq!(r.confidence, 0.0);
        assert!(r.min_value.is_none());
    }

    #[test]
    fn test_confidence() {
        let r = parse_salary("月給25万円", SalaryType::Monthly);
        assert!(r.confidence >= 0.7);
    }

    // ======== エッジケース ========

    #[test]
    fn test_min_only_range() {
        let r = parse_salary("月給20万円～", SalaryType::Monthly);
        assert!(r.has_range);
        assert_eq!(r.min_value, Some(200_000));
        assert!(r.max_value.is_none());
        assert_eq!(r.unified_monthly, Some(200_000));
    }

    #[test]
    fn test_negotiable() {
        let r = parse_salary("応相談", SalaryType::Monthly);
        assert!(r.min_value.is_none());
        assert!(r.unified_monthly.is_none());
    }

    #[test]
    fn test_commission() {
        let r = parse_salary("完全歩合制", SalaryType::Monthly);
        assert!(r.min_value.is_none());
    }

    #[test]
    fn test_man_with_extra() {
        let r = parse_salary("月給25万3000円", SalaryType::Monthly);
        assert_eq!(r.min_value, Some(253_000));
    }

    #[test]
    fn test_plain_number() {
        let r = parse_salary("月給250000円", SalaryType::Monthly);
        assert_eq!(r.min_value, Some(250_000));
    }

    #[test]
    fn test_daily() {
        let r = parse_salary("日給12000円", SalaryType::Monthly);
        assert_eq!(r.salary_type, SalaryType::Daily);
        assert_eq!(r.min_value, Some(12_000));
        // 12000 * 21.7 ≈ 260,400
        let m = r.unified_monthly.unwrap();
        assert!(m > 250_000 && m < 270_000);
    }

    #[test]
    fn test_fullwidth_tilde() {
        let r = parse_salary("月給２５万円～３０万円", SalaryType::Monthly);
        assert!(r.has_range);
        assert_eq!(r.min_value, Some(250_000));
        assert_eq!(r.max_value, Some(300_000));
    }

    #[test]
    fn test_sen_format() {
        let r = parse_salary("時給1千5百円", SalaryType::Monthly);
        assert_eq!(r.salary_type, SalaryType::Hourly);
        assert_eq!(r.min_value, Some(1_500)); // 1千5百 = 1500
    }

    #[test]
    fn test_sen_format_simple() {
        let r = parse_salary("時給2千円", SalaryType::Monthly);
        assert_eq!(r.min_value, Some(2_000));
    }

    #[test]
    fn test_range_category() {
        let r = parse_salary("月給28万円", SalaryType::Monthly);
        assert_eq!(r.range_category.as_deref(), Some("25~30万"));
    }
}
