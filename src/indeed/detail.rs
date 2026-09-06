//! 職種 1 つを深く見るためのデータ。
//!
//! # なぜ [`super::data::Snapshot`] に入れないか
//! 都道府県別の順位・検索語・時給まで抱えると、起動時に読む量が跳ね上がる。
//! 職種を選んだときにだけ、その職種の分を引く。1 回あたり数百行なので軽い。
//!
//! # 数字の意味を取り違えない
//! `rank_in_country` と `vs_national` は名前から意味を推測できる形をしていない。
//! 実際の作り（`scripts/indeed_build_insights.js`）を読んで確かめた:
//!
//! ```text
//! rows.sort((a, b) => b.seekers_per_posting - a.seekers_per_posting);  // 降順
//! insTP.run(..., i + 1, rows.length, r.seekers_per_posting / natRatio, ...)
//! ```
//!
//! * 順位 … 1 求人あたりに見た人が **多い順**。1 位が最も集まりやすい県
//! * 全国比 … 全国平均を 1 としたときの **比**。差分ではない（0.59 は「全国の 59%」）

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_f64_opt, get_i64_opt, get_str};

/// 都道府県 1 つ分。最新月の姿。
#[derive(Debug, Clone)]
pub struct PrefRow {
    pub prefecture: String,
    pub job: Option<f64>,
    pub ctk: Option<f64>,
    pub employers: Option<f64>,
    /// 1 求人あたりに見た人数
    pub spp: Option<f64>,
    /// 1 求人あたりが多い順の順位。1 位が最も集まりやすい
    pub rank: Option<i64>,
    /// 比べた県の数
    pub of: Option<i64>,
    /// 全国平均を 1 としたときの比
    pub vs_national: Option<f64>,
    /// Indeed が出す採用難易度（0〜1）
    pub difficulty: Option<f64>,
    /// この県でよく検索された語（上位）
    pub keywords: Vec<(String, i64)>,
    /// 時給の中央値。取れていない県もある
    pub wage_median: Option<f64>,
}

/// 検索語のシェアがどう動いたか。
#[derive(Debug, Clone)]
pub struct TermShift {
    pub term: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub diff: Option<f64>,
    pub clicks_after: Option<i64>,
}

/// 探している人の傾向。各項目は独立した割合で、足して 100% にはならない。
#[derive(Debug, Clone, Default)]
pub struct Attrs {
    pub month: String,
    pub total_clicks: Option<f64>,
    pub terms: Option<i64>,
    /// (表示名, %)
    pub shares: Vec<(&'static str, Option<f64>)>,
}

/// 検索ボリューム（検索エンジンの推定値）。
#[derive(Debug, Clone)]
pub struct SearchVolume {
    /// "job" は「職種名＋求人」、"name" は職種名だけ
    pub variant: String,
    pub avg_monthly: Option<i64>,
    pub latest: Option<i64>,
    pub yoy_pct: Option<f64>,
    pub competition: String,
    pub low_bid_yen: Option<f64>,
    pub high_bid_yen: Option<f64>,
}

/// 職種 1 つ分をまとめたもの。
#[derive(Debug, Default)]
pub struct TitleDetail {
    pub title: String,
    pub category: String,
    pub month: String,
    pub prefs: Vec<PrefRow>,
    pub shifts: Vec<TermShift>,
    pub attrs: Option<Attrs>,
    pub volumes: Vec<SearchVolume>,
}

/// 「語:回数;語:回数」の形をほどく。
fn parse_keywords(s: &str) -> Vec<(String, i64)> {
    s.split(';')
        .filter_map(|part| {
            let (term, n) = part.rsplit_once(':')?;
            if term.is_empty() {
                return None;
            }
            Some((term.to_string(), n.trim().parse::<i64>().ok()?))
        })
        .collect()
}

/// 職種 1 つ分を引く。見つからなければ `None`。
pub fn load(db: &LocalDb, title: &str) -> Result<Option<TitleDetail>, String> {
    let month: String = {
        let rows = db.query(
            "SELECT value FROM insight_meta WHERE key='latest_month'",
            &[],
        )?;
        match rows.first() {
            Some(r) => get_str(r, "value"),
            None => return Ok(None),
        }
    };

    let head = db.query(
        "SELECT display_category FROM insight_title WHERE norm_title = ?1",
        &[&title],
    )?;
    let category = head
        .first()
        .map(|r| get_str(r, "display_category"))
        .unwrap_or_default();

    // 時給は職種×県。時点は 1 つしか無い（2026-08 現在）ので最新を取る
    let mut wage: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for r in db.query(
        "SELECT prefecture, median_salary FROM insight_salary \
         WHERE norm_title = ?1 AND salary_period = 'HOURLY' \
           AND snapshot_month = (SELECT MAX(snapshot_month) FROM insight_salary)",
        &[&title],
    )? {
        if let Some(v) = get_f64_opt(&r, "median_salary") {
            wage.insert(get_str(&r, "prefecture"), v);
        }
    }

    let mut prefs: Vec<PrefRow> = db
        .query(
            "SELECT prefecture, job_count, ctk_count, employer_count, seekers_per_posting, \
                    rank_in_country, prefs_compared, vs_national, difficulty, top_keywords \
             FROM insight_title_pref WHERE norm_title = ?1 AND report_month = ?2",
            &[&title, &month],
        )?
        .iter()
        .map(|r| {
            let prefecture = get_str(r, "prefecture");
            PrefRow {
                job: get_f64_opt(r, "job_count"),
                ctk: get_f64_opt(r, "ctk_count"),
                employers: get_f64_opt(r, "employer_count"),
                spp: get_f64_opt(r, "seekers_per_posting"),
                rank: get_i64_opt(r, "rank_in_country"),
                of: get_i64_opt(r, "prefs_compared"),
                vs_national: get_f64_opt(r, "vs_national"),
                difficulty: get_f64_opt(r, "difficulty"),
                keywords: parse_keywords(&get_str(r, "top_keywords")),
                wage_median: wage.get(&prefecture).copied(),
                prefecture,
            }
        })
        .collect();
    if prefs.is_empty() {
        return Ok(None);
    }
    // 求人数の多い順。欠測は 0 ではなく最後に置く
    prefs.sort_by(|a, b| {
        b.job
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&a.job.unwrap_or(f64::NEG_INFINITY))
    });

    let shifts: Vec<TermShift> = db
        .query(
            "SELECT search_term, share_before, share_after, share_diff, clicks_after \
             FROM insight_kw_term_shift WHERE norm_title = ?1",
            &[&title],
        )?
        .iter()
        .map(|r| TermShift {
            term: get_str(r, "search_term"),
            before: get_f64_opt(r, "share_before"),
            after: get_f64_opt(r, "share_after"),
            diff: get_f64_opt(r, "share_diff"),
            clicks_after: get_i64_opt(r, "clicks_after"),
        })
        .collect();

    let attrs = db
        .query(
            "SELECT report_month, total_clicks, term_count, pct_condition, pct_senior, \
                    pct_homemaker, pct_student, pct_foreign, pct_inexperienced, \
                    pct_language, pct_qualified \
             FROM insight_kw_attr_trend WHERE norm_title = ?1 \
             ORDER BY report_month DESC LIMIT 1",
            &[&title],
        )?
        .first()
        .map(|r| Attrs {
            month: get_str(r, "report_month"),
            total_clicks: get_f64_opt(r, "total_clicks"),
            terms: get_i64_opt(r, "term_count"),
            // それぞれ独立に判定しているので、足しても 100% にはならない
            shares: vec![
                ("働き方の条件", get_f64_opt(r, "pct_condition")),
                ("シニア", get_f64_opt(r, "pct_senior")),
                ("主婦・主夫", get_f64_opt(r, "pct_homemaker")),
                ("学生", get_f64_opt(r, "pct_student")),
                ("外国人", get_f64_opt(r, "pct_foreign")),
                ("未経験", get_f64_opt(r, "pct_inexperienced")),
                ("語学", get_f64_opt(r, "pct_language")),
                ("資格", get_f64_opt(r, "pct_qualified")),
            ],
        });

    let volumes: Vec<SearchVolume> = db
        .query(
            "SELECT variant, avg_monthly, latest, yoy_pct, competition, low_bid_yen, high_bid_yen \
             FROM insight_search_trend WHERE norm_title = ?1",
            &[&title],
        )?
        .iter()
        .map(|r| SearchVolume {
            variant: get_str(r, "variant"),
            avg_monthly: get_i64_opt(r, "avg_monthly"),
            latest: get_i64_opt(r, "latest"),
            yoy_pct: get_f64_opt(r, "yoy_pct"),
            competition: get_str(r, "competition"),
            low_bid_yen: get_f64_opt(r, "low_bid_yen"),
            high_bid_yen: get_f64_opt(r, "high_bid_yen"),
        })
        .collect();

    Ok(Some(TitleDetail {
        title: title.to_string(),
        category,
        month,
        prefs,
        shifts,
        attrs,
        volumes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 検索語の文字列をほどける() {
        let v = parse_keywords("軽貨物:5340;ドライバー:4228;配送:2038");
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], ("軽貨物".to_string(), 5340));
        assert_eq!(v[2], ("配送".to_string(), 2038));
    }

    #[test]
    fn 壊れた並びでも落ちない() {
        // 区切りが足りない・数字でない・空のかけらが混ざっても、
        // 取れるものだけを返す
        assert!(parse_keywords("").is_empty());
        assert!(parse_keywords("こわれている").is_empty());
        assert_eq!(parse_keywords("正常:12;だめ;:9;x:あ").len(), 1);
    }

    #[test]
    fn 語にコロンが入っていても後ろの数字を数として読む() {
        let v = parse_keywords("a:b:30");
        assert_eq!(v, vec![("a:b".to_string(), 30)]);
    }
}
