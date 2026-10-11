//! 架電一覧の追加条件。項目・型・値を既存の取引カタログで検証する。
use super::property_catalog::CrmCatalogProperty;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const LIMIT_MESSAGE: &str =
    "条件が多すぎます。追加条件・担当者・日付の指定を減らしてください。";
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct PropertyCondition {
    pub property: String,
    pub operator: String,
    #[serde(default)]
    pub values: Vec<String>,
}

pub fn parse(raw: &str) -> Result<Vec<PropertyCondition>, &'static str> {
    if raw.len() > 12_000 {
        return Err("追加条件が長すぎます。");
    }
    let conditions: Vec<PropertyCondition> =
        serde_json::from_str(raw).map_err(|_| "追加条件の指定が正しくありません。")?;
    if conditions.len() > 3 {
        return Err("追加条件は 3 つまでです。");
    }
    for c in &conditions {
        if c.property.is_empty()
            || c.property.len() > 200
            || c.operator.len() > 30
            || c.values.len() > 20
            || c.values
                .iter()
                .any(|v| v.is_empty() || v.chars().count() > 200 || v.chars().any(char::is_control))
        {
            return Err("条件の値は 200 文字、選択肢は 20 個までです。");
        }
    }
    Ok(conditions)
}

pub fn validate(
    c: &PropertyCondition,
    def: Option<&CrmCatalogProperty>,
) -> Result<Value, &'static str> {
    let p = def.ok_or("選んだ項目は取引の一覧にありません。項目を選び直してください。")?;
    let op = c.operator.as_str();
    let empty = matches!(op, "HAS_PROPERTY" | "NOT_HAS_PROPERTY");
    let allowed = match p.property_type.as_str() {
        "enumeration" => matches!(op, "IN" | "NOT_IN"),
        "string" => empty || matches!(op, "EQ" | "CONTAINS_TOKEN"),
        "number" => empty || matches!(op, "EQ" | "GTE" | "LTE"),
        "date" | "datetime" => empty || matches!(op, "GTE" | "LTE"),
        "bool" => op == "EQ",
        _ => false,
    };
    if !allowed {
        return Err("この項目には選んだ条件を使えません。");
    }
    if empty {
        if !c.values.is_empty() {
            return Err("空かどうかの条件に値は指定できません。");
        }
        return Ok(json!({"propertyName":p.name,"operator":op}));
    }
    if p.property_type == "enumeration" {
        if c.values.is_empty()
            || c.values
                .iter()
                .any(|v| !p.options.iter().any(|o| &o.value == v))
        {
            return Err("一覧にある選択肢を 1 つ以上選んでください。");
        }
        return Ok(json!({"propertyName":p.name,"operator":op,"values":c.values}));
    }
    if c.values.len() != 1 {
        return Err("条件の値を 1 つ指定してください。");
    }
    let mut v = c.values[0].clone();
    match p.property_type.as_str() {
        "number" => {
            if !v.parse::<f64>().is_ok_and(|n| n.is_finite()) {
                return Err("数値を入力してください。");
            }
        }
        "bool" => {
            if !matches!(v.as_str(), "true" | "false") {
                return Err("はい・いいえを選んでください。");
            }
        }
        "date" | "datetime" => {
            let d = super::call_queue::parse_date(&v).ok_or("正しい日付を入力してください。")?;
            v = d
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc()
                .timestamp_millis()
                .to_string();
        }
        _ => {}
    }
    Ok(json!({"propertyName":p.name,"operator":op,"value":v}))
}

/// 検索を送る前に全グループの上限を確かめる。空のグループは検索対象外。
pub fn append(mut groups: Vec<Value>, extra: &[Value]) -> Result<Vec<Value>, &'static str> {
    if groups.len() > 5 {
        return Err(LIMIT_MESSAGE);
    }
    let mut total = 0;
    for g in &mut groups {
        let fs = g["filters"].as_array_mut().ok_or(LIMIT_MESSAGE)?;
        fs.extend_from_slice(extra);
        total += fs.len();
        if fs.len() > 6 {
            return Err(LIMIT_MESSAGE);
        }
    }
    if total > 18 {
        return Err(LIMIT_MESSAGE);
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn def(kind: &str) -> CrmCatalogProperty {
        CrmCatalogProperty {
            name: "amount".into(),
            label: "金額".into(),
            property_type: kind.into(),
            field_type: "text".into(),
            options: vec![],
            writable: false,
        }
    }
    fn c(op: &str, values: &[&str]) -> PropertyCondition {
        PropertyCondition {
            property: "amount".into(),
            operator: op.into(),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }
    #[test]
    fn 型と具体値を検証する() {
        for (kind, op, v, want) in [
            ("string", "CONTAINS_TOKEN", "採用", "採用"),
            ("number", "GTE", "1200", "1200"),
            ("date", "LTE", "2026-10-05", "1791158400000"),
            ("bool", "EQ", "true", "true"),
        ] {
            assert_eq!(
                validate(&c(op, &[v]), Some(&def(kind))).unwrap()["value"],
                want
            );
        }
        for kind in ["string", "number", "date"] {
            for op in ["HAS_PROPERTY", "NOT_HAS_PROPERTY"] {
                assert_eq!(
                    validate(&c(op, &[]), Some(&def(kind))).unwrap()["operator"],
                    op
                );
            }
        }
        assert!(validate(&c("CONTAINS_TOKEN", &["1"]), Some(&def("number"))).is_err());
        assert!(validate(&c("EQ", &["NaN"]), Some(&def("number"))).is_err());
        assert!(validate(&c("EQ", &["1"]), None).is_err());
    }
    #[test]
    fn 選択肢だけを許可する() {
        let mut p = def("enumeration");
        p.options
            .push(crate::handlers::crm_metadata::CrmPropertyOption {
                label: "承認".into(),
                value: "approved".into(),
                hidden: false,
            });
        for op in ["IN", "NOT_IN"] {
            assert_eq!(
                validate(&c(op, &["approved"]), Some(&p)).unwrap()["values"],
                json!(["approved"])
            );
        }
        assert!(validate(&c("IN", &["outside"]), Some(&p)).is_err());
    }
    #[test]
    fn 既存の各グループに追加して上限を拒否する() {
        let extra = json!({"propertyName":"amount","operator":"GTE","value":"1200"});
        let base = vec![
            json!({"filters":[{"propertyName":"dealstage","operator":"EQ","value":"1095387442"}]}),
            json!({"filters":[{"propertyName":"dealstage","operator":"EQ","value":"1095387445"}]}),
        ];
        let got = append(base.clone(), std::slice::from_ref(&extra)).unwrap();
        assert_eq!(got[0]["filters"][0], base[0]["filters"][0]);
        assert_eq!(got[1]["filters"][1], extra);
        assert_eq!(
            append(
                vec![json!({"filters":vec![extra.clone();6]})],
                std::slice::from_ref(&extra)
            ),
            Err(LIMIT_MESSAGE)
        );
        assert_eq!(
            append(vec![json!({"filters":vec![extra;4]}); 5], &[]),
            Err(LIMIT_MESSAGE)
        );
        assert_eq!(
            append(vec![json!({"filters":[]}); 6], &[]),
            Err(LIMIT_MESSAGE)
        );
    }
    #[test]
    fn 個数と長さを制限する() {
        assert!(parse(&serde_json::to_string(&vec![c("EQ", &["1"]); 4]).unwrap()).is_err());
        assert!(
            parse(&serde_json::to_string(&vec![c("EQ", &[&"a".repeat(201)])]).unwrap()).is_err()
        );
        assert!(parse(&serde_json::to_string(&vec![c("IN", &vec!["1"; 21])]).unwrap()).is_err());
    }
}
