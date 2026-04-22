//! 示唆テキストの表現検証モジュール
//!
//! 「相関≠因果」原則（memory `feedback_correlation_not_causation.md`）を徹底するため、
//! すべての示唆 body テキストは「傾向」「可能性」等の非断定表現を含み、
//! 「確実に」「必ず」「100%」等の断定表現を含まないことを保証する。
//!
//! debug_assert! で検証（本番ビルドでは警告ログ）。

/// 必須表現（いずれか1つ以上含まれるべき）
const REQUIRED_PHRASES: &[&str] = &[
    "傾向",
    "可能性",
    "見られ",
    "みられ",
    "示唆",
    "うかがえ",
    "推察",
    "思われ",
];

/// 禁止表現（相関を因果と誤認させる断定表現）
const FORBIDDEN_PHRASES: &[&str] = &[
    "確実に",
    "必ず",
    "100%",
    "絶対",
    "断定",
    "疑いの余地なく",
    "例外なく",
];

/// 示唆 body テキストの表現を検証する
///
/// # 戻り値
/// - `Ok(())` : 必須表現を含み、禁止表現を含まない
/// - `Err(reason)` : いずれかの条件違反
pub fn validate_insight_phrase(body: &str) -> Result<(), String> {
    // 禁止表現の検出（相関≠因果原則違反）
    for forbidden in FORBIDDEN_PHRASES {
        if body.contains(forbidden) {
            return Err(format!(
                "Forbidden phrase '{forbidden}' detected (correlation must not be stated as causation)"
            ));
        }
    }

    // 必須表現のチェック（1つ以上含まれる必要）
    let has_required = REQUIRED_PHRASES.iter().any(|p| body.contains(p));
    if !has_required {
        return Err(format!(
            "Missing required hedging phrase. Body must include one of: {:?}",
            REQUIRED_PHRASES
        ));
    }

    Ok(())
}

/// debug_assert で検証する簡便関数（本番では no-op）
///
/// 新規 insight を生成した直後に呼び出すことで、
/// 開発時に表現違反を早期検出する。
pub fn assert_valid_phrase(body: &str) {
    if let Err(reason) = validate_insight_phrase(body) {
        if cfg!(debug_assertions) {
            panic!("Insight phrase validation failed: {reason}\nBody: {body}");
        } else {
            tracing::warn!("Insight phrase validation failed: {reason}; body={body}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_with_tendency() {
        let body = "失業率が県平均より高く、未マッチ層が約XXX人いる可能性があります";
        assert!(validate_insight_phrase(body).is_ok());
    }

    #[test]
    fn test_valid_with_mirareru() {
        let body = "医療福祉の従業者が少ない傾向がみられます";
        assert!(validate_insight_phrase(body).is_ok());
    }

    #[test]
    fn test_invalid_missing_hedge() {
        let body = "失業率が県平均より高く、未マッチ層がXXX人です";
        assert!(validate_insight_phrase(body).is_err());
    }

    #[test]
    fn test_invalid_with_zettai() {
        let body = "絶対に採用難の地域です";
        let result = validate_insight_phrase(body);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("絶対"));
    }

    #[test]
    fn test_invalid_with_kanarazu() {
        let body = "必ず定着率が下がる傾向があります";
        let result = validate_insight_phrase(body);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("必ず"));
    }

    #[test]
    fn test_invalid_with_100_percent() {
        let body = "100%採用できる可能性があります";
        let result = validate_insight_phrase(body);
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_with_ukagau() {
        let body = "採用余力がうかがえる地域です";
        assert!(validate_insight_phrase(body).is_ok());
    }
}
