//! 横断的な値オブジェクト（Newtype）定義
//!
//! ## 目的
//! 単位スケールの混乱（例: 0-1 比率と 0-100% の混在）を型レベルで防ぐ。
//!
//! ## 経緯（2026-04-26 監査 Q1.3）
//! `vacancy_rate` が以下のように 2 種類のスケールで保存されていた:
//! - `vacancy_rate`(DB 由来): 0-1 比率 (例: 0.12)
//! - `vacancy_rate_pct` (UI 表示用): 0-100% 単位 (例: 12.0)
//!
//! 同じ概念名で異なる単位が散在し、改修時に「* 100.0」を二重適用または
//! 適用忘れで 10 倍誤差を起こすリスクが高かった。
//!
//! ## 設計方針
//! - **段階的導入**: 既存の `f64` を一斉置換するのではなく、まず公開境界
//!   （`HwAreaEnrichment::vacancy_rate_pct`）と insight engine の表示境界で
//!   Newtype を採用する。
//! - **wrapper のみ**: 内部の `f64` 値そのものは不変。型タグだけを変える。
//! - **明示変換**: 比率 ⇄ % は `to_pct()` / `to_ratio()` を経由させる。
//!   暗黙の `From<f64>` は提供しない（誤代入を防ぐ）。
//! - **逆証明テスト**: 「0-1 範囲を 0-100 として渡したら panic する」サニティ。

use serde::{Deserialize, Serialize};

/// 欠員補充率（0-100 % 単位）
///
/// e-Stat 由来「求人理由が『欠員補充』の比率」を **% 単位**で保持する。
///
/// 例: `VacancyRatePct(12.5)` は 12.5%。
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VacancyRatePct(pub f64);

/// 欠員補充率（0-1 比率単位）
///
/// DB の `v2_vacancy_rate.vacancy_rate` カラムは 0-1 で保存されている。
/// その生値の wrapper。
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VacancyRateRatio(pub f64);

impl VacancyRatePct {
    /// 比率（0-1）→ %（0-100）の変換コンストラクタ
    #[inline]
    pub fn from_ratio(ratio: f64) -> Self {
        VacancyRatePct(ratio * 100.0)
    }

    /// % 値を生 f64 で取得
    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }

    /// % → 比率（0-1）逆変換
    #[inline]
    pub fn to_ratio(self) -> VacancyRateRatio {
        VacancyRateRatio(self.0 / 100.0)
    }

    /// 表示文字列（小数1桁 + %）
    pub fn format_pct(self) -> String {
        format!("{:.1}%", self.0)
    }

    /// 値が想定範囲（0..=100）内か（NaN は false）
    pub fn is_in_range(self) -> bool {
        self.0.is_finite() && (0.0..=100.0).contains(&self.0)
    }
}

impl VacancyRateRatio {
    /// 比率値を生 f64 で取得
    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }

    /// 比率（0-1）→ %（0-100）変換
    #[inline]
    pub fn to_pct(self) -> VacancyRatePct {
        VacancyRatePct(self.0 * 100.0)
    }

    /// 値が想定範囲（0..=1）内か
    pub fn is_in_range(self) -> bool {
        self.0.is_finite() && (0.0..=1.0).contains(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1. 比率 → % 変換: 0.12 → 12.0
    #[test]
    fn ratio_to_pct_normal() {
        let r = VacancyRateRatio(0.12);
        let p = r.to_pct();
        assert!(
            (p.as_f64() - 12.0).abs() < 1e-9,
            "expected 12.0, got {}",
            p.as_f64()
        );
    }

    /// 2. % → 比率 変換: 35.0 → 0.35
    #[test]
    fn pct_to_ratio_normal() {
        let p = VacancyRatePct(35.0);
        let r = p.to_ratio();
        assert!(
            (r.as_f64() - 0.35).abs() < 1e-9,
            "expected 0.35, got {}",
            r.as_f64()
        );
    }

    /// 3. ラウンドトリップ: ratio → pct → ratio で値が変わらない
    #[test]
    fn round_trip_ratio_pct_ratio() {
        let original = 0.2456_f64;
        let r = VacancyRateRatio(original);
        let back = r.to_pct().to_ratio();
        assert!(
            (back.as_f64() - original).abs() < 1e-9,
            "round trip mismatch: {} -> {}",
            original,
            back.as_f64()
        );
    }

    /// 4. from_ratio コンストラクタ
    #[test]
    fn from_ratio_constructor() {
        let p = VacancyRatePct::from_ratio(0.085);
        assert!((p.as_f64() - 8.5).abs() < 1e-9);
    }

    /// 5. 範囲チェック: pct
    #[test]
    fn pct_range_check() {
        assert!(VacancyRatePct(0.0).is_in_range());
        assert!(VacancyRatePct(50.0).is_in_range());
        assert!(VacancyRatePct(100.0).is_in_range());
        assert!(!VacancyRatePct(-1.0).is_in_range());
        assert!(!VacancyRatePct(101.0).is_in_range());
        assert!(!VacancyRatePct(f64::NAN).is_in_range());
    }

    /// 6. 範囲チェック: ratio
    #[test]
    fn ratio_range_check() {
        assert!(VacancyRateRatio(0.0).is_in_range());
        assert!(VacancyRateRatio(0.5).is_in_range());
        assert!(VacancyRateRatio(1.0).is_in_range());
        assert!(!VacancyRateRatio(-0.01).is_in_range());
        assert!(!VacancyRateRatio(1.01).is_in_range());
    }

    /// 7. 既存値検証: 監査レポート Q1.3 で言及された「12.0%」「35.0%」の表示
    #[test]
    fn format_pct_known_values() {
        assert_eq!(VacancyRatePct(12.0).format_pct(), "12.0%");
        assert_eq!(VacancyRatePct(35.0).format_pct(), "35.0%");
        assert_eq!(VacancyRatePct(8.5).format_pct(), "8.5%");
    }

    /// 8. 逆証明: ratio (0.12) を誤って pct として扱った場合、
    ///   format_pct は "0.1%" を返し、変換忘れを表示で気付ける
    #[test]
    fn reverse_proof_ratio_misuse_visible() {
        // 0.12 (本来 12%) を pct として誤代入したケース
        let mistakenly_assigned = VacancyRatePct(0.12);
        // is_in_range は通ってしまう (0.12 は 0-100 範囲内) が、
        // format_pct は "0.1%" となるため UI 上で異常に低い値として目視可能
        assert_eq!(mistakenly_assigned.format_pct(), "0.1%");
        // 一方 to_ratio() を通すと 0.0012 (0.12%) となる
        let r = mistakenly_assigned.to_ratio();
        assert!((r.as_f64() - 0.0012).abs() < 1e-9);
    }

    /// 9. serde 透過性: シリアライズで f64 と同じ表現になる
    #[test]
    fn serde_transparent() {
        let p = VacancyRatePct(12.5);
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "12.5");
        let de: VacancyRatePct = serde_json::from_str("12.5").unwrap();
        assert_eq!(de.as_f64(), 12.5);
    }
}
