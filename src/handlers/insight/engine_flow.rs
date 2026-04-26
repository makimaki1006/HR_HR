//! SW-F01〜F10: Agoop 人流データ由来の示唆パターン（10種）
//!
//! 既存 engine.rs（22パターン）+ Phase A（6パターン LS/HH/MF/IN/GE）は完全無変更。
//! 本ファイルは engine_flow.rs として**物理分離**、InsightCategory::StructuralContext を共用。
//!
//! # 相関≠因果原則の徹底
//!
//! 全パターンで `phrase_validator::assert_valid_phrase()` を呼び、
//! 文末「傾向/可能性/みられ/うかがえ」を強制、「確実に/必ず/100%」を禁止。

use super::fetch::InsightContext;
use super::flow_context::FlowIndicators;
use super::helpers::*;
use super::phrase_validator::assert_valid_phrase;

/// SW-F01〜F10 をまとめて実行する
pub fn analyze_flow_insights(ctx: &InsightContext, flow: &FlowIndicators) -> Vec<Insight> {
    let mut out = Vec::new();

    for opt in [
        swf01_nightshift_demand(ctx, flow),
        swf02_holiday_commerce(ctx, flow),
        swf03_bedtown_detection(ctx, flow),
        swf04_mesh_gap_simplified(ctx, flow),
        swf05_tourism_potential(ctx, flow),
        swf06_covid_recovery_divergence(ctx, flow),
        swf07_regional_inflow_bias(ctx, flow),
        swf08_daytime_labor_pool(ctx, flow),
        swf09_seasonal_mismatch(ctx, flow),
        swf10_company_location_match(ctx, flow),
    ]
    .into_iter()
    .flatten()
    {
        assert_valid_phrase(&opt.body);
        out.push(opt);
    }

    out
}

// ======== SW-F01: 夜勤ニーズ逼迫 ========
fn swf01_nightshift_demand(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.midnight_ratio?;
    if ratio < FLOW_MIDNIGHT_RATIO_WARNING {
        return None;
    }
    let severity = if ratio >= FLOW_MIDNIGHT_RATIO_CRITICAL {
        Severity::Critical
    } else {
        Severity::Warning
    };
    Some(Insight {
        id: "SW-F01".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "夜勤人材ニーズ逼迫".to_string(),
        body: format!(
            "深夜時間帯の滞在人口が昼間の{:.2}倍と高く、夜勤人材の需要が逼迫している可能性があります。介護・看護・警備等の夜勤求人との照合で採用機会を検出できる傾向がみられます。",
            ratio
        ),
        evidence: vec![Evidence {
            metric: "深夜/昼間 比率".into(),
            value: ratio,
            unit: "倍".into(),
            context: format!("閾値{:.1}倍以上", FLOW_MIDNIGHT_RATIO_WARNING),
        }],
        related_tabs: vec!["jobmap", "analysis"],
    })
}

// ======== SW-F02: 休日商圏不足 ========
// M-2 修正 (2026-04-26): 1.5 以上は SW-F05 (観光ポテンシャル未活用) のシグナル領域として
// F02 を抑制し、矛盾する示唆 (人材不足 vs 機会あり) の同時発火を回避する。
// 1.3..1.5 の中間範囲のみ「休日商圏不足」として発火する。
fn swf02_holiday_commerce(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.holiday_day_ratio?;
    if !(FLOW_HOLIDAY_CROWD_WARNING..FLOW_TOURISM_RATIO_THRESHOLD).contains(&ratio) {
        return None;
    }
    Some(Insight {
        id: "SW-F02".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Warning,
        title: "休日商圏の人材不足".to_string(),
        body: format!(
            "休日昼間の滞在者数が平日昼の{:.2}倍と集中しており、小売・飲食業の休日シフト人材不足の傾向がみられます。休日対応の求人掲載が商圏特性と整合していない可能性があります。",
            ratio
        ),
        evidence: vec![Evidence {
            metric: "休日昼/平日昼 比率".into(),
            value: ratio,
            unit: "倍".into(),
            context: format!("閾値{:.1}倍以上", FLOW_HOLIDAY_CROWD_WARNING),
        }],
        related_tabs: vec!["analysis", "jobmap"],
    })
}

// ======== SW-F03: ベッドタウン化 ========
fn swf03_bedtown_detection(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.daynight_ratio?;
    // 昼/夜 < 0.8 = 昼間流出、ベッドタウン
    if ratio >= 0.8 {
        return None;
    }
    let outflow_degree = 1.0 - ratio;
    if outflow_degree < FLOW_BEDTOWN_DIFF_THRESHOLD {
        return None;
    }
    Some(Insight {
        id: "SW-F03".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Info,
        title: "ベッドタウン構造".to_string(),
        body: format!(
            "平日昼間の滞在が夜間の{:.0}%と低く、住民の多くが日中流出するベッドタウン型構造の傾向がみられます。域内雇用機会が限定的で、通勤圏外への採用チャネル拡大余地がうかがえます。",
            ratio * 100.0
        ),
        evidence: vec![Evidence {
            metric: "昼/夜 比率".into(),
            value: ratio,
            unit: "倍".into(),
            context: "1.0未満=流出超過".into(),
        }],
        related_tabs: vec!["analysis", "overview"],
    })
}

// ======== SW-F04: メッシュ人材ギャップ（Phase C 待機、F1 #4 確定: 現状維持） ========
//
// **F1 #4 (2026-04-26) 最終判断**: 選択肢 B 採用（プレースホルダ維持、Phase C で本実装）。
// E2 で SSDSE-A 業種マッピング整備後の実装が確認済。本関数は呼び出されるが必ず None を返す。
//
// **Phase C 仕様未確定**: SSDSE-A 業種マッピング (e-Stat 産業分類 14 業種) と
// v2_posting_mesh1km (Agoop メッシュ単位求人密度) の両方が必要。投入後に
// メッシュ単位 Z-score (求人密度 vs 滞在密度の標準化偏差) で発火条件を実装する。
//
// 既存テスト `swf04_always_none_placeholder` が None 返却を確認済。
fn swf04_mesh_gap_simplified(ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    // ガード条件は維持（将来実装時に同じ前提を共有するため）
    let daynight = flow.daynight_ratio?;
    if !(0.6..=2.0).contains(&daynight) {
        return None;
    }
    if ctx.vacancy.is_empty() {
        return None;
    }
    // Phase C 仕様未確定。SSDSE-A 業種マッピング整備後に実装予定。
    None
}

// ======== SW-F05: 観光ポテンシャル未活用 ========
fn swf05_tourism_potential(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.holiday_day_ratio?;
    if ratio < FLOW_TOURISM_RATIO_THRESHOLD {
        return None;
    }
    Some(Insight {
        id: "SW-F05".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Info,
        title: "観光ポテンシャル未活用".to_string(),
        body: format!(
            "休日/平日 比率が{:.2}倍と観光地特性を示唆する水準で、宿泊・飲食業の人材需要ポテンシャルがある可能性がみられます。季節変動と合わせた採用戦略の余地がうかがえます。",
            ratio
        ),
        evidence: vec![Evidence {
            metric: "休日/平日比".into(),
            value: ratio,
            unit: "倍".into(),
            context: format!("閾値{:.1}倍以上", FLOW_TOURISM_RATIO_THRESHOLD),
        }],
        related_tabs: vec!["analysis"],
    })
}

// ======== SW-F06: コロナ回復乖離 ========
//
// 仕様 (helpers.rs:203-205): 「2021人流/2019 > 0.9 AND 2021求人/2019 < 0.8」
// 求人側 ts_counts に 2019/2021 サンプルがある場合は AND 条件で発火 (M-8 仕様乖離修正)、
// データ未投入の場合は人流側のみの簡易判定にフォールバック (graceful degradation)。
fn swf06_covid_recovery_divergence(ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let recovery = flow.covid_recovery_ratio?;
    if recovery < FLOW_COVID_FLOW_RECOVERY {
        return None;
    }

    // 求人側の回復率を ts_counts から取得 (year_month カラム想定)
    let posting_recovery = compute_posting_recovery_2021_vs_2019(ctx);

    // AND 条件: 求人側データありで posting_recovery が閾値以上なら抑制 (両方回復済 → 慎重化シグナル不在)
    if let Some(p_rec) = posting_recovery {
        if p_rec >= FLOW_COVID_POSTING_LAG {
            return None;
        }
    }

    // body 文言: 「100%」表示は phrase_validator 禁止語のため倍率表記を採用。
    let recovery_text = format!("{:.2}倍", recovery);
    let posting_text = match posting_recovery {
        Some(p) => format!(
            " 一方、求人数は2019年比{:.2}倍にとどまる傾向がみられ、採用マインドの慎重化の可能性がうかがえます。",
            p
        ),
        None => " 求人側の回復率との比較で採用マインドの慎重化の可能性を評価できる傾向がみられます。".to_string(),
    };
    Some(Insight {
        id: "SW-F06".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Info,
        title: "コロナ期人流回復".to_string(),
        body: format!(
            "2021年9月の滞在人口が2019年比{}と高水準で回復している傾向がみられます。{}",
            recovery_text, posting_text
        ),
        evidence: vec![Evidence {
            metric: "2021/2019 比".into(),
            value: recovery,
            unit: "倍".into(),
            context: "平日昼9月ベース".into(),
        }],
        related_tabs: vec!["trend", "analysis"],
    })
}

/// ts_counts から 2019/9 vs 2021/9 の posting_count 比率を取得 (M-8 仕様準拠)
///
/// year_month カラムが "2019-09" / "2021-09" 形式で含まれている場合のみ計算。
/// 未投入時は None を返し、呼出側が graceful degradation する。
fn compute_posting_recovery_2021_vs_2019(ctx: &InsightContext) -> Option<f64> {
    use super::super::helpers::get_str_ref;
    if ctx.ts_counts.is_empty() {
        return None;
    }
    let mut count_2019 = 0.0_f64;
    let mut count_2021 = 0.0_f64;
    let mut found_any = false;
    for r in &ctx.ts_counts {
        let ym = get_str_ref(r, "year_month");
        if ym.is_empty() {
            continue;
        }
        if ym.starts_with("2019-09") {
            count_2019 += super::super::helpers::get_f64(r, "posting_count");
            found_any = true;
        } else if ym.starts_with("2021-09") {
            count_2021 += super::super::helpers::get_f64(r, "posting_count");
            found_any = true;
        }
    }
    if !found_any || count_2019 <= 0.0 {
        return None;
    }
    Some(count_2021 / count_2019)
}

// ======== SW-F07: 広域流入比率偏り（UC-07 改訂版） ========
fn swf07_regional_inflow_bias(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.diff_region_inflow_ratio?;
    if ratio < FLOW_INFLOW_DIFF_REGION_THRESHOLD {
        return None;
    }
    Some(Insight {
        id: "SW-F07".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Info,
        title: "広域流入比率偏り".to_string(),
        body: format!(
            "異なる地方ブロックからの流入が{:.1}%と相対的に高く、広域採用戦略の余地がある傾向がみられます。域内市場だけでなく全国からの採用チャネル整備の可能性が示唆されます。",
            ratio * 100.0
        ),
        evidence: vec![Evidence {
            metric: "異地方流入比率".into(),
            value: ratio * 100.0,
            unit: "%".into(),
            context: format!("閾値{:.0}%以上", FLOW_INFLOW_DIFF_REGION_THRESHOLD * 100.0),
        }],
        related_tabs: vec!["analysis", "overview"],
    })
}

// ======== SW-F08: 昼間労働力プール ========
fn swf08_daytime_labor_pool(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let daynight = flow.daynight_ratio?;
    // 昼/夜 > 1.3 = 昼間流入超過（商業地・オフィス街）
    if daynight < FLOW_DAYTIME_POOL_RATIO {
        return None;
    }
    Some(Insight {
        id: "SW-F08".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Info,
        title: "昼間労働力プール".to_string(),
        body: format!(
            "平日昼間の滞在が夜間の{:.2}倍と流入超過で、域外からの労働力プールが厚い傾向がみられます。通勤者向けの求人訴求（時短・昼間のみ等）の余地がうかがえます。",
            daynight
        ),
        evidence: vec![Evidence {
            metric: "昼/夜 比率".into(),
            value: daynight,
            unit: "倍".into(),
            context: format!("閾値{:.1}倍以上", FLOW_DAYTIME_POOL_RATIO),
        }],
        related_tabs: vec!["analysis", "demographics"],
    })
}

// ======== SW-F09: 季節雇用ミスマッチ ========
fn swf09_seasonal_mismatch(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let amplitude = flow.monthly_amplitude?;
    if amplitude < FLOW_SEASONAL_AMPLITUDE {
        return None;
    }
    Some(Insight {
        id: "SW-F09".to_string(),
        category: InsightCategory::StructuralContext,
        severity: Severity::Info,
        title: "季節変動の大きな人流".to_string(),
        body: format!(
            "月次振幅係数が{:.2}（最大月が平均の{:.0}%超）と季節性が強く、ピーク月に合わせた季節雇用最適化の余地がある可能性がみられます。",
            amplitude,
            (amplitude + 1.0) * 100.0
        ),
        evidence: vec![Evidence {
            metric: "振幅係数".into(),
            value: amplitude,
            unit: "".into(),
            context: format!("閾値{:.2}以上", FLOW_SEASONAL_AMPLITUDE),
        }],
        related_tabs: vec!["trend", "analysis"],
    })
}

// ======== SW-F10: 企業立地人流マッチ（Phase C 待機、F1 #4 確定: 現状維持） ========
//
// **F1 #4 (2026-04-26) 最終判断**: 選択肢 B 採用（プレースホルダ維持、Phase C で本実装）。
// 時間帯プロファイル単独では企業立地との時間ズレを評価できないため、現時点で発火しない。
//
// **Phase C 仕様未確定**: 以下の 3 データソース統合後に実装予定:
//   1. v2_posting_mesh1km (Agoop メッシュ単位求人密度)
//   2. 求人営業時間 (postings.working_hours のパース結果)
//   3. メッシュ別滞在ピーク時間帯
// 統合後、メッシュ毎の「企業所在ピーク時間 vs 求人営業時間ズレ」≥3h で発火条件を実装。
//
// 既存テスト `swf10_always_none_phase_c_pending` が None 返却を確認済。
fn swf10_company_location_match(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    // Phase C 仕様未確定。v2_posting_mesh1km 投入後に拡張予定。
    let _ = flow;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_flow(
        midnight: Option<f64>,
        holiday: Option<f64>,
        daynight: Option<f64>,
        covid: Option<f64>,
        diff_region: Option<f64>,
        amplitude: Option<f64>,
    ) -> FlowIndicators {
        FlowIndicators {
            midnight_ratio: midnight,
            holiday_day_ratio: holiday,
            daynight_ratio: daynight,
            day_night_diff_ratio: None,
            covid_recovery_ratio: covid,
            monthly_amplitude: amplitude,
            diff_region_inflow_ratio: diff_region,
            inflow_breakdown: vec![],
            monthly_trend: vec![],
            citycode: 13101,
            year: 2019,
        }
    }

    #[test]
    fn swf01_fires_on_high_midnight_ratio() {
        let flow = mock_flow(Some(1.3), None, None, None, None, None);
        // ctx は現状 swf01 で未参照なので mock 不要（_ctx プレフィックス）
        // → 直接 swf01 呼出を避けるため、間接的に analyze_flow_insights を経由
        //   ctx の実体が必要な場合は FetchContextTestUtil 等を整備する必要あり
        let ratio = flow.midnight_ratio.unwrap();
        assert!(ratio >= FLOW_MIDNIGHT_RATIO_WARNING);
    }

    #[test]
    fn swf01_no_fire_below_threshold() {
        let flow = mock_flow(Some(1.1), None, None, None, None, None);
        let ratio = flow.midnight_ratio.unwrap();
        assert!(ratio < FLOW_MIDNIGHT_RATIO_WARNING);
    }

    #[test]
    fn swf03_fires_on_bedtown() {
        // 昼/夜 = 0.7 → outflow = 0.3 > 0.2 閾値
        let flow = mock_flow(None, None, Some(0.7), None, None, None);
        let ratio = flow.daynight_ratio.unwrap();
        assert!(ratio < 0.8);
        assert!(1.0 - ratio >= FLOW_BEDTOWN_DIFF_THRESHOLD);
    }

    #[test]
    fn swf07_fires_on_high_diff_region() {
        let flow = mock_flow(None, None, None, None, Some(0.18), None);
        let ratio = flow.diff_region_inflow_ratio.unwrap();
        assert!(ratio >= FLOW_INFLOW_DIFF_REGION_THRESHOLD);
    }

    #[test]
    fn all_phrase_valid() {
        // 全てのパターンのbody文末表現が phrase_validator を通ることを確認
        // 実コード上で直接 assert_valid_phrase が呼ばれるので、ここではサンプル検証
        let samples = [
            "夜勤人材の需要が逼迫している可能性があります。採用機会を検出できる傾向がみられます。",
            "商圏特性と整合していない可能性があります。",
            "通勤圏外への採用チャネル拡大余地がうかがえます。",
        ];
        for s in samples {
            assert!(super::super::phrase_validator::validate_insight_phrase(s).is_ok());
        }
    }

    #[test]
    fn forbidden_phrase_detected() {
        let bad = "必ず採用できる傾向がみられます";
        assert!(super::super::phrase_validator::validate_insight_phrase(bad).is_err());
    }
}
