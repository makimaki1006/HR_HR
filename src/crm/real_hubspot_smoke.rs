//! 本物の HubSpot での読み取り確認 (手動実行のみ。既定では `#[ignore]`)。
//!
//! 2026-10-01 ユーザー承認: 読み取り (GET と読み取り用の batch read) だけ。書き込み系は呼ばない。
//! 鍵は環境変数 `HUBSPOT_ACCESS_TOKEN` で渡す。**値・個人情報は出力しない**
//! (出すのはキー名・件数・型・一致したかどうかだけ)。
//!
//! 実行:
//! `HUBSPOT_ACCESS_TOKEN=... cargo test --lib crm::real_hubspot_smoke -- --ignored --nocapture`
//!
//! 呼び出し回数: 探索 1 + 応答形 1 + レコード 3 件 × (最大 6 + 取り直し 1 + 比較 1) ≒ 26 回以内。
//! 既存バッチと 100 req/10 秒を共有しているので、レコードの間で 3 秒空ける。

use std::time::Duration;

use serde_json::Value;

use super::routes::{build_record_view, record_properties};
use crate::hubspot::{ClientOptions, HubSpotClient, RecordType, DEFAULT_BASE_URL};

fn shape(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Number(_) => "number".into(),
        Value::String(_) => "string".into(),
        Value::Array(a) => format!("array(len={})", a.len()),
        Value::Object(o) => format!("object(keys={:?})", o.keys().collect::<Vec<_>>()),
    }
}

async fn raw_get(http: &reqwest::Client, token: &str, path_and_query: &str) -> (u16, Value) {
    let resp = http
        .get(format!("{DEFAULT_BASE_URL}{path_and_query}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("HubSpot に接続できない");
    let status = resp.status().as_u16();
    let body = resp.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

fn first_assoc_id(v: &Value, key: &str) -> Option<String> {
    v["associations"][key]["results"]
        .as_array()?
        .first()?
        .get("id")?
        .as_str()
        .map(str::to_string)
}

#[tokio::test]
#[ignore = "manual: reads the real HubSpot (needs HUBSPOT_ACCESS_TOKEN)"]
async fn real_hubspot_read_smoke() {
    let Some(token) = std::env::var("HUBSPOT_ACCESS_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
    else {
        println!("HUBSPOT_ACCESS_TOKEN 未設定のため何もしない");
        return;
    };
    let token = token.trim().to_string();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();

    // 1) 探索: contact と company が両方付いた deal を 1 件 (GET 1 回)
    let (st, list) = raw_get(
        &http,
        &token,
        "/crm/v3/objects/deals?limit=20&properties=dealname&associations=contacts,companies",
    )
    .await;
    println!("[discover] status={st}");
    assert_eq!(st, 200, "deal 一覧の取得に失敗");
    let deal = list["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| {
            first_assoc_id(d, "contacts").is_some() && first_assoc_id(d, "companies").is_some()
        })
        .expect("contact と company が両方付いた deal が先頭 20 件に無い")
        .clone();
    let deal_id = deal["id"].as_str().unwrap().to_string();
    let contact_id = first_assoc_id(&deal, "contacts").unwrap();
    let company_id = first_assoc_id(&deal, "companies").unwrap();

    // 2) v3 GET ?associations= の応答形 (値は出さず、キー名と型だけ) (GET 1 回)
    let (st, raw) = raw_get(
        &http,
        &token,
        &format!(
            "/crm/v3/objects/deals/{deal_id}?properties=dealname&associations=contacts,companies,calls,notes,tasks,meetings"
        ),
    )
    .await;
    println!("[shape] status={st} top={}", shape(&raw));
    if let Some(assocs) = raw["associations"].as_object() {
        for (k, a) in assocs {
            let first = &a["results"][0];
            println!(
                "[shape] associations.{k}: {} results[0]={} id={} paging={}",
                shape(a),
                shape(first),
                shape(&first["id"]),
                shape(&a["paging"])
            );
        }
    }

    // 3) 本物のコード経路で 3 件読み、生の GET と property を突き合わせる
    let client = HubSpotClient::new(token.clone(), DEFAULT_BASE_URL, ClientOptions::default())
        .expect("client");
    for (rt, id) in [
        (RecordType::Deal, deal_id.as_str()),
        (RecordType::Contact, contact_id.as_str()),
        (RecordType::Company, company_id.as_str()),
    ] {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let view = match build_record_view(&client, rt, id, "0").await {
            Ok(v) => serde_json::to_value(v).unwrap(),
            Err(e) => {
                println!("[{}] error_kind={}", rt.as_str(), e.error_kind());
                continue;
            }
        };
        let props = view["properties"].as_object().unwrap();
        let non_null = props.values().filter(|v| !v.is_null()).count();
        let assoc_counts: Vec<String> = ["contacts", "companies", "deals"]
            .iter()
            .filter_map(|k| {
                view["associations"][*k]
                    .as_array()
                    .map(|a| format!("{k}={}", a.len()))
            })
            .collect();
        let acts = view["recent_activities"].as_array().unwrap();
        let mut by_type = std::collections::BTreeMap::<String, usize>::new();
        let mut via_other = 0;
        for a in acts {
            *by_type
                .entry(a["type"].as_str().unwrap().to_string())
                .or_default() += 1;
            if a["via"]["object_type"] != rt.as_str() {
                via_other += 1;
            }
        }
        let ts: Vec<Option<String>> = acts
            .iter()
            .map(|a| a["timestamp"].as_str().map(str::to_string))
            .collect();
        println!(
            "[{}] props={} non_null={} assoc=[{}] activities={} by_type={:?} via_other={} partial={} truncated={} ts_samples={:?}",
            rt.as_str(),
            props.len(),
            non_null,
            assoc_counts.join(","),
            acts.len(),
            by_type,
            via_other,
            view["meta"]["partial"],
            view["meta"]["activities_truncated"],
            // 時刻の形式だけ (値の並びを確かめるため。個人情報ではない)
            ts.iter().take(3).collect::<Vec<_>>()
        );

        // 生の GET (関連なし) と property の値が一致するか (値そのものは出さない)
        let q = record_properties(rt).join(",");
        let (st, raw) = raw_get(
            &http,
            &token,
            &format!("/crm/v3/objects/{}/{id}?properties={q}", rt.api_name()),
        )
        .await;
        let raw_props = raw["properties"].as_object().cloned().unwrap_or_default();
        let mut mismatched: Vec<&str> = Vec::new();
        for k in record_properties(rt) {
            let a = props.get(*k).cloned().unwrap_or(Value::Null);
            let b = raw_props.get(*k).cloned().unwrap_or(Value::Null);
            if a != b {
                mismatched.push(k);
            }
        }
        println!(
            "[{}] raw_status={st} compared={} mismatched={:?}",
            rt.as_str(),
            record_properties(rt).len(),
            mismatched
        );
    }
}
