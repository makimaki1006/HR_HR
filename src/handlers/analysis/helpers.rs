//! ヘルパー関数・定数定義（analysis モジュール内部用）

use std::collections::HashMap;
use serde_json::Value;

type Row = HashMap<String, Value>;

/// サブタブ定義
pub(crate) const ANALYSIS_SUBTABS: [(u8, &str); 6] = [
    (1, "求人動向"),
    (2, "給与分析"),
    (3, "テキスト分析"),
    (4, "市場構造"),
    (5, "異常値・外部"),
    (6, "予測・推定"),
];

/// Row から文字列参照を取得（借用版）
pub(crate) fn get_str<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

// ======== 色判定関数 ========

pub(crate) fn vacancy_color(rate: f64) -> &'static str {
    if rate >= 0.4 { "#ef4444" } else if rate >= 0.3 { "#f97316" } else if rate >= 0.2 { "#eab308" } else { "#22c55e" }
}

pub(crate) fn transparency_color(score: f64) -> &'static str {
    if score >= 0.8 { "#22c55e" } else if score >= 0.6 { "#eab308" } else if score >= 0.4 { "#f97316" } else { "#ef4444" }
}

pub(crate) fn evenness_color(ev: f64) -> &'static str {
    if ev >= 0.7 { "#22c55e" } else if ev >= 0.5 { "#eab308" } else { "#f97316" }
}

pub(crate) fn temp_color(t: f64) -> &'static str {
    if t >= 5.0 { "#ef4444" } else if t >= 2.0 { "#f97316" } else if t >= 0.0 { "#eab308" } else { "#3b82f6" }
}

pub(crate) fn salary_color(salary_min: f64) -> &'static str {
    if salary_min > 300000.0 { "#22c55e" } else if salary_min > 200000.0 { "#3b82f6" } else { "#94a3b8" }
}

pub(crate) fn rank_badge_color(rank: &str) -> (&'static str, &'static str) {
    // (背景色, テキスト色)
    match rank {
        "S" => ("#fbbf24", "#1e293b"),  // gold
        "A" => ("#10b981", "#ffffff"),  // emerald
        "B" => ("#3b82f6", "#ffffff"),  // blue
        "C" => ("#f59e0b", "#1e293b"),  // amber
        "D" => ("#64748b", "#ffffff"),  // slate
        _ => ("#475569", "#ffffff"),
    }
}

pub(crate) fn info_score_color(score: f64) -> &'static str {
    if score >= 0.8 { "#22c55e" } else if score >= 0.6 { "#3b82f6" } else if score >= 0.4 { "#eab308" } else { "#ef4444" }
}

pub(crate) fn keyword_category_label(cat: &str) -> &str {
    match cat {
        "urgent" => "急募系",
        "inexperienced" => "未経験系",
        "benefits" => "待遇系",
        "wlb" => "WLB系",
        "growth" => "成長系",
        "stability" => "安定系",
        _ => cat,
    }
}

pub(crate) fn keyword_category_color(cat: &str) -> &str {
    match cat {
        "urgent" => "#ef4444",
        "inexperienced" => "#f97316",
        "benefits" => "#22c55e",
        "wlb" => "#3b82f6",
        "growth" => "#a855f7",
        "stability" => "#14b8a6",
        _ => "#94a3b8",
    }
}

pub(crate) fn strategy_color(stype: &str) -> (&'static str, &'static str) {
    // (背景色, テキスト色)
    match stype {
        "プレミアム型" => ("#065f46", "#6ee7b7"),     // emerald dark bg
        "給与一本勝負型" => ("#1e3a5f", "#93c5fd"),   // blue dark bg
        "福利厚生重視型" => ("#78350f", "#fcd34d"),    // amber dark bg
        "コスト優先型" => ("#334155", "#94a3b8"),      // slate dark bg
        _ => ("#1e293b", "#cbd5e1"),
    }
}

pub(crate) fn concentration_badge(level: &str) -> (&'static str, &'static str) {
    // (背景色, テキスト色)
    match level {
        "高度集中" => ("#991b1b", "#fca5a5"),
        "中度集中" => ("#92400e", "#fcd34d"),
        "低度集中" => ("#166534", "#86efac"),
        "競争的" => ("#1e3a5f", "#93c5fd"),
        _ => ("#334155", "#94a3b8"),
    }
}
