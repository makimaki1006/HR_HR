//! 架電クオリティ可視化用 Serialize/Deserialize structs
//!
//! Google Sheets API が返す JSON は `values: Vec<Vec<String>>` という
//! 行列形式のため、本モジュールでは header をキーとした
//! `HashMap<String, String>` を Row として扱い、各シート固有の構造体
//! (DashboardRow, PrefectureRow 等) は from_row で構築する。
//!
//! GAS Code.gs `readSheet_()` 相当の仕様:
//!   - 1 行目をヘッダ
//!   - 数値も文字列で来るので Number 変換は呼び出し側で
//!   - Date 型はスプシ側で "yyyy-MM-dd" 文字列に正規化済み

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// シートから読んだ生 row (ヘッダ -> セル値)
pub type SheetRow = HashMap<String, String>;

/// シート全体の読込結果 (FastAPI プロト `_build_response` と同形式)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetResponse {
    pub rows: Vec<SheetRow>,
    #[serde(rename = "sourceRows")]
    pub source_rows: usize,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "fromCache")]
    pub from_cache: bool,
    pub schema: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SheetResponse {
    pub fn new(rows: Vec<SheetRow>) -> Self {
        let schema = rows
            .first()
            .map(|r| r.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let source_rows = rows.len();
        Self {
            rows,
            source_rows,
            updated_at: jst_now_str(),
            from_cache: false,
            schema,
            error: None,
        }
    }

    pub fn error(sheet_name: &str, err: impl std::fmt::Display) -> Self {
        Self {
            rows: vec![],
            source_rows: 0,
            updated_at: jst_now_str(),
            from_cache: false,
            schema: vec![],
            error: Some(format!("{err} (sheet={sheet_name})")),
        }
    }

    pub fn mark_from_cache(mut self) -> Self {
        self.from_cache = true;
        self
    }
}

/// P0 全社サマリで使う KPI 6 枚分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardKpi {
    pub label: String,
    pub value: f64,
    pub unit: String,
    /// 前月比 (delta = current - previous)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    /// 前月比率 (%) - 量的指標用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_pct: Option<f64>,
    /// 前月差 (ppt) - 率系指標用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_ppt: Option<f64>,
}

/// /api/call-quality/dashboard レスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardResponse {
    pub kpis: Vec<DashboardKpi>,
    /// 月次架電数推移 (P3 用) - 直近 8 ヶ月
    pub monthly_call_trend: Vec<MonthlyPoint>,
    /// 月次アポ率推移
    pub monthly_apo_rate_trend: Vec<MonthlyPoint>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "fromCache")]
    pub from_cache: bool,
}

/// 月次データポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyPoint {
    pub month: String, // "YYYY-MM"
    pub value: f64,
}

/// P4 都道府県別 アポ率 (ECharts 棒グラフ用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefectureBar {
    pub prefecture: String,
    pub call_count: f64,
    pub apo_count: f64,
    pub apo_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefectureResponse {
    pub bars: Vec<PrefectureBar>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "fromCache")]
    pub from_cache: bool,
}

/// データブラウザ (P7) 簡易版レスポンス
#[derive(Debug, Clone, Serialize)]
pub struct RawSheetListResponse {
    pub sheets: Vec<&'static str>,
}

// ---- フィルタクエリ ------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardQuery {
    /// "YYYY-MM-DD" 形式
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// パイプライン ID。未指定 or "__all__" で全パイプライン
    #[serde(default)]
    pub pipeline: Option<String>,
    /// メンバー owner_id (カンマ区切り)
    #[serde(default)]
    pub members: Option<String>,
    /// 都道府県
    #[serde(default)]
    pub prefecture: Option<String>,
}

impl DashboardQuery {
    pub fn pipeline_filter(&self) -> Option<&str> {
        self.pipeline
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "__all__")
    }

    pub fn prefecture_filter(&self) -> Option<&str> {
        self.prefecture
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "__all__")
    }

    pub fn member_filter(&self) -> Vec<String> {
        self.members
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "__all__")
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---- ホワイトリスト (P7 で参照可能なシート名) ----------------------------

/// GAS Code.gs RAW_SHEET_NAMES と同じ並び (UI 表示順)
pub const RAW_SHEET_NAMES: &[&str] = &[
    "日次明細",
    "セグメント明細",
    "セグメント月次集計",
    "月次明細",
    "メンバーマスタ",
    "最新サマリ",
    "異常検知",
    "Deal Health",
    "Deal Health owner月次",
    "月末予測",
    "時間帯ヒート",
    "N回目架電分析",
    "リサイクル間隔",
    "コンプライアンスパターン",
    "Recency owner月次",
    "滞留日数",
    "Pipeline Velocity",
    "コホート分析",
    "新規/既存/リサイクル",
    "ファネル4段",
    "曜日別集計",
    "曜日別 owner別",
    "都道府県月次",
];

// ---- ユーティリティ -----------------------------------------------------

/// 'yyyy-MM-dd HH:mm' (Asia/Tokyo) で現在時刻を返す
fn jst_now_str() -> String {
    use chrono::{FixedOffset, Utc};
    let jst = FixedOffset::east_opt(9 * 3600).expect("JST offset");
    Utc::now()
        .with_timezone(&jst)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// セル値を `f64` に変換 (空文字 / カンマ / 全角数字対策)
pub fn parse_number(v: &str) -> Option<f64> {
    if v.trim().is_empty() {
        return None;
    }
    let cleaned = v.replace(',', "").replace('％', "");
    cleaned.trim().parse::<f64>().ok()
}
