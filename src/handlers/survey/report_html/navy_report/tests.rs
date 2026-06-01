//! navy_report unit tests (A1 Commit 8 で分離)
//!
//! 元 mod.rs の末尾 `#[cfg(test)] mod tests` を物理移動。
//! 74 test cases / pure 内部関数の境界値・silent fallback 等を防御。
//!
//! `super::*` で従来通り unqualified に参照する（mod.rs 内の
//! `pub(super) use ...` で各 section から再エクスポート済み）。

#![cfg(test)]
#![allow(unused_imports)]

use super::*;

// ---- severity_label: 全 case 網羅 (silent fallback 検証) ----
#[test]
fn severity_label_pos_returns_pos() {
    assert_eq!(severity_label("pos"), "POS");
}
#[test]
fn severity_label_warn_returns_warn() {
    assert_eq!(severity_label("warn"), "WARN");
}
#[test]
fn severity_label_neg_returns_neg() {
    assert_eq!(severity_label("neg"), "NEG");
}
#[test]
fn severity_label_unknown_returns_neu_default() {
    // silent fallback: 未知 tag は NEU。`_` arm 仕様確認
    assert_eq!(severity_label(""), "NEU");
    assert_eq!(severity_label("info"), "NEU");
    assert_eq!(severity_label("critical"), "NEU");
}

// ---- format_mm: 万円換算境界値 ----
#[test]
fn format_mm_zero_returns_zero_point_zero() {
    assert_eq!(format_mm(0), "0.0");
}
#[test]
fn format_mm_10000_returns_one_point_zero() {
    assert_eq!(format_mm(10_000), "1.0");
}
#[test]
fn format_mm_250000_returns_25_point_zero() {
    // 月給 25 万円 (中央値想定)
    assert_eq!(format_mm(250_000), "25.0");
}
#[test]
fn format_mm_negative_does_not_panic() {
    // 負値も format するだけ (panic 防御確認)
    assert_eq!(format_mm(-10_000), "-1.0");
}

// ---- fmt_ratio / fmt_pct / fmt_pct_from_ratio: Option<f64> フォーマット ----
#[test]
fn fmt_ratio_some_formats_two_decimals() {
    assert_eq!(fmt_ratio(Some(1.234)), "1.23");
}
#[test]
fn fmt_ratio_none_returns_em_dash() {
    // データ不在は明示的に「—」(silent fallback 防御)
    assert_eq!(fmt_ratio(None), "—");
}
#[test]
fn fmt_pct_some_formats_one_decimal_with_percent() {
    assert_eq!(fmt_pct(Some(33.456)), "33.5%");
}
#[test]
fn fmt_pct_none_returns_em_dash() {
    assert_eq!(fmt_pct(None), "—");
}
#[test]
fn fmt_pct_from_ratio_some_multiplies_by_100() {
    // 0-1 ratio を 0-100% に変換
    assert_eq!(fmt_pct_from_ratio(Some(0.5)), "50.0");
    assert_eq!(fmt_pct_from_ratio(Some(0.123)), "12.3");
}
#[test]
fn fmt_pct_from_ratio_none_returns_em_dash() {
    assert_eq!(fmt_pct_from_ratio(None), "—");
}

// ---- compute_distribution_stats: 統計計算の境界 ----
#[test]
fn compute_distribution_stats_empty_returns_none() {
    assert!(compute_distribution_stats(&[], 10_000).is_none());
}
#[test]
fn compute_distribution_stats_all_zero_returns_none() {
    // 全 0 / 負値は filter で除外 → 空配列 → None
    assert!(compute_distribution_stats(&[0, 0, -100], 10_000).is_none());
}
#[test]
fn compute_distribution_stats_single_value_returns_stats() {
    let stats =
        compute_distribution_stats(&[250_000], 10_000).expect("single value should yield stats");
    assert_eq!(stats.n, 1);
    assert_eq!(stats.median, 250_000);
    assert_eq!(stats.min, 250_000);
    assert_eq!(stats.max, 250_000);
    assert_eq!(stats.mean, 250_000);
}
#[test]
fn compute_distribution_stats_multiple_values_invariants() {
    // ドメイン不変条件:
    //   min <= p25 <= median <= p75 <= p90 <= max
    //   n == values の正値件数
    let values: Vec<i64> = vec![
        200_000, 220_000, 250_000, 280_000, 300_000, 350_000, 400_000,
    ];
    let stats =
        compute_distribution_stats(&values, 10_000).expect("non-empty positive should yield stats");
    assert_eq!(stats.n, values.len());
    assert!(stats.min <= stats.p25, "min <= p25");
    assert!(stats.p25 <= stats.median, "p25 <= median");
    assert!(stats.median <= stats.p75, "median <= p75");
    assert!(stats.p75 <= stats.p90, "p75 <= p90");
    assert!(stats.p90 <= stats.max, "p90 <= max");
    assert!(!stats.bins.is_empty(), "bins must be non-empty");
    assert_eq!(stats.bin_step, 10_000, "bin_step is fixed 10,000 yen");
}
#[test]
fn compute_distribution_stats_filters_negative_and_zero() {
    // 負値 / 0 は filter (> 0 のみ採用)
    let values: Vec<i64> = vec![0, -100, 200_000, 300_000];
    let stats = compute_distribution_stats(&values, 10_000)
        .expect("two positive values should yield stats");
    assert_eq!(stats.n, 2, "negative / zero are filtered out");
    assert_eq!(stats.min, 200_000);
    assert_eq!(stats.max, 300_000);
}

// ============================================================
// Ext-6 (2026-05-28): compute_distribution_stats の n=1/2/5/100 全網羅
//   既存テストは「ある程度」の不変条件カバーだが、n の極端値 (1) と
//   大規模 (100) で 25%/50%/75%/90% 分位の順序関係 (min ≤ p25 ≤ median ≤ p75 ≤ p90 ≤ max)
//   が常に成立することを明示的に検証。
//
//   - n=1: 全分位 = 唯一値
//   - n=2: pct(0.25)=v[0], pct(0.50)=v[1] (=> p25 < median 可)
//   - n=5: 既存パターンの中間
//   - n=100: 大規模、ヒストグラム bins が複数生成される
// ============================================================

#[test]
fn compute_distribution_stats_invariants_n1() {
    let stats = compute_distribution_stats(&[250_000], 10_000).expect("n=1 yields stats");
    assert_eq!(stats.n, 1);
    // n=1 では全分位が唯一の値と一致 (順序不変条件は退化的に成立)
    assert!(stats.min <= stats.p25 && stats.p25 <= stats.median);
    assert!(stats.median <= stats.p75 && stats.p75 <= stats.p90);
    assert!(stats.p90 <= stats.max);
    assert_eq!(stats.min, stats.max, "n=1 で min == max");
    assert_eq!(stats.median, 250_000);
}

#[test]
fn compute_distribution_stats_invariants_n2() {
    // n=2: pct(p) = v[round((n-1)*p)] = v[round(p)] なので
    //   p25 → v[round(0.25)]=v[0]=100k, median → v[round(0.5)]=v[1]=200k (round half-to-even),
    //   p75 → v[round(0.75)]=v[1]=200k, p90 → v[round(0.90)]=v[1]=200k
    let stats = compute_distribution_stats(&[100_000, 200_000], 10_000).expect("n=2 yields stats");
    assert_eq!(stats.n, 2);
    assert_eq!(stats.min, 100_000);
    assert_eq!(stats.max, 200_000);
    assert!(
        stats.min <= stats.p25,
        "min({}) <= p25({})",
        stats.min,
        stats.p25
    );
    assert!(
        stats.p25 <= stats.median,
        "p25({}) <= median({})",
        stats.p25,
        stats.median
    );
    assert!(
        stats.median <= stats.p75,
        "median({}) <= p75({})",
        stats.median,
        stats.p75
    );
    assert!(
        stats.p75 <= stats.p90,
        "p75({}) <= p90({})",
        stats.p75,
        stats.p90
    );
    assert!(
        stats.p90 <= stats.max,
        "p90({}) <= max({})",
        stats.p90,
        stats.max
    );
}

#[test]
fn compute_distribution_stats_invariants_n5() {
    // n=5: 既存テストの中間ケースを切り出して順序不変条件のみ確認
    let stats = compute_distribution_stats(&[150_000, 200_000, 250_000, 300_000, 400_000], 10_000)
        .expect("n=5 yields stats");
    assert_eq!(stats.n, 5);
    assert!(
        stats.min <= stats.p25,
        "min({}) <= p25({})",
        stats.min,
        stats.p25
    );
    assert!(
        stats.p25 <= stats.median,
        "p25({}) <= median({})",
        stats.p25,
        stats.median
    );
    assert!(
        stats.median <= stats.p75,
        "median({}) <= p75({})",
        stats.median,
        stats.p75
    );
    assert!(
        stats.p75 <= stats.p90,
        "p75({}) <= p90({})",
        stats.p75,
        stats.p90
    );
    assert!(
        stats.p90 <= stats.max,
        "p90({}) <= max({})",
        stats.p90,
        stats.max
    );
    // n=5 で min/max は端点
    assert_eq!(stats.min, 150_000);
    assert_eq!(stats.max, 400_000);
}

#[test]
fn compute_distribution_stats_invariants_n100() {
    // n=100: 大規模ケース。均等分布で 100k〜1.1M。
    // 順序不変条件 + bins.len() > 1 (複数 bin 生成) を確認。
    let values: Vec<i64> = (0..100).map(|i| 100_000 + i * 10_000).collect();
    let stats = compute_distribution_stats(&values, 10_000).expect("n=100 yields stats");
    assert_eq!(stats.n, 100);
    assert!(
        stats.min <= stats.p25,
        "min({}) <= p25({})",
        stats.min,
        stats.p25
    );
    assert!(
        stats.p25 <= stats.median,
        "p25({}) <= median({})",
        stats.p25,
        stats.median
    );
    assert!(
        stats.median <= stats.p75,
        "median({}) <= p75({})",
        stats.median,
        stats.p75
    );
    assert!(
        stats.p75 <= stats.p90,
        "p75({}) <= p90({})",
        stats.p75,
        stats.p90
    );
    assert!(
        stats.p90 <= stats.max,
        "p90({}) <= max({})",
        stats.p90,
        stats.max
    );
    assert!(
        stats.bins.len() >= 2,
        "n=100 で bin が複数生成されるはず: bins.len()={}",
        stats.bins.len()
    );
    assert_eq!(stats.bin_step, 10_000, "bin_step 固定");
    // 平均: (100k + 1,090k) / 2 = 595k (sum / n)
    let expected_mean: i64 = values.iter().sum::<i64>() / 100;
    assert_eq!(stats.mean, expected_mean);
}

// ============================================================
// P1-6 (2026-05-28): compute_skew_severity 偏り判定境界値テスト
// ------------------------------------------------------------
// 検証範囲:
//   1. 空入力 → NEU "{label}データなし"
//   2. total <= 0 → NEU "{label}データなし" (全件 0 や負値)
//   3. 単一カテゴリ 100% → WARN 顕著
//   4. 上位 75% / 残り 25% → NEU 偏りあり
//   5. 上位 50% / 残り 50% → POS バランス良好
//   6. 境界値: 70.0% ちょうど → POS (strict >)
//              70.01% → NEU
//              85.0% ちょうど → NEU (strict >)
//              85.01% → WARN
// ============================================================

#[test]
fn compute_skew_severity_empty_returns_neu_no_data() {
    let (sev, msg) = compute_skew_severity(&[], "産業大分類");
    assert_eq!(sev, "neu");
    assert_eq!(msg, "産業大分類データなし");
}

#[test]
fn compute_skew_severity_total_zero_returns_neu_no_data() {
    // total <= 0 ガード: 全件 0 の場合
    let counts = vec![("A".to_string(), 0i64), ("B".to_string(), 0i64)];
    let (sev, msg) = compute_skew_severity(&counts, "職種");
    assert_eq!(sev, "neu");
    assert_eq!(msg, "職種データなし");
}

#[test]
fn compute_skew_severity_single_category_returns_warn() {
    // 1 カテゴリのみ → 100% → WARN
    let counts = vec![("医療,福祉".to_string(), 1000i64)];
    let (sev, msg) = compute_skew_severity(&counts, "産業大分類");
    assert_eq!(sev, "warn", "100% は WARN (> 85%)");
    assert!(msg.contains("顕著"), "msg={}", msg);
    assert!(msg.contains("100.0%"), "msg={}", msg);
    assert!(msg.contains("医療,福祉"), "msg={}", msg);
    assert!(msg.contains("サンプル代表性"), "msg={}", msg);
}

#[test]
fn compute_skew_severity_75_pct_returns_neu_skewed() {
    // 上位 75% (=750/1000) / 残り 25% → NEU 偏りあり
    let counts = vec![
        ("医療,福祉".to_string(), 750i64),
        ("製造業".to_string(), 250i64),
    ];
    let (sev, msg) = compute_skew_severity(&counts, "産業大分類");
    assert_eq!(sev, "neu", "75% は NEU (70% < 75 <= 85)");
    assert!(msg.contains("偏りあり"), "msg={}", msg);
    assert!(msg.contains("75.0%"), "msg={}", msg);
    assert!(msg.contains("データ代表性に注意"), "msg={}", msg);
}

#[test]
fn compute_skew_severity_50_pct_returns_pos_balanced() {
    // 上位 50% / 残り 50% → POS バランス良好
    let counts = vec![
        ("看護師".to_string(), 500i64),
        ("介護職".to_string(), 500i64),
    ];
    let (sev, msg) = compute_skew_severity(&counts, "職種");
    assert_eq!(sev, "pos", "50% は POS (<= 70%)");
    assert!(msg.contains("バランス 良好"), "msg={}", msg);
    assert!(msg.contains("50.0%"), "msg={}", msg);
}

// Ext-3 (2026-05-28): 境界値テストは定数 (`SKEW_NEU_THRESHOLD_PCT` /
//   `SKEW_WARN_THRESHOLD_PCT`) の現値が 70.0 / 85.0 であることを前提とする。
//   閾値変更時は本テスト群と定数の双方を必ず同期更新すること。
//   下記 `compute_skew_severity_threshold_constants_are_documented_values` で
//   定数値そのものを assert し、定数だけ変えてテストを忘れたとき検出する。

#[test]
fn compute_skew_severity_70_pct_exactly_returns_pos() {
    // 境界: SKEW_NEU_THRESHOLD_PCT (70.0%) ちょうど → POS (strict >)
    let counts = vec![("A".to_string(), 700i64), ("B".to_string(), 300i64)];
    let (sev, _msg) = compute_skew_severity(&counts, "職種");
    assert_eq!(
        sev, "pos",
        "{}% ちょうどは POS (strict >)",
        SKEW_NEU_THRESHOLD_PCT
    );
}

#[test]
fn compute_skew_severity_above_70_pct_returns_neu() {
    // 境界: 70.01% (701/1000) → NEU
    let counts = vec![("A".to_string(), 701i64), ("B".to_string(), 299i64)];
    let (sev, msg) = compute_skew_severity(&counts, "職種");
    assert_eq!(sev, "neu", "70.1% は NEU (> {}%)", SKEW_NEU_THRESHOLD_PCT);
    assert!(msg.contains("偏りあり"), "msg={}", msg);
}

#[test]
fn compute_skew_severity_85_pct_exactly_returns_neu() {
    // 境界: SKEW_WARN_THRESHOLD_PCT (85.0%) ちょうど → NEU (strict >)
    let counts = vec![("A".to_string(), 850i64), ("B".to_string(), 150i64)];
    let (sev, _msg) = compute_skew_severity(&counts, "産業大分類");
    assert_eq!(
        sev, "neu",
        "{}% ちょうどは NEU (strict >)",
        SKEW_WARN_THRESHOLD_PCT
    );
}

#[test]
fn compute_skew_severity_above_85_pct_returns_warn() {
    // 境界: 85.01% (851/1000) → WARN
    let counts = vec![("A".to_string(), 851i64), ("B".to_string(), 149i64)];
    let (sev, msg) = compute_skew_severity(&counts, "産業大分類");
    assert_eq!(
        sev, "warn",
        "85.1% は WARN (> {}%)",
        SKEW_WARN_THRESHOLD_PCT
    );
    assert!(msg.contains("顕著"), "msg={}", msg);
}

/// Ext-3 (2026-05-28): 閾値定数の現値が 70.0 / 85.0 であることを assert する。
///
/// 定数だけ変えて境界値テストを更新し忘れた場合、本テストが落ちて事故を未然に防ぐ。
/// 不変条件: `SKEW_NEU_THRESHOLD_PCT < SKEW_WARN_THRESHOLD_PCT` (順序保証)。
#[test]
fn compute_skew_severity_threshold_constants_are_documented_values() {
    assert_eq!(
        SKEW_NEU_THRESHOLD_PCT, 70.0,
        "NEU しきい値: docstring と境界値テストは 70.0 を前提"
    );
    assert_eq!(
        SKEW_WARN_THRESHOLD_PCT, 85.0,
        "WARN しきい値: docstring と境界値テストは 85.0 を前提"
    );
    assert!(
        SKEW_NEU_THRESHOLD_PCT < SKEW_WARN_THRESHOLD_PCT,
        "順序保証: NEU 閾値 < WARN 閾値"
    );
    assert!(
        SKEW_NEU_THRESHOLD_PCT > 0.0 && SKEW_WARN_THRESHOLD_PCT <= 100.0,
        "範囲: 両定数とも (0.0, 100.0] の範囲内"
    );
}

#[test]
fn compute_skew_severity_max_share_invariant() {
    // 不変条件: max_share ∈ [0, 100]
    // 多カテゴリで top が小さい場合も合計に対する比率は正常範囲
    let counts: Vec<(String, i64)> = (0..10).map(|i| (format!("cat{}", i), 100i64)).collect();
    let (sev, msg) = compute_skew_severity(&counts, "職種");
    // 10 カテゴリ均等 → top=100/total=1000 = 10.0% → POS
    assert_eq!(sev, "pos");
    assert!(msg.contains("10.0%"), "msg={}", msg);
}

#[test]
fn compute_skew_severity_negative_counts_excluded_from_total() {
    // 不変条件補足: total = sum (負値含む) なので、負値があると total が縮む。
    // 設計通り (postings fetch は cnt > 0 を保証するが、関数自体は防御的)。
    // 負値で total = 0 以下になれば NEU データなし。
    let counts = vec![("A".to_string(), -50i64), ("B".to_string(), -50i64)];
    let (sev, msg) = compute_skew_severity(&counts, "職種");
    assert_eq!(sev, "neu", "total <= 0 は NEU データなし");
    assert_eq!(msg, "職種データなし");
}

// ====================================================================
// P2-1 (2026-05-28): 給与レンジ 散布図 (Section 03 図 3-6)
//   - build_navy_salary_scatter_svg: 空 / 1 点 / 多数点
//   - build_salary_scatter_summary: n / 平均レンジ幅 / narrow% / wide%
//
// 設計メモ:
//   - silent fallback 防御 (空配列 → 空文字列)
//   - 不変条件: n >= 0, avg_width >= 0, 0 <= narrow_pct <= 100, 0 <= wide_pct <= 100
// ====================================================================

#[test]
fn build_navy_salary_scatter_svg_empty_returns_empty_string() {
    // 不変条件: 空入力 → 空文字列 (silent fallback ではなく明示的に省略)
    // Phase 2-A (2026-05-29): is_hourly 引数追加。月給モード (false) で旧動作互換。
    let svg = build_navy_salary_scatter_svg(&[], false);
    assert!(
        svg.is_empty(),
        "empty pairs → empty svg, got len={}",
        svg.len()
    );
}

#[test]
fn build_navy_salary_scatter_svg_single_point_contains_svg_tag() {
    // 1 点入力: <svg> タグ + 1 つの <circle> が含まれる
    let pairs = vec![(200_000.0_f64, 300_000.0_f64)];
    let svg = build_navy_salary_scatter_svg(&pairs, false);
    assert!(svg.contains("<svg"), "svg tag missing");
    assert!(svg.contains("</svg>"), "svg close tag missing");
    // 散布点は 1 つ。<circle ... opacity="0.4"/> が含まれる
    let circle_count = svg.matches("<circle").count();
    assert_eq!(circle_count, 1, "expected 1 circle, got {circle_count}");
    // 対角線 (金色破線) も常に描画される
    assert!(
        svg.contains("#C9A24B"),
        "diagonal line color (gold) missing"
    );
}

#[test]
fn build_navy_salary_scatter_svg_many_points_contains_opacity_and_navy_color() {
    // 多数点: opacity 0.4 / navy 色 #1F2D4D / 全点数の circle 出力
    let pairs: Vec<(f64, f64)> = (0..50)
        .map(|i| {
            (
                180_000.0 + (i as f64) * 1000.0,
                280_000.0 + (i as f64) * 2000.0,
            )
        })
        .collect();
    let svg = build_navy_salary_scatter_svg(&pairs, false);
    // 仕様: opacity 0.4 が散布点に含まれる
    assert!(
        svg.contains("opacity=\"0.4\""),
        "opacity=0.4 missing for scatter points"
    );
    // 仕様: navy ink-soft 色
    assert!(svg.contains("#1F2D4D"), "navy color (#1F2D4D) missing");
    let circle_count = svg.matches("<circle").count();
    assert_eq!(circle_count, pairs.len(), "circle count mismatch");
}

#[test]
fn build_navy_salary_scatter_svg_out_of_range_values_clamped_not_panic() {
    // 不変条件: 範囲外 (10万 / 100万円) でも panic せず描画は範囲内にクランプ
    let pairs = vec![
        (50_000.0, 80_000.0),       // 範囲外 (5万 / 8万)
        (1_000_000.0, 2_000_000.0), // 範囲外 (100万 / 200万)
        (250_000.0, 350_000.0),     // 範囲内
    ];
    let svg = build_navy_salary_scatter_svg(&pairs, false);
    assert!(
        svg.contains("<svg"),
        "svg should render even with out-of-range"
    );
    let circle_count = svg.matches("<circle").count();
    assert_eq!(circle_count, 3);
}

#[test]
fn build_salary_scatter_summary_empty_returns_empty_string() {
    let s = build_salary_scatter_summary(&[], false);
    assert!(s.is_empty(), "empty pairs → empty summary");
}

#[test]
fn build_salary_scatter_summary_computes_n_and_widths_correctly() {
    // 設計テストデータ n=5:
    //   (200000, 230000) → 幅 30000 = 3万円  (narrow: < 5万)
    //   (200000, 240000) → 幅 40000 = 4万円  (narrow)
    //   (200000, 260000) → 幅 60000 = 6万円  (中間)
    //   (200000, 300000) → 幅 100000 = 10万円 (wide: >= 10万)
    //   (200000, 350000) → 幅 150000 = 15万円 (wide)
    //
    //   avg_width = (30000+40000+60000+100000+150000)/5 = 76000 = 7.6 万円
    //   narrow_pct = 2/5 = 40.0%
    //   wide_pct = 2/5 = 40.0%
    let pairs = vec![
        (200_000.0, 230_000.0),
        (200_000.0, 240_000.0),
        (200_000.0, 260_000.0),
        (200_000.0, 300_000.0),
        (200_000.0, 350_000.0),
    ];
    let s = build_salary_scatter_summary(&pairs, false);
    // n
    assert!(s.contains("n=5"), "expected n=5 in summary: {s}");
    // 平均レンジ幅 (7.6 万円)
    assert!(s.contains("7.6万円"), "expected 7.6万円 in summary: {s}");
    // narrow_pct = 40.0%
    assert!(
        s.contains("40.0% (定額求人傾向)"),
        "expected narrow 40.0% (定額求人傾向) in summary: {s}"
    );
    // wide_pct = 40.0%
    assert!(
        s.contains("40.0% (歩合・等級制傾向)"),
        "expected wide 40.0% (歩合・等級制傾向) in summary: {s}"
    );
}

#[test]
fn build_salary_scatter_summary_invariants_pct_in_range_0_100() {
    // 不変条件: narrow_pct + wide_pct <= 100, 各 pct ∈ [0, 100]
    // 全件 narrow (幅 1万円固定)
    let pairs_all_narrow: Vec<(f64, f64)> =
        (0..10).map(|_| (200_000.0_f64, 210_000.0_f64)).collect();
    let s = build_salary_scatter_summary(&pairs_all_narrow, false);
    assert!(
        s.contains("100.0% (定額求人傾向)"),
        "expected narrow 100% when all narrow: {s}"
    );
    assert!(
        s.contains("0.0% (歩合・等級制傾向)"),
        "expected wide 0% when all narrow: {s}"
    );

    // 全件 wide (幅 20万円固定)
    let pairs_all_wide: Vec<(f64, f64)> = (0..10).map(|_| (200_000.0_f64, 400_000.0_f64)).collect();
    let s = build_salary_scatter_summary(&pairs_all_wide, false);
    assert!(
        s.contains("100.0% (歩合・等級制傾向)"),
        "expected wide 100% when all wide: {s}"
    );
    assert!(
        s.contains("0.0% (定額求人傾向)"),
        "expected narrow 0% when all wide: {s}"
    );
}

#[test]
fn build_salary_scatter_summary_avg_width_non_negative_invariant() {
    // 不変条件: 平均レンジ幅 >= 0 (hi >= lo を SQL で保証している前提)
    // hi == lo のケース (レンジ幅 0) でも panic せず avg 0.0 を出力
    let pairs = vec![(250_000.0, 250_000.0), (300_000.0, 300_000.0)];
    let s = build_salary_scatter_summary(&pairs, false);
    assert!(s.contains("n=2"));
    assert!(s.contains("0.0万円"), "expected avg width 0.0万円: {s}");
    // 全件 narrow (< 5万)
    assert!(s.contains("100.0% (定額求人傾向)"));
}

// ====================================================================
// P2-2 (2026-05-28): CSV 企業別給与ランキング (表 5-G) +
//                    注目企業リスト (表 5-H、求人数 top ∩ 給与 top の和集合)
//
//   - select_notable_companies: 空 / 単一 / 5社 / 上位重複 / 和集合サイズ
//   - build_navy_csv_company_salary_table: 空 / 1社 / SO WHAT 直前挿入位置
//   - build_navy_notable_companies_block: 空フォールバック
//
// 不変条件 (silent fallback 防御):
//   - 空 ranking → 戻り値空 Vec / 空文字列
//   - 戻り値 size <= 2 * top_n
//   - レンジ幅 >= 0 (upper >= lower)
// ====================================================================

fn make_csv_company(name: &str, posting_count: i64, lower: f64, upper: f64) -> CsvCompanySalary {
    // Phase 2-A (2026-05-29): native_unit フィールド追加。テスト fixture は月給モード想定。
    CsvCompanySalary {
        facility_name: name.to_string(),
        posting_count,
        salary_lower_median: lower,
        salary_upper_median: upper,
        native_unit: "月給".to_string(),
    }
}

#[test]
fn select_notable_companies_empty_returns_empty_vec() {
    // 不変条件: 空 ranking → 空 Vec (silent fallback ではなく明示)
    let result = select_notable_companies(&[], 5);
    assert!(result.is_empty());
}

#[test]
fn select_notable_companies_top_n_zero_returns_empty_vec() {
    let ranking = vec![make_csv_company("A 株式会社", 5, 20.0, 30.0)];
    let result = select_notable_companies(&ranking, 0);
    assert!(result.is_empty());
}

#[test]
fn select_notable_companies_single_returns_single() {
    let ranking = vec![make_csv_company("A 株式会社", 5, 20.0, 30.0)];
    let result = select_notable_companies(&ranking, 5);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].facility_name, "A 株式会社");
}

#[test]
fn select_notable_companies_five_companies_returns_five() {
    // 5 社 + top_n=5 → 全件返却 (和集合サイズ = 5)
    // ranking は upper_median 降順 (fetch 側ソート保証)
    let ranking = vec![
        make_csv_company("A", 10, 25.0, 50.0),
        make_csv_company("B", 8, 23.0, 45.0),
        make_csv_company("C", 6, 21.0, 40.0),
        make_csv_company("D", 4, 19.0, 35.0),
        make_csv_company("E", 2, 17.0, 30.0),
    ];
    let result = select_notable_companies(&ranking, 5);
    assert_eq!(result.len(), 5);
    // 不変条件: size <= 2 * top_n
    assert!(result.len() <= 10);
}

#[test]
fn select_notable_companies_perfect_overlap_returns_top_n() {
    // 求人数順序 = 給与順序 (完全重複)
    // → 和集合サイズ = top_n (重複排除済)
    let ranking = vec![
        make_csv_company("A", 10, 25.0, 50.0), // 求人 #1 / 給与 #1
        make_csv_company("B", 8, 23.0, 45.0),  // 求人 #2 / 給与 #2
        make_csv_company("C", 6, 21.0, 40.0),  // 求人 #3 / 給与 #3
    ];
    let result = select_notable_companies(&ranking, 3);
    assert_eq!(
        result.len(),
        3,
        "perfect overlap should return exactly top_n"
    );
    // 出現順序: 求人 top → 給与 top (重複は除外) → 結果は [A, B, C]
    assert_eq!(result[0].facility_name, "A");
    assert_eq!(result[1].facility_name, "B");
    assert_eq!(result[2].facility_name, "C");
}

#[test]
fn select_notable_companies_disjoint_returns_union() {
    // 給与 top と 求人数 top が完全 disjoint
    // ranking は upper_median 降順なので 給与 top = [A, B, C]
    // 求人数 top は [E, D, C] (E が最多) → 重複 C
    // 和集合: 求人 top [E, D, C] + 給与 top [A, B] = [E, D, C, A, B] = 5 件
    let ranking = vec![
        make_csv_company("A", 2, 30.0, 60.0),  // 給与 #1, 求人 最少
        make_csv_company("B", 2, 28.0, 55.0),  // 給与 #2
        make_csv_company("C", 5, 26.0, 50.0),  // 給与 #3, 求人 #3
        make_csv_company("D", 10, 18.0, 30.0), // 給与 #4, 求人 #2
        make_csv_company("E", 20, 15.0, 25.0), // 給与 #5, 求人 #1
    ];
    let result = select_notable_companies(&ranking, 3);
    // 和集合サイズ: posting top {E, D, C} ∪ salary top {A, B, C} = {A, B, C, D, E} = 5
    assert_eq!(result.len(), 5);
    // 出現順: posting top を先、salary top 残りを後
    let names: Vec<&str> = result.iter().map(|c| c.facility_name.as_str()).collect();
    assert_eq!(names, vec!["E", "D", "C", "A", "B"]);
    // 不変条件: size <= 2 * top_n
    assert!(result.len() <= 6);
}

#[test]
fn select_notable_companies_top_n_larger_than_ranking_returns_all() {
    // top_n > ranking.len() → 全件返却 (和集合は ranking 全体)
    let ranking = vec![
        make_csv_company("A", 5, 20.0, 30.0),
        make_csv_company("B", 3, 18.0, 25.0),
    ];
    let result = select_notable_companies(&ranking, 10);
    assert_eq!(result.len(), 2);
}

#[test]
fn build_navy_csv_company_salary_table_empty_renders_fallback_message() {
    // 空 ranking → 「該当企業なし」明示メッセージ (silent fallback 防御)
    let s = build_navy_csv_company_salary_table(&[], 10);
    assert!(s.contains("表 5-G"));
    assert!(
        s.contains("該当企業なし"),
        "empty ranking should render explicit fallback: {s}"
    );
}

#[test]
fn build_navy_csv_company_salary_table_single_company_renders_columns() {
    let ranking = vec![make_csv_company("テスト病院", 3, 22.5, 35.7)];
    let s = build_navy_csv_company_salary_table(&ranking, 10);
    // タイトル + 列ヘッダ + データ行
    assert!(s.contains("表 5-G"));
    assert!(s.contains("法人名"));
    assert!(s.contains("下限給与中央値"));
    assert!(s.contains("上限給与中央値"));
    assert!(s.contains("レンジ幅"));
    assert!(s.contains("テスト病院"));
    // 中央値が万円単位で表示される
    assert!(s.contains("22.5"));
    assert!(s.contains("35.7"));
    // レンジ幅 = 35.7 - 22.5 = 13.2
    assert!(s.contains("13.2"), "expected range width 13.2: {s}");
}

#[test]
fn build_navy_csv_company_salary_table_range_width_invariant_non_negative() {
    // 不変条件: lower == upper (固定給) でもレンジ幅 0 で panic せず描画
    let ranking = vec![make_csv_company("固定給会社", 2, 25.0, 25.0)];
    let s = build_navy_csv_company_salary_table(&ranking, 10);
    assert!(s.contains("固定給会社"));
    assert!(
        s.contains("0.0"),
        "fixed salary should render range width 0.0: {s}"
    );
}

#[test]
fn build_navy_notable_companies_block_empty_returns_empty_string() {
    // silent fallback 防御: 空 ranking → 空文字列 (Section に空 table を出さない)
    let s = build_navy_notable_companies_block(&[], 5);
    assert!(
        s.is_empty(),
        "empty ranking should yield empty string, got: {s}"
    );
}

#[test]
fn build_navy_notable_companies_block_renders_table_header_and_rows() {
    let ranking = vec![
        make_csv_company("A", 10, 25.0, 50.0),
        make_csv_company("B", 8, 23.0, 45.0),
    ];
    let s = build_navy_notable_companies_block(&ranking, 5);
    assert!(s.contains("表 5-H"));
    assert!(s.contains("注目企業"));
    assert!(s.contains("給与レンジ"));
    assert!(s.contains("A"));
    assert!(s.contains("B"));
    // 給与レンジ: "25.0〜50.0" の形式
    assert!(
        s.contains("25.0〜50.0"),
        "expected salary range 25.0〜50.0: {s}"
    );
}

#[test]
fn select_notable_companies_invariant_size_le_2_top_n() {
    // 不変条件: |posting_top ∪ salary_top| <= |posting_top| + |salary_top| = 2 * top_n
    // 任意の ranking に対し成立することを確認
    let ranking: Vec<CsvCompanySalary> = (0..20)
        .map(|i| {
            make_csv_company(
                &format!("Company {}", i),
                (20 - i) as i64,   // 求人数: 20, 19, ..., 1 (降順)
                10.0 + (i as f64), // 下限: 10, 11, ..., 29
                40.0 - (i as f64), // 上限: 40, 39, ..., 21 (降順)
            )
        })
        .collect();
    for top_n in 1..=10 {
        let result = select_notable_companies(&ranking, top_n);
        assert!(
            result.len() <= 2 * top_n,
            "invariant violated at top_n={}: result.len()={} > 2 * top_n={}",
            top_n,
            result.len(),
            2 * top_n
        );
    }
}

/// Ext-5 (2026-05-28): 不変条件 `size <= 2 * top_n` を明示的に
///   `[1, 3, 5, 10]` の代表値で検証する。
///
/// 既存 `..._size_le_2_top_n` は 1..=10 連続テストで包括的だが、
/// 「指定 top_n に対する明示的サイズ上限」 を docstring から直接トレース可能にし、
/// 仕様改訂時の影響範囲を可視化する。
///
/// 重要 invariants:
/// - `result.len() <= 2 * top_n` (常に成立)
/// - `result.len() <= ranking.len()` (元データを超えない)
/// - 各要素は ranking 内に存在 (ポインタ等価)
/// - 重複なし (HashSet で確認)
#[test]
fn select_notable_companies_invariant_size_le_double_top_n() {
    // 10 社の ranking。求人数と上限給与で意図的に分離 (和集合のサイズが top_n*2 に近づくよう設計)
    let ranking: Vec<CsvCompanySalary> = (0..10)
        .map(|i| {
            make_csv_company(
                &format!("Co{}", i),
                if i < 5 { (10 - i) as i64 } else { 1 }, // 求人数: 前半は降順、後半は 1 で固定
                20.0 + (i as f64),                       // 下限
                50.0 - (i as f64), // 上限 (降順 → ranking は upper_median 降順なので index と一致)
            )
        })
        .collect();

    for top_n in [1usize, 3, 5, 10] {
        let result = select_notable_companies(&ranking, top_n);

        // 不変条件 1: size <= 2 * top_n
        assert!(
            result.len() <= 2 * top_n,
            "top_n={} で size={} > 2*top_n={}",
            top_n,
            result.len(),
            2 * top_n
        );

        // 不変条件 2: size <= ranking.len()
        assert!(
            result.len() <= ranking.len(),
            "top_n={} で size={} > ranking.len()={}",
            top_n,
            result.len(),
            ranking.len()
        );

        // 不変条件 3: 重複なし (ポインタ等価で確認)
        let mut ptrs: Vec<*const CsvCompanySalary> =
            result.iter().map(|c| *c as *const _).collect();
        ptrs.sort();
        ptrs.dedup();
        assert_eq!(
            ptrs.len(),
            result.len(),
            "top_n={} で duplicate detected: {} unique vs {} result",
            top_n,
            ptrs.len(),
            result.len()
        );
    }
}

// ====================================================================
// R2-P0-1 (ultrathink Round 2, 2026-05-28): クランプ件数 caption の追記
//
// build_navy_salary_scatter_svg は軸 15-60 万円固定でデータをクランプ描画する。
// ユーザーに伝わるよう、build_salary_scatter_summary に
// 「N 件 (X%) が範囲外として端点に表示」の文言を caption に追加。
//
// 不変条件:
//   - クランプ件数 == 0 のとき caption に「範囲外」文言は含まない
//   - クランプ件数 > 0 のとき caption に件数 / % が含まれる
//   - clamp_count <= n
// ====================================================================

#[test]
fn build_salary_scatter_summary_clamp_zero_no_range_note() {
    // 全データが 15-60 万円範囲内 → クランプ件数 0 → 範囲外文言なし
    let pairs = vec![
        (200_000.0, 250_000.0), // 20-25 万 (範囲内)
        (300_000.0, 400_000.0), // 30-40 万 (範囲内)
        (450_000.0, 550_000.0), // 45-55 万 (範囲内)
    ];
    let s = build_salary_scatter_summary(&pairs, false);
    assert!(s.contains("n=3"), "expected n=3 in summary: {s}");
    assert!(
        !s.contains("範囲外"),
        "no out-of-range data → no range-clamp note expected, got: {s}"
    );
    assert!(
        !s.contains("端点に表示"),
        "no endpoint clamp text expected: {s}"
    );
}

#[test]
fn build_salary_scatter_summary_clamp_nonzero_renders_caption() {
    // 5 件中 2 件 (40%) が範囲外 (10 万 / 80 万) → caption に「2 件 (40.0%) が範囲外」表示
    let pairs = vec![
        (100_000.0, 150_000.0), // 10-15 万 (下限が範囲外)
        (800_000.0, 900_000.0), // 80-90 万 (両方範囲外)
        (200_000.0, 300_000.0), // 範囲内
        (250_000.0, 350_000.0), // 範囲内
        (400_000.0, 500_000.0), // 範囲内
    ];
    let s = build_salary_scatter_summary(&pairs, false);
    assert!(s.contains("n=5"), "n=5 expected: {s}");
    assert!(s.contains("2 件"), "expected clamp count 2: {s}");
    assert!(s.contains("40.0%"), "expected clamp pct 40.0%: {s}");
    assert!(s.contains("範囲外"), "expected '範囲外' wording: {s}");
}

#[test]
fn build_salary_scatter_summary_clamp_all_out_of_range() {
    // 全件範囲外 → 100% クランプ
    let pairs = vec![
        (100_000.0, 140_000.0), // 10-14 万
        (700_000.0, 800_000.0), // 70-80 万
    ];
    let s = build_salary_scatter_summary(&pairs, false);
    assert!(s.contains("2 件"), "expected 2 件: {s}");
    assert!(s.contains("100.0%"), "expected 100.0% clamp pct: {s}");
}

// ====================================================================
// R2-P1-1 (ultrathink Round 2, 2026-05-28): NaN/Inf 出力防止
//
// safe_pct helper が NaN / +Inf / -Inf / 100超 を [0, 100] にクランプ。
// safe_pct_like は NaN/Inf のみ 0.0 にし、上限クランプはしない。
// ====================================================================

#[test]
fn safe_pct_nan_returns_zero() {
    let v = f64::NAN;
    assert_eq!(safe_pct(v), 0.0, "NaN should map to 0.0");
}

#[test]
fn safe_pct_inf_returns_zero() {
    assert_eq!(safe_pct(f64::INFINITY), 0.0, "+Inf should map to 0.0");
    assert_eq!(safe_pct(f64::NEG_INFINITY), 0.0, "-Inf should map to 0.0");
}

#[test]
fn safe_pct_above_100_clamped() {
    // 浮動小数誤差で 100.0000001 になる場合に対する防御
    assert_eq!(safe_pct(100.0001), 100.0);
    assert_eq!(safe_pct(150.0), 100.0);
}

#[test]
fn safe_pct_negative_clamped_to_zero() {
    assert_eq!(safe_pct(-1.0), 0.0);
    assert_eq!(safe_pct(-0.0001), 0.0);
}

#[test]
fn safe_pct_normal_value_unchanged() {
    assert_eq!(safe_pct(42.5), 42.5);
    assert_eq!(safe_pct(0.0), 0.0);
    assert_eq!(safe_pct(100.0), 100.0);
}

#[test]
fn safe_pct_like_nan_returns_zero_but_no_upper_clamp() {
    // safe_pct_like は NaN/Inf を 0 にするが、>100 の大きな値は通す (avg などの非 % 値用)
    assert_eq!(safe_pct_like(f64::NAN), 0.0);
    assert_eq!(safe_pct_like(f64::INFINITY), 0.0);
    assert_eq!(
        safe_pct_like(500.0),
        500.0,
        "non-% values should not be upper-clamped"
    );
    assert_eq!(
        safe_pct_like(-3.0),
        -3.0,
        "negatives also pass for non-% helper"
    );
}

// ====================================================================
// R2-P1-3 (ultrathink Round 2, 2026-05-28): SVG <title> 要素追加 (a11y)
//
// build_navy_pyramid_svg / build_navy_pyramid_svg_mini /
// build_navy_salary_scatter_svg の 3 関数で <title>...</title> を含むことを確認
// ====================================================================

#[test]
fn build_navy_pyramid_svg_contains_title_element_for_a11y() {
    let bands = vec![
        ("20-29".to_string(), 1000i64, 950i64),
        ("30-39".to_string(), 1100i64, 1050i64),
    ];
    let svg = build_navy_pyramid_svg(&bands);
    assert!(
        svg.contains("<title>年齢階級別 人口ピラミッド</title>"),
        "expected <title> element in build_navy_pyramid_svg: {}",
        &svg[..svg.len().min(400)]
    );
}

#[test]
fn build_navy_pyramid_svg_mini_contains_title_element_for_a11y() {
    let bands = vec![
        ("20-29".to_string(), 100i64, 95i64),
        ("30-39".to_string(), 110i64, 105i64),
    ];
    let svg = build_navy_pyramid_svg_mini(&bands);
    assert!(
        svg.contains("<title>市区町村別 人口ピラミッド (年齢階級別 男女別 人口)</title>"),
        "expected <title> element in build_navy_pyramid_svg_mini: {}",
        &svg[..svg.len().min(400)]
    );
}

#[test]
fn build_navy_salary_scatter_svg_contains_title_element_for_a11y() {
    let pairs = vec![(200_000.0, 300_000.0)];
    let svg = build_navy_salary_scatter_svg(&pairs, false);
    assert!(
        svg.contains("<title>給与レンジ 散布図 (下限給与 × 上限給与)</title>"),
        "expected <title> element in build_navy_salary_scatter_svg: {}",
        &svg[..svg.len().min(400)]
    );
}

// ====================================================================
// R2-P1-4 (ultrathink Round 2, 2026-05-28): 表 scope="col" 追加 (a11y)
//
// build_navy_csv_company_salary_table / build_navy_notable_companies_block /
// build_distribution_table の列ヘッダに scope="col" が付与されることを確認。
// ====================================================================

#[test]
fn build_navy_csv_company_salary_table_th_has_scope_col() {
    let ranking = vec![make_csv_company("テスト病院", 3, 22.0, 32.0)];
    let s = build_navy_csv_company_salary_table(&ranking, 10);
    // 全 th に scope="col" が付与されているか (列ヘッダのみ存在する table)
    // <th スペース付きで grep → <thead> 等を誤カウントしない
    let th_count = s.matches("<th ").count();
    let scoped_count = s.matches("scope=\"col\"").count();
    assert!(
        th_count > 0 && th_count == scoped_count,
        "all <th> should have scope=\"col\": th={}, scoped={}",
        th_count,
        scoped_count
    );
}

#[test]
fn build_navy_notable_companies_block_th_has_scope_col() {
    let ranking = vec![
        make_csv_company("A", 10, 25.0, 50.0),
        make_csv_company("B", 8, 23.0, 45.0),
    ];
    let s = build_navy_notable_companies_block(&ranking, 5);
    // <th スペース付きで grep → <thead> 等を誤カウントしない
    let th_count = s.matches("<th ").count();
    let scoped_count = s.matches("scope=\"col\"").count();
    assert!(
        th_count > 0 && th_count == scoped_count,
        "all <th> should have scope=\"col\": th={}, scoped={}",
        th_count,
        scoped_count
    );
}

// 2026-06-01: build_distribution_table_th_has_scope_col /
// render_navy_section_06_posting_target_all_zero_distribution_kpi_dash /
// render_navy_section_06_posting_target_partial_zero_picks_non_zero を削除。
// HW postings 求人側集計 (図 6-3 / 表 6-G/H/I/J) のレンダリングブロック自体を
// section_06_demographics.rs から撤去したため、対象 helper /
// `render_navy_section_06_posting_target` も同時削除済み。3 件減算。
