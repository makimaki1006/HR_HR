//! 求人の正規化済み状態を優先し、欠損時だけ媒体の生値を使う。
pub fn label(normalized: Option<&str>, raw: Option<&str>, media: &str) -> &'static str {
    if let Some(value) = normalized.map(str::trim).filter(|v| !v.is_empty()) {
        return match value {
            "公開中" => "掲載中",
            "公開終了" => "掲載終了",
            "公開前" => "掲載前",
            _ => "不明",
        };
    }
    match (media, raw.map(str::trim)) {
        ("airwork", Some("02")) => "掲載中",
        ("airwork", Some("01")) => "掲載終了",
        (_, Some("公開" | "公開中" | "掲載中")) => "掲載中",
        ("airwork", Some("未掲載" | "掲載停止" | "停止中")) => "掲載終了",
        (_, Some("非公開" | "公開終了" | "掲載終了")) => "掲載終了",
        (_, Some("公開開始前" | "公開前" | "掲載前")) => "掲載前",
        _ => "不明",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concrete_publication_values_and_priority() {
        for (normalized, raw, media, expected) in [
            (None, Some("02"), "airwork", "掲載中"),
            (None, Some("01"), "airwork", "掲載終了"),
            (Some("公開前"), Some("02"), "airwork", "掲載前"),
            (Some("公開中"), Some("01"), "airwork", "掲載中"),
            (Some("公開終了"), Some("公開"), "hrh", "掲載終了"),
            (Some("  "), Some(" 02 "), "airwork", "掲載中"),
            (None, Some("公開"), "hrh", "掲載中"),
            (None, Some("非公開"), "hrh", "掲載終了"),
            (None, Some("公開終了"), "hrh", "掲載終了"),
            (None, Some("公開開始前"), "hrh", "掲載前"),
            (None, None, "hrh", "不明"),
            (Some(""), Some(""), "airwork", "不明"),
            (Some("未定義"), Some("02"), "airwork", "不明"),
            (None, Some("02"), "hrh", "不明"),
        ] {
            assert_eq!(label(normalized, raw, media), expected);
        }
    }
}
