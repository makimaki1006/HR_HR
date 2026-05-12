//! Prefecture-aware region filters preventing cross-prefecture data leakage.
//!
//! 2026-05-08 Round 2-2 (Worker 2): 数値矛盾・地域混在修正
//!
//! ## 背景 (Round 1-J 監査 / PDF2 群馬県)
//! 「群馬県」フィルタを通った PDF に「埼玉県 深谷市」のレコードが混入していた.
//! 深谷市は埼玉県の市だが、CSV の location_parser が誤って群馬県側として
//! parse したか、もしくは render 側で prefecture フィルタを適用せずに
//! by_municipality_salary を使用した結果、別県の市区町村が表示された.
//!
//! ## 同名市区町村 (要対策)
//! - 伊達市: 北海道 / 福島県
//! - 府中市: 東京都 / 広島県
//! - 同名市区町村は **必ず prefecture とのペアで区別する** 必要がある.
//!
//! ## 設計
//! - [`filter_municipalities_by_pref`] は対象都道府県の市区町村のみ抽出する.
//! - 引数 `target_pref` が空または「全国」の場合はフィルタしない (既存挙動).
//! - 同名市区町村ペアは prefecture が一致した場合のみ採用する.

use super::super::aggregator::MunicipalitySalaryAgg;

/// `by_municipality_salary` を対象都道府県の市区町村のみに絞り込む.
///
/// ## ロジック
/// - `target_pref` が空文字列 / "全国" / "すべて" の場合: 全件返す
/// - 上記以外: `m.prefecture == target_pref` のレコードのみ抽出
///
/// ## 用途
/// - region.rs::render_section_municipality_salary で表示直前に呼ぶ
/// - 群馬県フィルタ時に埼玉県の深谷市が出るのを防ぐ
pub(super) fn filter_municipalities_by_pref(
    munis: &[MunicipalitySalaryAgg],
    target_pref: &str,
) -> Vec<MunicipalitySalaryAgg> {
    if !is_pref_filter_active(target_pref) {
        return munis.to_vec();
    }
    munis
        .iter()
        .filter(|m| m.prefecture == target_pref)
        .cloned()
        .collect()
}

/// pref フィルタが有効か (空 / "全国" / "すべて" は無効扱い)
pub(super) fn is_pref_filter_active(target_pref: &str) -> bool {
    !target_pref.is_empty() && target_pref != "全国" && target_pref != "すべて"
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn muni(pref: &str, name: &str, count: usize) -> MunicipalitySalaryAgg {
        MunicipalitySalaryAgg {
            name: name.to_string(),
            prefecture: pref.to_string(),
            count,
            avg_salary: 250_000,
            median_salary: 240_000,
        }
    }

    /// 都道府県フィルタで他県の市区町村が除外される (深谷市混入事故の逆証明)
    #[test]
    fn region_filter_excludes_other_prefectures() {
        let data = vec![
            muni("群馬県", "前橋市", 30),
            muni("群馬県", "高崎市", 20),
            muni("埼玉県", "深谷市", 5), // PDF2 で群馬県集計に混入していたケース
        ];

        let filtered = filter_municipalities_by_pref(&data, "群馬県");

        assert_eq!(filtered.len(), 2, "群馬県フィルタで 2 市が残る");
        for m in &filtered {
            assert_eq!(m.prefecture, "群馬県");
            // 深谷市が混入していないことを逆証明
            assert_ne!(m.name, "深谷市", "別県の深谷市が混入してはならない");
        }
    }

    /// 同名市区町村 (伊達市: 北海道/福島県) は prefecture でペア一致したものだけ残る
    #[test]
    fn region_filter_handles_homonymous_municipalities() {
        let data = vec![
            muni("北海道", "伊達市", 10),
            muni("福島県", "伊達市", 8),
            muni("北海道", "札幌市", 50),
        ];

        let hokkaido = filter_municipalities_by_pref(&data, "北海道");
        assert_eq!(hokkaido.len(), 2);
        assert!(hokkaido.iter().all(|m| m.prefecture == "北海道"));
        assert_eq!(hokkaido.iter().filter(|m| m.name == "伊達市").count(), 1);

        let fukushima = filter_municipalities_by_pref(&data, "福島県");
        assert_eq!(fukushima.len(), 1);
        assert_eq!(fukushima[0].prefecture, "福島県");
        assert_eq!(fukushima[0].name, "伊達市");
    }

    /// 「全国」「すべて」「空文字」は pref フィルタ無効扱い
    #[test]
    fn region_filter_passes_through_when_no_target() {
        let data = vec![muni("群馬県", "前橋市", 30), muni("埼玉県", "深谷市", 5)];

        // 空文字 → 全件
        assert_eq!(filter_municipalities_by_pref(&data, "").len(), 2);
        // "全国" → 全件
        assert_eq!(filter_municipalities_by_pref(&data, "全国").len(), 2);
        // "すべて" → 全件
        assert_eq!(filter_municipalities_by_pref(&data, "すべて").len(), 2);
    }

    /// is_pref_filter_active の境界値を逆証明
    #[test]
    fn is_pref_filter_active_boundaries() {
        assert!(!is_pref_filter_active(""));
        assert!(!is_pref_filter_active("全国"));
        assert!(!is_pref_filter_active("すべて"));
        assert!(is_pref_filter_active("東京都"));
        assert!(is_pref_filter_active("群馬県"));
    }
}
