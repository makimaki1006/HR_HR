//! Display only. CSV and saved originals retain their raw values.
use std::{collections::BTreeMap, sync::LazyLock};
use unicode_normalization::UnicodeNormalization;
static LABELS: LazyLock<BTreeMap<String, BTreeMap<String, String>>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../frontend/src/screens/job-copy/hrhCodeLabels.json"
    ))
    .expect("checked-in code labels")
});
pub fn code(key: &str, value: &str) -> String {
    let normalized: String = value.nfkc().collect();
    let value = normalized.trim();
    match LABELS.get(key) {
        Some(labels) => labels.get(value).cloned().unwrap_or_else(|| {
            if value.parse::<f64>().is_ok() {
                "確認が必要".into()
            } else {
                value.into()
            }
        }),
        None => value.into(),
    }
}
pub fn money(value: &str) -> String {
    let normalized: String = value.nfkc().collect();
    let digits = normalized.trim().replace(',', "");
    if !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) {
        let len = digits.len();
        let text: String = digits
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                if i > 0 && (len - i) % 3 == 0 {
                    vec![',', c]
                } else {
                    vec![c]
                }
            })
            .collect();
        format!("{text}円")
    } else {
        value.into()
    }
}
pub fn holidays(row: &BTreeMap<String, String>) -> Option<String> {
    let get = |key: &str| row.get(key).filter(|v| !v.trim().is_empty()).cloned();
    get("休日休暇").or_else(|| {
        ["仕事情報補足", "自由項目"].into_iter().find_map(|prefix| {
            (1..=4).find_map(|n| {
                let title = row.get(&format!("{prefix}{n}のタイトル"))?;
                ["休日", "休日・休暇", "休日休暇", "休暇"]
                    .contains(&title.trim())
                    .then(|| get(&format!("{prefix}{n}の内容")))
                    .flatten()
            })
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unverified_codes_do_not_expose_numbers_and_money_keeps_original_amount() {
        for (key, value) in [
            ("試用・研修の有無", "2"),
            ("試用・研修時の雇用条件", "2"),
            ("試用・研修期の給与のタイプ", "3"),
            ("試用・研修期の雇用形態", "6"),
            ("雇用形態", "999"),
            ("給与形態", "３"),
            ("給与形態", "-1"),
        ] {
            assert_eq!(code(key, value), "確認が必要");
        }
        assert_eq!(code("給与形態", "月給"), "月給");
        assert_eq!(money("213000"), "213,000円");
        assert_eq!(money("２４９０００"), "249,000円");
        assert_eq!(money("0"), "0円");
        assert_eq!(money(""), "");
    }
}
