//! コンサル支援モジュール (採用仮説ブリーフ) — フェーズA+B
//!
//! 計画書: docs/CONSULT_SUPPORT_PLAN_2026-07-10.md
//!
//! 面談前に生成できる「市場側」の分析のみを行い、コンサルが顧客との会話で
//! 検証すべき仮説・矛盾・質問・分岐を事前に整理する (§1.3)。
//! 顧客ヒアリングデータに依存する分析 (応募ファネル等) はフェーズC/Dの領域。
//!
//! ## パイプライン (§4)
//! ```text
//! ConsultInput (input.rs)
//!   → 証拠登録 (evidence.rs, E-001形式)
//!   → 4軸判定 (axes.rs, 総合点なし §8.2)
//!   → シグナル (signals.rs, 15種, 閾値は config.rs)
//!   → 矛盾検出 (contradictions.rs, 最大5件)
//!   → 仮説生成 (hypotheses.rs, 8カテゴリ・TOP5)
//!   → 質問生成 (questions.rs, 目的+分岐)
//!   → evidence_pack.json (evidence_pack.rs, §15.2形式)
//!   → ブリーフHTML (brief_html.rs, 社内用4ページ)
//! ```
//!
//! ## 規律
//! - §19.2 禁止表現は使わない。仮説は必ず「〜の可能性」
//! - 出力に SalesNow という名称・内部テーブル名を出さない
//! - 介護データ・HW系テーブルを入力に使わない (V2ルール)
//! - DB読み取りのみ (書き込みなし)

pub mod axes;
pub mod brief_html;
pub mod config;
pub mod contradictions;
pub mod evidence;
pub mod evidence_pack;
pub mod handlers;
pub mod hypotheses;
pub mod input;
pub mod questions;
pub mod signals;

#[cfg(test)]
mod golden_test;

pub use handlers::{consult_brief, consult_evidence_pack_json};
