//! サブタブ3（テキスト分析）系 fetch 関数
//! - Phase 2B: 求人原稿品質、キーワードプロファイル
//! - Phase 2: テキスト温度計（H-2 加重平均）

use serde_json::Value;
use std::collections::HashMap;

use super::query_3level;

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

pub(crate) fn fetch_temperature_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, sample_count, temperature, \
        urgency_density, selectivity_density, urgency_hit_rate, selectivity_hit_rate";
    // H-2: 全国集計はsample_countで加重平均（離島と東京が同じ重みにならないように）
    let nat = "emp_group, SUM(sample_count) as sample_count, \
        SUM(temperature * sample_count) / SUM(sample_count) as temperature, \
        SUM(urgency_density * sample_count) / SUM(sample_count) as urgency_density, \
        SUM(selectivity_density * sample_count) / SUM(sample_count) as selectivity_density, \
        SUM(urgency_hit_rate * sample_count) / SUM(sample_count) as urgency_hit_rate, \
        SUM(selectivity_hit_rate * sample_count) / SUM(sample_count) as selectivity_hit_rate";
    query_3level(
        db,
        "v2_text_temperature",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group",
        nat,
        "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group",
    )
}

pub(crate) fn fetch_text_quality(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_count, avg_char_count, avg_unique_char_ratio, \
        avg_kanji_ratio, avg_numeric_ratio, avg_punctuation_density, information_score";
    let nat = "emp_group, SUM(total_count) as total_count, \
        AVG(avg_char_count) as avg_char_count, AVG(avg_unique_char_ratio) as avg_unique_char_ratio, \
        AVG(avg_kanji_ratio) as avg_kanji_ratio, AVG(avg_numeric_ratio) as avg_numeric_ratio, \
        AVG(avg_punctuation_density) as avg_punctuation_density, AVG(information_score) as information_score";
    query_3level(
        db,
        "v2_text_quality",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group",
        nat,
        "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group",
    )
}

pub(crate) fn fetch_keyword_profile(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, keyword_category, density, avg_count_per_posting";
    let nat = "emp_group, keyword_category, AVG(density) as density, AVG(avg_count_per_posting) as avg_count_per_posting";
    query_3level(db, "v2_keyword_profile", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group, keyword_category",
        nat, "AND industry_raw = '' GROUP BY emp_group, keyword_category ORDER BY emp_group, keyword_category")
}
