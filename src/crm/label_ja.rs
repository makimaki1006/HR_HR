//! HubSpot が最初から持っている項目 (標準プロパティ・標準グループ) の日本語の表示名。
//!
//! HubSpot のポータルは標準項目を英語の名前で返す (例: `amount` = "Amount")。画面にはそのまま出さず、
//! 項目の一覧 (`property_catalog`) と項目の定義 (`handlers::crm_metadata`) を組み立てる 1 か所で日本語に置き換える。
//! 対応表は `label_ja.json` (データ)。会社が自分で作った項目 (`bpo_*` 等) は表に入れず、HubSpot の定義が
//! `hubspotDefined: false` と答えた項目は、名前が表にあっても訳さない。
//! 表に無い名前は元の表示名のまま。保存済みのビューや URL は内部名だけを持つので影響を受けない。

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Default)]
struct ObjectTable {
    #[serde(default)]
    properties: HashMap<String, String>,
    #[serde(default)]
    groups: HashMap<String, String>,
}

fn table() -> &'static HashMap<String, ObjectTable> {
    static T: OnceLock<HashMap<String, ObjectTable>> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("label_ja.json")).unwrap_or_default())
}

/// 標準項目の日本語名。`definition` は HubSpot の項目定義 (`hubspotDefined` が false の項目は訳さない)
pub fn property_label(object: &str, name: &str, definition: &Value) -> Option<&'static str> {
    if definition.get("hubspotDefined").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    table()
        .get(object)?
        .properties
        .get(name)
        .map(String::as_str)
}

/// 標準グループの日本語名
pub fn group_label(object: &str, name: &str) -> Option<&'static str> {
    table().get(object)?.groups.get(name).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 標準の項目とグループを訳す() {
        assert_eq!(property_label("deals", "amount", &json!({})), Some("金額"));
        assert_eq!(
            property_label("deals", "dealname", &json!({"hubspotDefined": true})),
            Some("取引名")
        );
        assert_eq!(
            property_label("contacts", "firstname", &json!({})),
            Some("名")
        );
        assert_eq!(
            property_label("companies", "name", &json!({})),
            Some("会社名")
        );
        assert_eq!(group_label("deals", "dealinformation"), Some("取引情報"));
        assert_eq!(
            group_label("contacts", "contactinformation"),
            Some("コンタクト情報")
        );
        assert_eq!(
            group_label("companies", "companyinformation"),
            Some("会社情報")
        );
    }

    #[test]
    fn 会社の項目_表に無い名前_hubspot_defined_false_は訳さない() {
        assert_eq!(property_label("deals", "bpo_hsurl", &json!({})), None);
        assert_eq!(
            property_label("deals", "ai_email_alert_test", &json!({})),
            None
        );
        assert_eq!(
            property_label("deals", "no_such_property", &json!({})),
            None
        );
        assert_eq!(
            property_label("deals", "amount", &json!({"hubspotDefined": false})),
            None
        );
        assert_eq!(property_label("tickets", "amount", &json!({})), None);
        assert_eq!(group_label("deals", "custom_group"), None);
    }

    #[test]
    fn 表の中身は空でなく訳は日本語を含む() {
        for (obj, min) in [("deals", 70), ("contacts", 250), ("companies", 150)] {
            let t = &table()[obj];
            assert!(t.properties.len() >= min, "{obj}: {}", t.properties.len());
            assert!(!t.groups.is_empty());
            for (k, v) in t.properties.iter().chain(t.groups.iter()) {
                assert!(!v.trim().is_empty(), "{obj}.{k}");
                assert!(
                    v.chars().any(|c| !c.is_ascii()),
                    "{obj}.{k} が英語のまま: {v}"
                );
            }
        }
    }
}
