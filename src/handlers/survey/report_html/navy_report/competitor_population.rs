//! Population shares use the regional total, including the unrecorded age difference.
use crate::handlers::helpers::{escape_html, format_number};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

/// 総人口を分母にした構成比。分母は地域の総人口で、年齢別の合計との差は「年齢区分未収録」として別に出す。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PopulationShares {
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub male: i64,
    #[ts(type = "number")]
    pub female: i64,
    /// 総人口に対する男性・女性の割合 (%)。
    pub male_share_pct: f64,
    pub female_share_pct: f64,
    /// 0〜14 / 15〜64 / 65 以上 / 年齢区分未収録の 4 区分。3 区分のどれかが欠測、または
    /// 4 区分の合計が総人口と合わないときは null。
    pub age_groups: Option<Vec<AgeShare>>,
    /// 年齢の若い順。
    pub bands: Vec<ShareBand>,
    /// 総人口と年齢別人口の合計の差。
    pub unrecorded: ShareBand,
    /// ピラミッドの片側の軸の最大値 (%)。2 の倍数。
    pub axis_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct AgeShare {
    pub label: String,
    #[ts(type = "number")]
    pub count: i64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ShareBand {
    pub age_group: String,
    #[ts(type = "number")]
    pub male: i64,
    #[ts(type = "number")]
    pub female: i64,
    pub male_share_pct: f64,
    pub female_share_pct: f64,
    /// (男性 + 女性) / 総人口 (%)。
    pub total_share_pct: f64,
}

/// 構成比が成立するときだけ作る。総人口と男女合計の矛盾、年齢別の欠測、年齢別の合計が男女の
/// 総数を超える場合は None (割合を作らない)。
pub(super) fn build_shares(data: &Value) -> Option<PopulationShares> {
    let totals = &data["totals"];
    let count = |key: &str| totals[key].as_i64().filter(|v| *v >= 0);
    let (total, male, female) = (
        count("total_population")?,
        count("male_population")?,
        count("female_population")?,
    );
    if total == 0 || male.checked_add(female) != Some(total) {
        return None;
    }
    let rows = data["bands"].as_array().filter(|rows| !rows.is_empty())?;
    let mut bands = rows
        .iter()
        .map(|r| {
            Some((
                r["age_group"].as_str()?.to_owned(),
                r["male_count"].as_i64().filter(|n| *n >= 0)?,
                r["female_count"].as_i64().filter(|n| *n >= 0)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let sum = |sex: bool| {
        bands.iter().try_fold(0_i64, |sum, (_, m, f)| {
            sum.checked_add(if sex { *m } else { *f })
        })
    };
    let (known_male, known_female) = (sum(true)?, sum(false)?);
    if known_male > male || known_female > female {
        return None;
    }
    bands.sort_by_key(|(age, _, _)| {
        age.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    let unknown_male = male - known_male;
    let unknown_female = female - known_female;
    let unknown = unknown_male + unknown_female;
    let pct = |n: i64| 100.0 * n as f64 / total as f64;
    let band = |age: String, m: i64, f: i64| ShareBand {
        age_group: age,
        male: m,
        female: f,
        male_share_pct: pct(m),
        female_share_pct: pct(f),
        total_share_pct: pct(m + f),
    };
    let age_groups = (|| {
        let (youth, working, older) = (
            count("age_0_14")?,
            count("age_15_64")?,
            count("age_65_over")?,
        );
        let known = youth
            .checked_add(working)?
            .checked_add(older)?
            .checked_add(unknown)?;
        (known == total).then(|| {
            [
                ("0〜14歳", youth),
                ("15〜64歳", working),
                ("65歳以上", older),
                ("年齢区分未収録", unknown),
            ]
            .into_iter()
            .map(|(label, n)| AgeShare {
                label: label.to_owned(),
                count: n,
                share_pct: pct(n),
            })
            .collect::<Vec<_>>()
        })
    })();
    let max_share = bands
        .iter()
        .flat_map(|(_, m, f)| [pct(*m), pct(*f)])
        .fold(0.0_f64, f64::max);
    Some(PopulationShares {
        total,
        male,
        female,
        male_share_pct: pct(male),
        female_share_pct: pct(female),
        age_groups,
        bands: bands.into_iter().map(|(a, m, f)| band(a, m, f)).collect(),
        unrecorded: band(
            "年齢区分未収録（総人口との差分）".into(),
            unknown_male,
            unknown_female,
        ),
        axis_pct: (max_share / 2.0).ceil().max(1.0) * 2.0,
    })
}

const GROUP_COLORS: [&str; 4] = ["#5a9cb0", "#007d79", "#8567a5", "#a8b2bb"];

pub(super) fn render(html: &mut String, shares: &PopulationShares) {
    let (total, male, female) = (shares.total, shares.male, shares.female);
    html.push_str("<div class=\"population-metrics\">");
    for (label, n, pct) in [
        ("総人口", total, 100.0),
        ("男性", male, shares.male_share_pct),
        ("女性", female, shares.female_share_pct),
    ] {
        html.push_str(&format!(
            "<article>{label}<strong>{}人</strong><span>総人口の{pct:.1}%</span></article>",
            format_number(n)
        ));
    }
    html.push_str("</div>");
    if let Some(groups) = &shares.age_groups {
        html.push_str("<h2>年齢3区分の構成比</h2><div class=\"age-composition\" role=\"img\" aria-label=\"年齢3区分と年齢区分未収録分の構成比\">");
        for (group, color) in groups.iter().zip(GROUP_COLORS) {
            html.push_str(&format!("<span style=\"width:{:.8}%;background:{color}\" title=\"{}：{}人 / {:.2}%\"></span>",group.share_pct,group.label,format_number(group.count),group.share_pct));
        }
        html.push_str("</div><div class=\"population-legend\">");
        for (group, color) in groups.iter().zip(GROUP_COLORS) {
            html.push_str(&format!(
                "<span><i style=\"background:{color}\"></i>{} {:.1}%</span>",
                group.label, group.share_pct
            ));
        }
        html.push_str("</div>");
    }
    let axis = shares.axis_pct;
    let height = 90 + shares.bands.len() * 38;
    html.push_str(&format!("<h2>年齢別・男女別の構成比</h2><figure class=\"trend-card\"><div class=\"trend-plot\"><svg viewBox=\"0 0 1000 {height}\" role=\"img\" aria-label=\"人口ピラミッド。左：男性、右：女性。分母は地域の総人口\"><title>年齢別・男女別の人口構成比</title><text x=\"250\" y=\"24\" text-anchor=\"middle\" font-size=\"17\" fill=\"#4472c4\">男性</text><text x=\"750\" y=\"24\" text-anchor=\"middle\" font-size=\"17\" fill=\"#007d79\">女性</text>"));
    for i in 0..=5 {
        let share = axis * i as f64 / 5.0;
        for side in [-1.0, 1.0] {
            let x = if side < 0.0 { 450.0 } else { 550.0 } + side * share / axis * 400.0;
            html.push_str(&format!("<path d=\"M{x} 40 V{}\" stroke=\"#e1e8ec\"/><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-size=\"14\">{share:.1}%</text>",height-38,height-10));
        }
    }
    for (i, b) in shares.bands.iter().rev().enumerate() {
        let y = 48 + i * 38;
        html.push_str(&format!(
            "<text x=\"500\" y=\"{}\" text-anchor=\"middle\" font-size=\"15\">{}</text>",
            y + 19,
            escape_html(&b.age_group)
        ));
        for (sex, n, share, color) in [
            ("male_count", b.male, b.male_share_pct, "#4472c4"),
            ("female_count", b.female, b.female_share_pct, "#007d79"),
        ] {
            let width = share / axis * 400.0;
            let x = if sex == "male_count" {
                450.0 - width
            } else {
                550.0
            };
            html.push_str(&format!("<rect x=\"{x:.3}\" y=\"{y}\" width=\"{width:.3}\" height=\"26\" fill=\"{color}\" data-age=\"{}\" data-sex=\"{sex}\" data-count=\"{n}\" data-share=\"{share}\"><title>{}：{}人 / {share:.2}%</title></rect>",escape_html(&b.age_group),escape_html(&b.age_group),format_number(n)));
        }
    }
    html.push_str("</svg></div></figure><p class=\"note\">構成比の分母：地域の総人口。</p><table class=\"table-navy\"><thead><tr><th>年齢</th><th>男性（人）</th><th>女性（人）</th><th>合計（人）</th><th>構成比</th></tr></thead><tbody>");
    for b in shares
        .bands
        .iter()
        .chain(std::iter::once(&shares.unrecorded))
    {
        html.push_str(&format!("<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{:.2}%</td></tr>",escape_html(&b.age_group),format_number(b.male),format_number(b.female),format_number(b.male+b.female),b.total_share_pct));
    }
    html.push_str(&format!("<tr><th>合計</th><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">100.00%</td></tr></tbody></table>",format_number(male),format_number(female),format_number(total)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shares_include_unrecorded_age_in_denominator() {
        let mut html = String::new();
        let data = json!({"totals":{"total_population":1000,"male_population":400,"female_population":600},"bands":[{"age_group":"20-29","male_count":100,"female_count":200}]});
        render(&mut html, &build_shares(&data).unwrap());
        assert!(html.contains("data-count=\"100\" data-share=\"10\""));
        assert!(html.contains("data-count=\"200\" data-share=\"20\""));
        assert!(html.contains(
            "300</td><td class=\"num\">400</td><td class=\"num\">700</td><td class=\"num\">70.00%"
        ));
        let mut bad = data.clone();
        bad["totals"]["total_population"] = json!(500);
        assert!(build_shares(&bad).is_none());
        bad = data.clone();
        bad["bands"][0]["male_count"] = Value::Null;
        assert!(build_shares(&bad).is_none());
        bad = data;
        bad["bands"][0]["female_count"] = json!(601);
        assert!(build_shares(&bad).is_none());
    }

    #[test]
    fn age_groups_are_dropped_when_they_do_not_sum_to_the_total() {
        let mut data = json!({"totals":{"total_population":1000,"male_population":400,"female_population":600,
            "age_0_14":100,"age_15_64":600,"age_65_over":200},
            "bands":[{"age_group":"20-29","male_count":100,"female_count":200}]});
        // 区分 900 + 年齢別に入っていない分 700 = 1600 は総人口と合わないので、構成比の帯は出さない。
        assert!(build_shares(&data).unwrap().age_groups.is_none());
        data["totals"]["age_15_64"] = json!(100);
        data["totals"]["age_65_over"] = json!(100);
        let groups = build_shares(&data).unwrap().age_groups.unwrap();
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[3].count, 700);
        assert_eq!(groups.iter().map(|g| g.count).sum::<i64>(), 1000);
    }
}
