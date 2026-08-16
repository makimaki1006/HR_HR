//! 架電クオリティ: タブ実装の置き場
//!
//! 2026-08-14。GAS 版 16ページを 1ページ = 1ファイルで移す。
//! **並列実装のため、1ファイル1担当を厳守する**（同じファイルを複数人が触らない）。
//!
//! 各タブが従う約束（ここを守らないと画面ごとに数字の意味が変わる）:
//!
//! 1. **列は名前で引く**。`SheetData::col()` を使い、位置で決め打ちしない。
//!    GAS 側で「G列を会社名と誤読して突合が全滅」した事故がある。
//!
//! 2. **分母0のときは 0 でなく `None`(null)を返す**。
//!    「架電0だからアポ率0%」と読ませない。実データに架電0でアポありの行が存在する。
//!
//! 3. **上限で切ったら `truncated` を必ず立てる**。黙って上位N件にしない。
//!
//! 4. **並びを安定させる**。HashMap の反復順をそのまま返さない。
//!
//! 5. **営業スコープの既定は role=sales**。メンバー未選択時に BPO/コンサルを
//!    混ぜない（GAS 側で 141名混在させてアポ率が 0.93%→0.63% に希釈された事故がある）。
//!
//! 6. 集計は**サーバ側**で完結させ、生データを丸ごと返さない。
//!    これが移行の目的そのもの（実測 33.1MB → 6.7KB）。

use serde::Serialize;

pub mod p0_overview;
pub mod p1_members;
pub mod p15_pipeline_mgmt;
pub mod p11_activity;
pub mod pbpo_dashboard;
pub mod p14_owner360;
pub mod p10_future_actions;
pub mod prisk_riskboard;
pub mod ptf_target;
pub mod p3_timeseries;
pub mod p13_timeline;
pub mod p12_churn;
pub mod p8_consulting_contact;
pub mod p2_habits;
pub mod pja_job_application;
pub mod p7_data_browser;

/// 各タブが返す共通の外枠。
/// 画面はこれを見て「いつのデータか」「絞り込みで何件が対象になったか」を出す。
#[derive(Debug, Serialize)]
pub struct TabPayload<T: Serialize> {
    pub data: T,
    /// このタブが読んだシートと、その行数（透明性のため必ず返す）
    pub sources: Vec<SourceInfo>,
    /// サーバ側の処理時間(ms)
    pub elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub sheet: String,
    pub total_rows: usize,
    pub matched_rows: usize,
    /// 常駐キャッシュから返したか。false なら Sheets を叩いている。
    pub from_cache: bool,
    /// 取得からの経過秒。画面の「最終更新」に使う。
    ///
    /// **GAS 版はここを「シートを読んだ時刻」として出しており、
    /// 実際のデータ生成時刻と混同させていた**。データ自体の生成時刻は
    /// 各シートの「生成時刻」列を見ること。
    pub age_secs: u64,
}

/// 分母0を 0% にしないための共通ヘルパ。
/// 各タブで書くと必ずどこかで 0 を返す実装が混ざるので1箇所に置く。
pub fn rate(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator > 0.0 {
        Some(numerator / denominator * 100.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 分母0は0パーセントでなくnone() {
        assert_eq!(rate(3.0, 0.0), None, "架電0でアポ3件を 0% と表示してはいけない");
        assert_eq!(rate(0.0, 0.0), None);
    }

    #[test]
    fn 通常の率は計算される() {
        assert_eq!(rate(1.0, 200.0), Some(0.5));
    }

    #[test]
    fn 分子0で分母ありは0パーセント() {
        // これは「本当に0%」なので None ではない
        assert_eq!(rate(0.0, 100.0), Some(0.0));
    }
}
