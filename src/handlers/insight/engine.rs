//! 示唆計算ロジック（既存22パターン + Phase A 新規6パターン = 計28パターン）
//!
//! - 既存（L31-1329, 完全無変更）: HS/FC/RC/AP/CZ/CF 22パターン
//! - 追加（L1331以降、StructuralContext）: LS/HH/MF/IN/GE 6パターン（SSDSE-A Phase A）

use super::super::helpers::{get_f64, get_str_ref};
use super::fetch::InsightContext;
use super::helpers::*;
use super::phrase_validator::assert_valid_phrase;

// ======== エントリポイント ========

/// 全示唆を生成（Severityでソート済み）
pub fn generate_insights(ctx: &InsightContext) -> Vec<Insight> {
    let mut insights = Vec::with_capacity(20);

    // カテゴリ1: 採用構造分析
    insights.extend(analyze_hiring_structure(ctx));
    // カテゴリ2: 将来予測
    insights.extend(analyze_forecast(ctx));
    // カテゴリ3: 地域間比較
    insights.extend(analyze_regional_comparison(ctx));
    // カテゴリ5: 通勤圏分析
    insights.extend(analyze_commute_zone(ctx));
    // カテゴリ4: アクション提案（上記結果に依存）
    insights.extend(generate_action_proposals(ctx, &insights));
    // カテゴリ6: 構造分析（Phase A、SSDSE-Aベース、市区町村構造指標）
    insights.extend(analyze_structural_context(ctx));
    // カテゴリ7: Agoop 人流示唆（Phase B、SW-F01〜F10）
    if let Some(flow) = &ctx.flow {
        insights.extend(super::engine_flow::analyze_flow_insights(ctx, flow));
    }

    // Severityでソート（Critical → Warning → Info → Positive）
    insights.sort_by(|a, b| a.severity.cmp(&b.severity));
    insights
}

// ======== カテゴリ1: 採用構造分析（なぜ採れないか）========

fn analyze_hiring_structure(ctx: &InsightContext) -> Vec<Insight> {
    let mut out = Vec::new();

    // HS-1: 慢性的人材不足シグナル
    if let Some(insight) = hs1_chronic_shortage(ctx) {
        out.push(insight);
    }
    // HS-2: 給与競争力不足
    if let Some(insight) = hs2_salary_competitiveness(ctx) {
        out.push(insight);
    }
    // HS-3: 情報開示不足
    if let Some(insight) = hs3_transparency_gap(ctx) {
        out.push(insight);
    }
    // HS-4: テキスト温度と採用難の乖離
    if let Some(insight) = hs4_temperature_mismatch(ctx) {
        out.push(insight);
    }
    // HS-5: 雇用者集中（独占的市場）
    if let Some(insight) = hs5_monopsony(ctx) {
        out.push(insight);
    }
    // HS-6: 空間的ミスマッチ
    if let Some(insight) = hs6_spatial_mismatch(ctx) {
        out.push(insight);
    }

    out
}

/// HS-1: 慢性的人材不足（vacancy + ts_vacancy）
fn hs1_chronic_shortage(ctx: &InsightContext) -> Option<Insight> {
    // 正社員の欠員率を取得
    let vac_row = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let vacancy_rate = get_f64(vac_row, "vacancy_rate");
    if vacancy_rate < VACANCY_WARNING {
        return None;
    }

    // 時系列で直近3ヶ月を確認
    let ts_rates: Vec<f64> = ctx
        .ts_vacancy
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .collect();
    let chronic = if ts_rates.len() >= 3 {
        ts_rates[ts_rates.len() - 3..]
            .iter()
            .all(|&r| r > VACANCY_TREND_THRESHOLD)
    } else {
        false
    };

    let severity = if vacancy_rate >= VACANCY_CRITICAL && chronic {
        Severity::Critical
    } else if vacancy_rate >= VACANCY_CRITICAL {
        Severity::Warning
    } else if vacancy_rate >= VACANCY_WARNING {
        Severity::Warning
    } else {
        Severity::Info
    };

    let trend_text = if ts_rates.len() >= 3 {
        let recent = &ts_rates[ts_rates.len() - 3..];
        format!(
            "過去3ヶ月: {:.1}% → {:.1}% → {:.1}%",
            recent[0] * 100.0,
            recent[1] * 100.0,
            recent[2] * 100.0
        )
    } else {
        "時系列データなし".to_string()
    };

    Some(Insight {
        id: "HS-1".to_string(),
        category: InsightCategory::HiringStructure,
        severity,
        title: "慢性的人材不足シグナル".to_string(),
        body: format!(
            "正社員の欠員率は{:.1}%{}。{}",
            vacancy_rate * 100.0,
            if chronic {
                "で、過去3ヶ月連続で高水準を維持しています"
            } else {
                "です"
            },
            trend_text
        ),
        evidence: vec![Evidence {
            metric: "欠員率".into(),
            value: vacancy_rate,
            unit: "%".into(),
            context: format!("閾値{:.0}%", VACANCY_CRITICAL * 100.0),
        }],
        related_tabs: vec!["overview", "balance"],
    })
}

/// HS-2: 給与競争力不足（salary_comp + wage_compliance）
fn hs2_salary_competitiveness(ctx: &InsightContext) -> Option<Insight> {
    let comp_row = ctx
        .salary_comp
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let comp_index = get_f64(comp_row, "competitiveness_index");
    if comp_index <= 0.0 || comp_index >= SALARY_COMP_WARNING {
        return None;
    }

    let local_avg = get_f64(comp_row, "local_mean_min");
    let national_avg = get_f64(comp_row, "national_mean_min");

    // 最低賃金違反チェック
    let below_count: i64 = ctx
        .wage_compliance
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| super::super::helpers::get_i64(r, "below_count"))
        .sum();

    let severity = if comp_index < SALARY_COMP_CRITICAL {
        Severity::Critical
    } else {
        Severity::Warning
    };

    Some(Insight {
        id: "HS-2".to_string(),
        category: InsightCategory::HiringStructure,
        severity,
        title: "給与競争力不足".to_string(),
        body: format!(
            "正社員の平均月給({:.0}円)は全国平均({:.0}円)の{:.0}%で、競争力が不足しています。{}",
            local_avg,
            national_avg,
            comp_index * 100.0,
            if below_count > 0 {
                format!("最低賃金未満の求人が{}件あります。", below_count)
            } else {
                String::new()
            }
        ),
        evidence: vec![
            Evidence {
                metric: "競争力指数".into(),
                value: comp_index,
                unit: "".into(),
                context: format!("全国平均=1.0, 閾値{:.1}", SALARY_COMP_WARNING),
            },
            Evidence {
                metric: "平均月給".into(),
                value: local_avg,
                unit: "円".into(),
                context: format!("全国{:.0}円", national_avg),
            },
        ],
        related_tabs: vec!["workstyle", "analysis"],
    })
}

/// HS-3: 情報開示不足（transparency + text_quality）
fn hs3_transparency_gap(ctx: &InsightContext) -> Option<Insight> {
    let trans_row = ctx
        .transparency
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let avg_trans = get_f64(trans_row, "avg_transparency");
    if avg_trans >= TRANSPARENCY_WARNING {
        return None;
    }

    // 最も開示率が低い項目を特定
    let disclosure_items = [
        ("年間休日", "disclosure_annual_holidays"),
        ("賞与月数", "disclosure_bonus_months"),
        ("従業員数", "disclosure_employee_count"),
        ("残業時間", "disclosure_overtime"),
        ("女性比率", "disclosure_female_ratio"),
    ];
    let mut worst_item = "不明";
    let mut worst_rate = 1.0_f64;
    for (label, col) in &disclosure_items {
        let rate = get_f64(trans_row, col);
        if rate < worst_rate {
            worst_rate = rate;
            worst_item = label;
        }
    }

    let severity = if avg_trans < TRANSPARENCY_CRITICAL {
        Severity::Critical
    } else {
        Severity::Warning
    };

    Some(Insight {
        id: "HS-3".to_string(),
        category: InsightCategory::HiringStructure,
        severity,
        title: "求人情報の開示不足".to_string(),
        body: format!(
            "求人情報の開示度は{:.0}%と低く、特に「{}」の開示率が{:.0}%にとどまっています。\
             情報量が少ない求人は応募率が低下する傾向があります。",
            avg_trans * 100.0,
            worst_item,
            worst_rate * 100.0
        ),
        evidence: vec![
            Evidence {
                metric: "開示度".into(),
                value: avg_trans,
                unit: "%".into(),
                context: format!("閾値{:.0}%", TRANSPARENCY_WARNING * 100.0),
            },
            Evidence {
                metric: format!("{}開示率", worst_item),
                value: worst_rate,
                unit: "%".into(),
                context: "最低項目".into(),
            },
        ],
        related_tabs: vec!["analysis"],
    })
}

/// HS-4: テキスト温度と採用難の乖離
fn hs4_temperature_mismatch(ctx: &InsightContext) -> Option<Insight> {
    let temp_row = ctx
        .temperature
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let temperature = get_f64(temp_row, "temperature");
    let urgency_density = get_f64(temp_row, "urgency_density");

    let vac_row = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let vacancy_rate = get_f64(vac_row, "vacancy_rate");

    // 高欠員率なのにテキスト温度が低い = 乖離
    if vacancy_rate < VACANCY_CRITICAL || temperature >= TEMP_LOW_THRESHOLD {
        return None;
    }

    Some(Insight {
        id: "HS-4".to_string(),
        category: InsightCategory::HiringStructure,
        severity: Severity::Warning,
        title: "求人テキストと実態の乖離".to_string(),
        body: format!(
            "欠員率{:.1}%と高いにもかかわらず、求人文のテキスト温度は{:.2}と低く、\
             緊急性が伝わっていません。急募キーワードの密度は{:.2}です。",
            vacancy_rate * 100.0,
            temperature,
            urgency_density
        ),
        evidence: vec![
            Evidence {
                metric: "欠員率".into(),
                value: vacancy_rate,
                unit: "%".into(),
                context: "高水準".into(),
            },
            Evidence {
                metric: "テキスト温度".into(),
                value: temperature,
                unit: "".into(),
                context: format!("閾値{:.1}", TEMP_LOW_THRESHOLD),
            },
        ],
        related_tabs: vec!["analysis"],
    })
}

/// HS-5: 雇用者集中（独占的市場）
fn hs5_monopsony(ctx: &InsightContext) -> Option<Insight> {
    let mono_row = ctx
        .monopsony
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let hhi = get_f64(mono_row, "hhi");
    let top1_share = get_f64(mono_row, "top1_share");

    if hhi < HHI_CRITICAL && top1_share < TOP1_SHARE_CRITICAL {
        return None;
    }

    let severity = if hhi >= HHI_CRITICAL {
        Severity::Critical
    } else {
        Severity::Warning
    };

    Some(Insight {
        id: "HS-5".to_string(),
        category: InsightCategory::HiringStructure,
        severity,
        title: "求人市場の集中（独占的構造）".to_string(),
        body: format!(
            "上位1社が求人の{:.0}%を占め、HHI={:.3}と高い集中度です。\
             少数の事業者が賃金水準をコントロールしている可能性があります。",
            top1_share * 100.0,
            hhi
        ),
        evidence: vec![
            Evidence {
                metric: "HHI".into(),
                value: hhi,
                unit: "".into(),
                context: format!("閾値{}", HHI_CRITICAL),
            },
            Evidence {
                metric: "上位1社シェア".into(),
                value: top1_share,
                unit: "%".into(),
                context: format!("閾値{}%", (TOP1_SHARE_CRITICAL * 100.0) as i64),
            },
        ],
        related_tabs: vec!["analysis", "competitive"],
    })
}

/// HS-6: 空間的ミスマッチ（spatial_mismatch + 昼夜間人口）
fn hs6_spatial_mismatch(ctx: &InsightContext) -> Option<Insight> {
    let sm_row = ctx.spatial_mismatch.first()?;
    let isolation = get_f64(sm_row, "isolation_score");
    if isolation < ISOLATION_WARNING {
        return None;
    }

    // 昼夜間人口比を取得
    let daytime_ratio = ctx
        .ext_daytime_pop
        .first()
        .map(|r| get_f64(r, "daytime_ratio"))
        .unwrap_or(1.0);

    let body = if daytime_ratio < DAYTIME_POP_RATIO_LOW {
        format!(
            "孤立スコア{:.2}（通勤圏内のアクセス可能求人が少ない）。\
             昼夜間人口比{:.2}から、通勤で人口が流出するベッドタウン型の地域です。\
             求人エリアの拡大が有効です。",
            isolation, daytime_ratio
        )
    } else {
        format!(
            "孤立スコア{:.2}で、通勤圏内のアクセス可能求人が限られています。",
            isolation
        )
    };

    Some(Insight {
        id: "HS-6".to_string(),
        category: InsightCategory::HiringStructure,
        severity: Severity::Warning,
        title: "空間的ミスマッチ（通勤圏の制約）".to_string(),
        body,
        evidence: vec![
            Evidence {
                metric: "孤立スコア".into(),
                value: isolation,
                unit: "".into(),
                context: format!("閾値{:.1}", ISOLATION_WARNING),
            },
            Evidence {
                metric: "昼夜間人口比".into(),
                value: daytime_ratio,
                unit: "".into(),
                context: "1.0未満=ベッドタウン型".into(),
            },
        ],
        related_tabs: vec!["jobmap"],
    })
}

// ======== カテゴリ2: 将来予測 ========

fn analyze_forecast(ctx: &InsightContext) -> Vec<Insight> {
    let mut out = Vec::new();
    if let Some(i) = fc1_posting_trend(ctx) {
        out.push(i);
    }
    if let Some(i) = fc2_salary_pressure(ctx) {
        out.push(i);
    }
    if let Some(i) = fc3_population_outlook(ctx) {
        out.push(i);
    }
    if let Some(i) = fc4_fulfillment_worsening(ctx) {
        out.push(i);
    }
    out
}

/// FC-1: 求人量トレンド
fn fc1_posting_trend(ctx: &InsightContext) -> Option<Insight> {
    let values: Vec<f64> = ctx
        .ts_counts
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "posting_count"))
        .collect();
    let slope = linear_slope(&values)?;

    let (severity, direction) = if slope > TREND_INCREASE_THRESHOLD {
        (Severity::Info, "増加")
    } else if slope < TREND_DECREASE_THRESHOLD {
        (Severity::Warning, "減少")
    } else {
        (Severity::Positive, "横ばい")
    };

    let latest = values.last().copied().unwrap_or(0.0);
    let forecast_6m = (latest * (1.0 + slope * 6.0)).max(0.0);

    Some(Insight {
        id: "FC-1".to_string(),
        category: InsightCategory::Forecast,
        severity,
        title: format!("求人数は{}トレンド", direction),
        body: format!(
            "過去{}ヶ月で求人数は月平均{:+.1}%のペースで{}しています。\
             現在{:.0}件 → 6ヶ月後の推定{:.0}件。",
            values.len(),
            slope * 100.0,
            direction,
            latest,
            forecast_6m
        ),
        evidence: vec![Evidence {
            metric: "月次変化率".into(),
            value: slope,
            unit: "%/月".into(),
            context: format!("{}ヶ月線形回帰", values.len()),
        }],
        related_tabs: vec!["trend", "overview"],
    })
}

/// FC-2: 給与上昇圧力
fn fc2_salary_pressure(ctx: &InsightContext) -> Option<Insight> {
    let salary_vals: Vec<f64> = ctx
        .ts_salary
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "mean_min"))
        .collect();
    let salary_slope = linear_slope(&salary_vals)?;

    // 最低賃金の推移
    let min_wage_vals: Vec<f64> = ctx
        .ext_min_wage
        .iter()
        .map(|r| get_f64(r, "hourly_min_wage"))
        .filter(|&v| v > 0.0)
        .collect();
    let wage_slope_annual = linear_slope(&min_wage_vals).unwrap_or(0.0);
    // 年次→月次に換算して比較
    let wage_slope_monthly = wage_slope_annual / 12.0;

    let comparison = if salary_slope > wage_slope_monthly {
        "上回っています"
    } else if salary_slope < wage_slope_monthly * 0.5 {
        "大きく下回っています"
    } else {
        "ほぼ同水準です"
    };

    let severity = if salary_slope < wage_slope_monthly * 0.5 {
        Severity::Warning
    } else if salary_slope > wage_slope_monthly {
        Severity::Positive
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "FC-2".to_string(),
        category: InsightCategory::Forecast,
        severity,
        title: "給与上昇圧力の動向".to_string(),
        body: format!(
            "求人給与の月次上昇率({:+.2}%/月)と最低賃金の月次換算上昇率({:+.2}%/月)を比較すると、{}",
            salary_slope * 100.0, wage_slope_monthly * 100.0, comparison
        ),
        evidence: vec![
            Evidence { metric: "給与上昇率".into(), value: salary_slope, unit: "%/月".into(),
                context: "HW求人月次推移".into() },
        ],
        related_tabs: vec!["trend", "workstyle"],
    })
}

/// FC-3: 人口動態による労働力予測
fn fc3_population_outlook(ctx: &InsightContext) -> Option<Insight> {
    if ctx.ext_pyramid.is_empty() {
        return None;
    }

    // 人口ピラミッド（18区分: 5歳階級）から生産年齢人口を推計
    let mut working_age = 0.0_f64; // 15-64歳
    let mut retiring_soon = 0.0_f64; // 55-64歳

    for row in &ctx.ext_pyramid {
        let age = get_str_ref(row, "age_group");
        // male_count + female_count = 総人口（populationカラムは存在しない）
        let pop = get_f64(row, "male_count") + get_f64(row, "female_count");
        // 18区分(5歳階級): 0-4,5-9,10-14,15-19,...,80-84,85+
        // 9区分(10歳階級): 0-9,10-19,...,70-79,80+ (両方に対応)
        match age {
            // 18区分対応
            "15-19" | "20-24" | "25-29" | "30-34" | "35-39" | "40-44" | "45-49" | "50-54"
            | "55-59" | "60-64" => working_age += pop,
            // 9区分フォールバック
            "10-19" | "20-29" | "30-39" | "40-49" | "50-59" | "60-69" => working_age += pop,
            _ => {}
        }
        match age {
            "55-59" | "60-64" | "50-59" | "60-69" => retiring_soon += pop,
            _ => {}
        }
    }

    if working_age < 1.0 {
        return None;
    }
    let decline_rate = safe_divide(retiring_soon, working_age)?;

    // 転入転出
    let net_migration = ctx
        .ext_migration
        .first()
        .map(|r| {
            let in_m = get_f64(r, "inflow");
            let out_m = get_f64(r, "outflow");
            in_m - out_m
        })
        .unwrap_or(0.0);

    let severity = if decline_rate > 0.30 && net_migration < 0.0 {
        Severity::Critical
    } else if decline_rate > 0.25 {
        Severity::Warning
    } else {
        Severity::Info
    };

    let migration_text = if net_migration > 0.0 {
        "転入超過"
    } else if net_migration < 0.0 {
        "転出超過"
    } else {
        "均衡"
    };

    Some(Insight {
        id: "FC-3".to_string(),
        category: InsightCategory::Forecast,
        severity,
        title: "人口動態による労働力予測".to_string(),
        body: format!(
            "生産年齢人口のうち{:.0}%が55歳以上で、10年以内に大量退職が見込まれます。\
             人口移動は{}（純移動{:+.0}人）。{}",
            decline_rate * 100.0,
            migration_text,
            net_migration,
            if decline_rate > 0.25 && net_migration < 0.0 {
                "人口流出と高齢化の二重圧力で、労働力の確保が困難になるリスクがあります。"
            } else {
                ""
            }
        ),
        evidence: vec![
            Evidence {
                metric: "退職予備率".into(),
                value: decline_rate,
                unit: "%".into(),
                context: "55歳以上/生産年齢人口".into(),
            },
            Evidence {
                metric: "純移動数".into(),
                value: net_migration,
                unit: "人".into(),
                context: migration_text.into(),
            },
        ],
        related_tabs: vec!["trend", "overview"],
    })
}

/// FC-4: 充足困難度の悪化予兆
fn fc4_fulfillment_worsening(ctx: &InsightContext) -> Option<Insight> {
    let listing_days: Vec<f64> = ctx
        .ts_fulfillment
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "avg_listing_days"))
        .collect();
    let days_slope = linear_slope(&listing_days)?;

    let churn_rates: Vec<f64> = ctx
        .ts_tracking
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "churn_rate"))
        .collect();
    let churn_slope = linear_slope(&churn_rates).unwrap_or(0.0);

    // 両方が悪化傾向の場合のみ発火
    if days_slope <= 0.0 && churn_slope <= 0.0 {
        return None;
    }

    let latest_days = listing_days.last().copied().unwrap_or(0.0);
    let latest_churn = churn_rates.last().copied().unwrap_or(0.0);

    let severity = if days_slope > 0.03 && churn_slope > 0.02 {
        Severity::Warning
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "FC-4".to_string(),
        category: InsightCategory::Forecast,
        severity,
        title: "充足困難度の悪化傾向".to_string(),
        body: format!(
            "求人の平均掲載日数{:.0}日（月次変化{:+.1}%）、離職率{:.1}%（月次変化{:+.1}%）。\
             充足が困難になるリスクがあります。",
            latest_days,
            days_slope * 100.0,
            latest_churn * 100.0,
            churn_slope * 100.0
        ),
        evidence: vec![
            Evidence {
                metric: "掲載日数変化率".into(),
                value: days_slope,
                unit: "%/月".into(),
                context: "増加=悪化".into(),
            },
            Evidence {
                metric: "離職率変化率".into(),
                value: churn_slope,
                unit: "%/月".into(),
                context: "増加=悪化".into(),
            },
        ],
        related_tabs: vec!["trend"],
    })
}

// ======== カテゴリ3: 地域間比較 ========

fn analyze_regional_comparison(ctx: &InsightContext) -> Vec<Insight> {
    let mut out = Vec::new();
    if let Some(i) = rc1_benchmark_ranking(ctx) {
        out.push(i);
    }
    if let Some(i) = rc2_salary_gap(ctx) {
        out.push(i);
    }
    if let Some(i) = rc3_population_density_cross(ctx) {
        out.push(i);
    }
    out
}

/// RC-1: 総合ベンチマーク順位
fn rc1_benchmark_ranking(ctx: &InsightContext) -> Option<Insight> {
    if ctx.region_benchmark.is_empty() || ctx.muni.is_empty() {
        return None;
    }

    let my_row = ctx.region_benchmark.first()?;
    let composite = get_f64(my_row, "composite_benchmark");
    if composite <= 0.0 {
        return None;
    }

    let severity = if composite < 30.0 {
        Severity::Warning
    } else if composite > 70.0 {
        Severity::Positive
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "RC-1".to_string(),
        category: InsightCategory::RegionalCompare,
        severity,
        title: "地域総合ベンチマーク".to_string(),
        body: format!(
            "{}の総合ベンチマークスコアは{:.1}点です。{}",
            ctx.muni,
            composite,
            if composite < 30.0 {
                "県内で下位に位置しており、改善が必要です。"
            } else if composite > 70.0 {
                "県内で上位に位置しています。"
            } else {
                "県内で中位の水準です。"
            }
        ),
        evidence: vec![Evidence {
            metric: "ベンチマーク".into(),
            value: composite,
            unit: "点".into(),
            context: "0-100スケール".into(),
        }],
        related_tabs: vec!["analysis", "competitive"],
    })
}

/// RC-2: 給与・休日の地域差
fn rc2_salary_gap(ctx: &InsightContext) -> Option<Insight> {
    if ctx.cascade.is_empty() {
        return None;
    }

    let my_row = ctx
        .cascade
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let local_salary = get_f64(my_row, "avg_salary_min");
    let local_holidays = get_f64(my_row, "avg_annual_holidays");

    if local_salary <= 0.0 {
        return None;
    }

    // 全国平均との比較（salary_compテーブルから）
    let comp_row = ctx
        .salary_comp
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員");
    let national_avg = comp_row
        .map(|r| get_f64(r, "national_mean_min"))
        .unwrap_or(0.0);

    if national_avg <= 0.0 {
        return None;
    }
    let diff = local_salary - national_avg;

    let severity = if diff < -20000.0 {
        Severity::Warning
    } else if diff > 10000.0 {
        Severity::Positive
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "RC-2".to_string(),
        category: InsightCategory::RegionalCompare,
        severity,
        title: "給与水準の地域差".to_string(),
        body: format!(
            "正社員の平均月給{:.0}円は全国平均比{:+.0}円。年間休日は平均{:.0}日。",
            local_salary, diff, local_holidays
        ),
        evidence: vec![
            Evidence {
                metric: "平均月給".into(),
                value: local_salary,
                unit: "円".into(),
                context: format!("全国{:.0}円", national_avg),
            },
            Evidence {
                metric: "全国比差額".into(),
                value: diff,
                unit: "円".into(),
                context: "マイナス=下回る".into(),
            },
        ],
        related_tabs: vec!["workstyle", "competitive"],
    })
}

/// RC-3: 人口構造×求人密度クロス
fn rc3_population_density_cross(ctx: &InsightContext) -> Option<Insight> {
    if ctx.ext_population.is_empty() {
        return None;
    }

    let pop_row = ctx.ext_population.first()?;
    let total_pop = get_f64(pop_row, "total_population");
    if total_pop < 100.0 {
        return None;
    }

    // 求人数合計
    let total_postings: f64 = ctx.vacancy.iter().map(|r| get_f64(r, "total_count")).sum();

    let density = safe_divide(total_postings * 1000.0, total_pop)?;

    let severity = if density > 50.0 {
        Severity::Warning
    } else if density < 5.0 {
        Severity::Positive
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "RC-3".to_string(),
        category: InsightCategory::RegionalCompare,
        severity,
        title: "人口あたりの求人密度".to_string(),
        body: format!(
            "人口{:.0}人に対し求人{:.0}件（1,000人あたり{:.1}件）。{}",
            total_pop,
            total_postings,
            density,
            if density > 50.0 {
                "求人密度が非常に高く、採用競争が激しい地域です。"
            } else if density < 5.0 {
                "求人密度が低く、比較的穏やかな採用環境です。"
            } else {
                ""
            }
        ),
        evidence: vec![Evidence {
            metric: "求人密度".into(),
            value: density,
            unit: "件/千人".into(),
            context: "人口対比".into(),
        }],
        related_tabs: vec!["overview", "jobmap"],
    })
}

// ======== カテゴリ4: アクション提案 ========

fn generate_action_proposals(ctx: &InsightContext, existing: &[Insight]) -> Vec<Insight> {
    let mut out = Vec::new();

    // AP-1: HS-2（給与競争力不足）が発火していれば給与改善提案
    if existing.iter().any(|i| i.id == "HS-2") {
        if let Some(i) = ap1_salary_improvement(ctx) {
            out.push(i);
        }
    }
    // AP-2: HS-3 or HS-4 が発火していれば求人原稿改善提案
    if existing.iter().any(|i| i.id == "HS-3" || i.id == "HS-4") {
        if let Some(i) = ap2_posting_improvement(ctx) {
            out.push(i);
        }
    }
    // AP-3: HS-6 が発火していれば採用エリア拡大提案
    if existing.iter().any(|i| i.id == "HS-6") {
        if let Some(i) = ap3_area_expansion(ctx) {
            out.push(i);
        }
    }

    out
}

/// AP-1: 給与改善提案
fn ap1_salary_improvement(ctx: &InsightContext) -> Option<Insight> {
    let comp_row = ctx
        .salary_comp
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let local_avg = get_f64(comp_row, "local_mean_min");
    let national_median = get_f64(comp_row, "national_median_min");
    if national_median <= 0.0 || local_avg <= 0.0 {
        return None;
    }

    let increase = national_median - local_avg;
    if increase <= 0.0 {
        return None;
    }
    let annual_cost = increase * 12.0;

    Some(Insight {
        id: "AP-1".to_string(),
        category: InsightCategory::ActionProposal,
        severity: Severity::Info,
        title: "給与水準の改善提案".to_string(),
        body: format!(
            "月給を{:+.0}円引き上げ({:.0}円→{:.0}円)すれば全国中央値に到達できます。\
             1人あたり年間人件費増は約{:.0}円です。",
            increase, local_avg, national_median, annual_cost
        ),
        evidence: vec![
            Evidence {
                metric: "必要増額".into(),
                value: increase,
                unit: "円/月".into(),
                context: "全国中央値到達".into(),
            },
            Evidence {
                metric: "年間コスト増".into(),
                value: annual_cost,
                unit: "円/人".into(),
                context: "12ヶ月換算".into(),
            },
        ],
        related_tabs: vec!["workstyle"],
    })
}

/// AP-2: 求人原稿改善提案
fn ap2_posting_improvement(ctx: &InsightContext) -> Option<Insight> {
    let trans_row = ctx
        .transparency
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;

    // 未開示項目のリストアップ
    let items = [
        ("年間休日", "disclosure_annual_holidays"),
        ("賞与月数", "disclosure_bonus_months"),
        ("従業員数", "disclosure_employee_count"),
        ("残業時間", "disclosure_overtime"),
        ("女性比率", "disclosure_female_ratio"),
        ("パート比率", "disclosure_parttime_ratio"),
    ];
    let missing: Vec<&str> = items
        .iter()
        .filter(|(_, col)| get_f64(trans_row, col) < 0.3)
        .map(|(label, _)| *label)
        .collect();

    if missing.is_empty() {
        return None;
    }

    Some(Insight {
        id: "AP-2".to_string(),
        category: InsightCategory::ActionProposal,
        severity: Severity::Info,
        title: "求人原稿の改善提案".to_string(),
        body: format!(
            "以下の情報を追加開示してください: {}。\
             情報量が多い求人は応募率が高まる傾向があります。",
            missing.join("、")
        ),
        evidence: vec![Evidence {
            metric: "未開示項目数".into(),
            value: missing.len() as f64,
            unit: "件".into(),
            context: "開示率30%未満".into(),
        }],
        related_tabs: vec!["analysis"],
    })
}

/// AP-3: 採用エリア拡大提案
fn ap3_area_expansion(ctx: &InsightContext) -> Option<Insight> {
    let daytime_ratio = ctx
        .ext_daytime_pop
        .first()
        .map(|r| get_f64(r, "daytime_ratio"))
        .unwrap_or(1.0);

    if daytime_ratio >= 1.0 {
        return None;
    } // 昼間人口 > 夜間人口の都市部では不要

    Some(Insight {
        id: "AP-3".to_string(),
        category: InsightCategory::ActionProposal,
        severity: Severity::Info,
        title: "採用エリアの拡大提案".to_string(),
        body: format!(
            "昼夜間人口比{:.2}（ベッドタウン型）のため、通勤圏の求職者にリーチできていない可能性があります。\
             近隣都市部への求人掲載エリア拡大を検討してください。",
            daytime_ratio
        ),
        evidence: vec![
            Evidence { metric: "昼夜間人口比".into(), value: daytime_ratio, unit: "".into(),
                context: "1.0未満=ベッドタウン".into() },
        ],
        related_tabs: vec!["jobmap"],
    })
}

// ======== カテゴリ5: 通勤圏分析 ========

fn analyze_commute_zone(ctx: &InsightContext) -> Vec<Insight> {
    let mut out = Vec::new();
    if ctx.commute_zone_count == 0 && ctx.commute_inflow_total == 0 {
        return out;
    }
    // 距離ベース
    if ctx.commute_zone_count > 0 {
        if let Some(i) = cz1_population_distribution(ctx) {
            out.push(i);
        }
        if let Some(i) = cz2_salary_gap(ctx) {
            out.push(i);
        }
        if let Some(i) = cz3_aging_risk(ctx) {
            out.push(i);
        }
    }
    // 実フローベース（国勢調査OD）
    if ctx.commute_inflow_total > 0 {
        if let Some(i) = cf1_actual_commute_zone(ctx) {
            out.push(i);
        }
        if let Some(i) = cf2_inflow_targeting(ctx) {
            out.push(i);
        }
        if let Some(i) = cf3_self_commute_analysis(ctx) {
            out.push(i);
        }
    }
    out
}

/// CZ-1: 通勤圏人口ポテンシャル
fn cz1_population_distribution(ctx: &InsightContext) -> Option<Insight> {
    if ctx.commute_zone_total_pop <= 0 {
        return None;
    }
    let local_pop: i64 = ctx
        .ext_pyramid
        .iter()
        .map(|r| get_f64(r, "male_count") as i64 + get_f64(r, "female_count") as i64)
        .sum();
    if local_pop <= 0 {
        return None;
    }
    let local_share = local_pop as f64 / ctx.commute_zone_total_pop as f64;

    Some(Insight {
        id: "CZ-1".to_string(),
        category: InsightCategory::RegionalCompare,
        severity: if local_share < 0.05 {
            Severity::Positive
        } else {
            Severity::Info
        },
        title: "通勤圏の人口ポテンシャル".to_string(),
        body: format!(
            "30km通勤圏内に{}市区町村（{}県）、総人口{}人。{}は圏内の{:.1}%。{}",
            ctx.commute_zone_count,
            ctx.commute_zone_pref_count,
            super::super::helpers::format_number(ctx.commute_zone_total_pop),
            ctx.muni,
            local_share * 100.0,
            if local_share < 0.05 {
                "広域採用戦略が有効です。"
            } else {
                ""
            }
        ),
        evidence: vec![Evidence {
            metric: "通勤圏人口".into(),
            value: ctx.commute_zone_total_pop as f64,
            unit: "人".into(),
            context: format!("{}市区町村", ctx.commute_zone_count),
        }],
        related_tabs: vec!["analysis"],
    })
}

/// CZ-2: 通勤圏給与格差
fn cz2_salary_gap(ctx: &InsightContext) -> Option<Insight> {
    let local_row = ctx
        .cascade
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let local_sal = get_f64(local_row, "avg_salary_min");
    let sm_row = ctx
        .spatial_mismatch
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .or_else(|| ctx.spatial_mismatch.first())?;
    let acc_sal = get_f64(sm_row, "accessible_avg_salary_30km");
    if local_sal <= 0.0 || acc_sal <= 0.0 {
        return None;
    }
    let gap_pct = (local_sal - acc_sal) / acc_sal * 100.0;

    Some(Insight {
        id: "CZ-2".to_string(),
        category: InsightCategory::RegionalCompare,
        severity: if gap_pct < -10.0 {
            Severity::Warning
        } else if gap_pct > 5.0 {
            Severity::Positive
        } else {
            Severity::Info
        },
        title: "通勤圏内の給与格差".to_string(),
        body: format!(
            "地元月給({:.0}円) vs 圏内平均({:.0}円) = {:+.1}%。{}",
            local_sal,
            acc_sal,
            gap_pct,
            if gap_pct < -10.0 {
                "周辺に人材が流出するリスクあり。"
            } else if gap_pct > 5.0 {
                "人材を引き付けやすい環境。"
            } else {
                ""
            }
        ),
        evidence: vec![Evidence {
            metric: "給与差".into(),
            value: gap_pct,
            unit: "%".into(),
            context: "30km圏比".into(),
        }],
        related_tabs: vec!["analysis", "competitive"],
    })
}

/// CZ-3: 通勤圏高齢化リスク
fn cz3_aging_risk(ctx: &InsightContext) -> Option<Insight> {
    if ctx.commute_zone_total_pop <= 0 {
        return None;
    }
    let rate = ctx.commute_zone_elderly as f64 / ctx.commute_zone_total_pop as f64;
    if rate < 0.20 {
        return None;
    }

    Some(Insight {
        id: "CZ-3".to_string(),
        category: InsightCategory::Forecast,
        severity: if rate > 0.30 {
            Severity::Warning
        } else {
            Severity::Info
        },
        title: "通勤圏の高齢化動向".to_string(),
        body: format!(
            "通勤圏高齢化率{:.1}%。生産年齢人口{}人。{}",
            rate * 100.0,
            super::super::helpers::format_number(ctx.commute_zone_working_age),
            if rate > 0.30 {
                "長期的な労働力減少が懸念されます。"
            } else {
                ""
            }
        ),
        evidence: vec![Evidence {
            metric: "高齢化率".into(),
            value: rate,
            unit: "%".into(),
            context: "通勤圏30km".into(),
        }],
        related_tabs: vec!["trend", "analysis"],
    })
}

// ======== 通勤フロー実データ示唆（国勢調査OD） ========

/// CF-1: 実通勤圏の発見（距離ポテンシャル vs 実フロー）
fn cf1_actual_commute_zone(ctx: &InsightContext) -> Option<Insight> {
    if ctx.commute_zone_total_pop <= 0 || ctx.commute_inflow_total <= 0 {
        return None;
    }

    let actual_ratio = ctx.commute_inflow_total as f64 / ctx.commute_zone_total_pop as f64;
    let top3_text: Vec<String> = ctx
        .commute_inflow_top3
        .iter()
        .map(|(p, m, c)| format!("{}{}({}人)", p, m, super::super::helpers::format_number(*c)))
        .collect();

    Some(Insight {
        id: "CF-1".to_string(),
        category: InsightCategory::RegionalCompare,
        severity: if actual_ratio < 0.01 {
            Severity::Warning
        } else {
            Severity::Info
        },
        title: "実通勤フローの発見".to_string(),
        body: format!(
            "距離圏(30km)の人口{}人に対し、実際の通勤流入は{}人({:.2}%)。主要流入元: {}。{}",
            super::super::helpers::format_number(ctx.commute_zone_total_pop),
            super::super::helpers::format_number(ctx.commute_inflow_total),
            actual_ratio * 100.0,
            if top3_text.is_empty() {
                "データなし".to_string()
            } else {
                top3_text.join("、")
            },
            if actual_ratio < 0.01 {
                "距離ポテンシャルと実態の乖離が大きく、地理的障壁の可能性。"
            } else {
                ""
            }
        ),
        evidence: vec![
            Evidence {
                metric: "実通勤流入".into(),
                value: ctx.commute_inflow_total as f64,
                unit: "人".into(),
                context: "国勢調査2020".into(),
            },
            Evidence {
                metric: "距離圏人口比".into(),
                value: actual_ratio,
                unit: "%".into(),
                context: "実流入/距離圏人口".into(),
            },
        ],
        related_tabs: vec!["analysis"],
    })
}

/// CF-2: 流入元ターゲティング
fn cf2_inflow_targeting(ctx: &InsightContext) -> Option<Insight> {
    if ctx.commute_inflow_top3.is_empty() {
        return None;
    }

    let (top_pref, top_muni, top_count) = &ctx.commute_inflow_top3[0];
    let is_cross_pref = top_pref != &ctx.pref;

    Some(Insight {
        id: "CF-2".to_string(),
        category: InsightCategory::ActionProposal,
        severity: Severity::Info,
        title: "通勤流入元ターゲティング".to_string(),
        body: format!(
            "最大の通勤流入元は{}{} ({}人)。{}求人掲載エリアにこの地域を追加することで応募者プールの拡大が見込めます。",
            top_pref, top_muni,
            super::super::helpers::format_number(*top_count),
            if is_cross_pref { format!("都道府県をまたぐ通勤フロー（{}→{}）。", top_pref, ctx.pref) }
            else { String::new() }
        ),
        evidence: vec![
            Evidence { metric: "最大流入元".into(), value: *top_count as f64, unit: "人".into(),
                context: format!("{}{}", top_pref, top_muni) },
        ],
        related_tabs: vec!["analysis", "jobmap"],
    })
}

/// CF-3: 地元就業率分析
fn cf3_self_commute_analysis(ctx: &InsightContext) -> Option<Insight> {
    if ctx.commute_self_rate <= 0.0 {
        return None;
    }

    let severity = if ctx.commute_self_rate > 0.7 {
        Severity::Positive
    } else if ctx.commute_self_rate < 0.3 {
        Severity::Warning
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "CF-3".to_string(),
        category: InsightCategory::HiringStructure,
        severity,
        title: "地元就業率".to_string(),
        body: format!(
            "住民の{:.1}%が地元で就業。{}通勤流出先への人材流出は{}人。",
            ctx.commute_self_rate * 100.0,
            if ctx.commute_self_rate < 0.3 {
                "地元就業率が低く、多くの住民が他地域に流出。"
            } else if ctx.commute_self_rate > 0.7 {
                "地元就業率が高く、地域内で労働力が循環。"
            } else {
                ""
            },
            super::super::helpers::format_number(ctx.commute_outflow_total),
        ),
        evidence: vec![
            Evidence {
                metric: "地元就業率".into(),
                value: ctx.commute_self_rate,
                unit: "%".into(),
                context: "国勢調査2020".into(),
            },
            Evidence {
                metric: "通勤流出".into(),
                value: ctx.commute_outflow_total as f64,
                unit: "人".into(),
                context: "他市区町村への通勤者".into(),
            },
        ],
        related_tabs: vec!["analysis", "overview"],
    })
}

// ======== カテゴリ6: 構造分析（Phase A、SSDSE-Aベース）========
//
// 市区町村レベルの構造指標（世帯・労働力・医療福祉・教育施設・地理）から、
// HW求人市場では見えない「採用構造の背景」を示唆する。
// 全関数で文末「傾向がみられます」「可能性があります」等の非断定表現を強制し、
// phrase_validator で検証する（相関≠因果原則）。

fn analyze_structural_context(ctx: &InsightContext) -> Vec<Insight> {
    let mut out = Vec::new();

    if let Some(insight) = ls1_employment_capacity(ctx) {
        assert_valid_phrase(&insight.body);
        out.push(insight);
    }
    if let Some(insight) = ls2_industry_concentration(ctx) {
        assert_valid_phrase(&insight.body);
        out.push(insight);
    }
    if let Some(insight) = hh1_single_household_persona(ctx) {
        assert_valid_phrase(&insight.body);
        out.push(insight);
    }
    if let Some(insight) = mf1_medical_welfare_density(ctx) {
        assert_valid_phrase(&insight.body);
        out.push(insight);
    }
    if let Some(insight) = in1_industry_mismatch(ctx) {
        assert_valid_phrase(&insight.body);
        out.push(insight);
    }
    if let Some(insight) = ge1_habitable_density(ctx) {
        assert_valid_phrase(&insight.body);
        out.push(insight);
    }

    out
}

/// LS-1: 採用余力シグナル（SSDSE-A labor_force + pref_avg_unemployment_rate ベース）
///
/// 失業率が県平均 × 1.2 を超え、かつ pref_avg が有意な場合に発火。
/// ただし HW求人数との比率は v2_posting_mesh1km 実装後に拡張予定（現状は失業率のみで判定）。
fn ls1_employment_capacity(ctx: &InsightContext) -> Option<Insight> {
    let lf = ctx.ext_labor_force.first()?;
    let unemp = get_f64(lf, "unemployment_rate");
    if unemp.is_nan() || unemp <= 0.0 {
        return None;
    }
    let pref_avg = ctx.pref_avg_unemployment_rate?;
    if pref_avg <= 0.0 {
        return None;
    }
    let ratio = unemp / pref_avg;
    if ratio < UNEMPLOYMENT_RATE_MULTIPLIER_WARNING {
        return None;
    }
    let severity = if ratio >= UNEMPLOYMENT_RATE_MULTIPLIER_CRITICAL {
        Severity::Critical
    } else {
        Severity::Warning
    };
    let unemployed_count = get_f64(lf, "unemployed");

    Some(Insight {
        id: "LS-1".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "採用余力シグナル".to_string(),
        body: format!(
            "失業率が{:.2}%（県平均{:.2}%の{:.2}倍）で、未マッチ層が約{:.0}人いる可能性があります。採用余力がうかがえる傾向がみられます。",
            unemp, pref_avg, ratio, unemployed_count
        ),
        evidence: vec![
            Evidence {
                metric: "失業率".into(),
                value: unemp,
                unit: "%".into(),
                context: format!("県平均{:.2}%", pref_avg),
            },
            Evidence {
                metric: "県平均比".into(),
                value: ratio,
                unit: "倍".into(),
                context: format!("閾値{:.1}倍以上", UNEMPLOYMENT_RATE_MULTIPLIER_WARNING),
            },
        ],
        related_tabs: vec!["overview", "analysis"],
    })
}

/// LS-2: 産業偏在リスク（3次産業85%超 or 1次産業20%超）
///
/// SUM方式で計算（SUM(tertiary) / SUM(employed) × 100）。
/// 単一産業への依存度が高いと、業種転換余地が限定的な傾向を示唆する。
fn ls2_industry_concentration(ctx: &InsightContext) -> Option<Insight> {
    let lf = ctx.ext_labor_force.first()?;
    let employed = get_f64(lf, "employed");
    if employed <= 0.0 {
        return None;
    }
    let tertiary = get_f64(lf, "tertiary_industry_employed");
    let primary = get_f64(lf, "primary_industry_employed");
    let tertiary_share = tertiary / employed * 100.0;
    let primary_share = primary / employed * 100.0;

    let (industry_label, share, threshold) = if tertiary_share > TERTIARY_CONCENTRATION_THRESHOLD {
        ("第三次産業", tertiary_share, TERTIARY_CONCENTRATION_THRESHOLD)
    } else if primary_share > PRIMARY_CONCENTRATION_THRESHOLD {
        ("第一次産業", primary_share, PRIMARY_CONCENTRATION_THRESHOLD)
    } else {
        return None;
    };

    let severity = if share > threshold + 10.0 {
        Severity::Warning
    } else {
        Severity::Info
    };

    Some(Insight {
        id: "LS-2".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "産業偏在リスク".to_string(),
        body: format!(
            "{}就業者比率が{:.1}%（閾値{:.0}%）と高く、単一産業への依存により業種転換の余地が限定的な傾向がみられます。",
            industry_label, share, threshold
        ),
        evidence: vec![
            Evidence {
                metric: format!("{}比率", industry_label),
                value: share,
                unit: "%".into(),
                context: format!("閾値{:.0}%以上", threshold),
            },
            Evidence {
                metric: "就業者総数".into(),
                value: employed,
                unit: "人".into(),
                context: "市区町村合計".into(),
            },
        ],
        related_tabs: vec!["analysis", "overview"],
    })
}

/// HH-1: 単独世帯型求職者推定（単独世帯率 > 40%）
///
/// 単独世帯率が高いと、転居可能な求職者層（若年・単身）が多い可能性を示唆。
fn hh1_single_household_persona(ctx: &InsightContext) -> Option<Insight> {
    let hh = ctx.ext_households.first()?;
    let single_rate = get_f64(hh, "single_rate");
    if single_rate.is_nan() || single_rate <= SINGLE_HOUSEHOLD_RATE_THRESHOLD {
        return None;
    }
    let single_count = get_f64(hh, "single_households");
    let pref_avg = ctx.pref_avg_single_rate;

    let severity = if single_rate > SINGLE_HOUSEHOLD_RATE_THRESHOLD + 15.0 {
        Severity::Warning
    } else {
        Severity::Info
    };

    let avg_text = match pref_avg {
        Some(p) if p > 0.0 => format!("（県平均{:.1}%）", p),
        _ => String::new(),
    };

    Some(Insight {
        id: "HH-1".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "単独世帯型求職者層".to_string(),
        body: format!(
            "単独世帯率が{:.1}%{}と高く、単独世帯数{:.0}世帯。転居可能な求職者層が多い可能性がみられます。",
            single_rate, avg_text, single_count
        ),
        evidence: vec![Evidence {
            metric: "単独世帯率".into(),
            value: single_rate,
            unit: "%".into(),
            context: format!("閾値{:.0}%以上", SINGLE_HOUSEHOLD_RATE_THRESHOLD),
        }],
        related_tabs: vec!["demographics", "overview"],
    })
}

/// MF-1: 医療福祉供給密度ギャップ
///
/// 人口1万人あたり医師数が県平均× 0.8 を下回る（医療系求人採用難の可能性）。
/// pref_avg_physicians_per_10k は build_insight_context ではなく本関数内で動的計算する
/// （人口データとの相互依存を回避）。
fn mf1_medical_welfare_density(ctx: &InsightContext) -> Option<Insight> {
    let mw = ctx.ext_medical_welfare.first()?;
    let pop_row = ctx.ext_population.first()?;
    let total_pop = get_f64(pop_row, "total_population");
    if total_pop <= 0.0 {
        return None;
    }
    let physicians = get_f64(mw, "physicians");
    if physicians.is_nan() || physicians < 0.0 {
        return None;
    }
    let local_density = physicians / total_pop * 10_000.0;

    // 県平均: get_f64 で pref_avg ベース（本来は別途事前計算、ここでは daycare/children から推定不可のためスキップ可）
    // 代替として全国参考値（2022年公式: 約27人/10万人 = 2.7人/1万人）を使用
    const NATIONAL_PHYSICIANS_PER_10K: f64 = 27.0;
    let ratio = if NATIONAL_PHYSICIANS_PER_10K > 0.0 {
        local_density / NATIONAL_PHYSICIANS_PER_10K
    } else {
        return None;
    };

    if ratio >= MEDICAL_DENSITY_GAP_RATIO {
        return None;
    }

    let severity = if ratio < MEDICAL_DENSITY_CRITICAL_RATIO {
        Severity::Critical
    } else {
        Severity::Warning
    };

    Some(Insight {
        id: "MF-1".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "医療供給密度ギャップ".to_string(),
        body: format!(
            "人口1万人あたり医師数が{:.2}人（全国参考値{:.1}人の{:.0}%）と、医療系求人が採りにくい構造的傾向がみられます。",
            local_density,
            NATIONAL_PHYSICIANS_PER_10K,
            ratio * 100.0
        ),
        evidence: vec![
            Evidence {
                metric: "医師数".into(),
                value: physicians,
                unit: "人".into(),
                context: "市区町村内".into(),
            },
            Evidence {
                metric: "10k人あたり".into(),
                value: local_density,
                unit: "人".into(),
                context: format!("全国参考{:.1}人", NATIONAL_PHYSICIANS_PER_10K),
            },
        ],
        related_tabs: vec!["overview", "analysis"],
    })
}

/// IN-1: 産業構造ミスマッチ（事業所業種分布 vs HW求人業種分布のコサイン類似度）
///
/// 現Phase Aでは HW求人業種分布を直接取得していないため、
/// 簡易版: 事業所のうち医療福祉（C210850）比率と HW全体の欠員率の乖離を代替指標とする。
/// フル実装（コサイン類似度）は `industry_mapping.py` ssdse_mapping と合わせて Phase B で拡張。
fn in1_industry_mismatch(ctx: &InsightContext) -> Option<Insight> {
    if ctx.ext_establishments.is_empty() {
        return None;
    }
    let medical_welfare_row = ctx
        .ext_establishments
        .iter()
        .find(|r| get_str_ref(r, "industry") == "850")?;
    let mw_count = get_f64(medical_welfare_row, "establishment_count");
    let total: f64 = ctx
        .ext_establishments
        .iter()
        .map(|r| get_f64(r, "establishment_count"))
        .sum();
    if total <= 0.0 {
        return None;
    }
    let mw_share = mw_count / total;

    // HW求人の医療福祉系欠員率との乖離を簡易判定
    // vacancy は正社員全体のみ。今回は mw_share が20%以上で Warning、10%以下で Info
    let severity = if !(0.05..=0.3).contains(&mw_share) {
        Severity::Info
    } else {
        return None;
    };

    Some(Insight {
        id: "IN-1".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "産業構造の偏り".to_string(),
        body: format!(
            "事業所のうち医療・福祉が{:.1}%を占めており、HW求人職種分布と構造的な乖離がある可能性がうかがえます。",
            mw_share * 100.0
        ),
        evidence: vec![Evidence {
            metric: "医療・福祉事業所比率".into(),
            value: mw_share * 100.0,
            unit: "%".into(),
            context: "全業種合計比".into(),
        }],
        related_tabs: vec!["analysis", "overview"],
    })
}

/// GE-1: 可住地密度ペナルティ（過密 or 過疎）
///
/// 可住地面積あたり人口密度が極端（>10,000 or <50 人/km²）な場合、
/// 通勤圏の広がりや採用母集団の確保に影響を及ぼす傾向を示唆。
fn ge1_habitable_density(ctx: &InsightContext) -> Option<Insight> {
    let geo = ctx.ext_geography.first()?;
    let habitable = get_f64(geo, "habitable_area_km2");
    if habitable <= 0.0 {
        return None;
    }
    let pop_row = ctx.ext_population.first()?;
    let total_pop = get_f64(pop_row, "total_population");
    if total_pop <= 0.0 {
        return None;
    }
    let density = total_pop / habitable;

    let (pattern, severity) = if density > HABITABLE_DENSITY_CRITICAL_MAX {
        ("過密", Severity::Warning)
    } else if density < HABITABLE_DENSITY_CRITICAL_MIN {
        ("極端な過疎", Severity::Warning)
    } else if density > HABITABLE_DENSITY_MAX {
        ("過密傾向", Severity::Info)
    } else if density < HABITABLE_DENSITY_MIN {
        ("過疎傾向", Severity::Info)
    } else {
        return None;
    };

    Some(Insight {
        id: "GE-1".to_string(),
        category: InsightCategory::StructuralContext,
        severity,
        title: "可住地密度ペナルティ".to_string(),
        body: format!(
            "可住地面積{:.1}km²あたり人口密度が{:.0}人/km²と{}で、通勤圏を広げないと採用難の可能性があります。",
            habitable, density, pattern
        ),
        evidence: vec![
            Evidence {
                metric: "可住地人口密度".into(),
                value: density,
                unit: "人/km²".into(),
                context: format!("標準範囲{}-{}人/km²", HABITABLE_DENSITY_MIN as i64, HABITABLE_DENSITY_MAX as i64),
            },
            Evidence {
                metric: "可住地面積".into(),
                value: habitable,
                unit: "km²".into(),
                context: "".into(),
            },
        ],
        related_tabs: vec!["jobmap", "overview"],
    })
}
