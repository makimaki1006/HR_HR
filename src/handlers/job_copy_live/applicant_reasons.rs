//! Bounded recorded reasons; neither causal claims nor copy-version attribution.
use super::Record;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const PROPERTIES: [&str; 3] = ["oubodouki", "ouboriyuu_baitaikisai", "ouboriyuu_hiaringu"];
pub const MAX_ITEMS: usize = 100;
pub const MAX_TEXT_CHARS: usize = 2000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reasons {
    pub available: bool,
    pub source: String,
    pub basis: String,
    pub source_property: Option<String>,
    pub fetched_at: String,
    pub total_applicants: usize,
    pub total_source_values: usize,
    pub source_counts: BTreeMap<String, SourceCounts>,
    pub items: Vec<Reason>,
    pub missing: usize,
    pub blank: usize,
    pub truncated: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCounts {
    pub missing: usize,
    pub blank: usize,
    pub nonblank: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reason {
    pub id: String,
    pub text: String,
    pub source: String,
    pub source_property: String,
    pub application_date: Option<String>,
    pub collected_at: Option<String>,
    pub version_id: Option<String>,
}

fn date(raw: &str) -> Option<String> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .map(|date| date.to_string())
        .filter(|canonical| canonical == raw)
}

/// What replaces a masked part of a reason text.
pub const MASK: &str = "＊＊";

fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}
fn is_kanji_digit(c: char) -> bool {
    "一二三四五六七八九十〇".contains(c)
}
fn is_dash(c: char) -> bool {
    "-－‐−―ー".contains(c)
}
fn is_han(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || c == '々' || c == 'ヶ' || c == 'ケ'
}
fn is_katakana(c: char) -> bool {
    ('\u{30A0}'..='\u{30FF}').contains(&c)
}
fn is_mail_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || "._%+-".contains(c)
}
fn is_mail_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || ".-".contains(c)
}

/// Masks the parts of a free-text reason that can point at one person before it leaves the
/// server: an address finer than 市区町村 (丁目・番地・号・「3-10-1」 and the town or building
/// name written right before it), a phone number, an e-mail address, and a name written with
/// さん・様・氏. Each part becomes 「＊＊」. This is a best-effort filter, not anonymization; the
/// screen still says the text may hold personal information.
pub fn mask_personal_details(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut masked = vec![false; chars.len()];
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        // e-mail address
        if c == '@' {
            let mut start = index;
            while start > 0 && is_mail_local(chars[start - 1]) {
                start -= 1;
            }
            let mut end = index + 1;
            while end < chars.len() && is_mail_domain(chars[end]) {
                end += 1;
            }
            if start < index && chars[index + 1..end].contains(&'.') {
                masked[start..end].iter_mut().for_each(|m| *m = true);
            }
            index = end.max(index + 1);
            continue;
        }
        // a run of numbers, dashes and address words (3丁目10番地1号, 3-10-1, 097-123-4567)
        let starts_number = is_digit(c)
            || (is_kanji_digit(c) && {
                let mut look = index;
                while look < chars.len() && is_kanji_digit(chars[look]) {
                    look += 1;
                }
                chars.get(look) == Some(&'丁')
            });
        if starts_number {
            let mut end = index;
            let mut digits = 0;
            let mut address = false;
            let mut dashed = false;
            while end < chars.len() {
                let here = chars[end];
                let next = chars.get(end + 1).copied();
                if is_digit(here) || (is_kanji_digit(here) && !address) {
                    digits += usize::from(is_digit(here));
                    end += 1;
                } else if is_dash(here) && next.is_some_and(is_digit) && end > index {
                    dashed = true;
                    end += 1;
                } else if (here == '丁' && next == Some('目'))
                    || (here == '番' && next == Some('地'))
                {
                    address = true;
                    end += 2;
                } else if (here == '番' && next != Some('目')) || here == '号' {
                    address = true;
                    end += 1;
                    if chars.get(end) == Some(&'室') {
                        end += 1;
                    }
                } else if here == '(' || here == '（' || here == ')' || here == '）' {
                    // 097(123)4567
                    if digits > 0 && next.is_some_and(is_digit) {
                        dashed = true;
                        end += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            if address || dashed || digits >= 8 {
                // the town or building name written right before an address number (not before
                // a phone number: 「携帯09012345678」 keeps 「携帯」)
                let street = address || (dashed && digits < 10);
                let mut start = index;
                let mut taken = 0;
                while street
                    && start > 0
                    && taken < 12
                    && (is_han(chars[start - 1]) || is_katakana(chars[start - 1]))
                {
                    start -= 1;
                    taken += 1;
                }
                masked[start..end].iter_mut().for_each(|m| *m = true);
            }
            index = end.max(index + 1);
            continue;
        }
        // a name followed by さん・様・氏
        let honorific = ["さん", "様", "氏", "くん", "ちゃん"]
            .iter()
            .find(|word| chars[index..].starts_with(&word.chars().collect::<Vec<_>>()));
        if let Some(word) = honorific {
            let mut start = index;
            while start > 0
                && index - start < 4
                && (is_han(chars[start - 1]) || is_katakana(chars[start - 1]))
            {
                start -= 1;
            }
            let name: String = chars[start..index].iter().collect();
            if start < index && !["皆", "客", "お客", "奥", "神", "王"].contains(&name.as_str())
            {
                masked[start..index].iter_mut().for_each(|m| *m = true);
            }
            index += word.chars().count();
            continue;
        }
        index += 1;
    }
    let mut result = String::with_capacity(text.len());
    let mut previous = false;
    for (c, hide) in chars.into_iter().zip(masked) {
        if hide {
            if !previous {
                result.push_str(MASK);
            }
        } else {
            result.push(c);
        }
        previous = hide;
    }
    result
}

pub fn extract(listing: &str, rows: &[Record], fetched_at: String) -> Reasons {
    // Same deterministic duplicate handling as the existing aggregate summary.
    let unique: BTreeMap<_, _> = rows.iter().map(|row| (&row.id, row)).collect();
    let mut reasons = Reasons {
        available: true,
        source: "hubspot".into(),
        basis: "recorded_applicant_reason".into(),
        source_property: None,
        fetched_at,
        total_applicants: unique.len(),
        total_source_values: unique.len() * PROPERTIES.len(),
        source_counts: PROPERTIES
            .iter()
            .map(|key| ((*key).into(), SourceCounts::default()))
            .collect(),
        items: Vec::new(),
        missing: 0,
        blank: 0,
        truncated: false,
    };
    for row in unique.values() {
        for property in PROPERTIES {
            let counts = reasons
                .source_counts
                .get_mut(property)
                .expect("fixed source property");
            let Some(raw) = row.properties.get(property).and_then(Option::as_deref) else {
                counts.missing += 1;
                reasons.missing += 1;
                continue;
            };
            let text = raw.trim();
            if text.is_empty() {
                counts.blank += 1;
                reasons.blank += 1;
                continue;
            }
            counts.nonblank += 1;
            if reasons.items.len() == MAX_ITEMS {
                reasons.truncated = true;
                continue;
            }
            let mut hash = Sha256::new();
            for part in [listing, row.id.as_str(), property] {
                hash.update(part.as_bytes());
                hash.update([0]);
            }
            // Masked before it is cut, so a cut never leaves half of an address behind.
            let bounded: String = mask_personal_details(text)
                .chars()
                .take(MAX_TEXT_CHARS)
                .collect();
            reasons.truncated |= text.chars().count() > MAX_TEXT_CHARS;
            reasons.items.push(Reason {
                id: format!("{:x}", hash.finalize()),
                text: bounded,
                source: "hubspot".into(),
                source_property: property.into(),
                application_date: row.value("yingmuri").and_then(date),
                collected_at: None,
                version_id: None,
            });
        }
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str, values: &[(&str, Option<&str>)]) -> Record {
        Record {
            id: id.into(),
            properties: values
                .iter()
                .map(|(key, value)| ((*key).into(), value.map(str::to_owned)))
                .collect(),
        }
    }
    fn extract_rows(rows: &[Record]) -> Reasons {
        extract("30", rows, "2026-10-05T00:00:00Z".into())
    }
    #[test]
    fn source_counts_conserve_missing_blank_and_nonblank_without_person_fields() {
        let a = row(
            "50",
            &[
                ("oubodouki", Some(" Flexible hours ")),
                ("ouboriyuu_baitaikisai", Some(" \n ")),
                ("ouboriyuu_hiaringu", None),
                ("yingmuri", Some("2026-10-03")),
                ("email", Some("fictional@example.invalid")),
            ],
        );
        let b = row(
            "51",
            &[
                ("ouboriyuu_hiaringu", Some("Flexible hours")),
                ("yingmuri", Some("invalid")),
            ],
        );
        let reasons = extract_rows(&[a.clone(), a, b]);
        assert_eq!(
            (reasons.total_applicants, reasons.total_source_values),
            (2, 6)
        );
        assert_eq!(
            (reasons.missing, reasons.blank, reasons.items.len()),
            (3, 1, 2)
        );
        assert_eq!(reasons.items[0].text, "Flexible hours");
        assert_eq!(
            reasons.items[0].application_date.as_deref(),
            Some("2026-10-03")
        );
        assert_eq!(reasons.items[1].application_date, None);
        assert!(reasons
            .items
            .iter()
            .all(|item| item.version_id.is_none() && item.collected_at.is_none()));
        let json = serde_json::to_value(reasons).unwrap();
        assert!(!json.to_string().contains("fictional@example.invalid"));
        assert!(!json.to_string().contains("\"email\""));
    }
    #[test]
    fn different_sources_are_distinct_descriptions_and_ids_are_scoped() {
        let rows = [row(
            "50",
            &[
                ("oubodouki", Some("Same")),
                ("ouboriyuu_hiaringu", Some("Same")),
            ],
        )];
        let reasons = extract_rows(&rows);
        assert_eq!(reasons.items.len(), 2);
        assert_ne!(reasons.items[0].id, reasons.items[1].id);
        assert_eq!(reasons.items[0].id.len(), 64);
        assert_eq!(reasons.items[0].id, extract_rows(&rows).items[0].id);
        assert_ne!(
            reasons.items[0].id,
            extract("31", &rows, reasons.fetched_at).items[0].id
        );
    }
    #[test]
    fn unicode_length_and_item_caps_do_not_change_source_observation_counts() {
        let long = "働".repeat(MAX_TEXT_CHARS + 1);
        let rows: Vec<_> = (0..MAX_ITEMS + 1)
            .map(|index| row(&index.to_string(), &[("oubodouki", Some(&long))]))
            .collect();
        let reasons = extract_rows(&rows);
        assert_eq!(reasons.items.len(), MAX_ITEMS);
        assert!(reasons
            .items
            .iter()
            .all(|item| item.text.chars().count() == MAX_TEXT_CHARS));
        assert!(reasons.truncated);
        assert_eq!(reasons.source_counts["oubodouki"].nonblank, MAX_ITEMS + 1);
        assert_eq!(reasons.missing, (MAX_ITEMS + 1) * 2);
    }
    #[test]
    fn addresses_phone_numbers_mail_and_names_are_masked() {
        for (raw, expected) in [
            ("大分市府内町3丁目から近いため", "＊＊から近いため"),
            (
                "自宅は府内町3-10-1 府内ビル201号室です",
                "自宅は＊＊ ＊＊です",
            ),
            ("由布市湯布院町1234番地に住んでいます", "＊＊に住んでいます"),
            ("連絡は097-123-4567まで", "連絡は＊＊まで"),
            ("携帯09012345678", "携帯＊＊"),
            (
                "mail: taro.yamada@example.co.jp でお願いします",
                "mail: ＊＊ でお願いします",
            ),
            ("山田さんの紹介で応募", "＊＊さんの紹介で応募"),
            ("三丁目の店舗に近い", "＊＊の店舗に近い"),
        ] {
            assert_eq!(mask_personal_details(raw), expected, "{raw}");
        }
    }
    #[test]
    fn ordinary_reason_texts_are_left_as_they_are() {
        for text in [
            "週3日から働けるため",
            "月給25万円以上で、土日休みだったので",
            "1日8件程度の配送なら続けられそう",
            "皆さんの雰囲気が良さそうだった",
            "お客様と話す仕事がしたい",
            "応募は2回目です。3番目に見た求人でした",
        ] {
            assert_eq!(mask_personal_details(text), text);
        }
    }
    #[test]
    fn extracted_texts_carry_no_street_address() {
        let rows = [row(
            "50",
            &[("oubodouki", Some("大分市府内町3丁目10-1から近いため"))],
        )];
        let reasons = extract_rows(&rows);
        assert_eq!(reasons.items[0].text, "＊＊から近いため");
        let json = serde_json::to_string(&reasons).unwrap();
        assert!(!json.contains("府内町"));
        assert!(!json.contains("3丁目"));
    }
    #[test]
    fn verified_empty_read_differs_from_absent_optional_snapshot_field() {
        let reasons = extract_rows(&[]);
        assert!(reasons.available);
        assert_eq!(reasons.total_source_values, 0);
        assert!(reasons.items.is_empty());
        assert!(!reasons.truncated);
    }
}
