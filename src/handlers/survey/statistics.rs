//! 統計モジュール（GAS Statistics.js移植）
//! Bootstrap信頼区間、トリム平均、四分位統計

use rand::Rng;
use serde::Serialize;

// ======== Bootstrap信頼区間 ========

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BootstrapCI {
    pub lower: i64,
    pub upper: i64,
    pub bootstrap_mean: i64,
    pub sample_mean: i64,
    pub sample_size: usize,
    pub confidence_level: f64,
    pub iterations: usize,
}

/// Bootstrap法による95%信頼区間
pub fn bootstrap_confidence_interval(data: &[i64], iterations: usize) -> Option<BootstrapCI> {
    let valid: Vec<f64> = data.iter().filter(|&&v| v > 0).map(|&v| v as f64).collect();
    let n = valid.len();
    if n == 0 {
        return None;
    }

    let sample_mean = valid.iter().sum::<f64>() / n as f64;

    if n == 1 {
        return Some(BootstrapCI {
            lower: sample_mean as i64,
            upper: sample_mean as i64,
            bootstrap_mean: sample_mean as i64,
            sample_mean: sample_mean as i64,
            sample_size: 1,
            confidence_level: 0.95,
            iterations: 0,
        });
    }

    let mut rng = rand::thread_rng();
    let mut means = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let mut sum = 0.0;
        for _ in 0..n {
            let idx = rng.gen_range(0..n);
            sum += valid[idx];
        }
        means.push(sum / n as f64);
    }

    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let lower_idx = (iterations as f64 * 0.025) as usize;
    let upper_idx = (iterations as f64 * 0.975) as usize;
    let bootstrap_mean = means.iter().sum::<f64>() / means.len() as f64;

    Some(BootstrapCI {
        lower: means[lower_idx] as i64,
        upper: means[upper_idx.min(means.len() - 1)] as i64,
        bootstrap_mean: bootstrap_mean as i64,
        sample_mean: sample_mean as i64,
        sample_size: n,
        confidence_level: 0.95,
        iterations,
    })
}

// ======== トリム平均 ========

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct TrimmedMeanResult {
    pub trimmed_mean: i64,
    pub original_mean: i64,
    pub trimmed_count: usize,
    pub removed_count: usize,
    pub trim_percent: f64,
}

/// トリム平均（上下trim_percent%を除外）
pub fn trimmed_mean(data: &[i64], trim_percent: f64) -> Option<TrimmedMeanResult> {
    let mut valid: Vec<i64> = data.iter().filter(|&&v| v > 0).copied().collect();
    let n = valid.len();
    if n == 0 {
        return None;
    }

    let original_mean = valid.iter().sum::<i64>() / n as i64;

    valid.sort();
    let trim_count = (n as f64 * trim_percent) as usize;
    if n <= trim_count * 2 {
        return Some(TrimmedMeanResult {
            trimmed_mean: original_mean,
            original_mean,
            trimmed_count: n,
            removed_count: 0,
            trim_percent,
        });
    }

    let trimmed = &valid[trim_count..n - trim_count];
    let trimmed_mean = trimmed.iter().sum::<i64>() / trimmed.len() as i64;

    Some(TrimmedMeanResult {
        trimmed_mean,
        original_mean,
        trimmed_count: trimmed.len(),
        removed_count: trim_count * 2,
        trim_percent,
    })
}

// ======== 四分位統計 ========

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct QuartileStats {
    pub q1: i64,
    pub q2: i64, // median
    pub q3: i64,
    pub iqr: i64,
    pub lower_bound: i64,
    pub upper_bound: i64,
    pub outlier_count: usize,
    pub inlier_count: usize,
}

/// 四分位統計（IQR法）
pub fn quartile_stats(data: &[i64]) -> Option<QuartileStats> {
    let mut valid: Vec<i64> = data.iter().filter(|&&v| v > 0).copied().collect();
    if valid.len() < 4 {
        return None;
    }

    valid.sort();
    let n = valid.len();
    let q1 = percentile(&valid, 25.0);
    let q2 = percentile(&valid, 50.0);
    let q3 = percentile(&valid, 75.0);
    let iqr = q3 - q1;
    let lower_bound = q1 - (iqr as f64 * 1.5) as i64;
    let upper_bound = q3 + (iqr as f64 * 1.5) as i64;

    let outlier_count = valid
        .iter()
        .filter(|&&v| v < lower_bound || v > upper_bound)
        .count();

    Some(QuartileStats {
        q1,
        q2,
        q3,
        iqr,
        lower_bound,
        upper_bound,
        outlier_count,
        inlier_count: n - outlier_count,
    })
}

/// IQR 法による外れ値除外
///
/// 2026-04-24 追加: ユーザー要求「CSV読み込み時に外れ値削除」対応。
///
/// - Q1 - iqr_multiplier × IQR 未満 / Q3 + iqr_multiplier × IQR 超 を除外
/// - iqr_multiplier=1.5 が標準的 (Tukey の箱ひげ図)
/// - 件数不足 (n<4) の場合は全件通過（IQR 計算不能のため）
///
/// # Returns
/// `(filtered_values, removed_count)`
pub fn filter_outliers_iqr(values: &[i64], iqr_multiplier: f64) -> (Vec<i64>, usize) {
    let valid: Vec<i64> = values.iter().filter(|&&v| v > 0).copied().collect();
    let n = valid.len();
    if n < 4 {
        return (valid, 0);
    }
    let mut sorted = valid.clone();
    sorted.sort();
    let q1 = percentile(&sorted, 25.0);
    let q3 = percentile(&sorted, 75.0);
    let iqr = q3 - q1;
    if iqr <= 0 {
        return (valid, 0); // IQR=0 は全部同値に近く除外不要
    }
    let margin = (iqr as f64 * iqr_multiplier) as i64;
    let lower = q1 - margin;
    let upper = q3 + margin;
    let filtered: Vec<i64> = valid
        .iter()
        .filter(|&&v| v >= lower && v <= upper)
        .copied()
        .collect();
    let removed = valid.len() - filtered.len();
    (filtered, removed)
}

#[cfg(test)]
mod outlier_tests {
    use super::*;

    #[test]
    fn filter_removes_high_outlier() {
        // 200-300 のレンジに 1000 の外れ値
        let data = vec![200, 220, 240, 250, 260, 280, 300, 1000];
        let (filtered, removed) = filter_outliers_iqr(&data, 1.5);
        assert_eq!(removed, 1);
        assert!(!filtered.contains(&1000));
        assert_eq!(filtered.len(), 7);
    }

    #[test]
    fn filter_keeps_normal_data() {
        let data = vec![200, 220, 240, 260, 280, 300];
        let (filtered, removed) = filter_outliers_iqr(&data, 1.5);
        assert_eq!(removed, 0);
        assert_eq!(filtered.len(), 6);
    }

    #[test]
    fn filter_small_sample_passes_through() {
        let data = vec![100, 1_000_000]; // n<4
        let (filtered, removed) = filter_outliers_iqr(&data, 1.5);
        assert_eq!(removed, 0);
        assert_eq!(filtered, data);
    }

    // 2026-04-26 Fix-A 逆証明テスト: IQR は両側適用 (Q1-1.5×IQR / Q3+1.5×IQR の両端で除外)
    // notes.rs / executive_summary.rs / employment.rs のドキュメント文言と整合。
    #[test]
    fn fixa_iqr_filter_removes_low_outlier_proves_two_sided() {
        // 修正前: 「下側のみ」と説明されたケースでも実装は両側 → ドキュメントとコードの不一致疑義
        // 修正後: 両側適用がコード上の事実であることを逆証明
        // 200-300 のレンジに 1 円の下側外れ値 → 除外されることを assert
        let data = vec![1, 200, 220, 240, 250, 260, 280, 300];
        let (filtered, removed) = filter_outliers_iqr(&data, 1.5);
        assert_eq!(removed, 1, "下側 1 円は両側 IQR で除外される");
        assert!(!filtered.contains(&1), "下側外れ値が残ってはいけない");
    }

    #[test]
    fn fixa_iqr_filter_removes_both_sides_simultaneously() {
        // 上下両端に外れ値がある場合、両方除外される
        let data = vec![1, 200, 220, 240, 250, 260, 280, 300, 99_999_999];
        let (filtered, removed) = filter_outliers_iqr(&data, 1.5);
        assert_eq!(removed, 2, "上下両側の外れ値を同時除外");
        assert!(!filtered.contains(&1));
        assert!(!filtered.contains(&99_999_999));
    }
}

fn percentile(sorted: &[i64], p: f64) -> i64 {
    let idx = (sorted.len() as f64 - 1.0) * p / 100.0;
    let lower = idx.floor() as usize;
    let upper = idx.ceil() as usize;
    if lower == upper || upper >= sorted.len() {
        sorted[lower.min(sorted.len() - 1)]
    } else {
        let frac = idx - lower as f64;
        (sorted[lower] as f64 * (1.0 - frac) + sorted[upper] as f64 * frac) as i64
    }
}

// ======== 統合統計 ========

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct EnhancedStats {
    pub count: usize,
    pub mean: i64,
    pub median: i64,
    pub min: i64,
    pub max: i64,
    pub std_dev: i64,
    pub bootstrap_ci: Option<BootstrapCI>,
    pub trimmed_mean: Option<TrimmedMeanResult>,
    pub quartiles: Option<QuartileStats>,
    pub reliability: String,
}

/// 統合的な給与統計
pub fn enhanced_salary_statistics(values: &[i64]) -> Option<EnhancedStats> {
    let valid: Vec<i64> = values.iter().filter(|&&v| v > 0).copied().collect();
    let n = valid.len();
    if n == 0 {
        return None;
    }

    let mean = valid.iter().sum::<i64>() / n as i64;
    let mut sorted = valid.clone();
    sorted.sort();
    let median = if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2
    } else {
        sorted[n / 2]
    };
    let min = sorted[0];
    let max = sorted[n - 1];

    let variance = valid
        .iter()
        .map(|&v| ((v - mean) as f64).powi(2))
        .sum::<f64>()
        / n as f64;
    let std_dev = variance.sqrt() as i64;

    let bootstrap_ci = if n >= 5 {
        bootstrap_confidence_interval(&valid, 2000)
    } else {
        None
    };
    let trimmed = if n >= 10 {
        trimmed_mean(&valid, 0.1)
    } else {
        None
    };
    let quartiles = quartile_stats(&valid);

    let reliability = match n {
        n if n >= 30 => "high",
        n if n >= 10 => "medium",
        n if n >= 5 => "low",
        _ => "very_low",
    };

    Some(EnhancedStats {
        count: n,
        mean,
        median,
        min,
        max,
        std_dev,
        bootstrap_ci,
        trimmed_mean: trimmed,
        quartiles,
        reliability: reliability.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap() {
        let data = vec![
            200_000, 250_000, 230_000, 280_000, 260_000, 240_000, 270_000,
        ];
        let ci = bootstrap_confidence_interval(&data, 1000).unwrap();
        assert!(ci.lower < ci.upper);
        assert!(ci.lower >= 200_000);
        assert!(ci.upper <= 300_000);
        assert_eq!(ci.sample_size, 7);
    }

    #[test]
    fn test_trimmed_mean() {
        let data = vec![
            100_000, 200_000, 250_000, 260_000, 270_000, 280_000, 290_000, 300_000, 350_000,
            500_000,
        ];
        let tm = trimmed_mean(&data, 0.1).unwrap();
        // 外れ値100kと500kが除外されるのでtrimmed_meanはoriginalより中央寄り
        assert!(tm.trimmed_mean > tm.original_mean - 50_000);
        assert_eq!(tm.removed_count, 2);
    }

    #[test]
    fn test_quartile() {
        let data = vec![
            200_000, 220_000, 250_000, 260_000, 280_000, 300_000, 350_000, 400_000,
        ];
        let qs = quartile_stats(&data).unwrap();
        assert!(qs.q1 < qs.q2);
        assert!(qs.q2 < qs.q3);
        assert!(qs.iqr > 0);
    }

    #[test]
    fn test_enhanced() {
        let data = vec![200_000, 250_000, 230_000, 280_000, 260_000];
        let stats = enhanced_salary_statistics(&data).unwrap();
        assert_eq!(stats.count, 5);
        assert_eq!(stats.reliability, "low");
    }

    // ======== エッジケース ========

    #[test]
    fn test_bootstrap_single() {
        let data = vec![250_000];
        let ci = bootstrap_confidence_interval(&data, 100).unwrap();
        assert_eq!(ci.lower, ci.upper);
        assert_eq!(ci.sample_size, 1);
    }

    #[test]
    fn test_bootstrap_two() {
        let data = vec![200_000, 300_000];
        let ci = bootstrap_confidence_interval(&data, 500).unwrap();
        assert!(ci.lower >= 200_000);
        assert!(ci.upper <= 300_000);
    }

    #[test]
    fn test_trimmed_mean_small() {
        let data = vec![100_000, 200_000];
        let tm = trimmed_mean(&data, 0.1).unwrap();
        // n=2でtrim=0なので通常平均と同じ
        assert_eq!(tm.trimmed_mean, 150_000);
        assert_eq!(tm.removed_count, 0);
    }

    #[test]
    fn test_quartile_minimum() {
        let data = vec![100, 200, 300, 400];
        let qs = quartile_stats(&data).unwrap();
        assert!(qs.q1 <= qs.q2);
        assert!(qs.q2 <= qs.q3);
    }

    #[test]
    fn test_quartile_too_few() {
        let data = vec![100, 200, 300];
        assert!(quartile_stats(&data).is_none());
    }

    #[test]
    fn test_empty_data() {
        assert!(bootstrap_confidence_interval(&[], 100).is_none());
        assert!(trimmed_mean(&[], 0.1).is_none());
        assert!(quartile_stats(&[]).is_none());
        assert!(enhanced_salary_statistics(&[]).is_none());
    }

    #[test]
    fn test_zeros_filtered() {
        let data = vec![0, 0, 250_000, 300_000];
        let ci = bootstrap_confidence_interval(&data, 100).unwrap();
        assert_eq!(ci.sample_size, 2); // 0は除外
    }
}
