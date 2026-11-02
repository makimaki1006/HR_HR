//! 「人気の分かれ方」の 6 グループ (給料 / 待遇・手当 / 休み・時間・通勤 / 応募できる人 / 職場の自由さ / 募集の出し方)。
//!
//! 1 本の Indeed CSV の求人を「その項目が載っている求人 / 載っていない求人」に分け、人気・超人気が占める件数を
//! 数える。割合は画面で計算する。n または rest_n が 5 件未満の項目は出さない。
//! タグの割り振りは下の TAG_TABLE (タグ 72 種)。3 つ目の値は分類に迷いが残るタグ。

use super::popularity_split::{Bucket, FeatureChart, FeatureGroup, FeatureRow, Group, Item};

/// 件数がこれ未満の項目 (載っている側・載っていない側のどちらか) は出さない。
const MIN_N: u32 = 5;

/// (タグ, グループ id, 分類に迷いが残る)。
const TAG_TABLE: &[(&str, &str, bool)] = &[
    ("交通費支給", "treat", false),
    ("学歴不問", "who", false),
    ("車通勤OK", "time", false),
    ("賞与あり", "treat", false),
    ("昇給あり", "treat", false),
    ("転勤なし", "time", false),
    ("ブランクOK", "who", false),
    ("社会保険完備", "treat", false),
    ("研修あり", "who", true),
    ("健康保険あり", "treat", false),
    ("資格取得支援あり", "treat", true),
    ("フリーター歓迎", "who", false),
    ("厚生年金あり", "treat", false),
    ("経験者歓迎", "who", false),
    ("雇用保険あり", "treat", false),
    ("長期休暇あり", "time", false),
    ("第二新卒歓迎", "who", false),
    ("労災保険あり", "treat", false),
    ("バイク通勤OK", "time", false),
    ("シフト制", "time", false),
    ("積極採用中", "post", false),
    ("即日勤務OK", "post", false),
    ("有資格者歓迎", "who", false),
    ("残業月20時間以内", "time", false),
    ("未経験者歓迎", "who", false),
    ("住宅手当あり", "treat", false),
    ("固定時間制", "time", false),
    ("主婦・主夫歓迎", "who", false),
    ("髪型・髪色自由", "free", false),
    ("駅近5分以内", "time", false),
    ("40代以上も応募可", "who", false),
    ("長期", "time", true),
    ("残業なし", "time", false),
    ("制服貸与", "treat", false),
    ("寮・社宅あり", "treat", true),
    ("50代以上も応募可", "who", false),
    ("変形労働時間制", "time", false),
    ("ピアスOK", "free", false),
    ("U・Iターン歓迎", "who", false),
    ("履歴書不要", "who", false),
    ("服装自由", "free", false),
    ("シフト自由", "time", false),
    ("ネイルOK", "free", false),
    ("急募", "post", false),
    ("60代以上も応募可", "who", false),
    ("食事補助あり", "treat", false),
    ("昇格あり", "treat", false),
    ("育児サポートあり", "treat", true),
    ("勤務開始時期調整", "post", false),
    ("ひげOK", "free", false),
    ("副業・WワークOK", "free", true),
    ("友達と応募OK", "who", false),
    ("平日のみOK", "time", false),
    ("社割あり", "treat", false),
    ("リモート面接OK", "who", false),
    ("週4日以上OK", "time", false),
    ("社員登用あり", "who", true),
    ("PCスキル", "who", false),
    ("マネージャー採用", "who", false),
    ("在宅OK", "time", false),
    ("家庭都合休OK", "time", false),
    ("月1シフト提出", "time", false),
    ("週払いOK", "treat", false),
    ("即日払いOK", "treat", false),
    ("土日祝のみOK", "time", false),
    ("大量募集", "post", false),
    ("扶養内勤務OK", "time", false),
    ("時差出勤", "time", false),
    ("残業月20時間以上", "time", false),
    ("独立支援あり", "who", true),
    ("送迎あり", "treat", true),
    ("面接時マスク着用", "who", false),
];

/// これら 4 語のどれかがある求人を「各種保険あり」1 行にまとめる。
const INSURANCE_TAGS: [&str; 4] = [
    "健康保険あり",
    "厚生年金あり",
    "雇用保険あり",
    "労災保険あり",
];

const GROUPS: [(&str, &str, &str); 6] = [
    ("pay", "給料", "月給の下限・上限・書き方"),
    ("treat", "待遇・手当", "賞与・昇給・保険・手当"),
    ("time", "休み・時間・通勤", "通勤・残業・シフト"),
    ("who", "応募できる人", "経験・年代・歓迎"),
    ("free", "職場の自由さ", "髪型・服装など"),
    ("post", "募集の出し方", "掲載日数・見出し"),
];

fn row(label: &str, n: u32, p: u32, total: u32, total_p: u32) -> FeatureRow {
    FeatureRow {
        label: label.to_owned(),
        n,
        popular: p,
        rest_n: total - n,
        rest_popular: total_p - p,
        uncertain: false,
        note: None,
    }
}

fn shown(r: &FeatureRow) -> bool {
    r.n >= MIN_N && r.rest_n >= MIN_N
}

// ---- 給料の読み取り ----

#[derive(Debug, Clone, Copy, PartialEq)]
enum Form {
    Month,
    Hour,
    Day,
    Year,
    Income,
    Week,
}

impl Form {
    fn label(self) -> &'static str {
        match self {
            Form::Month => "月給",
            Form::Hour => "時給",
            Form::Day => "日給",
            Form::Year => "年俸",
            Form::Income => "年収",
            Form::Week => "週給",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Style {
    /// 「○万〜○万円」
    Range,
    /// 「○万円以上」
    Min,
    /// 「○万円」ひとつ
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pay {
    form: Form,
    lo: i64,
    hi: Option<i64>,
    style: Style,
}

/// 金額 1 つ: `(<数>万)?(<数>)?円`。先頭から読み、残りを返す。
fn amount(s: &str) -> Option<(i64, &str)> {
    fn num(s: &str) -> (&str, &str) {
        let end = s
            .char_indices()
            .find(|(i, c)| !(c.is_ascii_digit() || (*i > 0 && (*c == '.' || *c == ','))))
            .map_or(s.len(), |(i, _)| i);
        s.split_at(end)
    }
    let mut v = 0.0f64;
    let mut rest = s;
    let (a, r) = num(rest);
    if !a.is_empty() {
        if let Some(r2) = r.strip_prefix('万') {
            v += a.replace(',', "").parse::<f64>().ok()? * 10000.0;
            rest = r2;
        }
    }
    let (b, r) = num(rest);
    if !b.is_empty() {
        v += b.replace(',', "").parse::<f64>().ok()?;
        rest = r;
    }
    let rest = rest.strip_prefix('円')?;
    Some((v.round() as i64, rest))
}

fn parse_pay(s: &str) -> Option<Pay> {
    let s = s.trim();
    let form = [
        Form::Month,
        Form::Hour,
        Form::Day,
        Form::Year,
        Form::Income,
        Form::Week,
    ]
    .into_iter()
    .find(|f| s.starts_with(f.label()))?;
    let rest = s[form.label().len()..].trim_start();
    let (lo, rest) = amount(rest)?;
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Some(Pay {
            form,
            lo,
            hi: None,
            style: Style::Fixed,
        });
    }
    if rest.trim_end() == "以上" {
        return Some(Pay {
            form,
            lo,
            hi: None,
            style: Style::Min,
        });
    }
    let rest = rest
        .strip_prefix('~')
        .or_else(|| rest.strip_prefix('〜'))
        .or_else(|| rest.strip_prefix('～'))?
        .trim_start();
    let (hi, rest) = amount(rest)?;
    rest.trim().is_empty().then_some(Pay {
        form,
        lo,
        hi: Some(hi),
        style: Style::Range,
    })
}

fn quant(v: &[i64], q: f64) -> i64 {
    v[((v.len() as f64 * q) as usize).min(v.len() - 1)]
}

/// 金額の表示 (月給は「25.0万円」、時給などは「1,200円」)。
fn money(form: Form, v: i64) -> String {
    if form == Form::Month {
        return format!("{:.1}万円", v as f64 / 10000.0);
    }
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{out}円")
}

fn others_note(others: &[(Form, u32)], none_n: u32, main: Form) -> Option<String> {
    let mut parts: Vec<String> = others
        .iter()
        .map(|(f, n)| format!("{}の求人 {}件", f.label(), n))
        .collect();
    if none_n > 0 {
        parts.push(format!("給料の記載がない求人 {none_n}件"));
    }
    (!parts.is_empty()).then(|| {
        format!(
            "{}。{}以外は含めていません。",
            parts.join("、"),
            main.label()
        )
    })
}

fn pay_group(items: &[&Item]) -> Option<FeatureGroup> {
    let pays: Vec<(&Item, Option<Pay>)> = items
        .iter()
        .map(|i| (*i, parse_pay(&i.rec.salary_raw)))
        .collect();
    let count = |f: Form| {
        pays.iter()
            .filter(|(_, p)| p.is_some_and(|p| p.form == f))
            .count()
    };
    let main = if count(Form::Hour) > count(Form::Month) {
        Form::Hour
    } else {
        Form::Month
    };
    let none_n = pays.iter().filter(|(_, p)| p.is_none()).count() as u32;
    let mut others: Vec<(Form, u32)> = [
        Form::Day,
        Form::Year,
        Form::Income,
        Form::Week,
        Form::Month,
        Form::Hour,
    ]
    .into_iter()
    .filter(|f| *f != main)
    .map(|f| (f, count(f) as u32))
    .filter(|(_, n)| *n > 0)
    .collect();
    others.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let mj: Vec<(&Item, Pay)> = pays
        .iter()
        .filter_map(|(i, p)| p.filter(|p| p.form == main).map(|p| (*i, p)))
        .collect();
    if mj.is_empty() {
        return None;
    }
    let m_total = mj.len() as u32;
    let m_pop = mj.iter().filter(|(i, _)| i.popular).count() as u32;
    let main_note = others_note(&others, none_n, main);
    let tally = |sel: &[&(&Item, Pay)]| -> (u32, u32) {
        (
            sel.len() as u32,
            sel.iter().filter(|(i, _)| i.popular).count() as u32,
        )
    };
    let mut charts = Vec::new();

    // 下限の帯 (25/50/75 パーセンタイルで 4 区分)
    let mut los: Vec<i64> = mj.iter().map(|(_, p)| p.lo).collect();
    los.sort_unstable();
    let cuts: Vec<i64> = [0.25, 0.5, 0.75].iter().map(|q| quant(&los, *q)).collect();
    let m = |v: i64| money(main, v);
    // 「25.0〜29.5万円」のように単位は後ろだけに付ける。
    let span = |a: i64, b: i64| {
        let a = m(a);
        format!(
            "{}〜{}",
            a.trim_end_matches("万円").trim_end_matches('円'),
            m(b)
        )
    };
    let labels = [
        format!("{}未満", m(cuts[0])),
        format!("{}未満", span(cuts[0], cuts[1])),
        format!("{}未満", span(cuts[1], cuts[2])),
        format!("{}以上", m(cuts[2])),
    ];
    let mut rows = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let sel: Vec<_> = mj
            .iter()
            .filter(|(_, p)| cuts.iter().filter(|c| p.lo >= **c).count() == i)
            .collect();
        let (n, p) = tally(&sel);
        rows.push(row(label, n, p, m_total, m_pop));
    }
    charts.push(chart(
        "lo",
        "下限の帯",
        m_total,
        m_pop,
        true,
        true,
        rows,
        main_note.clone().into_iter().collect(),
    ));

    // 上限の帯 (幅のある求人だけ。上限の 1/3, 2/3 点で 3 区分)
    let rng: Vec<&(&Item, Pay)> = mj.iter().filter(|(_, p)| p.style == Style::Range).collect();
    let r_total = rng.len() as u32;
    let r_pop = rng.iter().filter(|(i, _)| i.popular).count() as u32;
    let range_note = format!(
        "{}を「○万〜○万円」のように範囲で書いた求人 {}件だけで比べています。",
        main.label(),
        r_total
    );
    if !rng.is_empty() {
        let mut his: Vec<i64> = rng.iter().filter_map(|(_, p)| p.hi).collect();
        his.sort_unstable();
        let hc = [quant(&his, 1.0 / 3.0), quant(&his, 2.0 / 3.0)];
        let labels = [
            format!("{}未満", m(hc[0])),
            format!("{}未満", span(hc[0], hc[1])),
            format!("{}以上", m(hc[1])),
        ];
        let mut rows = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            let sel: Vec<_> = rng
                .iter()
                .copied()
                .filter(|(_, p)| hc.iter().filter(|c| p.hi.unwrap_or(0) >= **c).count() == i)
                .collect();
            let (n, p) = tally(&sel);
            rows.push(row(label, n, p, r_total, r_pop));
        }
        charts.push(chart(
            "hi",
            "上限の帯",
            r_total,
            r_pop,
            true,
            true,
            rows,
            vec![range_note.clone()],
        ));
    }

    // 書き方
    let mut rows = Vec::new();
    for (label, st) in [
        ("「○万〜○万円」と範囲で書く", Style::Range),
        ("「○万円以上」とだけ書く", Style::Min),
        ("「○万円」と1つだけ書く", Style::Fixed),
    ] {
        let sel: Vec<_> = mj.iter().filter(|(_, p)| p.style == st).collect();
        let (n, p) = tally(&sel);
        rows.push(row(label, n, p, m_total, m_pop));
    }
    charts.push(chart(
        "form",
        &format!("{}の書き方", main.label()),
        m_total,
        m_pop,
        true,
        true,
        rows,
        main_note.into_iter().collect(),
    ));

    // 上限と下限の差
    let bands: [(&str, i64, i64); 4] = if main == Form::Month {
        [
            ("上限と下限の差が3万円未満", 0, 30000),
            ("上限と下限の差が3万〜6万円未満", 30000, 60000),
            ("上限と下限の差が6万〜10万円未満", 60000, 100000),
            ("上限と下限の差が10万円以上", 100000, i64::MAX),
        ]
    } else {
        [
            ("上限と下限の差が100円未満", 0, 100),
            ("上限と下限の差が100〜300円未満", 100, 300),
            ("上限と下限の差が300〜500円未満", 300, 500),
            ("上限と下限の差が500円以上", 500, i64::MAX),
        ]
    };
    let mut rows = Vec::new();
    for (label, a, b) in bands {
        let sel: Vec<_> = rng
            .iter()
            .copied()
            .filter(|(_, p)| {
                let w = p.hi.unwrap_or(p.lo) - p.lo;
                a <= w && w < b
            })
            .collect();
        let (n, p) = tally(&sel);
        rows.push(row(label, n, p, r_total, r_pop));
    }
    charts.push(chart(
        "width",
        "上限と下限の差",
        r_total,
        r_pop,
        true,
        true,
        rows,
        vec![range_note],
    ));

    for c in &mut charts {
        c.rows.retain(shown);
    }
    charts.retain(|c| !c.rows.is_empty());
    Some(FeatureGroup {
        id: "pay".to_owned(),
        label: "給料".to_owned(),
        caption: format!("{}の下限・上限・書き方", main.label()),
        charts,
    })
}

#[allow(clippy::too_many_arguments)]
fn chart(
    id: &str,
    title: &str,
    total: u32,
    total_popular: u32,
    ordered: bool,
    band: bool,
    rows: Vec<FeatureRow>,
    notes: Vec<String>,
) -> FeatureChart {
    FeatureChart {
        id: id.to_owned(),
        title: title.to_owned(),
        total,
        total_popular,
        ordered,
        band,
        rows,
        notes,
    }
}

/// 6 グループを作る。`by_age` / `writing` / `work_style` は `compute` が数えた区分をそのまま使う。
pub(super) fn build(
    items: &[Item],
    by_age: &[Bucket],
    writing: &[Group],
    work_style: &[Bucket],
) -> Vec<FeatureGroup> {
    let total = items.len() as u32;
    let total_p = items.iter().filter(|i| i.popular).count() as u32;
    let refs: Vec<&Item> = items.iter().collect();
    let mut groups: Vec<(&str, Vec<FeatureRow>)> =
        GROUPS.iter().map(|(id, _, _)| (*id, Vec::new())).collect();
    let push = |groups: &mut Vec<(&str, Vec<FeatureRow>)>, gid: &str, r: FeatureRow| {
        if let Some(g) = groups.iter_mut().find(|g| g.0 == gid) {
            g.1.push(r);
        }
    };
    // タグ。保険 4 語は別にまとめる。
    for (tag, gid, uncertain) in TAG_TABLE {
        if INSURANCE_TAGS.contains(tag) {
            continue;
        }
        let with: Vec<&&Item> = refs
            .iter()
            .filter(|i| i.cond.iter().any(|t| t == tag))
            .collect();
        if with.is_empty() {
            continue;
        }
        let p = with.iter().filter(|i| i.popular).count() as u32;
        let mut r = row(tag, with.len() as u32, p, total, total_p);
        r.uncertain = *uncertain;
        push(&mut groups, gid, r);
    }
    let ins: Vec<&&Item> = refs
        .iter()
        .filter(|i| i.cond.iter().any(|t| INSURANCE_TAGS.contains(&t.as_str())))
        .collect();
    if !ins.is_empty() {
        let p = ins.iter().filter(|i| i.popular).count() as u32;
        let mut r = row("各種保険あり", ins.len() as u32, p, total, total_p);
        r.note = Some("健康・厚生年金・雇用・労災のいずれか".to_owned());
        push(&mut groups, "treat", r);
    }
    // 夜勤 (勤務形態の区分から)
    if let Some(b) = work_style.iter().find(|b| b.label == "夜勤") {
        if b.n > 0 {
            push(
                &mut groups,
                "time",
                row("夜勤", b.n, b.popular, total, total_p),
            );
        }
    }
    // 募集の出し方: 掲載日数・見出し・休日日数
    for b in by_age.iter().filter(|b| b.n > 0 && b.label != "不明") {
        push(
            &mut groups,
            "post",
            row(&format!("掲載 {}", b.label), b.n, b.popular, total, total_p),
        );
    }
    for g in writing {
        for b in &g.rows {
            if b.n == 0 || b.label == "見出し記号なし" || b.label == "休日日数の明記なし"
            {
                continue;
            }
            let label = if g.title == "見出しの長さ" {
                if b.label == "見出しなし" {
                    "見出しがない".to_owned()
                } else {
                    format!("見出しが{}", b.label)
                }
            } else {
                b.label.clone()
            };
            push(
                &mut groups,
                "post",
                row(&label, b.n, b.popular, total, total_p),
            );
        }
    }

    let mut out = Vec::new();
    for (gid, label, caption) in GROUPS {
        if gid == "pay" {
            if let Some(g) = pay_group(&refs) {
                out.push(g);
            }
            continue;
        }
        let mut rows = groups
            .iter_mut()
            .find(|g| g.0 == gid)
            .map(|g| std::mem::take(&mut g.1))
            .unwrap_or_default();
        rows.retain(shown);
        if rows.is_empty() {
            continue;
        }
        out.push(FeatureGroup {
            id: gid.to_owned(),
            label: label.to_owned(),
            caption: caption.to_owned(),
            charts: vec![chart(
                "main",
                &format!("{label}の項目ごとの割合"),
                total,
                total_p,
                false,
                false,
                rows,
                Vec::new(),
            )],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::popularity_split::compute;
    use super::super::upload::{
        parse_csv_bytes_with_hints, CsvSource, SurveyRecord, UserSourceHint,
    };
    use super::*;

    fn rec(salary: &str, tags: &str, snippet: &str) -> SurveyRecord {
        let mut r = parse_csv_bytes_with_hints(
            "css-bxyec3,css-14qk2ra,css-18rxko3,css-u74ql7\nタイトル,会社,月給20万円,人気\n"
                .as_bytes(),
            None,
            UserSourceHint::Auto,
        )
        .unwrap()
        .remove(0);
        r.source = CsvSource::IndeedSp;
        r.salary_raw = salary.to_owned();
        r.tags_raw = tags.to_owned();
        r.snippet = snippet.to_owned();
        r
    }

    #[test]
    fn pay_parse() {
        let p = parse_pay("月給 25万円 ~ 30.5万円").unwrap();
        assert_eq!((p.lo, p.hi, p.style), (250000, Some(305000), Style::Range));
        let p = parse_pay("時給 1,200円").unwrap();
        assert_eq!((p.form, p.lo, p.style), (Form::Hour, 1200, Style::Fixed));
        let p = parse_pay("月給 23万5000円 以上").unwrap();
        assert_eq!((p.lo, p.style), (235000, Style::Min));
        assert!(parse_pay("応相談").is_none());
    }

    #[test]
    fn groups_insurance_threshold_and_pay_bands() {
        // 20 件: 10 件は賞与+健康保険+交通費(人気)、10 件は何も無し。月給は 20万〜39万の 1 万刻み(下限)、上限=下限+5万。
        let mut v = Vec::new();
        for i in 0..20 {
            let lo = 200000 + i * 10000;
            let s = format!("月給 {}万円 ~ {}万円", lo / 10000, (lo + 50000) / 10000);
            let tags = if i < 10 {
                "人気,賞与あり,健康保険あり,交通費支給,急募"
            } else {
                ""
            };
            v.push(rec(&s, tags, ""));
        }
        let s = compute(&v).unwrap();
        let g = |id: &str| s.features.iter().find(|g| g.id == id).unwrap();
        let treat = &g("treat").charts[0];
        let r = |l: &str| treat.rows.iter().find(|r| r.label == l).unwrap();
        assert_eq!(
            (
                r("賞与あり").n,
                r("賞与あり").popular,
                r("賞与あり").rest_n,
                r("賞与あり").rest_popular
            ),
            (10, 10, 10, 0)
        );
        assert_eq!(r("各種保険あり").n, 10);
        assert!(treat.rows.iter().all(|r| r.label != "健康保険あり"));
        assert_eq!(
            r("各種保険あり").note.as_deref(),
            Some("健康・厚生年金・雇用・労災のいずれか")
        );
        // 急募 は 10 件でグループ post、5 件未満の項目は出ない
        assert!(g("post").charts[0]
            .rows
            .iter()
            .any(|r| r.label == "急募" && r.n == 10));
        assert!(s.features.iter().all(|g| g.id != "free"));
        // 下限の帯: 20 件, 25/50/75% 点は index 5,10,15 -> 25万・30万・35万
        let lo = &g("pay").charts[0];
        assert_eq!(lo.id, "lo");
        assert_eq!(lo.total, 20);
        let got: Vec<(String, u32, u32)> = lo
            .rows
            .iter()
            .map(|r| (r.label.clone(), r.n, r.popular))
            .collect();
        assert_eq!(
            got,
            vec![
                ("25.0万円未満".to_owned(), 5, 5),
                ("25.0〜30.0万円未満".to_owned(), 5, 5),
                ("30.0〜35.0万円未満".to_owned(), 5, 0),
                ("35.0万円以上".to_owned(), 5, 0),
            ]
        );
        // 上限の帯: 上限 25..44 万、20 件、1/3 点 index 6 -> 31万、2/3 点 index 13 -> 38万
        let hi = g("pay").charts.iter().find(|c| c.id == "hi").unwrap();
        let got: Vec<u32> = hi.rows.iter().map(|r| r.n).collect();
        assert_eq!(got, vec![6, 7, 7]);
        // 差は全部 5 万 = 1 区分に 20 件、ほかの区分は 5 件未満で出ないため width グラフ自体が無い
        assert!(g("pay").charts.iter().all(|c| c.id != "width"));
    }

    #[test]
    fn real_csv_a_features() {
        let path = "/Users/s_fujimaki/Downloads/indeed-2026-10-27 (3).csv";
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let recs = parse_csv_bytes_with_hints(&bytes, None, UserSourceHint::Auto).unwrap();
        let s = compute(&recs).unwrap();
        // 求人キー (jk) は 350 種類。求人名・URL の列 (css-cdutv5) を読めなかった頃は 331 件 / 150 件だった。
        assert_eq!((s.total, s.popular), (350, 157));
        let out = std::env::var("FEATURES_OUT").unwrap_or_default();
        if !out.is_empty() {
            std::fs::write(out, serde_json::to_string_pretty(&s.features).unwrap()).unwrap();
        }
    }
}
