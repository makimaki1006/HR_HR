//! HubSpot 画面へのリンク。契約用のスタブ。実装は担当 B。

use super::types::RecordType;

/// portal ID の既定値 (リクロジ事業部)
pub const HUBSPOT_PORTAL_ID_DEFAULT: &str = "23708633";

/// `HUBSPOT_PORTAL_ID` (未設定・空白なら既定値)
pub fn hubspot_portal_id() -> String {
    todo!("B")
}

pub fn hubspot_portal_id_from(_env: Option<&str>) -> String {
    todo!("B")
}

/// `https://app.hubspot.com/contacts/{portal}/record/{type_id}/{id}/`
pub fn record_url(_portal_id: &str, _record: RecordType, _id: &str) -> String {
    todo!("B")
}
