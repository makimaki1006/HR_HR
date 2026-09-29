//! HubSpot 画面へのリンク (Deep Link) と portal ID。
//!
//! もとは `handlers::cs_dashboard::routes` にあった portal ID の決め方をここへ移し、
//! コンタクト・会社・取引のレコードページ URL を一か所で組み立てる。

use super::types::RecordType;

/// HubSpot の portal_id の既定値（リクロジ事業部）。取引ページ
/// `https://app.hubspot.com/contacts/<portal_id>/record/0-3/<deal_id>/` の一部で、
/// 公開して困る値ではない（2026-09-28 藤巻さん確認）。
pub const HUBSPOT_PORTAL_ID_DEFAULT: &str = "23708633";

/// HubSpot の portal_id。環境変数 `HUBSPOT_PORTAL_ID` で上書きでき、
/// 未設定・空白だけなら既定値。
pub fn hubspot_portal_id() -> String {
    hubspot_portal_id_from(std::env::var("HUBSPOT_PORTAL_ID").ok().as_deref())
}

/// 環境変数の値から portal_id を決める（テストで環境変数を触らずに確かめるため分けてある）。
/// 前後の空白は落とす。
pub fn hubspot_portal_id_from(env: Option<&str>) -> String {
    env.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| HUBSPOT_PORTAL_ID_DEFAULT.to_string())
}

/// `https://app.hubspot.com/contacts/{portal}/record/{type_id}/{id}/`
pub fn record_url(portal_id: &str, record: RecordType, id: &str) -> String {
    format!(
        "https://app.hubspot.com/contacts/{portal_id}/record/{}/{id}/",
        record.type_id()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn deep_link_record_url_uses_type_id_per_record_type() {
        let p = HUBSPOT_PORTAL_ID_DEFAULT;
        assert_eq!(
            record_url(p, RecordType::Contact, "123"),
            "https://app.hubspot.com/contacts/23708633/record/0-1/123/"
        );
        assert_eq!(
            record_url(p, RecordType::Company, "456"),
            "https://app.hubspot.com/contacts/23708633/record/0-2/456/"
        );
        assert_eq!(
            record_url(p, RecordType::Deal, "789"),
            "https://app.hubspot.com/contacts/23708633/record/0-3/789/"
        );
    }

    #[test]
    fn deep_link_portal_id_from_env_value_or_default() {
        assert_eq!(HUBSPOT_PORTAL_ID_DEFAULT, "23708633");
        let custom = hubspot_portal_id_from(Some("999"));
        assert_eq!(custom, "999");
        assert_eq!(
            record_url(&custom, RecordType::Deal, "789"),
            "https://app.hubspot.com/contacts/999/record/0-3/789/"
        );
        assert_eq!(hubspot_portal_id_from(Some("  ")), "23708633");
        assert_eq!(hubspot_portal_id_from(Some("")), "23708633");
        assert_eq!(hubspot_portal_id_from(None), "23708633");
        assert_eq!(hubspot_portal_id_from(Some(" 42 ")), "42");
    }

    /// 逆証明: 型 ID の取り違え (例: Company に 0-3) があれば、ここで URL か型 ID が重なって落ちる。
    #[test]
    fn deep_link_type_ids_are_one_to_one_across_all_record_types() {
        let expected = [
            (RecordType::Contact, "0-1"),
            (RecordType::Company, "0-2"),
            (RecordType::Deal, "0-3"),
        ];
        assert_eq!(RecordType::ALL.len(), expected.len());
        for (rt, (exp_rt, exp_id)) in RecordType::ALL.iter().zip(expected.iter()) {
            assert_eq!(rt, exp_rt);
            assert_eq!(rt.type_id(), *exp_id);
        }

        let ids: HashSet<&str> = RecordType::ALL.iter().map(|r| r.type_id()).collect();
        assert_eq!(ids.len(), RecordType::ALL.len());

        let urls: HashSet<String> = RecordType::ALL
            .iter()
            .map(|r| record_url("1", *r, "7"))
            .collect();
        assert_eq!(urls.len(), RecordType::ALL.len());
        for r in RecordType::ALL {
            let url = record_url("1", r, "7");
            assert!(url.contains(&format!("/record/{}/7/", r.type_id())), "{url}");
        }
    }
}
