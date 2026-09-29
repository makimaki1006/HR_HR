//! 採用診断 API のレスポンス型の共通部品 (Phase 1A-1, 2026-09-29)
//!
//! 9 本の API は以前 `json!()` で応答を組み立てていた。React 画面 (`/app/recruitment-diag`) へ
//! TypeScript 型を渡すため、応答を `#[derive(Serialize, TS)]` の struct に置き換えている。
//! 置き換えの前後で JSON は 1 バイトも変えない (キー順・数値の整数/小数の別を含む)。
//!
//! 命名規則:
//! - TS に出る型はすべて `Rd` で始める (`frontend/src/generated/` は全画面で共有するため)。
//! - 成功時の本体は `Rd{Panel}Response`、パネル固有のエラー本体は `Rd{Panel}Error`。
//! - ハンドラの戻り値は `Json<Rd{Panel}Result>`。`Rd{Panel}Result` は `#[serde(untagged)]` の
//!   enum で、TS では `Rd{Panel}Response | Rd{Panel}Error` になる。フロントは `"error" in body`
//!   で判別する (エラーも HTTP 200 で返す既存挙動のまま)。
//! - `i64` / `u64` は TS で `number` にする (`ts_config()` の `with_large_int`)。
//!   件数・人口はどれも 2^53 未満。

use serde::Serialize;
use ts_rs::TS;

use super::{CAUSATION_NOTE, HW_SCOPE_NOTE};

/// `notes` が `hw_scope` と `causation` の 2 キーだけのときの型。
/// キーを足すパネルは自前の `Rd{Panel}Notes` を持つ。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdNotes {
    /// HW 掲載求人のみが対象である旨 (`HW_SCOPE_NOTE`)。
    pub hw_scope: String,
    /// 相関であって因果ではない旨 (`CAUSATION_NOTE`)。
    pub causation: String,
}

impl RdNotes {
    pub fn standard() -> Self {
        Self {
            hw_scope: HW_SCOPE_NOTE.to_string(),
            causation: CAUSATION_NOTE.to_string(),
        }
    }
}

/// `{"error": ..., "notes": {"hw_scope", "causation"}}` の形のエラー本体
/// (Panel 1〜3 の `handlers.rs::error_body`)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdErrorResponse {
    pub error: String,
    pub notes: RdNotes,
}

impl RdErrorResponse {
    pub fn new(msg: &str) -> Self {
        Self {
            error: msg.to_string(),
            notes: RdNotes::standard(),
        }
    }
}

/// TS 型の書き出し設定。`i64` / `u64` を `number` にする。
#[cfg(test)]
pub(crate) fn ts_config() -> ts_rs::Config {
    ts_rs::Config::new().with_large_int("number")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rd_error_response_matches_legacy_json() {
        let legacy = serde_json::json!({
            "error": "test",
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
            },
        });
        let new = serde_json::to_string(&RdErrorResponse::new("test")).unwrap();
        assert_eq!(new, serde_json::to_string(&legacy).unwrap());
    }

    /// 採用診断の TS 型を `frontend/src/generated/` に書き出す。依存する型も一緒に出る。
    /// 型を増やしたらここに 1 行足す。
    #[test]
    fn export_rd_ts_bindings() {
        let out_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/generated");
        let cfg = ts_config().with_out_dir(&out_dir);
        RdErrorResponse::export_all(&cfg).expect("RdErrorResponse");
    }
}
