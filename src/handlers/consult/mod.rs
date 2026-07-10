//! コンサル支援モジュール (商談準備レポート = 内部識別子は brief のまま) — フェーズA+B
//!
//! 計画書: docs/CONSULT_SUPPORT_PLAN_2026-07-10.md
//!
//! 表示名は「商談準備レポート」(社内用)。プログラム内部の識別子・ルート (/consult/brief)・
//! ファイル名 (brief_html.rs 等) は互換のため変更しない。
//!
//! 面談前に生成できる「市場側」の分析のみを行い、コンサルが顧客との会話で
//! 検証すべき仮説・矛盾・質問・分岐を事前に整理する (§1.3)。
//! 顧客ヒアリングデータに依存する分析 (応募ファネル等) はフェーズC/Dの領域。
//!
//! ## パイプライン (§4)
//! ```text
//! ConsultInput (input.rs, 公的統計+媒体CSV+企業DBを幅広く投入)
//!   → 証拠登録 (evidence.rs, E-001形式)
//!   → 4軸判定 (axes.rs, 総合点なし §8.2)
//!   → シグナル (signals.rs, 30種, 閾値は config.rs)
//!   → 矛盾検出 (contradictions.rs, 最大10件)
//!   → 仮説生成 (hypotheses.rs, 8カテゴリ・複数経路・TOP5)
//!   → 質問生成 (questions.rs, 目的+分岐)
//!   → evidence_pack.json (evidence_pack.rs, §15.2形式)
//!   → AI文章化 (ai.rs, Gemini。一文要約 + 複合考察。検証つき graceful degradation)
//!   → 商談準備レポートHTML (brief_html.rs, 社内用 最大8ページ)
//! ```
//!
//! ## 規律
//! - §19.2 禁止表現は使わない。仮説は必ず「〜の可能性」
//! - 出力に SalesNow という名称・内部テーブル名を出さない
//! - 介護データ・HW求人 (求人スクレイピング・時系列・介護需要) を入力に使わない (V2ルール)
//! - LLM は文章化のみ。数値計算・集計・閾値判定はコード側で確定 (§18)
//! - DB読み取りのみ (書き込みなし)

pub mod ai;
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

#[cfg(test)]
mod hw_audit_test;

pub use handlers::{consult_brief, consult_evidence_pack_json};
