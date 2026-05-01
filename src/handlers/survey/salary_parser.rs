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
    /// 賞与月数 (例: "年4ヶ月" → 4.0、"賞与年2.5月" → 2.5)。
    /// 抽出元テキストに賞与表記がない場合は `None`。
    /// 2026-04-26 Fix-A 追加: HW Panel 5 (`condition_gap.rs`) と整合する年収計算
    /// `annual_with_bonus = monthly_min × (12 + bonus_months)` を可能にする。
    /// 後方互換: 既存呼出は本フィールドを参照しないため影響なし。
    #[serde(default)]
    pub bonus_months: Option<f64>,
}

// ======== 変換レート ========

// 厚労省「就業条件総合調査 2024」基準。aggregator.rs と統一済み (C-3, 2026-04-26)。
// 旧値 (GAS 互換): HOURLY=173.8 (8h×21.7日), DAILY=21.7。GAS 互換性は V2 HW では要件外と判断し統一。
// 月給換算は (時給 × 167) または (日給 × 21)、週給は ×4.33 (=52週/12月)。
// 影響: 既存テストで一部期待値変更あり (リリースノート参照)。
const HOURLY_TO_MONTHLY: f64 = 167.0; // 8h × 20.875日 (厚労省基準)
const DAILY_TO_MONTHLY: f64 = 21.0; // 月間勤務日数 (20.875 切り上げ、aggregator と一致)
const WEEKLY_TO_MONTHLY: f64 = 4.33; // 月間週数 (= 52週/12月、aggregator と一致)

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
    let bonus_months = parse_bonus_months(text);

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
        bonus_months,
    }
}

// ======== 賞与パース (2026-04-26 Fix-A) ========

/// テキストから賞与月数 (年あたり) を抽出する。
///
/// 対応表記 (網羅的):
/// - 「年4ヶ月」「年4.5ヶ月」「年4ケ月」「年4か月」「年4月」(賞与/ボーナスを含む文脈)
/// - 「賞与4ヶ月」「賞与年2.5ヶ月」「賞与4.5月」「賞与計4ヶ月」
/// - 「ボーナス2回/年」 (回数のみは月数換算できないため抽出対象外: 数値根拠なし)
/// - 「賞与あり (年2回)」 → 月数特定不能のため None
///
/// # 戻り値
/// 賞与月数を `f64` で返す。0.5..=12.0 の妥当範囲外 / 抽出失敗時は `None`。
///
/// # 設計判断
/// - 「ボーナス2回/年」のような **回数のみ** は月数推定の根拠が無いため意図的に None とする
///   (1回1ヶ月、2回1ヶ月、2回2ヶ月などビジネス慣行が分かれるため、推測しない)。
/// - bonus_months > 12 や 0 は CSV データ異常として保守的に弾く。
pub fn parse_bonus_months(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    let normalized = normalize_text(text);
    // 「賞与」「ボーナス」「年」のいずれかを含む文脈のみ対象
    let has_bonus_kw = normalized.contains("賞与") || normalized.contains("ボーナス");
    let has_year_kw = normalized.contains("年");
    if !has_bonus_kw && !has_year_kw {
        return None;
    }
    // ヶ月 / ケ月 / か月 / カ月 / 月 を統一: 月単位のサフィックスを順に試す
    // 「月給」誤検出を防ぐため、サフィックス前にスペースまたは数字を要求
    let candidates = ["ヶ月", "ケ月", "か月", "カ月", "ヵ月", "箇月"];
    let mut best: Option<f64> = None;
    for suffix in candidates.iter() {
        if let Some(v) = extract_months_before_suffix(&normalized, suffix) {
            if (0.5..=12.0).contains(&v) {
                // 賞与/ボーナス/年 の文脈一致を確認
                if has_bonus_kw || has_year_kw {
                    best = Some(best.map(|b| b.max(v)).unwrap_or(v));
                }
            }
        }
    }
    // フォールバック: 「年X月」「賞与X月」のように単独「月」表記
    // ただし「月給」「月収」は除外する必要があるため、賞与/年の近傍のみ対象
    if best.is_none() {
        if let Some(v) = extract_bonus_months_after_keyword(&normalized) {
            if (0.5..=12.0).contains(&v) {
                best = Some(v);
            }
        }
    }
    best
}

/// "X.XヶY月" のような数値+suffix を全位置から抽出して最大値を返す。
fn extract_months_before_suffix(text: &str, suffix: &str) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(suffix) {
        let pos = search_from + rel;
        let before = &text[..pos];
        // 末尾から連続数字+小数点を取得
        let num_str: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(v) = num_str.parse::<f64>() {
            // 賞与/ボーナス/年 が pos の前 30 文字以内にあるか
            // バイト境界ではなく char 境界で切り出す (マルチバイト対応)
            let window_start = before
                .char_indices()
                .rev()
                .nth(29)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let window = &before[window_start..];
            if window.contains("賞与") || window.contains("ボーナス") || window.contains("年")
            {
                best = Some(best.map(|b| b.max(v)).unwrap_or(v));
            }
        }
        search_from = pos + suffix.len();
    }
    best
}

/// 「賞与X月」「年X月」 (ヶ月 suffix なし) パターンの抽出。
/// 「月給」「月収」「月額」とは衝突しないように、月の直後が「給/収/額」でないことを確認。
fn extract_bonus_months_after_keyword(text: &str) -> Option<f64> {
    for keyword in ["賞与", "ボーナス", "年"].iter() {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(keyword) {
            let pos = search_from + rel;
            let after = &text[pos + keyword.len()..];
            // 「年4月」 → 4.0、ただし「年4月入社」「2024年4月」のような日付は除外する
            // ここでは「賞与」「ボーナス」のときのみ採用、「年」キーワードはノイズが多いので
            // 直前/直後に「賞与」が無いと保守的に弾く
            if *keyword == "年"
                && !text[..pos].contains("賞与")
                && !text[..pos].contains("ボーナス")
            {
                search_from = pos + keyword.len();
                continue;
            }
            // 数字を取得
            let num_str: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if num_str.is_empty() {
                search_from = pos + keyword.len();
                continue;
            }
            // 「月」が次に来るか確認 (ヶ月 等は別ルートで処理済)
            let after_num = &after[num_str.len()..];
            if after_num.starts_with('月') {
                // 「月給」「月収」「月額」を除外
                let after_tsuki = &after_num['月'.len_utf8()..];
                if after_tsuki.starts_with('給')
                    || after_tsuki.starts_with('収')
                    || after_tsuki.starts_with('額')
                {
                    search_from = pos + keyword.len();
                    continue;
                }
                if let Ok(v) = num_str.parse::<f64>() {
                    return Some(v);
                }
            }
            search_from = pos + keyword.len();
        }
    }
    None
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
        bonus_months: None,
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
        // C-3 統一後 (167h): 1200 * 167 = 200,400
        assert_eq!(r.unified_monthly, Some(200_400));
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
        // C-3 統一後 (21日/月): 12000 * 21 = 252,000
        assert_eq!(r.unified_monthly, Some(252_000));
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

    // ========================================================================
    // 2026-04-26 Fix-A 賞与パース 逆証明テスト
    // 修正前: ParsedSalary に bonus_months なし → 賞与表記を持つ求人でも年収=月給×12 のみ
    // 修正後: bonus_months: Option<f64> 追加 → 年収=月給×(12+bonus_months) が可能
    // HW Panel 5 (condition_gap.rs:115-126) と整合
    // ========================================================================

    #[test]
    fn fixa_bonus_parse_year_4_kagetsu() {
        // 修正前: bonus_months フィールド存在せず
        // 修正後: "年4ヶ月" → Some(4.0)
        let r = parse_salary("月給25万円 賞与年4ヶ月", SalaryType::Monthly);
        assert_eq!(r.bonus_months, Some(4.0));
    }

    #[test]
    fn fixa_bonus_parse_decimal_2_5_kagetsu() {
        // "賞与年2.5ヶ月" のような小数月数
        let r = parse_salary("月給20万円 賞与年2.5ヶ月", SalaryType::Monthly);
        assert_eq!(r.bonus_months, Some(2.5));
    }

    #[test]
    fn fixa_bonus_parse_kekanji() {
        // "ケ月" / "か月" / "カ月" の表記ゆれ
        let r1 = parse_salary("月給25万円 賞与年4ケ月", SalaryType::Monthly);
        assert_eq!(r1.bonus_months, Some(4.0));
        let r2 = parse_salary("月給25万円 賞与年3か月", SalaryType::Monthly);
        assert_eq!(r2.bonus_months, Some(3.0));
        let r3 = parse_salary("月給25万円 賞与年2カ月", SalaryType::Monthly);
        assert_eq!(r3.bonus_months, Some(2.0));
    }

    #[test]
    fn fixa_bonus_parse_kanji_kagetsu() {
        // 「ヶ月」suffix なしで「賞与年4月」 (month 単位、月給ではない文脈)
        // 現実的な CSV 表記: 「年4月」「賞与年5月」などで月数を表す
        let r = parse_salary("月給20万円 賞与年4月", SalaryType::Monthly);
        assert_eq!(r.bonus_months, Some(4.0), "「賞与年4月」は4ヶ月と解釈");
    }

    #[test]
    fn fixa_bonus_parse_no_bonus_returns_none() {
        // 賞与表記なし → None (旧 ParsedSalary 型では存在しないフィールド)
        let r = parse_salary("月給25万円", SalaryType::Monthly);
        assert_eq!(r.bonus_months, None);
    }

    #[test]
    fn fixa_bonus_parse_bonus_count_only_returns_none() {
        // 「賞与年2回」は回数のみ → 月数特定不能 → None
        // 設計判断: 推測しないこと (memory feedback_never_guess_data)
        let r = parse_salary("月給25万円 賞与年2回", SalaryType::Monthly);
        assert_eq!(r.bonus_months, None, "賞与「回」のみは月数換算不能 → None");
    }

    #[test]
    fn fixa_bonus_parse_clamp_invalid() {
        // 妥当範囲外 (>12ヶ月 や 0ヶ月) は None
        let r1 = parse_salary("月給25万円 賞与年20ヶ月", SalaryType::Monthly);
        assert_eq!(r1.bonus_months, None, "20ヶ月は妥当範囲外 (>12)");
        let r2 = parse_salary("月給25万円 賞与年0ヶ月", SalaryType::Monthly);
        assert_eq!(r2.bonus_months, None, "0ヶ月は範囲外");
    }

    #[test]
    fn fixa_bonus_parse_does_not_confuse_gekkyu() {
        // 「月給」「月収」「月額」を賞与月数と誤認しない
        let r = parse_salary("月給25万円", SalaryType::Monthly);
        assert_eq!(r.bonus_months, None);
        let r2 = parse_salary("月収25万円", SalaryType::Monthly);
        assert_eq!(r2.bonus_months, None);
    }

    /// 2026-05-01 マルチバイト境界パニック回帰テスト:
    /// `before.len().saturating_sub(30)` でバイト演算していたためマルチバイト文字の
    /// 途中でスライスして panic していたケース。
    /// 失敗テキスト 122 byte の場合 122-30=92 が `円` (90..93) の途中。
    #[test]
    fn regression_multibyte_boundary_no_panic() {
        // 求人タイトルのような長い日本語 + 末尾 ヶ月 表記
        let text = "電気工事・通信工事スタッフ*積水ハウス専属50年の安定感*月給30万円~*20・30代活躍中*賞与5ヶ月分";
        // panic せずに処理できればOK (extract_months_before_suffix が安全に走る)
        let r = parse_salary(text, SalaryType::Monthly);
        // 賞与5ヶ月の前 30 文字以内に "賞与" があるので bonus_months=5.0 を取れる
        assert_eq!(r.bonus_months, Some(5.0));
    }

    #[test]
    fn fixa_bonus_annual_with_bonus_calc_alignment() {
        // HW Panel 5 (condition_gap.rs:115-126) の年収計算と整合:
        //   annual_with_bonus = monthly_min × (12 + bonus_months)
        // 例: 月給20万円 + 賞与4ヶ月 → 20万 × 16 = 320万円
        let r = parse_salary("月給20万円 賞与年4ヶ月", SalaryType::Monthly);
        assert_eq!(r.min_value, Some(200_000));
        assert_eq!(r.bonus_months, Some(4.0));
        let monthly = r.min_value.unwrap() as f64;
        let bonus = r.bonus_months.unwrap();
        let annual_with_bonus = (monthly * (12.0 + bonus)) as i64;
        assert_eq!(annual_with_bonus, 3_200_000, "20万 × 16 = 320万");
    }
}
