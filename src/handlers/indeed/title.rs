//! 職種 1 つを深く見る画面 `/tab/indeed/title?name=...`。
//!
//! 一覧は「どれを見るか」を決めるための画面で、ここは「決めたあとに何が言えるか」の画面。
//! 商談の前にこの 1 枚を見れば、その職種について地域・言葉・時給まで話せる状態にする。
//!
//! # 名前で参照している
//! この分析層の主キーは職種名（`norm_title`）そのもの。数値の id は無い。
//! URL に日本語が入るが、既存の職種辞典タブも同じ作りなので合わせる。

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::Html,
};
use serde::Deserialize;

use super::render::{
    arrow, dec1_opt, dir_class, dumbbell_chart, esc, hbar_chart, line_chart, metric_card,
    num_opt, pct_opt, tornado_chart,
};
use crate::indeed::aggregate::Overview;
use crate::indeed::data::snapshot;
use crate::indeed::detail::{self, TitleDetail};
use crate::AppState;

/// 前年比を出してよい月平均の下限。
///
/// # なぜ切るのか
/// 検索ボリュームは粗いきざみで報告される。検索数が少ないほど、
/// 前年比が市場の動きではなくきざみのぶれになる。実測では
///
///     月あたり  〜50 回   前年比の絶対値 中央 23.5%
///           300 回以上                    9.7%
///
/// 施工管理技術者は月 7 回の検索で「前年比 -44.4%」と出ていた。
const MIN_VOL_FOR_YOY: f64 = 100.0;

#[derive(Debug, Deserialize, Default)]
pub struct TitleQuery {
    /// 見たい職種名
    pub name: Option<String>,
}

/// 都道府県ごとの最低賃金。
///
/// # なぜ共通ヘルパを使わないか
/// `regional_analysis::fetch::query_external` は Turso が空なら
/// ローカルの hellowork.db に落ちる作りになっている。この機能では
/// ハローワークのデータを入力に使わない方針なので、Turso だけを見る。
/// 取れなければ列を「—」にするだけで、画面は成立する。
///
/// 出どころ: `v2_external_minimum_wage(prefecture, hourly_min_wage, fiscal_year)`
/// （`src/handlers/analysis/fetch/subtab5_phase4.rs:74` と同じ表）
struct MinWages {
    by_pref: std::collections::HashMap<String, f64>,
    fiscal_year: Option<i64>,
}

fn load_min_wages(state: &AppState) -> MinWages {
    let mut by_pref = std::collections::HashMap::new();
    let mut fiscal_year = None;
    let Some(tdb) = state.turso_db.as_ref() else {
        return MinWages {
            by_pref,
            fiscal_year,
        };
    };
    let rows = match tdb.query(
        "SELECT prefecture, hourly_min_wage, fiscal_year FROM v2_external_minimum_wage",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("最低賃金を引けませんでした: {e}");
            return MinWages {
                by_pref,
                fiscal_year,
            };
        }
    };
    for r in &rows {
        let pref = r
            .get("prefecture")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(w) = r.get("hourly_min_wage").and_then(|v| v.as_f64()) {
            by_pref.insert(pref, w);
        }
        if fiscal_year.is_none() {
            fiscal_year = r.get("fiscal_year").and_then(|v| v.as_i64());
        }
    }
    MinWages {
        by_pref,
        fiscal_year,
    }
}

const GUARD: &str = r#"<script>
(function(){
  if (!document.querySelector('nav')) {
    var target = location.pathname + location.search;
    location.replace('/?tab=' + encodeURIComponent(target));
  }
})();
</script>"#;

const CARD: &str = "bg-navy-800/60 border border-slate-700 rounded-lg p-4";
const TD: &str = "px-3 py-2 border-b border-slate-800 text-slate-200";
const TH: &str = "text-slate-400 font-medium px-3 py-2 border-b border-slate-700";

pub async fn tab_indeed_title(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TitleQuery>,
) -> Html<String> {
    let Some(db) = state.indeed_db.as_ref() else {
        return Html(note("Indeed 分析データが積まれていません。"));
    };
    let Some(name) = q.name.as_deref().filter(|s| !s.is_empty()) else {
        return Html(note("職種が指定されていません。一覧から選んでください。"));
    };
    let snap = match snapshot(db) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("indeed snapshot failed: {e}");
            return Html(note("Indeed 分析データを読めませんでした。"));
        }
    };
    let d = match detail::load(db, name) {
        Ok(Some(d)) => d,
        Ok(None) => return Html(note(&format!("「{name}」のデータが見つかりませんでした。"))),
        Err(e) => {
            tracing::error!("indeed detail failed: {e}");
            return Html(note("この職種のデータを読めませんでした。"));
        }
    };
    // 全国の推移は一覧と同じ集計を通す。ここで作り直さない
    let overview = snap
        .by_title
        .get(name)
        .map(|s| Overview::from_series(name, s, &snap.meta.months));

    let wages = load_min_wages(&state);
    // 検索語まわり。引けなければその節を出さないだけ
    // 検索エンジン側の月次。Indeed 側と重ねるのに使う
    let season = crate::indeed::season::load(db)
        .unwrap_or_else(|e| {
            tracing::warn!("検索エンジン側の月次を読めませんでした: {e}");
            Vec::new()
        })
        .into_iter()
        .find(|s| s.title == name);
    let (term_months, terms) = crate::indeed::keywords::term_monthly(db, name).unwrap_or_else(|e| {
        tracing::warn!("検索語の月次を読めませんでした: {e}");
        (Vec::new(), Vec::new())
    });
    let attrs = crate::indeed::keywords::attr_months(db, name).unwrap_or_else(|e| {
        tracing::warn!("属性の内訳を読めませんでした: {e}");
        Vec::new()
    });
    // スマホ比率は職種そのものの性質。要点で使う
    let mobile = snap
        .titles
        .iter()
        .find(|t| t.name == name)
        .and_then(|t| t.mobile_pct);
    Html(render(
        &d,
        overview.as_ref(),
        &snap.meta.months,
        &wages,
        &term_months,
        &terms,
        &attrs,
        mobile,
        season.as_ref(),
    ))
}

fn note(msg: &str) -> String {
    format!(
        "{GUARD}<div class=\"p-6\"><div class=\"bg-navy-800/60 border border-amber-600/40 rounded-lg p-4 text-amber-200\">{}</div>\
         <p class=\"mt-3\"><a class=\"text-sky-400\" href=\"/tab/indeed\" hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML\">一覧に戻る</a></p></div>",
        esc(msg)
    )
}

fn render(
    d: &TitleDetail,
    ov: Option<&Overview>,
    months: &[String],
    w: &MinWages,
    term_months: &[String],
    terms: &[crate::indeed::keywords::TermSeries],
    attrs: &[crate::indeed::keywords::AttrMonth],
    mobile: Option<f64>,
    season: Option<&crate::indeed::season::TitleSeason>,
) -> String {
    let mut h = String::with_capacity(120_000);
    h.push_str(GUARD);
    h.push_str("<div class=\"space-y-6\">");

    // 見出しと戻り道
    h.push_str(&format!(
        "<div>\
         <p class=\"text-slate-400 text-sm\">\
         <a class=\"text-sky-400 hover:underline\" href=\"/tab/indeed\" \
            hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML\">← 採用市場の一覧</a></p>\
         <h2 class=\"text-xl font-bold text-gray-100 mt-1\">{t}</h2>\
         <p class=\"text-slate-400 text-sm\">{c}／最新月 {m}</p></div>",
        t = esc(&d.title),
        c = esc(&d.category),
        m = esc(&d.month)
    ));

    // 結論を先に置く。図と表はその根拠として下に続く
    h.push_str(&takeaway_section(d, ov, w, terms, mobile));

    // 全国の姿
    if let Some(o) = ov {
        h.push_str("<div class=\"grid grid-cols-2 lg:grid-cols-5 gap-3\">");
        for m in [&o.job, &o.ctk, &o.emp, &o.spp, &o.ppe] {
            h.push_str(&metric_card(m, true));
        }
        h.push_str("</div>");
        h.push_str(&format!(
            "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">全国の動き</h3>\
             <p class=\"text-slate-400 text-xs mb-2\">最初の月を 100 とした指数です。</p>{chart}\
             <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">{s}</p></div>",
            chart = line_chart(
                months,
                &[
                    ("求人の数".to_string(), o.job.indexed()),
                    ("求人を見た人数".to_string(), o.ctk.indexed()),
                    ("募集している企業の数".to_string(), o.emp.indexed()),
                ],
                true,
                280
            ),
            s = esc(&o.spp.sentence)
        ));
    }

    // 図を先、表を後にする。先に形で掴んでから数字を確かめる順
    h.push_str(&pref_bar(d, ov.and_then(|o| o.spp.latest)));
    h.push_str(&wage_gap_chart(d, w));
    h.push_str(&source_compare_section(d, ov, months, season));
    h.push_str(&term_series_section(term_months, terms));
    h.push_str(&attr_section(attrs));
    h.push_str(&pref_table(d, w));
    h.push_str(&keywords_block(d));
    h.push_str(&attrs_block(d));
    h.push_str(&volume_block(d));

    h.push_str("</div>");
    h
}

/// 都道府県別。順位と全国比は、見出しに意味を書いてから出す。
fn pref_table(d: &TitleDetail, w: &MinWages) -> String {
    let of = d.prefs.iter().filter_map(|p| p.of).max().unwrap_or(0);
    let mut h = format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">都道府県別（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         「順位」は 1 求人あたりに見た人が多い順です。<strong>1 位がいちばん集まりやすい県</strong>で、\
         {of} 位がいちばん集まりにくい県になります。\
         「全国比」は全国平均を 1.00 としたときの比です（0.59 なら全国の 59%）。差ではありません。\
         時給は求人票に書かれた金額の中央値で、取れていない県もあります。{wage_note}</p>\
         <div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\" style=\"min-width:980px\"><thead><tr>",
        m = esc(&d.month),
        of = of,
        // 最低賃金は別の出どころなので、どこから来た数字かを書いてから並べる
        wage_note = if w.by_pref.is_empty() {
            String::new()
        } else {
            format!(
                "　最低賃金は{y}の地域別最低賃金（時間額）です。\
                 「差」は掲示時給の中央値からこれを引いたもので、\
                 その県で相場が下限からどれだけ離れているかを見るためのものです。",
                y = match w.fiscal_year {
                    Some(y) => format!("{y} 年度"),
                    None => "公表値".to_string(),
                }
            )
        }
    );
    let mut cols: Vec<(&str, &str)> = vec![
        ("都道府県", "left"),
        ("求人数", "right"),
        ("見た人数", "right"),
        ("募集企業数", "right"),
        ("1求人あたり", "right"),
        ("順位", "right"),
        ("全国比", "right"),
        ("採用難易度", "right"),
        ("時給の中央値", "right"),
    ];
    if !w.by_pref.is_empty() {
        cols.push(("最低賃金", "right"));
        cols.push(("差", "right"));
    }
    for (name, align) in cols {
        h.push_str(&format!(
            "<th scope=\"col\" class=\"{TH}\" style=\"text-align:{align}\">{name}</th>"
        ));
    }
    h.push_str("</tr></thead><tbody>");
    for p in &d.prefs {
        h.push_str(&format!(
            "<tr><th scope=\"row\" class=\"{TD} font-normal\" style=\"text-align:left\">{pf}</th>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{c}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{e}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{s}</td>\
             <td class=\"{TD} tabular-nums text-slate-400\" style=\"text-align:right\">{r}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{v}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{d}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{wg}</td>{extra}</tr>",
            pf = esc(&p.prefecture),
            j = num_opt(p.job),
            c = num_opt(p.ctk),
            e = num_opt(p.employers),
            s = dec1_opt(p.spp),
            r = match (p.rank, p.of) {
                (Some(r), Some(o)) => format!("{r} / {o}"),
                _ => "—".to_string(),
            },
            v = match p.vs_national {
                Some(v) => format!("{v:.2}"),
                None => "—".to_string(),
            },
            d = match p.difficulty {
                Some(v) => format!("{:.0}", v * 100.0),
                None => "—".to_string(),
            },
            wg = match p.wage_median {
                Some(v) => format!("{} 円", num_opt(Some(v))),
                None => "—".to_string(),
            },
            // 最低賃金が引けていないときは、列そのものを出さない
            extra = if w.by_pref.is_empty() {
                String::new()
            } else {
                let mw = w.by_pref.get(&p.prefecture).copied();
                let gap = match (p.wage_median, mw) {
                    (Some(a), Some(b)) => Some(a - b),
                    _ => None,
                };
                format!(
                    "<td class=\"{TD} tabular-nums text-slate-400\" style=\"text-align:right\">{m}</td>\
                     <td class=\"{TD} tabular-nums {gc}\" style=\"text-align:right\">{g}</td>",
                    m = match mw {
                        Some(v) => format!("{} 円", num_opt(Some(v))),
                        None => "—".to_string(),
                    },
                    gc = dir_class(gap, true),
                    g = match gap {
                        Some(v) => format!("{}{} 円", if v >= 0.0 { "+" } else { "-" }, num_opt(Some(v.abs()))),
                        None => "—".to_string(),
                    }
                )
            }
        ));
    }
    h.push_str("</tbody></table></div></div>");
    h
}

/// 検索語。いま何で探されていて、何が増えたか。
fn keywords_block(d: &TitleDetail) -> String {
    // 県ごとの上位語を足し合わせて、この職種の全国の姿にする
    let mut total: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    for p in &d.prefs {
        for (term, n) in &p.keywords {
            *total.entry(term.as_str()).or_insert(0) += n;
        }
    }
    let mut top: Vec<(&str, i64)> = total.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    top.truncate(20);

    let mut h = format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">探すときに使われた言葉</h3>\
         <p class=\"text-slate-400 text-xs mb-3\">\
         求職者が実際に入力した語と、その語からの流入数です。応募数ではありません。</p>\
         <div class=\"grid grid-cols-1 lg:grid-cols-2 gap-4\">"
    );

    h.push_str("<div><p class=\"text-slate-400 text-xs mb-1\">いま多い語（全国の合計）</p>\
                <div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\">");
    if top.is_empty() {
        h.push_str("<tbody><tr><td class=\"text-slate-400 text-sm py-2\">取れていません</td></tr></tbody>");
    } else {
        h.push_str("<tbody>");
        for (t, n) in &top {
            h.push_str(&format!(
                "<tr><td class=\"{TD}\">{t}</td>\
                 <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{n}</td></tr>",
                t = esc(t),
                n = num_opt(Some(*n as f64))
            ));
        }
        h.push_str("</tbody>");
    }
    h.push_str("</table></div></div>");

    // シェアの動き。増えた順・減った順の両方を出す
    let mut shifts: Vec<_> = d.shifts.iter().filter(|s| s.diff.is_some()).collect();
    shifts.sort_by(|a, b| b.diff.unwrap_or(0.0).total_cmp(&a.diff.unwrap_or(0.0)));
    let picked: Vec<_> = shifts
        .iter()
        .take(6)
        .chain(shifts.iter().rev().take(6))
        .collect();

    h.push_str("<div><p class=\"text-slate-400 text-xs mb-1\">シェアが動いた語（期間の前半と後半の比較）</p>\
                <div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\">");
    if picked.is_empty() {
        h.push_str("<tbody><tr><td class=\"text-slate-400 text-sm py-2\">比べられるだけのデータがありません</td></tr></tbody>");
    } else {
        h.push_str("<tbody>");
        let mut seen: Vec<&str> = Vec::new();
        for s in picked {
            if seen.contains(&s.term.as_str()) {
                continue;
            }
            seen.push(s.term.as_str());
            h.push_str(&format!(
                "<tr><td class=\"{TD}\">{t}</td>\
                 <td class=\"{TD} tabular-nums text-slate-400\" style=\"text-align:right\">{b} → {a}</td>\
                 <td class=\"{TD} tabular-nums {dc}\" style=\"text-align:right\">{ar} {df}</td></tr>",
                t = esc(&s.term),
                b = match s.before { Some(v) => format!("{v:.1}%"), None => "—".into() },
                a = match s.after { Some(v) => format!("{v:.1}%"), None => "—".into() },
                dc = dir_class(s.diff, true),
                ar = arrow(s.diff),
                df = match s.diff { Some(v) => format!("{v:+.1}pt"), None => "—".into() }
            ));
        }
        h.push_str("</tbody>");
    }
    h.push_str("</table></div></div></div></div>");
    h
}

/// 探している人の傾向。
fn attrs_block(d: &TitleDetail) -> String {
    let Some(a) = &d.attrs else {
        return String::new();
    };
    let mut h = format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">どんな人が探しているか（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-3 leading-relaxed\">\
         入力された語に、その言葉が含まれていた割合です。1 つの語が複数に当てはまることがあるので、\
         <strong>足しても 100% にはなりません</strong>。語の種類 {n} 件、流入 {c} 件が対象です。</p>\
         <div class=\"grid grid-cols-2 sm:grid-cols-4 gap-3\">",
        m = esc(&a.month),
        n = a.terms.map(|v| v.to_string()).unwrap_or("—".into()),
        c = num_opt(a.total_clicks)
    );
    for (name, v) in &a.shares {
        h.push_str(&format!(
            "<div class=\"bg-navy-900/40 border border-slate-700 rounded p-3\">\
             <div class=\"text-slate-400 text-xs\">{n}</div>\
             <div class=\"text-slate-100 text-lg font-bold tabular-nums\">{v}</div></div>",
            n = esc(name),
            v = match v {
                Some(x) => format!("{x:.1}%"),
                None => "—".to_string(),
            }
        ));
    }
    h.push_str("</div></div>");
    h
}

/// 検索エンジン側の検索ボリューム。
fn volume_block(d: &TitleDetail) -> String {
    if d.volumes.is_empty() {
        return String::new();
    }
    let mut h = format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">この職種が検索された量</h3>\
         <p class=\"text-slate-400 text-xs mb-3 leading-relaxed\">\
         検索エンジンの推定値で、月ごとに丸められています。\
         職種名だけの検索は求職以外の意図も含むため、求職の目安には「職種名＋求人」を見てください。\
         <strong>月平均が {MIN_VOL_FOR_YOY} 回に満たない行は前年比を出していません。</strong>\
         丸めのきざみが大きく、市場の動きではなくきざみを見ることになるためです\
         （実測で、月 50 回未満では前年比の絶対値が中央 23.5%、月 300 回以上では 9.7%）。</p>\
         <div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\" style=\"min-width:640px\"><thead><tr>"
    );
    for (n, a) in [
        ("検索の仕方", "left"),
        ("月平均", "right"),
        ("直近", "right"),
        ("前年比", "right"),
        ("競合の多さ", "left"),
        ("広告単価の目安", "right"),
    ] {
        h.push_str(&format!(
            "<th scope=\"col\" class=\"{TH}\" style=\"text-align:{a}\">{n}</th>"
        ));
    }
    h.push_str("</tr></thead><tbody>");
    for v in &d.volumes {
        // 丸めのきざみが大きい行の前年比は落とす。
        // 施工管理技術者は月 7 回の検索で「前年比 -44.4%」と出ていた
        let yoy = match v.avg_monthly {
            Some(a) if a as f64 >= MIN_VOL_FOR_YOY => v.yoy_pct,
            _ => None,
        };
        let label = match v.variant.as_str() {
            "job" => "職種名 ＋ 求人",
            "name" => "職種名だけ",
            other => other,
        };
        h.push_str(&format!(
            "<tr><th scope=\"row\" class=\"{TD} font-normal\" style=\"text-align:left\">{l}</th>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{a}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{n}</td>\
             <td class=\"{TD} tabular-nums {dc}\" style=\"text-align:right\">{ar} {y}</td>\
             <td class=\"{TD} text-slate-300\">{cp}</td>\
             <td class=\"{TD} tabular-nums text-slate-400\" style=\"text-align:right\">{bid}</td></tr>",
            l = esc(label),
            a = num_opt(v.avg_monthly.map(|x| x as f64)),
            n = num_opt(v.latest.map(|x| x as f64)),
            dc = dir_class(yoy, true),
            ar = arrow(yoy),
            y = pct_opt(yoy),
            cp = esc(&v.competition),
            bid = match (v.low_bid_yen, v.high_bid_yen) {
                (Some(a), Some(b)) => format!("{:.0} 〜 {:.0} 円", a, b),
                _ => "—".to_string(),
            }
        ));
    }
    h.push_str("</tbody></table></div></div>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indeed::detail::PrefRow;

    fn row(pref: &str, wage: Option<f64>) -> PrefRow {
        PrefRow {
            prefecture: pref.to_string(),
            job: Some(100.0),
            ctk: Some(800.0),
            employers: Some(20.0),
            spp: Some(8.0),
            rank: Some(1),
            of: Some(47),
            vs_national: Some(1.0),
            difficulty: Some(0.5),
            keywords: vec![],
            wage_median: wage,
        }
    }

    fn detail(prefs: Vec<PrefRow>) -> TitleDetail {
        TitleDetail {
            title: "試し".into(),
            category: "試し分類".into(),
            month: "2026-07".into(),
            prefs,
            shifts: vec![],
            attrs: None,
            volumes: vec![],
        }
    }

    /// 最低賃金が引けたときは、列と差が出ること。
    #[test]
    fn 最低賃金が引けたら列が出て差が計算される() {
        let d = detail(vec![row("東京都", Some(1545.0)), row("沖縄県", None)]);
        let w = MinWages {
            by_pref: [("東京都".to_string(), 1226.0)].into_iter().collect(),
            fiscal_year: Some(2025),
        };
        let html = pref_table(&d, &w);
        assert!(html.contains("最低賃金"), "見出しに最低賃金が無い");
        assert!(html.contains("2025 年度"), "何年度の値かが書かれていない");
        assert!(html.contains("1,226 円"), "最低賃金の値が出ていない");
        // 1545 - 1226 = 319
        assert!(html.contains("+319 円"), "差が出ていない: {html}");
        // 時給が取れていない県で、差を 0 や最低賃金そのものにしない
        let after = html.split("沖縄県").nth(1).unwrap_or("");
        assert!(
            !after.contains("+1,226") && !after.contains("+0 円"),
            "時給が無い県で差を作ってしまっている"
        );
    }

    /// 最低賃金が引けないときは、列そのものを出さないこと。
    ///
    /// 空欄の列が並ぶより、無いほうが読みやすい。
    #[test]
    fn 最低賃金が引けなければ列を出さない() {
        let d = detail(vec![row("東京都", Some(1545.0))]);
        let w = MinWages {
            by_pref: Default::default(),
            fiscal_year: None,
        };
        let html = pref_table(&d, &w);
        assert!(!html.contains("最低賃金"), "引けていないのに列が出ている");
        assert!(html.contains("1,545 円"), "時給の列まで消えている");
    }

    /// 順位と全国比の説明が、表と一緒に必ず出ること。
    ///
    /// 数字だけ出すと、順位の向きも全国比が比なのかも伝わらない。
    #[test]
    fn 順位と全国比の説明が表と一緒に出る() {
        let d = detail(vec![row("東京都", None)]);
        let w = MinWages {
            by_pref: Default::default(),
            fiscal_year: None,
        };
        let html = pref_table(&d, &w);
        assert!(
            html.contains("1 位がいちばん集まりやすい県"),
            "順位の向きの説明が無い"
        );
        assert!(html.contains("差ではありません"), "全国比が比だという断りが無い");
    }
}

/// この職種について言えることを、先頭にまとめる。
///
/// # なぜ足したのか
/// 図と表は並んでいたが、「この職種の商談で何を言えるか」は
/// 読み手が組み立てるしかなかった。材料は下にそろっているので、
/// 答えの形にして先頭に置く。
///
/// # 線引き
/// 下の図から出せることだけを書く。無い材料の行は出さない。
/// 断定を避け、「〜の候補になります」「〜と見えます」に留める。
fn takeaway_section(
    d: &TitleDetail,
    ov: Option<&Overview>,
    w: &MinWages,
    terms: &[crate::indeed::keywords::TermSeries],
    mobile: Option<f64>,
) -> String {
    let mut items: Vec<String> = Vec::new();

    // 1. 集まりやすさ
    if let Some(o) = ov {
        if let (Some(sp), Some(j)) = (o.spp.latest, o.job.latest) {
            let ch = o.spp.change_pct;
            let dir = match ch {
                Some(c) if c < -5.0 => "集まりにくさが強まっています。求人票か媒体を見直す候補です。",
                Some(c) if c > 5.0 => "集まりやすくなっています。競合が引いた可能性があります。",
                _ => "集まりやすさはこの期間ほぼ横ばいです。",
            };
            items.push(format!(
                "<li>いま求人 <strong>{}</strong> 件、1 求人あたりに見た人は <strong>{}</strong> 人。\
                 この期間で {}。{}</li>",
                num_opt(Some(j)),
                dec1_opt(Some(sp)),
                pct_opt(ch),
                dir
            ));
        }
    }

    // 2. どの県で戦うか
    let mut top: Vec<&crate::indeed::detail::PrefRow> =
        d.prefs.iter().filter(|p| p.spp.is_some()).collect();
    if top.len() >= 5 {
        top.sort_by(|a, b| b.spp.unwrap_or(0.0).total_cmp(&a.spp.unwrap_or(0.0)));
        let easy: Vec<String> = top.iter().take(3).map(|p| esc(&p.prefecture)).collect();
        let hard: Vec<String> = top
            .iter()
            .rev()
            .take(3)
            .map(|p| esc(&p.prefecture))
            .collect();
        items.push(format!(
            "<li>人が集まりやすいのは <strong>{}</strong>、集まりにくいのは <strong>{}</strong>。\
             同じ求人票でも、県によって手応えが変わります。</li>",
            easy.join("・"),
            hard.join("・")
        ));
    }

    // 3. 求人票の職種名
    let mut moved: Vec<&crate::indeed::keywords::TermSeries> = terms.iter().collect();
    moved.sort_by(|a, b| b.diff_pt.unwrap_or(0.0).total_cmp(&a.diff_pt.unwrap_or(0.0)));
    if let Some(up) = moved.first().filter(|x| x.diff_pt.unwrap_or(0.0) > 1.0) {
        let down = moved.last().filter(|x| x.diff_pt.unwrap_or(0.0) < -1.0);
        items.push(format!(
            "<li>探すときの言葉が「{}」に寄っています{}。\
             <strong>求人票の職種名に入っているか確かめる価値があります。</strong></li>",
            esc(&up.term),
            match down {
                Some(x) => format!("（逆に「{}」は減りました）", esc(&x.term)),
                None => String::new(),
            }
        ));
    }

    // 4. 応募の入口
    if let Some(m) = mobile {
        // 表示は小数第 1 位まで。判定も同じ丸めで行う。
        // 生の 69.95 は「70.0%」と出るのに「70 未満」と判定され、
        // 「70.0% がスマホから。スマホと PC が混ざります」と食い違っていた
        let m = (m * 10.0).round() / 10.0;
        let s = if m >= 70.0 {
            "ほとんどスマホです。応募フォームがスマホで完了するか確かめてください。"
        } else if m <= 55.0 {
            "PC が比較的多い職種です。スマホ前提の作りだけでは取りこぼす可能性があります。"
        } else {
            "スマホと PC が混ざります。"
        };
        items.push(format!(
            "<li>この職種を探す人の <strong>{:.1}%</strong> がスマホから。{}</li>",
            m, s
        ));
    }

    // 5. 時給
    if !w.by_pref.is_empty() {
        let below: Vec<&str> = d
            .prefs
            .iter()
            .filter(|p| match (p.wage_median, w.by_pref.get(&p.prefecture)) {
                (Some(x), Some(m)) => x < *m,
                _ => false,
            })
            .map(|p| p.prefecture.as_str())
            .collect();
        if !below.is_empty() {
            items.push(format!(
                "<li><strong>{} 県で、掲示時給の中央値が最低賃金を下回っています</strong>（{}など）。\
                 業務委託や基本給だけの掲示が混ざるとこうなります。\
                 提示額を決めるときは、この相場をそのまま使わないでください。</li>",
                below.len(),
                below
                    .iter()
                    .take(3)
                    .map(|s| esc(s))
                    .collect::<Vec<_>>()
                    .join("・")
            ));
        }
    }

    if items.is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"bg-sky-900/30 border-l-4 border-sky-400 rounded-r-lg p-4\">\
         <h3 class=\"text-sky-100 text-base font-bold mb-2\">この職種で言えること</h3>\
         <ul class=\"text-slate-200 text-sm leading-relaxed list-disc pl-5 space-y-2\">{}</ul>\
         <p class=\"text-slate-500 text-xs mt-3 leading-relaxed\">\
         下の図と表から出せることだけを書いています。根拠は各図の下にあります。</p></div>",
        items.join("")
    )
}

/// 検索エンジン側と Indeed 側を、同じ時間軸に並べる。
///
/// # 何が違うデータなのか
/// * 検索エンジン … Indeed の外で「◯◯ 求人」と検索した回数。48 か月ある
/// * Indeed …… Indeed の中で求人を見た人数。14 か月しかない
///
/// 前者は「そもそもこの仕事を探している人がどれだけいるか」、
/// 後者は「そのうち Indeed まで来て求人を見た人がどれだけか」。
/// 数えているものが違うので、量は比べられない。**動きの向き**だけを見る。
///
/// # どこまで言えるか
/// 重なるのは 13 か月しかない。13 点だと相関はよほど大きくないと
/// 偶然と区別できない（|r| > 0.55 が目安）。実測では
///
///     検索エンジンの月平均   職種数   |相関| の中央   0.55 を超える割合
///          〜50            15        0.26          26%
///        50〜200           21        0.21           4%
///       200〜1000          26        0.33          19%
///        1000〜            19        0.46          31%
///
/// と、検索数が多いほど関係が強い。ただし多い帯でも 3 割しか超えない。
/// **相関の数字だけを出して「連動している」と書かない。**両方の線を出して、
/// 読み手が形を見られるようにする。
fn source_compare_section(
    d: &TitleDetail,
    ov: Option<&Overview>,
    months: &[String],
    season: Option<&crate::indeed::season::TitleSeason>,
) -> String {
    let (Some(o), Some(sn)) = (ov, season) else {
        return String::new();
    };
    if sn.months.len() < 12 || months.is_empty() {
        return String::new();
    }
    // 検索エンジン側の月をそのまま軸にし、Indeed 側は重なる月にだけ置く
    let axis: Vec<String> = sn.months.clone();
    let idx: std::collections::HashMap<&str, usize> = months
        .iter()
        .enumerate()
        .map(|(i, m)| (m.as_str(), i))
        .collect();
    let indeed: Vec<Option<f64>> = axis
        .iter()
        .map(|m| idx.get(m.as_str()).and_then(|i| o.ctk.series.get(*i).copied().flatten()))
        .collect();
    let overlap = indeed.iter().filter(|v| v.is_some()).count();
    if overlap < 6 {
        return String::new();
    }
    // 量が違うので指数にそろえる。重なりの最初の月を 100 にする
    let base_at = indeed.iter().position(|v| v.is_some()).unwrap_or(0);
    let to_index = |v: &[Option<f64>]| -> Vec<Option<f64>> {
        let base = v.get(base_at).copied().flatten();
        match base {
            Some(b) if b > 0.0 => v.iter().map(|x| x.map(|y| y / b * 100.0)).collect(),
            _ => vec![None; v.len()],
        }
    };
    let g = to_index(&sn.series);
    let i2 = to_index(&indeed);
    if g.iter().all(|x| x.is_none()) || i2.iter().all(|x| x.is_none()) {
        return String::new();
    }

    // 重なる区間の相関。断定には使わず、形の説明にだけ添える
    let pairs: Vec<(f64, f64)> = g
        .iter()
        .zip(i2.iter())
        .filter_map(|(a, b)| match (a, b) {
            (Some(x), Some(y)) => Some((*x, *y)),
            _ => None,
        })
        .collect();
    let r = if pairs.len() >= 6 {
        let n = pairs.len() as f64;
        let ma = pairs.iter().map(|p| p.0).sum::<f64>() / n;
        let mb = pairs.iter().map(|p| p.1).sum::<f64>() / n;
        let num: f64 = pairs.iter().map(|p| (p.0 - ma) * (p.1 - mb)).sum();
        let da: f64 = pairs.iter().map(|p| (p.0 - ma).powi(2)).sum::<f64>().sqrt();
        let db: f64 = pairs.iter().map(|p| (p.1 - mb).powi(2)).sum::<f64>().sqrt();
        if da > 0.0 && db > 0.0 {
            Some(num / (da * db))
        } else {
            None
        }
    } else {
        None
    };
    let say = match r {
        Some(x) if x > 0.55 => format!(
            "重なる {overlap} か月では<strong>同じ向きに動いて見えます</strong>（相関 {x:+.2}）。\
             外での関心と Indeed での動きが揃っている職種です。"
        ),
        Some(x) if x < -0.55 => format!(
            "重なる {overlap} か月では<strong>逆向きに動いて見えます</strong>（相関 {x:+.2}）。\
             外で探す人が増えているのに Indeed では見られていない、\
             またはその逆が起きている可能性があります。"
        ),
        Some(x) => format!(
            "重なる {overlap} か月では、はっきりした関係は見えません（相関 {x:+.2}）。\
             13 か月ほどでは、相関が ±0.55 を超えないと偶然と区別できません。"
        ),
        None => String::new(),
    };

    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         検索エンジンでの関心と、Indeed での動き</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         <strong>数えているものが違います。</strong>\
         青は Indeed の外で「{t} 求人」と検索された回数（{y} か月ぶん）、\
         もう 1 本は Indeed の中で求人を見た人数（{n} か月ぶん）です。\
         量は比べられないので、<strong>重なりの最初の月を 100 とした指数</strong>で\
         形だけを並べています。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">{say}</p>\
         <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">\
         <strong>青い線が階段状なのは、検索エンジン側が粗い刻みで報告するため</strong>です\
         （実際に「5,600」「4,400」のような値しか返りません）。細かい上下は刻みの影響で、\
         意味のある動きではありません。長い期間の傾きだけを見てください。<br>\
         重なりは 13 か月ほどしかありません。この長さでは、相関が ±0.55 を超えても\
         偶然と言い切れないことに注意してください。<br>\
         検索エンジン側は職種名の 1 語だけで、Indeed 側のような語ごとの内訳は取れません。\
         そのため「どの言葉で探しているか」の比較はできず、動きの向きだけを見ています。</p></div>",
        t = esc(&d.title),
        y = sn.months.len(),
        n = overlap,
        chart = line_chart(
            &axis,
            &[
                ("検索エンジンでの検索数".to_string(), g),
                ("Indeed で求人を見た人数".to_string(), i2),
            ],
            true,
            340
        ),
        say = say
    )
}

/// 探し方の言葉の月次推移。
///
/// # なぜ月次で出すのか
/// はじめは前 3 か月と直近 3 か月の平均を比べた図（中央 0 の横棒）にしていたが、
/// 2 点しか無いのでトレンドが読めない。実データの「事務」は
/// 45.3 → 46.4 → 46.3 → 44.8 → 41.4 → 40.8 → 34.9 → 36.6 → 37.9 →
/// 35.7 → 35.8 → 31.9 → 29.8 → 26.8 と、2025-09 から一貫して下がり続けている。
/// 2 点に丸めると「46.1 → 29.8」としか読めず、
/// いつから動いたのか・まだ続いているのかが分からない。判断を誤る。
///
/// # どの語を出すか
/// 直近のシェアが大きい順に 6 本まで。折れ線を増やすほど読めなくなる。
/// 残りは本文で件数だけ伝える。
fn term_series_section(months: &[String], series: &[crate::indeed::keywords::TermSeries]) -> String {
    if months.len() < 4 || series.len() < 2 {
        return String::new();
    }
    const SHOW: usize = 6;
    let lines: Vec<(String, Vec<Option<f64>>)> = series
        .iter()
        .take(SHOW)
        .map(|t| (t.term.clone(), t.pct.clone()))
        .collect();

    // 期間を通していちばん増えた語・減った語
    let mut by_move: Vec<&crate::indeed::keywords::TermSeries> = series.iter().collect();
    by_move.sort_by(|a, b| {
        b.diff_pt
            .unwrap_or(0.0)
            .total_cmp(&a.diff_pt.unwrap_or(0.0))
    });
    let say = |t: Option<&&crate::indeed::keywords::TermSeries>| match t {
        Some(x) => {
            let first = x.pct.iter().flatten().next().copied();
            format!(
                "「{}」（{} → {}）",
                esc(&x.term),
                pct1(first),
                pct1(x.latest)
            )
        }
        None => "—".to_string(),
    };
    let up = by_move.first().filter(|x| x.diff_pt.unwrap_or(0.0) > 0.0);
    let down = by_move.last().filter(|x| x.diff_pt.unwrap_or(0.0) < 0.0);

    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         探し方の言葉の移り変わり</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         この職種を探す人が使った言葉の内訳を、<strong>月ごと</strong>に出しています。\
         縦はその月の検索の中で占める割合（%）。直近で大きい {n} 語だけを出しています\
         （全 {all} 語）。県ごとに上位 10 語しか取れないため、\
         どの月も 1% に届かない語は入っていません。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         期間を通していちばん増えたのは{u}、いちばん減ったのは{d}です。\
         <strong>線が交差していれば、呼び名が置き換わっている最中です。</strong>\
         求人票の職種名を、増えている側に寄せると見つけてもらいやすくなります。</p></div>",
        n = lines.len(),
        all = series.len(),
        chart = line_chart(months, &lines, true, 340),
        u = say(up),
        d = say(down)
    )
}

/// 探している人の内訳の月次推移。
///
/// # なぜ月次で出すのか
/// 8 区分の割合を「頭 3 か月の平均」と「直近 3 か月の平均」で比べていたが、
/// 2 点では動き方が分からない。月ごとに出せば、
/// じわじわ動いているのか、ある月から急に変わったのかが見える。
/// データは最初から 14 か月ぶんある。丸めていたのはこちらの都合だった。
///
/// # 出さない区分
/// 全期間 0% の区分は線が軸に張り付くだけなので外し、名前だけ文章で挙げる。
fn attr_section(months_all: &[crate::indeed::keywords::AttrMonth]) -> String {
    use crate::indeed::keywords::ATTR_LABELS;
    if months_all.len() < 4 {
        return String::new();
    }
    let months: Vec<String> = months_all.iter().map(|m| m.month.clone()).collect();
    let mut live: Vec<usize> = Vec::new();
    let mut dead: Vec<&str> = Vec::new();
    for i in 0..8 {
        if months_all
            .iter()
            .any(|m| m.pct[i].unwrap_or(0.0) > 0.0)
        {
            live.push(i);
        } else {
            dead.push(ATTR_LABELS[i]);
        }
    }
    if live.is_empty() {
        return String::new();
    }
    // 直近の割合が大きい順。折れ線は 7 本まで（色がひと回りする）
    live.sort_by(|a, b| {
        let last = |i: usize| {
            months_all
                .iter()
                .rev()
                .find_map(|m| m.pct[i])
                .unwrap_or(0.0)
        };
        last(*b).total_cmp(&last(*a))
    });
    live.truncate(7);
    let lines: Vec<(String, Vec<Option<f64>>)> = live
        .iter()
        .map(|i| {
            (
                ATTR_LABELS[*i].to_string(),
                months_all.iter().map(|m| m.pct[*i]).collect(),
            )
        })
        .collect();

    // 母数。薄い月は割合が振れる
    let mut tc: Vec<f64> = months_all.iter().filter_map(|m| m.term_count).collect();
    tc.sort_by(|a, b| a.total_cmp(b));
    let med = tc.get(tc.len() / 2).copied();

    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         探している人の内訳の移り変わり</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         検索語に出てくる言葉を 8 つに区分し、<strong>月ごとの割合</strong>を出しています。\
         単位は %。{dead}</p>{chart}\
         <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">\
         この職種の母数（月あたりの語数）は中央 {tc} です。\
         母数が小さい月ほど割合が振れます。実測では、母数 20 未満の職種で\
         月ごとの振れ幅が中央 12.2 ポイント、60 以上では 3.1 ポイントでした。\
         <strong>1 か月だけの上下で判断しないでください。</strong></p></div>",
        dead = if dead.is_empty() {
            String::new()
        } else {
            format!(
                "{}は全期間 0% なので出していません。",
                dead.iter().map(|s| esc(s)).collect::<Vec<_>>().join("・")
            )
        },
        chart = line_chart(&months, &lines, true, 340),
        tc = med.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".to_string())
    )
}

/// 割合を「12.3%」の形にする。欠測は「—」。
fn pct1(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.1}%"),
        None => "—".to_string(),
    }
}

/// 都道府県を並べた横棒。順位を目で追うための図。
///
/// # なぜ表と両方出すのか
/// 表は 47 行あり、上から読まないと順位が分からない。長さで並べれば
/// 「どこが集めやすいか」は一目で決まる。数字そのものは下の表で確かめる。
fn pref_bar(d: &TitleDetail, nation_spp: Option<f64>) -> String {
    let mut rows: Vec<&crate::indeed::detail::PrefRow> =
        d.prefs.iter().filter(|p| p.spp.is_some()).collect();
    if rows.len() < 5 {
        return String::new();
    }
    // 1 求人あたりが多い順。上が「集まりやすい」
    rows.sort_by(|a, b| b.spp.unwrap_or(0.0).total_cmp(&a.spp.unwrap_or(0.0)));
    let labels: Vec<String> = rows.iter().map(|p| p.prefecture.clone()).collect();
    let values: Vec<Option<f64>> = rows.iter().map(|p| p.spp).collect();
    let height = 140 + (rows.len() as u32) * 15;

    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         都道府県別の 1 求人あたりに見た人数（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         多い順に並べています。<strong>上ほど 1 件の求人に人が集まっている</strong>県です。\
         {base}</p>{chart}</div>",
        m = esc(&d.month),
        base = match nation_spp {
            Some(v) => format!(
                "破線は全国（{}）です。これより下にある県は、全国より集まりにくいことになります。",
                dec1_opt(Some(v))
            ),
            None => String::new(),
        },
        chart = hbar_chart(
            &labels,
            &values,
            "1 求人あたりに見た人数",
            nation_spp.map(|v| (v, "全国")),
            true,
            height
        )
    )
}

/// 掲示時給が最低賃金からどれだけ離れているかを、県ごとに帯で見せる。
///
/// 棒を 2 本並べると差そのものが読み取りにくい。下限からの帯にすると、
/// 「この県は下限すれすれ」「この県は余裕がある」が長さで分かる。
fn wage_gap_chart(d: &TitleDetail, w: &MinWages) -> String {
    if w.by_pref.is_empty() {
        return String::new();
    }
    let mut rows: Vec<(String, f64, f64)> = d
        .prefs
        .iter()
        .filter_map(|p| {
            let wage = p.wage_median?;
            let min = w.by_pref.get(&p.prefecture).copied()?;
            Some((p.prefecture.clone(), min, wage))
        })
        .collect();
    if rows.len() < 5 {
        return String::new();
    }
    // 差の大きい順。上ほど下限から離れている
    rows.sort_by(|a, b| (b.2 - b.1).total_cmp(&(a.2 - a.1)));
    let labels: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let low: Vec<Option<f64>> = rows.iter().map(|r| Some(r.1)).collect();
    let high: Vec<Option<f64>> = rows.iter().map(|r| Some(r.2)).collect();
    let height = 140 + (rows.len() as u32) * 15;

    let top = &rows[0];
    // 掲示が下限を割っている県。丸めずに数え、理由も添える
    let below: Vec<&str> = rows
        .iter()
        .filter(|r| r.2 < r.1)
        .map(|r| r.0.as_str())
        .collect();
    let tail = if below.is_empty() {
        let b = &rows[rows.len() - 1];
        format!(
            "いちばん狭いのは{}（{} 円）です。",
            esc(&b.0),
            num_opt(Some(b.2 - b.1))
        )
    } else {
        // 下回るのは異常値ではない。理由が言えるので言っておく
        format!(
            "一方、{}の {} 県では、掲示時給の中央値が最低賃金を下回っています。\
             基本給だけを載せた求人、改定前に出された求人、\
             最低賃金の対象にならない業務委託の求人が混ざるとこうなります。",
            below
                .iter()
                .take(3)
                .map(|s| esc(s))
                .collect::<Vec<_>>()
                .join("・")
                + if below.len() > 3 { "など" } else { "" },
            below.len()
        )
    };
    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         掲示時給と最低賃金の開き</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         帯の左端が{y}の最低賃金、右端がこの職種の掲示時給の中央値です。\
         <strong>帯が長いほど、相場と下限が離れています。</strong>青は下限より上、オレンジは下限を下回っている県です。\
         時給で出ている求人だけが対象で、月給・日給の求人は含みません。\n         そのため出ている県は 47 県のうち <strong>{cnt} 県</strong>です。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         いちばん開いているのは{t}（{tv} 円）です。{tail}</p></div>",
        y = match w.fiscal_year {
            Some(y) => format!("{y} 年度"),
            None => "公表値".to_string(),
        },
        chart = dumbbell_chart(
            &labels,
            &low,
            &high,
            "最低賃金",
            "最低賃金からの上乗せ",
            "最低賃金を下回る分",
            "円",
            true,
            height
        ),
        // 何県ぶんの話なのかを書く。月給で出す職種は時給の掲載が少なく、
        // 土木技術者は 7 県しか無い。7 本の棒を 47 県の話と読まれないようにする
        cnt = rows.len(),
        t = esc(&top.0),
        tv = num_opt(Some(top.2 - top.1)),
        tail = tail
    )
}

#[cfg(test)]
mod volume_tests {
    use super::*;
    use crate::indeed::detail::SearchVolume;

    fn v(avg: i64, yoy: f64) -> SearchVolume {
        SearchVolume {
            variant: "job".to_string(),
            avg_monthly: Some(avg),
            latest: Some(avg),
            yoy_pct: Some(yoy),
            competition: "MEDIUM".to_string(),
            low_bid_yen: Some(50.0),
            high_bid_yen: Some(600.0),
        }
    }

    fn detail_with(vols: Vec<SearchVolume>) -> TitleDetail {
        TitleDetail {
            title: "施工管理技術者".to_string(),
            volumes: vols,
            ..Default::default()
        }
    }

    /// 検索数が少ない行の前年比は出さない。
    ///
    /// 施工管理技術者は月 7 回の検索で「前年比 -44.4%」と出ていた。
    /// 48 か月の値が 0 か 10 しかなく、丸めのきざみを見ているだけ。
    /// 実測でも、月 50 回未満は前年比の絶対値が中央 23.5%、
    /// 月 300 回以上では 9.7% と、データが薄いほど大きく振れる。
    #[test]
    fn 検索数が少ない行の前年比は出さない() {
        let h = volume_block(&detail_with(vec![v(7, -44.4)]));
        assert!(!h.contains("44.4"), "きざみのぶれを前年比として出している");
        assert!(h.contains("月平均が 100 回に満たない行は前年比を出していません"));
    }

    /// 十分な検索数がある行は今までどおり出す。
    #[test]
    fn 検索数が足りていれば前年比を出す() {
        let h = volume_block(&detail_with(vec![v(1300, 9.4)]));
        assert!(h.contains("9.4"), "出せるはずの前年比が消えている");
    }

    /// しきい値ちょうどは出す側に入れる。
    #[test]
    fn しきい値ちょうどは出す() {
        let h = volume_block(&detail_with(vec![v(100, 12.0)]));
        assert!(h.contains("12.0"));
    }
}

#[cfg(test)]
mod keyword_section_tests {
    use super::*;
    use crate::indeed::keywords::{AttrMonth, TermShift};

    fn shift(term: &str, before: f64, after: f64) -> TermShift {
        TermShift {
            term: term.to_string(),
            before_pct: Some(before),
            after_pct: Some(after),
            diff_pt: Some(after - before),
            clicks_after: Some(1000.0),
        }
    }

    fn series(term: &str, pct: &[f64]) -> crate::indeed::keywords::TermSeries {
        let v: Vec<Option<f64>> = pct.iter().map(|x| Some(*x)).collect();
        crate::indeed::keywords::TermSeries {
            term: term.to_string(),
            diff_pt: Some(pct[pct.len() - 1] - pct[0]),
            latest: Some(pct[pct.len() - 1]),
            pct: v,
        }
    }

    /// 2 期間に丸めず、月ごとに出す。
    ///
    /// 前 3 か月と直近 3 か月の平均どうしでは 2 点しか無く、
    /// いつから動いたのか・まだ続いているのかが読めない。
    /// 実データの「事務」は 45.3 から 26.8 まで 2025-09 以降ずっと下がっているが、
    /// 2 点に丸めると「46.1 → 29.8」としか見えない。
    #[test]
    fn 検索語は月ごとに出す() {
        let months: Vec<String> = (7..=12)
            .map(|m| format!("2025-{m:02}"))
            .collect();
        let v = vec![
            series("事務", &[45.3, 46.4, 46.3, 44.8, 41.4, 40.8]),
            series("一般事務", &[5.3, 4.9, 5.0, 7.4, 10.1, 7.7]),
            series("正社員", &[13.0, 12.6, 12.1, 11.1, 11.4, 13.4]),
        ];
        let h = term_series_section(&months, &v);
        // 月がすべて図に渡っていること
        for m in &months {
            assert!(h.contains(m.as_str()), "{m} が図に無い");
        }
        // 各語の全月ぶんの値が入っていること（2 点に丸めていない）
        assert!(h.contains("46.400") && h.contains("40.800"), "途中の月が落ちている");
        assert!(h.contains("いちばん増えたのは「一般事務」"));
        assert!(h.contains("いちばん減ったのは「事務」"));
        // 2 期間の比較だと書かない
        assert!(!h.contains("前の 3 か月"));
        // 月が少なすぎる・語が少なすぎるときは出さない
        assert_eq!(term_series_section(&months[..3], &v), "");
        assert_eq!(term_series_section(&months, &v[..1]), "");
    }

    fn month(m: &str, senior: f64, cond: f64, tc: f64) -> AttrMonth {
        let mut pct = [Some(0.0); 8];
        pct[0] = Some(cond);
        pct[1] = Some(senior);
        AttrMonth {
            month: m.to_string(),
            pct,
            term_count: Some(tc),
        }
    }

    /// 属性も 2 期間に丸めず、月ごとに出す。
    ///
    /// 研磨作業のシニアは端点だと 19.5% → 32.0% だが、途中は 14.0〜23.5 を
    /// 行き来している。3 か月ならしても「17.8 → 27.0」の 2 点にしかならず、
    /// じわじわ動いたのか、ある月から急に変わったのかが分からない。
    #[test]
    fn 属性は月ごとに出す() {
        let v: Vec<AttrMonth> = [19.5, 17.0, 15.0, 20.7, 23.5, 26.1, 32.0]
            .iter()
            .enumerate()
            .map(|(i, s)| month(&format!("2025-{:02}", i + 7), *s, 30.0, 80.0))
            .collect();
        let h = attr_section(&v);
        for i in 0..7 {
            assert!(h.contains(&format!("2025-{:02}", i + 7)), "{i} 月目が図に無い");
        }
        // 途中の月の値が落ちていないこと
        assert!(h.contains("15.000") && h.contains("26.100"), "途中の月が落ちている");
        // 2 期間の言い方をしない
        assert!(!h.contains("最初の 3 か月"));
        assert!(h.contains("1 か月だけの上下で判断しないでください"));
    }

    /// 全期間 0% の区分は線を出さず、名前だけ挙げる。
    ///
    /// 線が軸に張り付くだけで何も言えない。黙って落とすと
    /// 「なぜ 2 本しかないのか」が分からないので、名前は出す。
    #[test]
    fn 全期間ゼロの区分は名前だけ挙げる() {
        let v: Vec<AttrMonth> = (0..7)
            .map(|i| month(&format!("2025-{:02}", i + 7), 5.0, 30.0, 80.0))
            .collect();
        let h = attr_section(&v);
        assert!(h.contains("全期間 0% なので出していません"));
        assert!(h.contains("学生"), "外した区分の名前が出ていない");
        let cfg = h
            .split("data-chart-config='")
            .nth(1)
            .and_then(|x| x.split('\'').next())
            .unwrap_or("");
        assert!(!cfg.contains("学生"), "0% の区分が図に入っている");
        assert!(cfg.contains("シニア"), "値のある区分が図に無い");
    }

    /// 母数を出して、1 か月の上下で判断しないよう書く。
    #[test]
    fn 母数と読み方の注意を出す() {
        let v: Vec<AttrMonth> = (0..7)
            .map(|i| month(&format!("2025-{:02}", i + 7), 5.0, 30.0, 42.0))
            .collect();
        let h = attr_section(&v);
        assert!(h.contains("中央 42"), "母数が出ていない");
        assert!(h.contains("12.2 ポイント") && h.contains("3.1 ポイント"), "振れ幅の実測が出ていない");
    }
}
