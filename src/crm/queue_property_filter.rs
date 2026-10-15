//! 取引カタログで追加条件と並べ替えを検証する。新旧のURLを同じ条件へ解決する。
use super::property_catalog::CrmCatalogProperty;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const LIMIT_MESSAGE: &str = "条件が多すぎます。条件かまとまりを減らしてください。";
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct PropertyCondition {
    pub property: String,
    pub operator: String,
    #[serde(default)]
    pub values: Vec<String>,
}

fn shape(cs: &[PropertyCondition]) -> Result<(), &'static str> {
    if cs.len() > 6 {
        return Err(LIMIT_MESSAGE);
    }
    for c in cs {
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
    Ok(())
}
pub fn parse(raw: &str) -> Result<Vec<PropertyCondition>, &'static str> {
    if raw.len() > 12_000 {
        return Err("追加条件が長すぎます。");
    }
    let cs: Vec<PropertyCondition> =
        serde_json::from_str(raw).map_err(|_| "条件の指定が正しくありません。")?;
    shape(&cs)?;
    Ok(cs)
}
pub fn parse_groups(raw: &str) -> Result<Vec<Vec<PropertyCondition>>, &'static str> {
    if raw.len() > 36_000 {
        return Err("追加条件が長すぎます。");
    }
    let gs: Vec<Vec<PropertyCondition>> =
        serde_json::from_str(raw).map_err(|_| "条件の指定が正しくありません。")?;
    if gs.len() > 3 {
        return Err(LIMIT_MESSAGE);
    }
    if gs.iter().any(Vec::is_empty) {
        return Err("空のまとまりには条件を追加してください。");
    }
    for cs in &gs {
        shape(cs)?;
    }
    Ok(gs)
}

/// 選択肢がプロパティ定義の外にある項目の「選べる値」。
/// `dealstage` はパイプラインごとに違うので、いま選んでいるパイプラインのステージ ID だけを受け付ける。
/// `pipeline` は架電キューで選べるパイプラインの ID だけ。担当者を選ぶ型 (`owner_ref`) は数字の ID。
#[derive(Debug, Clone, Default)]
pub struct ValueScope {
    /// 選んだパイプラインのステージ ID
    pub stages: Vec<String>,
    /// 許可されたパイプラインの ID
    pub pipelines: Vec<String>,
}

pub fn sortable(p: &CrmCatalogProperty) -> bool {
    matches!(
        p.property_type.as_str(),
        "string" | "number" | "date" | "datetime" | "bool" | "enumeration"
    ) && p.field_type != "checkbox"
}

/// 日付型はUTC午前0時の暦日、日時型はJSTの一日の開始/終了。相対指定は今日を含むN日間。
pub fn resolve(
    c: &PropertyCondition,
    def: Option<&CrmCatalogProperty>,
    now: DateTime<Utc>,
    scope: &ValueScope,
) -> Result<Value, &'static str> {
    shape(std::slice::from_ref(c))?;
    let p = def.ok_or("選んだ項目は取引の一覧にありません。項目を選び直してください。")?;
    let mut op = c.operator.clone();
    let mut values = c.values.clone();
    let date = matches!(p.property_type.as_str(), "date" | "datetime");
    if p.property_type == "bool" && matches!(op.as_str(), "IS_TRUE" | "IS_FALSE") {
        if !values.is_empty() {
            return Err("この条件に値は指定できません。");
        }
        values = vec![if op == "IS_TRUE" { "true" } else { "false" }.into()];
        op = "EQ".into();
    }

    if date
        && matches!(
            op.as_str(),
            "TODAY" | "BEFORE_TODAY" | "LAST_N_DAYS" | "NEXT_N_DAYS"
        )
    {
        let today = now
            .with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"))
            .date_naive();
        if matches!(op.as_str(), "TODAY" | "BEFORE_TODAY") && !values.is_empty() {
            return Err("この日付条件に値は指定できません。");
        }
        let (lo, hi) = match op.as_str() {
            "TODAY" => (today, today),
            "BEFORE_TODAY" => (today, today),
            _ => {
                if values.len() != 1 {
                    return Err("日数を指定してください。");
                }
                let n: i64 = values[0]
                    .parse()
                    .map_err(|_| "日数は 1〜3650 の整数で指定してください。")?;
                if !(1..=3650).contains(&n) {
                    return Err("日数は 1〜3650 の整数で指定してください。");
                }
                if op == "LAST_N_DAYS" {
                    (today - Duration::days(n - 1), today)
                } else {
                    (today, today + Duration::days(n - 1))
                }
            }
        };
        values = if op == "BEFORE_TODAY" {
            vec![lo.to_string()]
        } else {
            vec![lo.to_string(), hi.to_string()]
        };
        op = if op == "BEFORE_TODAY" {
            "LT"
        } else {
            "BETWEEN"
        }
        .into();
    }
    let empty = matches!(op.as_str(), "HAS_PROPERTY" | "NOT_HAS_PROPERTY");
    let allowed = match p.property_type.as_str() {
        "enumeration" => empty || matches!(op.as_str(), "IN" | "NOT_IN"),
        "string" => {
            empty
                || matches!(
                    op.as_str(),
                    "EQ" | "NEQ" | "CONTAINS_TOKEN" | "NOT_CONTAINS_TOKEN"
                )
        }
        "number" => empty || matches!(op.as_str(), "EQ" | "NEQ" | "GTE" | "LTE" | "BETWEEN"),
        "date" | "datetime" => empty || matches!(op.as_str(), "GTE" | "LTE" | "BETWEEN" | "LT"),
        "bool" => matches!(op.as_str(), "EQ" | "NOT_HAS_PROPERTY"),
        _ => false,
    };
    if !allowed {
        return Err("この項目には選んだ条件を使えません。");
    }
    if empty {
        if !values.is_empty() {
            return Err("空かどうかの条件に値は指定できません。");
        }
        return Ok(json!({"propertyName":p.name,"operator":op}));
    }
    if p.property_type == "enumeration" {
        let known = |v: &String| match p.name.as_str() {
            "dealstage" => scope.stages.contains(v),
            "pipeline" => scope.pipelines.contains(v),
            _ if p.owner_ref => v.len() <= 20 && v.chars().all(|ch| ch.is_ascii_digit()),
            _ => p.options.iter().any(|o| &o.value == v),
        };
        if values.is_empty() || values.iter().any(|v| !known(v)) {
            return Err("一覧にある選択肢を 1 つ以上選んでください。");
        }
        return Ok(json!({"propertyName":p.name,"operator":op,"values":values}));
    }
    let range = op == "BETWEEN";
    if values.len() != if range { 2 } else { 1 } {
        return Err("条件の値を確認してください。");
    }
    if p.property_type == "number" {
        let ns: Vec<f64> = values
            .iter()
            .map(|v| {
                v.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or("数値を入力してください。")
            })
            .collect::<Result<_, _>>()?;
        if range && ns[0] > ns[1] {
            return Err("下限は上限以下にしてください。");
        }
    }
    if p.property_type == "bool" && !matches!(values[0].as_str(), "true" | "false") {
        return Err("はい・いいえを選んでください。");
    }
    if date {
        let ds = values
            .iter()
            .map(|v| super::call_queue::parse_date(v).ok_or("正しい日付を入力してください。"))
            .collect::<Result<Vec<_>, _>>()?;
        if range && ds[0] > ds[1] {
            return Err("開始日は終了日以前にしてください。");
        }
        values = ds
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let start = d
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight")
                    .and_utc()
                    .timestamp_millis();
                let base = if p.property_type == "datetime" {
                    start - 9 * 3600 * 1000
                } else {
                    start
                };
                let end = p.property_type == "datetime" && ((range && i == 1) || op == "LTE");
                (base + if end { 86_400_000 - 1 } else { 0 }).to_string()
            })
            .collect();
    }
    let mut out = json!({"propertyName":p.name,"operator":op,"value":values[0]});
    if range {
        out["highValue"] = json!(values[1]);
    }
    Ok(out)
}

// 旧テスト/呼び出しの互換入口。相対日付の実処理は同じ要求のnowを渡すresolveで行う。
pub fn validate(
    c: &PropertyCondition,
    def: Option<&CrmCatalogProperty>,
) -> Result<Value, &'static str> {
    resolve(c, def, Utc::now(), &ValueScope::default())
}

fn bounded(gs: Vec<Value>) -> Result<Vec<Value>, &'static str> {
    if gs.len() > 5 {
        return Err(LIMIT_MESSAGE);
    }
    let mut total = 0;
    for g in &gs {
        let n = g["filters"].as_array().ok_or(LIMIT_MESSAGE)?.len();
        if n > 6 {
            return Err(LIMIT_MESSAGE);
        }
        total += n;
    }
    if total > 18 {
        return Err(LIMIT_MESSAGE);
    }
    Ok(gs)
}
/// 既存条件 AND (追加まとまり1 OR 追加まとまり2)。Searchを分割しない。
pub fn append_groups(base: Vec<Value>, extra: &[Vec<Value>]) -> Result<Vec<Value>, &'static str> {
    if extra.is_empty() {
        return bounded(base);
    }
    if extra.len() > 3
        || extra.iter().any(|g| g.is_empty() || g.len() > 6)
        || base.len() * extra.len() > 5
    {
        return Err(LIMIT_MESSAGE);
    }
    let product = |base: &[Value]| -> Result<Vec<Value>, &'static str> {
        let mut out = Vec::new();
        for b in base {
            for e in extra {
                let mut fs = b["filters"].as_array().ok_or(LIMIT_MESSAGE)?.clone();
                fs.extend_from_slice(e);
                out.push(json!({"filters":fs}));
            }
        }
        bounded(out)
    };
    match product(&base) {
        Ok(v) => Ok(v),
        Err(_) => {
            // 架電停止の除外は従来どおり後段にも必ず適用。追加条件は削らない。
            let stripped: Vec<Value> = base
                .into_iter()
                .map(|mut b| {
                    if let Some(fs) = b["filters"].as_array_mut() {
                        fs.retain(|f| {
                            !(matches!(f["propertyName"].as_str(), Some("bpo_3" | "bpo_4"))
                                && f["operator"] == "NOT_HAS_PROPERTY")
                        });
                    }
                    b
                })
                .collect();
            product(&stripped)
        }
    }
}
pub fn append(base: Vec<Value>, extra: &[Value]) -> Result<Vec<Value>, &'static str> {
    if extra.is_empty() {
        bounded(base)
    } else {
        append_groups(base, &[extra.to_vec()])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn def(kind: &str) -> CrmCatalogProperty {
        CrmCatalogProperty {
            name: "amount".into(),
            label: "金額".into(),
            label_en: None,
            property_type: kind.into(),
            field_type: "text".into(),
            options: vec![],
            owner_ref: false,
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
            json!({"filters":[{"propertyName":"dealstage","operator":"EQ","value":"fake-stage-a"}]}),
            json!({"filters":[{"propertyName":"dealstage","operator":"EQ","value":"fake-stage-b"}]}),
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
        assert!(parse(&serde_json::to_string(&vec![c("EQ", &["1"]); 7]).unwrap()).is_err());
        assert!(
            parse(&serde_json::to_string(&vec![c("EQ", &[&"a".repeat(201)])]).unwrap()).is_err()
        );
        assert!(parse(&serde_json::to_string(&vec![c("IN", &vec!["1"; 21])]).unwrap()).is_err());
    }
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use chrono::TimeZone;
    fn prop(kind: &str) -> CrmCatalogProperty {
        CrmCatalogProperty {
            name: "sample_value".into(),
            label: "架空項目".into(),
            label_en: None,
            property_type: kind.into(),
            field_type: "text".into(),
            owner_ref: false,
            writable: false,
            options: vec![crate::handlers::crm_metadata::CrmPropertyOption {
                label: "架空選択肢".into(),
                value: "sample_option".into(),
                hidden: false,
            }],
        }
    }
    fn cond(op: &str, vs: &[&str]) -> PropertyCondition {
        PropertyCondition {
            property: "sample_value".into(),
            operator: op.into(),
            values: vs.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn resolve(
        c: &PropertyCondition,
        def: Option<&CrmCatalogProperty>,
        now: DateTime<Utc>,
    ) -> Result<Value, &'static str> {
        super::resolve(c, def, now, &ValueScope::default())
    }
    fn stage_def(name: &str, owner_ref: bool) -> CrmCatalogProperty {
        let mut p = prop("enumeration");
        p.name = name.into();
        p.owner_ref = owner_ref;
        // 全パイプライン分が混ざった選択肢が入っていても、値の範囲には使わない
        p.options[0].value = "mixed-all-pipelines".into();
        p
    }
    fn scope() -> ValueScope {
        ValueScope {
            stages: vec!["s1".into(), "s2".into()],
            pipelines: vec!["p1".into()],
        }
    }
    fn named(name: &str, op: &str, vs: &[&str]) -> PropertyCondition {
        PropertyCondition {
            property: name.into(),
            operator: op.into(),
            values: vs.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn scoped(
        name: &str,
        op: &str,
        vs: &[&str],
        def: &CrmCatalogProperty,
    ) -> Result<Value, &'static str> {
        super::resolve(&named(name, op, vs), Some(def), now(), &scope())
    }
    #[test]
    fn 取引ステージは選んだパイプラインのステージだけ受け付ける() {
        let d = stage_def("dealstage", false);
        assert_eq!(
            scoped("dealstage", "IN", &["s1", "s2"], &d).unwrap(),
            json!({"propertyName":"dealstage","operator":"IN","values":["s1","s2"]})
        );
        assert_eq!(
            scoped("dealstage", "NOT_IN", &["s2"], &d).unwrap()["operator"],
            "NOT_IN"
        );
        assert_eq!(
            scoped("dealstage", "NOT_HAS_PROPERTY", &[], &d).unwrap()["operator"],
            "NOT_HAS_PROPERTY"
        );
        // 範囲外 (別パイプラインのステージ・定義の選択肢だけにある値) は拒否
        for bad in [&["s3"][..], &["mixed-all-pipelines"], &["s1", "s3"]] {
            assert!(scoped("dealstage", "IN", bad, &d).is_err());
        }
    }
    #[test]
    fn パイプラインと担当者の値の範囲() {
        let pl = stage_def("pipeline", false);
        assert!(scoped("pipeline", "IN", &["p1"], &pl).is_ok());
        assert!(scoped("pipeline", "IN", &["p9"], &pl).is_err());
        let ow = stage_def("hubspot_owner_id", true);
        assert_eq!(
            scoped("hubspot_owner_id", "IN", &["123456"], &ow).unwrap()["values"],
            json!(["123456"])
        );
        for bad in ["abc", "12 3", "1.5"] {
            assert!(scoped("hubspot_owner_id", "IN", &[bad], &ow).is_err());
        }
    }
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 3, 15, 0, 0).unwrap()
    }
    #[test]
    fn 各型の比べ方と範囲を具体値で変換する() {
        for (kind, op, vs, expected) in [
            (
                "string",
                "NOT_CONTAINS_TOKEN",
                vec!["sample"],
                json!({"operator":"NOT_CONTAINS_TOKEN","value":"sample"}),
            ),
            (
                "string",
                "NEQ",
                vec!["sample"],
                json!({"operator":"NEQ","value":"sample"}),
            ),
            (
                "number",
                "NEQ",
                vec!["17"],
                json!({"operator":"NEQ","value":"17"}),
            ),
            (
                "number",
                "BETWEEN",
                vec!["17", "29"],
                json!({"operator":"BETWEEN","value":"17","highValue":"29"}),
            ),
            (
                "date",
                "BETWEEN",
                vec!["2026-02-03", "2026-02-04"],
                json!({"operator":"BETWEEN","value":"1770076800000","highValue":"1770163200000"}),
            ),
            (
                "bool",
                "IS_FALSE",
                vec![],
                json!({"operator":"EQ","value":"false"}),
            ),
            (
                "bool",
                "IS_TRUE",
                vec![],
                json!({"operator":"EQ","value":"true"}),
            ),
            (
                "bool",
                "NOT_HAS_PROPERTY",
                vec![],
                json!({"operator":"NOT_HAS_PROPERTY"}),
            ),
            (
                "enumeration",
                "HAS_PROPERTY",
                vec![],
                json!({"operator":"HAS_PROPERTY"}),
            ),
            (
                "enumeration",
                "NOT_HAS_PROPERTY",
                vec![],
                json!({"operator":"NOT_HAS_PROPERTY"}),
            ),
        ] {
            let mut want = expected;
            want["propertyName"] = json!("sample_value");
            assert_eq!(
                resolve(&cond(op, &vs), Some(&prop(kind)), now()).unwrap(),
                want
            );
        }
        assert!(resolve(
            &cond("BETWEEN", &["29", "17"]),
            Some(&prop("number")),
            now()
        )
        .is_err());
        assert!(resolve(
            &cond("BETWEEN", &["2026-02-04", "2026-02-03"]),
            Some(&prop("date")),
            now()
        )
        .is_err());
        assert!(resolve(&cond("EQ", &["17"]), None, now()).is_err());
        assert!(resolve(&cond("IS_TRUE", &["extra"]), Some(&prop("bool")), now()).is_err());
    }
    #[test]
    fn jstの境目と日時型の一日を解決する() {
        let before = now() - Duration::milliseconds(1);
        let today = cond("TODAY", &[]);
        let a = resolve(&today, Some(&prop("date")), before).unwrap();
        let b = resolve(&today, Some(&prop("date")), now()).unwrap();
        assert_eq!(a["value"], "1770076800000");
        assert_eq!(b["value"], "1770163200000");
        assert_eq!(b["highValue"], "1770163200000");
        let stamp = resolve(&today, Some(&prop("datetime")), now()).unwrap();
        assert_eq!(stamp["value"], now().timestamp_millis().to_string());
        assert_eq!(
            stamp["highValue"],
            (now().timestamp_millis() + 86_400_000 - 1).to_string()
        );
        for (op, lo, hi) in [
            ("LAST_N_DAYS", "2026-02-02", "2026-02-04"),
            ("NEXT_N_DAYS", "2026-02-04", "2026-02-06"),
        ] {
            assert_eq!(
                resolve(&cond(op, &["3"]), Some(&prop("date")), now()).unwrap(),
                resolve(&cond("BETWEEN", &[lo, hi]), Some(&prop("date")), now()).unwrap()
            );
        }
        let past = resolve(&cond("BEFORE_TODAY", &[]), Some(&prop("datetime")), now()).unwrap();
        assert_eq!(past["operator"], "LT");
        assert_eq!(past["value"], now().timestamp_millis().to_string());
        for n in ["0", "3651", "1.5", "NaN"] {
            assert!(resolve(&cond("LAST_N_DAYS", &[n]), Some(&prop("date")), now()).is_err());
        }
    }
    #[test]
    fn 既存条件とまたはを直積にし安全上限を全組合せで守る() {
        let a = json!({"propertyName":"sample_stage","operator":"EQ","value":"sample_a"});
        let b = json!({"propertyName":"sample_stage","operator":"EQ","value":"sample_b"});
        let x = json!({"propertyName":"sample_value","operator":"GTE","value":"17"});
        let y = json!({"propertyName":"sample_value","operator":"LTE","value":"29"});
        let out = append_groups(
            vec![json!({"filters":[a]}), json!({"filters":[b]})],
            &[vec![x.clone()], vec![y.clone()]],
        )
        .unwrap();
        assert_eq!(
            json!(out),
            json!([{"filters":[a,x]},{"filters":[a,y]},{"filters":[b,x]},{"filters":[b,y]}])
        );
        assert_eq!(
            append_groups(vec![json!({"filters":[a]}); 2], &vec![vec![x.clone()]; 3]),
            Err(LIMIT_MESSAGE)
        );
        for bases in 1..=5 {
            for required in 1..=6 {
                for gs in 1..=3 {
                    for count in 1..=6 {
                        let result = append_groups(
                            vec![json!({"filters":vec![a.clone();required]}); bases],
                            &vec![vec![x.clone(); count]; gs],
                        );
                        if let Ok(v) = result {
                            assert!(v.len() <= 5);
                            assert!(v
                                .iter()
                                .all(|g| g["filters"].as_array().unwrap().len() <= 6));
                            assert!(
                                v.iter()
                                    .map(|g| g["filters"].as_array().unwrap().len())
                                    .sum::<usize>()
                                    <= 18
                            );
                        }
                    }
                }
            }
        }
        let stop = json!({"propertyName":"bpo_3","operator":"NOT_HAS_PROPERTY"});
        let out = append_groups(
            vec![json!({"filters":[a,stop]}); 2],
            &[vec![x; 3], vec![y; 3]],
        )
        .unwrap();
        assert!(out
            .iter()
            .all(|g| g["filters"].as_array().unwrap().len() == 4));
    }
    #[test]
    fn 新旧形式と並べ替え型を制限する() {
        let c = cond("EQ", &["17"]);
        assert_eq!(
            parse(&serde_json::to_string(&vec![c.clone(); 3]).unwrap())
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            parse_groups(&serde_json::to_string(&vec![vec![c.clone(); 6]; 3]).unwrap())
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            parse_groups(&serde_json::to_string(&vec![vec![c]; 4]).unwrap()),
            Err(LIMIT_MESSAGE)
        );
        assert!(parse_groups("[[]]").is_err());
        assert!(sortable(&prop("number")));
        assert!(!sortable(&prop("json")));
        let mut multi = prop("enumeration");
        multi.field_type = "checkbox".into();
        assert!(!sortable(&multi));
    }
}
