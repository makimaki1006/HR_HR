//! 示唆エンジン用定数・閾値・フォーマッタ

use super::super::helpers::Row;

/// サブタブ定義
/// 旧 "アクション" タブ (id=4) は冗長のため 2026-04-23 削除。
/// アクション系示唆は採用構造/将来予測/地域比較/構造分析の各 insight card body に統合済。
pub(crate) const INSIGHT_SUBTABS: [(u8, &str); 4] = [
    (1, "採用構造"),
    (2, "将来予測"),
    (3, "地域比較"),
    (5, "構造分析"),
];

// ======== 示唆カテゴリ ========

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum InsightCategory {
    HiringStructure,   // 採用構造分析
    Forecast,          // 将来予測
    RegionalCompare,   // 地域間比較
    ActionProposal,    // アクション提案
    StructuralContext, // 構造分析 (SSDSE-A Phase A、市区町村構造指標ベース)
}

impl InsightCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::HiringStructure => "採用構造",
            Self::Forecast => "将来予測",
            Self::RegionalCompare => "地域比較",
            Self::ActionProposal => "アクション",
            Self::StructuralContext => "構造分析",
        }
    }

    pub fn icon_class(&self) -> &'static str {
        match self {
            Self::HiringStructure => "text-red-400",
            Self::Forecast => "text-amber-400",
            Self::RegionalCompare => "text-blue-400",
            Self::ActionProposal => "text-green-400",
            Self::StructuralContext => "text-purple-400",
        }
    }

    pub fn subtab_id(&self) -> u8 {
        match self {
            Self::HiringStructure => 1,
            Self::Forecast => 2,
            Self::RegionalCompare => 3,
            Self::ActionProposal => 4,
            Self::StructuralContext => 5,
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
            Self::Critical => "#ef4444", // red-500
            Self::Warning => "#f59e0b",  // amber-500
            Self::Info => "#3b82f6",     // blue-500
            Self::Positive => "#10b981", // emerald-500
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

// ======== Phase A: SSDSE-A 構造分析（LS/HH/MF/IN/GE）========

/// LS-1: 採用余力シグナル
/// 失業率 > 県平均 × 1.2 かつ HW求人数/就業者 < 県平均 で発火
pub const UNEMPLOYMENT_RATE_MULTIPLIER_WARNING: f64 = 1.2;
pub const UNEMPLOYMENT_RATE_MULTIPLIER_CRITICAL: f64 = 1.5;

/// LS-2: 産業偏在リスク
pub const TERTIARY_CONCENTRATION_THRESHOLD: f64 = 85.0; // %
pub const PRIMARY_CONCENTRATION_THRESHOLD: f64 = 20.0; // %

/// HH-1: 単独世帯型求職者推定
pub const SINGLE_HOUSEHOLD_RATE_THRESHOLD: f64 = 40.0; // %

/// MF-1: 医療福祉供給密度ギャップ
pub const MEDICAL_DENSITY_GAP_RATIO: f64 = 0.8; // 県平均 × 0.8 未満で発火
pub const MEDICAL_DENSITY_CRITICAL_RATIO: f64 = 0.6; // 県平均 × 0.6 未満で Critical

/// IN-1: 産業構造ミスマッチ
pub const INDUSTRY_MISMATCH_COSINE_THRESHOLD: f64 = 0.5; // 類似度 < 0.5 で発火
pub const INDUSTRY_MISMATCH_CRITICAL: f64 = 0.3; // < 0.3 で Critical

/// GE-1: 可住地密度ペナルティ
pub const HABITABLE_DENSITY_MAX: f64 = 10_000.0; // 人/km²
pub const HABITABLE_DENSITY_MIN: f64 = 50.0;
pub const HABITABLE_DENSITY_CRITICAL_MAX: f64 = 20_000.0;
pub const HABITABLE_DENSITY_CRITICAL_MIN: f64 = 20.0;

// ======== Phase B: Agoop 人流 SW-F01〜F10（Round 2） ========

/// SW-F01: 夜勤ニーズ逼迫（深夜滞在 / 昼間滞在 の比率）
pub const FLOW_MIDNIGHT_RATIO_WARNING: f64 = 1.2;
pub const FLOW_MIDNIGHT_RATIO_CRITICAL: f64 = 1.5;

/// SW-F02: 休日商圏不足（休日昼 / 平日昼 の比率）
pub const FLOW_HOLIDAY_CROWD_WARNING: f64 = 1.3;

/// SW-F03: ベッドタウン化（平日昼-夜差の絶対値 / 夜 比率）
pub const FLOW_BEDTOWN_DIFF_THRESHOLD: f64 = 0.2;

/// SW-F04: メッシュ人材ギャップ（求人密度 vs 滞在密度のZスコア絶対値）
pub const FLOW_MESH_ZSCORE_THRESHOLD: f64 = 1.5;

/// SW-F05: 観光ポテンシャル未活用（休日/平日比が高い × 宿泊飲食求人少ない）
pub const FLOW_TOURISM_RATIO_THRESHOLD: f64 = 1.5;

/// SW-F06: コロナ回復乖離（2021人流/2019 > 0.9 AND 2021求人/2019 < 0.8）
pub const FLOW_COVID_FLOW_RECOVERY: f64 = 0.9;
pub const FLOW_COVID_POSTING_LAG: f64 = 0.8;

/// SW-F07: 広域流入比率偏り（from_area=3 異地方比率）
pub const FLOW_INFLOW_DIFF_REGION_THRESHOLD: f64 = 0.15; // 15%超

/// SW-F08: 昼間労働力プール（平日昼滞在 / 居住人口 比率）
pub const FLOW_DAYTIME_POOL_RATIO: f64 = 1.3;

/// SW-F09: 季節雇用ミスマッチ（月次振幅係数、最大/平均 - 1）
pub const FLOW_SEASONAL_AMPLITUDE: f64 = 0.3;

/// SW-F10: 企業立地人流マッチ（企業所在メッシュ滞在ピーク時間 vs 求人営業時間のズレ）
pub const FLOW_COMPANY_TIME_DIFF_HOURS: f64 = 3.0;

/// 共通: 最小サンプルサイズ（統計的妥当性）
pub const FLOW_MIN_SAMPLE_SIZE: usize = 30;

// ======== ヘルパー関数 ========

/// 安全な除算（ゼロ除算ガード）
pub fn safe_divide(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator.abs() < f64::EPSILON {
        None
    } else {
        Some(numerator / denominator)
    }
}

/// f64取得（NaN/Infガード付き）
pub fn get_f64_safe(row: &Row, key: &str) -> f64 {
    let v = super::super::helpers::get_f64(row, key);
    if v.is_nan() || v.is_infinite() {
        0.0
    } else {
        v
    }
}

/// 線形回帰の傾きを計算（正規化済み: 月あたり変化率）
pub fn linear_slope(values: &[f64]) -> Option<f64> {
    let n = values.len();
    if n < 3 {
        return None;
    }
    let n_f = n as f64;
    let x_mean = (n_f - 1.0) / 2.0;
    let y_mean: f64 = values.iter().sum::<f64>() / n_f;
    if y_mean.abs() < f64::EPSILON {
        return None;
    }

    let mut num = 0.0;
    let mut den = 0.0;
    for (i, &y) in values.iter().enumerate() {
        let x = i as f64 - x_mean;
        num += x * (y - y_mean);
        den += x * x;
    }
    if den.abs() < f64::EPSILON {
        return None;
    }
    // 月あたり変化率として正規化
    Some((num / den) / y_mean)
}
