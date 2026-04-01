//! 示唆エンジン用定数・閾値・フォーマッタ

use super::super::helpers::Row;

/// サブタブ定義
pub(crate) const INSIGHT_SUBTABS: [(u8, &str); 4] = [
    (1, "採用構造"),
    (2, "将来予測"),
    (3, "地域比較"),
    (4, "アクション"),
];

// ======== 示唆カテゴリ ========

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum InsightCategory {
    HiringStructure,  // 採用構造分析
    Forecast,         // 将来予測
    RegionalCompare,  // 地域間比較
    ActionProposal,   // アクション提案
}

impl InsightCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::HiringStructure => "採用構造",
            Self::Forecast => "将来予測",
            Self::RegionalCompare => "地域比較",
            Self::ActionProposal => "アクション",
        }
    }

    pub fn icon_class(&self) -> &'static str {
        match self {
            Self::HiringStructure => "text-red-400",
            Self::Forecast => "text-amber-400",
            Self::RegionalCompare => "text-blue-400",
            Self::ActionProposal => "text-green-400",
        }
    }

    pub fn subtab_id(&self) -> u8 {
        match self {
            Self::HiringStructure => 1,
            Self::Forecast => 2,
            Self::RegionalCompare => 3,
            Self::ActionProposal => 4,
        }
    }
}

// ======== 重要度 ========

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Severity {
    Critical = 0,
    Warning = 1,
    Info = 2,
    Positive = 3,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Critical => "重大",
            Self::Warning => "注意",
            Self::Info => "情報",
            Self::Positive => "良好",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::Critical => "#ef4444",  // red-500
            Self::Warning => "#f59e0b",   // amber-500
            Self::Info => "#3b82f6",      // blue-500
            Self::Positive => "#10b981",  // emerald-500
        }
    }

    pub fn bg_class(&self) -> &'static str {
        match self {
            Self::Critical => "bg-red-500/10 border-red-500/30",
            Self::Warning => "bg-amber-500/10 border-amber-500/30",
            Self::Info => "bg-blue-500/10 border-blue-500/30",
            Self::Positive => "bg-emerald-500/10 border-emerald-500/30",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Critical => "bg-red-500/20 text-red-400",
            Self::Warning => "bg-amber-500/20 text-amber-400",
            Self::Info => "bg-blue-500/20 text-blue-400",
            Self::Positive => "bg-emerald-500/20 text-emerald-400",
        }
    }
}

// ======== 示唆構造体 ========

#[derive(Clone, Debug, serde::Serialize)]
pub struct Evidence {
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub context: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Insight {
    pub id: String,
    pub category: InsightCategory,
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub evidence: Vec<Evidence>,
    pub related_tabs: Vec<&'static str>,
}

// ======== 閾値定数 ========

/// HS-1: 慢性的人材不足
pub const VACANCY_CRITICAL: f64 = 0.30;
pub const VACANCY_WARNING: f64 = 0.20;
pub const VACANCY_TREND_THRESHOLD: f64 = 0.25;

/// HS-2: 給与競争力
pub const SALARY_COMP_CRITICAL: f64 = 0.80;
pub const SALARY_COMP_WARNING: f64 = 0.90;

/// HS-3: 情報開示
pub const TRANSPARENCY_CRITICAL: f64 = 0.40;
pub const TRANSPARENCY_WARNING: f64 = 0.50;

/// HS-4: テキスト温度
pub const TEMP_LOW_THRESHOLD: f64 = 0.0;

/// HS-5: 雇用者集中
pub const HHI_CRITICAL: f64 = 0.25;
pub const TOP1_SHARE_CRITICAL: f64 = 0.30;

/// HS-6: 空間ミスマッチ
pub const ISOLATION_WARNING: f64 = 0.50;
pub const DAYTIME_POP_RATIO_LOW: f64 = 0.90;

/// FC-1: トレンド判定
pub const TREND_INCREASE_THRESHOLD: f64 = 0.05;
pub const TREND_DECREASE_THRESHOLD: f64 = -0.05;

// ======== ヘルパー関数 ========

/// 安全な除算（ゼロ除算ガード）
pub fn safe_divide(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator.abs() < f64::EPSILON { None } else { Some(numerator / denominator) }
}

/// f64取得（NaN/Infガード付き）
pub fn get_f64_safe(row: &Row, key: &str) -> f64 {
    let v = super::super::helpers::get_f64(row, key);
    if v.is_nan() || v.is_infinite() { 0.0 } else { v }
}

/// 線形回帰の傾きを計算（正規化済み: 月あたり変化率）
pub fn linear_slope(values: &[f64]) -> Option<f64> {
    let n = values.len();
    if n < 3 { return None; }
    let n_f = n as f64;
    let x_mean = (n_f - 1.0) / 2.0;
    let y_mean: f64 = values.iter().sum::<f64>() / n_f;
    if y_mean.abs() < f64::EPSILON { return None; }

    let mut num = 0.0;
    let mut den = 0.0;
    for (i, &y) in values.iter().enumerate() {
        let x = i as f64 - x_mean;
        num += x * (y - y_mean);
        den += x * x;
    }
    if den.abs() < f64::EPSILON { return None; }
    // 月あたり変化率として正規化
    Some((num / den) / y_mean)
}
