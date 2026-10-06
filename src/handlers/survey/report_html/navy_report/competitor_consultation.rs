//! Bounded, descriptive consultation notes: observations are not hiring forecasts.
use crate::handlers::helpers::escape_html;
use crate::handlers::survey::aggregator::{BoundStats, SurveyAggregation, COMPETITOR_HEAD_MAX};
use serde::Serialize;
use ts_rs::TS;

/// 「採用のヒント」タブ。事実の比較だけを載せ、採用の見込みは言わない。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ConsultationSection {
    /// 「総合」の集計対象の説明 (SP の実額か、SP が無くて給与分布と同じ対象か)。
    pub cohort: String,
    /// 中央値の比較 (下限・上限)。
    pub salary: Vec<ConsultationSalaryRow>,
    /// 給与分布に使った有効件数。
    pub distribution_min_n: u32,
    pub distribution_max_n: u32,
    /// どれかの比較が 1〜9 件。参考値として扱う注記を出す。
    pub small_sample: bool,
    pub gaps: GapsSection,
    /// 外部データの取得状況。
    pub external: Vec<ExternalStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ConsultationSalaryRow {
    /// "下限" / "上限"。
    pub label: String,
    /// 表示単位 (月給 = 万円、時給 = 円) の中央値。未取得は null。
    pub all_median: Option<f64>,
    pub all_n: u32,
    pub popular_median: Option<f64>,
    pub popular_n: u32,
    /// 総合 − 人気。どちらかが未取得なら null。
    pub delta: Option<f64>,
}

/// 全体より先頭の占有率が低い語 (訴求を確認する候補)。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GapsSection {
    /// 先頭のタグ記録・母数が足りない、または集計が矛盾していて判断できない。
    Insufficient,
    /// 確認できるタグでは、全体より先頭の占有率が低い語がない。
    NoLowerShare,
    Rows {
        head_n: u32,
        all_n: u32,
        rows: Vec<GapRow>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct GapRow {
    pub word: String,
    pub head: u32,
    pub all: u32,
    pub head_share_pct: f64,
    pub all_share_pct: f64,
    /// 先頭率 − 全体率 (パーセントポイント。負)。
    pub points: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ExternalStatus {
    pub label: String,
    pub fetched: bool,
}

/// 各外部データが取得できたか (モデルが組んだ各セクションの状態から作る)。
pub(super) struct ExternalAvailability {
    pub google_demand: bool,
    pub google_suggestions: bool,
    pub indeed: bool,
    pub population: bool,
}

struct Gap<'a> {
    word: &'a str,
    head: usize,
    all: usize,
    points: f64,
}

fn gaps(agg: &SurveyAggregation, top_n: usize) -> Option<(usize, Vec<Gap<'_>>)> {
    let total = agg.total_count;
    let n = top_n.min(total).min(COMPETITOR_HEAD_MAX);
    if n == 0 || agg.competitor.head_tags.len() < n {
        return None;
    }
    let mut result = Vec::new();
    for (word, count) in &agg.competitor.tag_counts_all {
        let head = agg.competitor.head_tags[..n]
            .iter()
            .filter(|tags| tags.contains(word))
            .count();
        // Inconsistent aggregates cannot support a gap, even if subtraction is possible.
        if *count > total || head > *count || count - head > total - n {
            return None;
        }
        if (head as u128) * (total as u128) < (*count as u128) * (n as u128) {
            result.push(Gap {
                word,
                head,
                all: *count,
                points: 100.0 * (head as f64 / n as f64 - *count as f64 / total as f64),
            });
        }
    }
    if agg.competitor.head_tags[..n]
        .iter()
        .flatten()
        .any(|word| !agg.competitor.tag_counts_all.iter().any(|(w, _)| w == word))
    {
        return None;
    }
    result.sort_by(|a, b| {
        a.points
            .total_cmp(&b.points)
            .then_with(|| a.word.cmp(b.word))
    });
    result.truncate(3);
    Some((n, result))
}

pub(super) fn build(
    agg: &SurveyAggregation,
    all: &BoundStats,
    popular: &BoundStats,
    top_n: usize,
    fallback: bool,
    external: &ExternalAvailability,
) -> ConsultationSection {
    let scale = if agg.is_hourly { 1.0 } else { 10000.0 };
    let disp = |v: i64| v as f64 / scale;
    let cohort = if fallback {
        if agg.is_hourly {
            "総合：CSV内の時給求人の実額（SP比較対象がないため給与分布と同じ対象）"
        } else {
            "総合：CSV内の給与を既存規則で月給換算（SP比較対象がないため給与分布と同じ対象）"
        }
    } else if agg.is_hourly {
        "総合：Indeed SPの時給求人の実額"
    } else {
        "総合：Indeed SPの月給求人の実額（月給換算は含みません）"
    };
    let (dist_min, dist_max) = if agg.is_hourly {
        (
            agg.salary_min_values_native.len(),
            agg.salary_max_values_native.len(),
        )
    } else {
        (agg.salary_min_values.len(), agg.salary_max_values.len())
    };
    let salary = [
        (
            "下限",
            all.min_median,
            all.min_n,
            popular.min_median,
            popular.min_n,
        ),
        (
            "上限",
            all.max_median,
            all.max_n,
            popular.max_median,
            popular.max_n,
        ),
    ]
    .into_iter()
    .map(|(label, a, an, b, bn)| ConsultationSalaryRow {
        label: label.to_owned(),
        all_median: a.map(disp),
        all_n: an as u32,
        popular_median: b.map(disp),
        popular_n: bn as u32,
        // 整数のまま引いてから表示単位に直す (浮動小数の丸め差を出さない)。
        delta: a.zip(b).map(|(a, b)| disp(a - b)),
    })
    .collect();
    let gaps = match gaps(agg, top_n) {
        None => GapsSection::Insufficient,
        Some((_, rows)) if rows.is_empty() => GapsSection::NoLowerShare,
        Some((n, rows)) => GapsSection::Rows {
            head_n: n as u32,
            all_n: agg.total_count as u32,
            rows: rows
                .into_iter()
                .map(|row| GapRow {
                    word: row.word.to_owned(),
                    head: row.head as u32,
                    all: row.all as u32,
                    head_share_pct: row.head as f64 / n as f64 * 100.0,
                    all_share_pct: row.all as f64 / agg.total_count as f64 * 100.0,
                    points: row.points,
                })
                .collect(),
        },
    };
    ConsultationSection {
        cohort: cohort.to_owned(),
        salary,
        distribution_min_n: dist_min as u32,
        distribution_max_n: dist_max as u32,
        small_sample: [all.min_n, all.max_n, popular.min_n, popular.max_n]
            .iter()
            .any(|n| *n > 0 && *n < 10),
        gaps,
        external: [
            ("Google検索需要", external.google_demand),
            ("Google関連語", external.google_suggestions),
            ("Indeed採用市場", external.indeed),
            ("人口・地域", external.population),
        ]
        .into_iter()
        .map(|(label, fetched)| ExternalStatus {
            label: label.to_owned(),
            fetched,
        })
        .collect(),
    }
}

/// `unit` は給与の単位 ("万円" / "円/時")、`decimals` は表示の小数桁 (月給 2・時給 0)。
pub(super) fn render(html: &mut String, c: &ConsultationSection, unit: &str, decimals: u8) {
    let amount = |value: Option<f64>| {
        value
            .map(|v| format!("{:.*}", decimals as usize, v))
            .unwrap_or_else(|| "—".into())
    };
    html.push_str("<section class=\"page-navy\"><h1>採用のヒント</h1><p class=\"note\">競合データをもとに、給与・求人票・掲載後の反応を見直しましょう。</p><div class=\"consultation-grid\"><article class=\"consultation-card\"><h2>1. 給与条件を見直す</h2>");
    html.push_str(&format!("<p>{}。人気求人：SPの「人気」「超人気」付き、選択単位の実額。</p><table class=\"table-navy\"><thead><tr><th>中央値（{unit}）</th><th>総合 / 有効件数</th><th>人気 / 有効件数</th><th>総合−人気</th></tr></thead><tbody>", escape_html(&c.cohort)));
    for row in &c.salary {
        let delta = row
            .delta
            .map(|d| format!("{:+.*}", decimals as usize, d))
            .unwrap_or_else(|| "—".into());
        html.push_str(&format!(
            "<tr><th>{}</th><td>{} / {}件</td><td>{} / {}件</td><td>{delta}</td></tr>",
            row.label,
            amount(row.all_median),
            row.all_n,
            amount(row.popular_median),
            row.popular_n
        ));
    }
    html.push_str(&format!(
        "</tbody></table><p class=\"note\">給与分布の有効件数：下限 {} 件・上限 {} 件。</p>",
        c.distribution_min_n, c.distribution_max_n
    ));
    if c.small_sample {
        html.push_str("<p class=\"note\">10件未満の比較は参考値です。</p>");
    }
    html.push_str("<p>給与相場と自社の条件を比較し、勤務時間・手当も含めて見直しましょう。</p></article><article class=\"consultation-card\"><h2>2. 求人票の訴求を見直す</h2><p class=\"note\">先頭の求人と全体を比較。先頭は収録順です。</p>");
    match &c.gaps {
        GapsSection::Insufficient => html.push_str("<p>先頭のタグ記録または比較母数が不足・不整合のため、差の判断を保留します。</p>"),
        GapsSection::NoLowerShare => html.push_str("<p>確認できるタグでは、全体より先頭の占有率が低い語はありません。訴求が十分であることの証明ではありません。</p>"),
        GapsSection::Rows { head_n, all_n, rows } => {
            html.push_str("<table class=\"table-navy\"><thead><tr><th>語</th><th>先頭 件/母数</th><th>全体 件/母数</th><th>差(pt)</th></tr></thead><tbody>");
            for row in rows {
                html.push_str(&format!("<tr><td>{}</td><td>{}/{head_n} ({:.1}%)</td><td>{}/{all_n} ({:.1}%)</td><td>{:+.1}</td></tr>", escape_html(&row.word), row.head, row.head_share_pct, row.all, row.all_share_pct, row.points));
            }
            html.push_str("</tbody></table>");
        }
    }
    html.push_str("<p>実際に提供できる待遇を、求人票で分かりやすく伝えましょう。</p></article></div><h2>3. 掲載後の反応を確認する</h2><table class=\"table-navy\"><tbody>");
    for status in &c.external {
        html.push_str(&format!(
            "<tr><th>{}</th><td>{}</td></tr>",
            status.label,
            if status.fetched {
                "取得済み"
            } else {
                "未取得"
            }
        ));
    }
    html.push_str("</tbody></table><p class=\"note\">外部データの対象地域・基準日は各タブに表示しています。</p><p>職務内容に合う検索語を選び、掲載後の閲覧数・応募数・有効応募数を比較しましょう。</p></section>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::{aggregate_records_with_mode, salary_fixture};
    use crate::handlers::survey::upload::WageMode;

    fn fixture() -> SurveyAggregation {
        let mut records =
            salary_fixture::records("月給 20万円\n月給 21万円\n月給 22万円", "大阪府 大阪市");
        records[1].tags_raw = "研修あり".into();
        records[2].tags_raw = "研修あり".into();
        aggregate_records_with_mode(&records, WageMode::Monthly)
    }

    #[test]
    fn consultation_missing_head_is_not_observed_zero() {
        let mut agg = fixture();
        let (n, rows) = gaps(&agg, 1).unwrap();
        assert_eq!(n, 1);
        assert_eq!((rows[0].head, rows[0].all), (0, 2));
        assert!((rows[0].points + 200.0 / 3.0).abs() < 1e-9);
        agg.competitor.head_tags.clear();
        assert!(gaps(&agg, 1).is_none());
    }

    #[test]
    fn consultation_identical_population_has_no_negative_gap() {
        assert!(gaps(&fixture(), 45).unwrap().1.is_empty());
    }

    #[test]
    fn consultation_inconsistent_all_count_is_not_a_recommendation() {
        let mut agg = fixture();
        agg.competitor.tag_counts_all[0].1 = 4;
        assert!(gaps(&agg, 1).is_none());
    }
}
