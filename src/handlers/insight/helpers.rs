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

// ======== 示唆 ID（型安全 enum、2026-05-22 導入）========

/// 示唆 ID（旧 String → enum 化）。
///
/// 2026-05-21 事故: `match insight.id.as_str() { ... _ => String::new() }` の
/// silent fallback で 14 件の Insight ID が未登録だったため、推奨アクションが
/// 空表示になっていた。
///
/// 本 enum を導入したことで、以下が **コンパイル時に検出される** ようになった:
///
/// - 新規 Insight 追加 → `InsightId::XxN` variant 追加要求
/// - `generate_so_what` 等の exhaustive match → 漏れがあれば即 compile error
///
/// 各 variant の元文字列（"HS-1" 等）は `as_str()` / `Display` / `Serialize` で取得可能。
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum InsightId {
    // 採用構造分析 (HS: HiringStructure)
    #[serde(rename = "HS-1")]
    Hs1,
    #[serde(rename = "HS-2")]
    Hs2,
    #[serde(rename = "HS-3")]
    Hs3,
    #[serde(rename = "HS-4")]
    Hs4,
    #[serde(rename = "HS-5")]
    Hs5,
    #[serde(rename = "HS-6")]
    Hs6,
    // 将来予測 (FC: Forecast)
    #[serde(rename = "FC-1")]
    Fc1,
    #[serde(rename = "FC-2")]
    Fc2,
    #[serde(rename = "FC-3")]
    Fc3,
    #[serde(rename = "FC-4")]
    Fc4,
    // 地域比較 (RC: RegionalCompare)
    #[serde(rename = "RC-1")]
    Rc1,
    #[serde(rename = "RC-2")]
    Rc2,
    #[serde(rename = "RC-3")]
    Rc3,
    // 通勤圏 (CZ: Commuting Zone)
    #[serde(rename = "CZ-1")]
    Cz1,
    #[serde(rename = "CZ-2")]
    Cz2,
    #[serde(rename = "CZ-3")]
    Cz3,
    // 通勤フロー (CF: Commuting Flow)
    #[serde(rename = "CF-1")]
    Cf1,
    #[serde(rename = "CF-2")]
    Cf2,
    #[serde(rename = "CF-3")]
    Cf3,
    // アクション提案 (AP: ActionProposal)
    #[serde(rename = "AP-1")]
    Ap1,
    #[serde(rename = "AP-2")]
    Ap2,
    #[serde(rename = "AP-3")]
    Ap3,
    // SSDSE-A 構造分析 Phase A
    #[serde(rename = "LS-1")]
    Ls1,
    #[serde(rename = "LS-2")]
    Ls2,
    #[serde(rename = "HH-1")]
    Hh1,
    #[serde(rename = "MF-1")]
    Mf1,
    #[serde(rename = "IN-1")]
    In1,
    #[serde(rename = "GE-1")]
    Ge1,
    // Agoop 人流 Phase B (SW-F: Stay Workflow)
    // 現状 engine_flow.rs 未実装の F04 / F10 も外部 recruitment_diag/insights.rs
    // のホワイトリストに登録されているため variant として確保しておく。
    #[serde(rename = "SW-F01")]
    SwF01,
    #[serde(rename = "SW-F02")]
    SwF02,
    #[serde(rename = "SW-F03")]
    SwF03,
    #[serde(rename = "SW-F04")]
    SwF04,
    #[serde(rename = "SW-F05")]
    SwF05,
    #[serde(rename = "SW-F06")]
    SwF06,
    #[serde(rename = "SW-F07")]
    SwF07,
    #[serde(rename = "SW-F08")]
    SwF08,
    #[serde(rename = "SW-F09")]
    SwF09,
    #[serde(rename = "SW-F10")]
    SwF10,
}

impl InsightId {
    /// 元の文字列表現（"HS-1" 等）を返す
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hs1 => "HS-1",
            Self::Hs2 => "HS-2",
            Self::Hs3 => "HS-3",
            Self::Hs4 => "HS-4",
            Self::Hs5 => "HS-5",
            Self::Hs6 => "HS-6",
            Self::Fc1 => "FC-1",
            Self::Fc2 => "FC-2",
            Self::Fc3 => "FC-3",
            Self::Fc4 => "FC-4",
            Self::Rc1 => "RC-1",
            Self::Rc2 => "RC-2",
            Self::Rc3 => "RC-3",
            Self::Cz1 => "CZ-1",
            Self::Cz2 => "CZ-2",
            Self::Cz3 => "CZ-3",
            Self::Cf1 => "CF-1",
            Self::Cf2 => "CF-2",
            Self::Cf3 => "CF-3",
            Self::Ap1 => "AP-1",
            Self::Ap2 => "AP-2",
            Self::Ap3 => "AP-3",
            Self::Ls1 => "LS-1",
            Self::Ls2 => "LS-2",
            Self::Hh1 => "HH-1",
            Self::Mf1 => "MF-1",
            Self::In1 => "IN-1",
            Self::Ge1 => "GE-1",
            Self::SwF01 => "SW-F01",
            Self::SwF02 => "SW-F02",
            Self::SwF03 => "SW-F03",
            Self::SwF04 => "SW-F04",
            Self::SwF05 => "SW-F05",
            Self::SwF06 => "SW-F06",
            Self::SwF07 => "SW-F07",
            Self::SwF08 => "SW-F08",
            Self::SwF09 => "SW-F09",
            Self::SwF10 => "SW-F10",
        }
    }
}

impl std::fmt::Display for InsightId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for InsightId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "HS-1" => Ok(Self::Hs1),
            "HS-2" => Ok(Self::Hs2),
            "HS-3" => Ok(Self::Hs3),
            "HS-4" => Ok(Self::Hs4),
            "HS-5" => Ok(Self::Hs5),
            "HS-6" => Ok(Self::Hs6),
            "FC-1" => Ok(Self::Fc1),
            "FC-2" => Ok(Self::Fc2),
            "FC-3" => Ok(Self::Fc3),
            "FC-4" => Ok(Self::Fc4),
            "RC-1" => Ok(Self::Rc1),
            "RC-2" => Ok(Self::Rc2),
            "RC-3" => Ok(Self::Rc3),
            "CZ-1" => Ok(Self::Cz1),
            "CZ-2" => Ok(Self::Cz2),
            "CZ-3" => Ok(Self::Cz3),
            "CF-1" => Ok(Self::Cf1),
            "CF-2" => Ok(Self::Cf2),
            "CF-3" => Ok(Self::Cf3),
            "AP-1" => Ok(Self::Ap1),
            "AP-2" => Ok(Self::Ap2),
            "AP-3" => Ok(Self::Ap3),
            "LS-1" => Ok(Self::Ls1),
            "LS-2" => Ok(Self::Ls2),
            "HH-1" => Ok(Self::Hh1),
            "MF-1" => Ok(Self::Mf1),
            "IN-1" => Ok(Self::In1),
            "GE-1" => Ok(Self::Ge1),
            "SW-F01" => Ok(Self::SwF01),
            "SW-F02" => Ok(Self::SwF02),
            "SW-F03" => Ok(Self::SwF03),
            "SW-F04" => Ok(Self::SwF04),
            "SW-F05" => Ok(Self::SwF05),
            "SW-F06" => Ok(Self::SwF06),
            "SW-F07" => Ok(Self::SwF07),
            "SW-F08" => Ok(Self::SwF08),
            "SW-F09" => Ok(Self::SwF09),
            "SW-F10" => Ok(Self::SwF10),
            other => Err(format!("unknown InsightId: {}", other)),
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
    pub id: InsightId,
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

/// HS-4: テキスト温度（F1 修正、2026-04-26）
/// 修正前: 0.0（中立）。実データ分布調査で v2_text_temperature 市区町村レベル正社員 1004件の
/// P25 = -0.1417、負値割合 37.8% であることが判明（hellowork.db 直接照会、2026-04-26）。
/// 「中立0.0以下で発火」では低温区域の半数近くで発火し、過剰検出となる傾向。
/// 都道府県レベル(47件)でも P25 = -0.0377 と 0.0 周辺に集中、min=-0.4063, max=0.6063。
/// 統計的下位四分位を「真に低温」と判定するため、固定閾値を -0.15 に変更
/// (市区町村 P25 -0.1417 を保守側に丸めた値)。
/// 単位: temperature = (urgency_words - selectivity_words) / total_chars * 1000 (パーミル)
/// 出典: scripts/compute_v2_phase2.py:104, ETL 計算ロジック
pub const TEMP_LOW_THRESHOLD: f64 = -0.15;

/// HS-5: 雇用者集中
pub const HHI_CRITICAL: f64 = 0.25;
pub const TOP1_SHARE_CRITICAL: f64 = 0.30;

/// HS-6: 空間ミスマッチ
pub const ISOLATION_WARNING: f64 = 0.50;
pub const DAYTIME_POP_RATIO_LOW: f64 = 0.90;

/// FC-1: トレンド判定
pub const TREND_INCREASE_THRESHOLD: f64 = 0.05;
pub const TREND_DECREASE_THRESHOLD: f64 = -0.05;

/// RC-2: 給与差の相対閾値（M-10 修正、2026-04-26）
/// 全国平均比 -10% 以下で Warning、+5% 超で Positive。
/// 固定額 (±10000/-20000円) では低給与職種(介護)で誤発火、高給与職種(IT)で過小発火していた。
pub const RC2_SALARY_GAP_WARNING_PCT: f64 = -0.10;
pub const RC2_SALARY_GAP_POSITIVE_PCT: f64 = 0.05;

/// AP-1: 年間人件費換算（M-13 修正、2026-04-26）
/// 月給 × (12 + 賞与4ヶ月) × (1 + 法定福利16%)
/// 厚労省「就業条件総合調査」中央値ベースの簡易補正。
pub const AP1_BONUS_MONTHS_DEFAULT: f64 = 4.0;
pub const AP1_LEGAL_WELFARE_RATIO: f64 = 0.16;

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
