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
    let shifts = crate::indeed::keywords::term_shifts(db, name).unwrap_or_else(|e| {
        tracing::warn!("検索語の入れ替わりを読めませんでした: {e}");
        Vec::new()
    });
    let attrs = crate::indeed::keywords::attr_months(db, name).unwrap_or_else(|e| {
        tracing::warn!("属性の内訳を読めませんでした: {e}");
        Vec::new()
    });
    Html(render(
        &d,
        overview.as_ref(),
        &snap.meta.months,
        &wages,
        &shifts,
        &attrs,
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
    shifts: &[crate::indeed::keywords::TermShift],
    attrs: &[crate::indeed::keywords::AttrMonth],
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
    h.push_str(&term_shift_section(shifts));
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

/// 探し方の言葉が入れ替わった分。
///
/// # なぜ「傾向」と書かないのか
/// 前 3 か月と直近 3 か月の**比較**であって、毎月そう動いているわけではない。
/// 生成側（scripts/indeed_build_insights.js）が 2 期間の平均で作っている。
/// 「増えている」と書くと、来月も増えると読まれる。
///
/// # 品質の確認
/// 季節性で踏んだ「データが薄いほど大きく出る」歪みが無いか、
/// クリック数の帯で層別して確かめた。増減幅の中央値は
/// 〜500 で 1.7pt、500〜5000 で 1.6pt、5000〜5万 で 1.7pt、5万〜 で 1.6pt と
/// ほぼ一定で、薄いデータほど大きく出る傾向は無かった。そのまま出してよい。
fn term_shift_section(shifts: &[crate::indeed::keywords::TermShift]) -> String {
    if shifts.len() < 4 {
        return String::new();
    }
    let rows: Vec<(String, Option<f64>)> = shifts
        .iter()
        .map(|s| (s.term.clone(), s.diff_pt))
        .collect();
    let up = shifts.iter().find(|s| s.diff_pt.unwrap_or(0.0) > 0.0);
    let down = shifts.iter().rev().find(|s| s.diff_pt.unwrap_or(0.0) < 0.0);
    let say = |t: Option<&crate::indeed::keywords::TermShift>| match t {
        Some(x) => format!(
            "「{}」（{} → {}）",
            esc(&x.term),
            pct1(x.before_pct),
            pct1(x.after_pct)
        ),
        None => "—".to_string(),
    };
    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         探し方の言葉が入れ替わった分</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         この職種を探す人が使った言葉の内訳が、<strong>前の 3 か月と直近 3 か月で\
         どれだけ入れ替わったか</strong>です。右が増えた語、左が減った語。\
         単位はポイント（シェアの差）です。\
         2 期間の比較なので、毎月そう動いているという意味ではありません。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         いちばん増えたのは{u}、いちばん減ったのは{d}です。\
         求人票の職種名を、増えている側の言葉に寄せると見つけてもらいやすくなります。</p></div>",
        chart = tornado_chart(&rows, "ポイント", true, 140 + (rows.len() as u32) * 22),
        u = say(up),
        d = say(down)
    )
}

/// 探している人の内訳が、期間の頭と終わりでどう変わったか。
///
/// # なぜ端の 1 か月どうしを比べないのか
/// 月ごとの割合はよく振れる。実測では母数（語数）が 20 未満の職種で
/// 振れ幅が中央 12.2pt、60 以上で 3.1pt と、薄いほど大きく振れる。
/// 端の月を 1 つずつ取ると、たまたま高い月と低い月を選んだだけで
/// 大きな変化に見える。頭と終わりを 3 か月ずつならして比べる。
///
/// # なぜ水準ではなく変化の図にするのか
/// はじめは「頭の平均 → 終わりの平均」を帯（ダンベル）で出したが、読めなかった。
/// 水準が 24% の区分と 5% の区分が同じ軸に乗るため、24% 側の 0.2pt の動きが
/// 線 1 本に潰れる。さらに 8 区分のうち 5 つは両方の期間とも 0% で、
/// 空の行が並んで壊れているように見えた。
/// 知りたいのは「何が動いたか」なので、動いた量そのものを軸に置く。
/// 今の水準はラベルに添える。両方 0% の区分は図から外し、名前を文章で挙げる。
fn attr_section(months: &[crate::indeed::keywords::AttrMonth]) -> String {
    use crate::indeed::keywords::{attr_change, ATTR_LABELS};
    let Some(ch) = attr_change(months) else {
        return String::new();
    };
    let delta = |i: usize| match (ch.before[i], ch.after[i]) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };
    // 両方の期間で 0% の区分は図に出さない（空の行が並ぶだけで何も言えない）
    let live: Vec<usize> = (0..8)
        .filter(|i| {
            ch.before[*i].unwrap_or(0.0) > 0.0 || ch.after[*i].unwrap_or(0.0) > 0.0
        })
        .collect();
    let dead: Vec<&str> = (0..8)
        .filter(|i| !live.contains(i))
        .map(|i| ATTR_LABELS[i])
        .collect();
    if live.is_empty() {
        return String::new();
    }
    let mut order = live.clone();
    order.sort_by(|a, b| {
        delta(*b)
            .unwrap_or(0.0)
            .total_cmp(&delta(*a).unwrap_or(0.0))
    });
    // ラベルに今の水準を添える。図は「動いた量」、ラベルは「今どのくらいか」
    let rows: Vec<(String, Option<f64>)> = order
        .iter()
        .map(|i| {
            (
                format!("{}（{}）", ATTR_LABELS[*i], pct1(ch.after[*i])),
                delta(*i),
            )
        })
        .collect();
    let top = order
        .iter()
        .copied()
        .max_by(|a, b| {
            delta(*a)
                .map(f64::abs)
                .unwrap_or(-1.0)
                .total_cmp(&delta(*b).map(f64::abs).unwrap_or(-1.0))
        })
        .unwrap_or(order[0]);
    // どのくらいの動きなら「目立つ」と言ってよいか。
    // 全 104 職種 × 8 区分の 832 件で、3 か月平均どうしの変化幅を測った。
    //     中央 0.02pt / 75% 0.70pt / 90% 3.15pt / 95% 5.51pt
    // 母数の帯ごとの 90 パーセンタイルは 〜20 で 7.82pt、20 以上では 2.1〜3.3pt。
    // 上位 1 割に入るかどうかを線にする。43 職種がこれを超える。
    let bar = match ch.term_count_median {
        Some(t) if t < 20.0 => 8.0,
        _ => 3.0,
    };
    let biggest = delta(top).map(f64::abs).unwrap_or(0.0);
    // 「誤差」とは書かない。誤差の推定はしていない。
    // 測ったのは「他の職種と比べて大きいか」なので、そう書く
    let lead = if biggest < bar {
        format!(
            "<strong>目立つ動きはありませんでした。</strong>\
             いちばん大きい「{tl}」でも {mv:.1} ポイントで、\
             全職種の変化の 9 割はこれより小さい範囲に収まります。",
            tl = esc(ATTR_LABELS[top]),
            mv = biggest
        )
    } else {
        format!(
            "いちばん動いたのは「{tl}」で、{b} から {a} になりました\
             （{mv:.1} ポイント）。これは全職種の変化のうち<strong>上位 1 割</strong>に入る大きさです。",
            tl = esc(ATTR_LABELS[top]),
            b = pct1(ch.before[top]),
            a = pct1(ch.after[top]),
            mv = biggest
        )
    };
    format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">\
         探している人の内訳の変化</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         検索語に出てくる言葉を 8 つに区分し、<strong>最初の {w} か月の平均と\
         直近 {w} か月の平均で、どれだけ動いたか</strong>を出しています。\
         単位はポイント。右が増えた区分、左が減った区分です。\
         かっこの中は直近 {w} か月の割合（今どのくらいか）です。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         {lead}{dead}</p>\
         <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">\
         この職種の母数（月あたりの語数）は中央 {tc} です。\
         「上位 1 割」の線は {bar:.0} ポイントに置いています\
         （全 104 職種 × 8 区分 832 件の変化幅を並べたときの 90 パーセンタイルが 3.2 ポイント、\
         母数 20 未満の職種に限ると 7.8 ポイントでした）。</p></div>",
        w = ch.window,
        chart = tornado_chart(&rows, "ポイント", true, 140 + (rows.len() as u32) * 26),
        lead = lead,
        dead = if dead.is_empty() {
            String::new()
        } else {
            format!(
                "{}は、どちらの期間も 0% だったので図から外しています。",
                dead.iter().map(|s| esc(s)).collect::<Vec<_>>().join("・")
            )
        },
        tc = ch
            .term_count_median
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "—".to_string()),
        bar = bar
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

    /// 「増えている」と書かない。2 期間の比較であって傾向ではない。
    ///
    /// 生成側は前 3 か月と直近 3 か月の平均を比べている。
    /// 「増えている」と書くと来月も増えると読まれる。
    #[test]
    fn 入れ替わりを傾向として書かない() {
        let v = vec![
            shift("一般事務", 5.1, 18.4),
            shift("ハローワーク", 0.2, 2.9),
            shift("土日祝休み", 6.9, 3.5),
            shift("事務", 46.1, 29.8),
        ];
        let h = term_shift_section(&v);
        assert!(h.contains("前の 3 か月と直近 3 か月"), "期間比較だと書いていない");
        assert!(h.contains("毎月そう動いているという意味ではありません"));
        assert!(h.contains("いちばん増えたのは「一般事務」"));
        assert!(h.contains("いちばん減ったのは「事務」"));
        // 語が少なすぎるときは出さない
        assert_eq!(term_shift_section(&v[..3]), "");
    }

    fn month(m: &str, senior: f64, cond: f64, tc: f64) -> AttrMonth {
        let mut pct = [None; 8];
        pct[0] = Some(cond);
        pct[1] = Some(senior);
        AttrMonth {
            month: m.to_string(),
            pct,
            term_count: Some(tc),
        }
    }

    /// 端の 1 か月どうしではなく、3 か月ずつならして比べる。
    ///
    /// 研磨作業のシニアは端点だと 19.5% → 32.0% だが、途中は 14.0〜23.5 を
    /// 行き来している。3 か月ならすと 17.8% → 27.0%。
    #[test]
    fn 属性は三か月ならして比べる() {
        // 頭 3 か月 20/18/16 → 平均 18.0、終わり 3 か月 26/32/23 → 平均 27.0
        let v = vec![
            month("2025-07", 20.0, 30.0, 50.0),
            month("2025-08", 18.0, 30.0, 50.0),
            month("2025-09", 16.0, 30.0, 50.0),
            month("2025-10", 15.0, 30.0, 50.0),
            month("2025-11", 26.0, 30.0, 50.0),
            month("2025-12", 32.0, 30.0, 50.0),
            month("2026-01", 23.0, 30.0, 50.0),
        ];
        let h = attr_section(&v);
        assert!(h.contains("18.0%"), "頭 3 か月の平均が出ていない");
        assert!(h.contains("27.0%"), "終わり 3 か月の平均が出ていない");
        // 端点の 20.0 → 23.0 で語っていないこと
        assert!(!h.contains("いちばん動いたのは「シニア」で、20.0% から 23.0%"));
    }

    /// 両方の期間で 0% の区分は図に出さず、名前を文章で挙げる。
    ///
    /// はじめは 8 区分すべてを帯で出していたが、配送ドライバーでは 5 つが
    /// 両方 0% で、空の行が並んで壊れているように見えた。
    /// 黙って落とすと「なぜ 3 つしか無いのか」が分からないので、名前は出す。
    #[test]
    fn 両方ゼロの区分は図から外して名前を挙げる() {
        // 条件と シニア だけ値があり、残り 6 区分は 0%
        let mk = |m: &str, cond: f64, senior: f64| {
            let mut pct = [Some(0.0); 8];
            pct[0] = Some(cond);
            pct[1] = Some(senior);
            AttrMonth {
                month: m.to_string(),
                pct,
                term_count: Some(80.0),
            }
        };
        let v: Vec<AttrMonth> = (0..7)
            .map(|i| mk(&format!("2025-{:02}", i + 7), 24.0, if i < 3 { 6.0 } else { 3.0 }))
            .collect();
        let h = attr_section(&v);
        assert!(
            h.contains("どちらの期間も 0% だったので図から外しています"),
            "外した区分の説明が無い"
        );
        assert!(h.contains("学生"), "外した区分の名前が出ていない");
        // 図に渡るのは生きている 2 区分だけ
        let cfg = h
            .split("data-chart-config='")
            .nth(1)
            .and_then(|x| x.split('\'').next())
            .unwrap_or("");
        assert!(!cfg.contains("学生"), "0% の区分が図に入っている");
        assert!(cfg.contains("シニア"), "動いた区分が図に入っていない");
    }

    /// 誤差に埋もれる動きを「動いた」と語らない。
    ///
    /// 配送ドライバー（母数 24）はいちばん大きい動きが 2.8 ポイントで、
    /// この母数だと 5 ポイントまでは月ごとの振れで説明がつく。
    /// それを「シニアが 5.6% から 2.8% になりました」と書くと、
    /// 他の職種と比べて小さい動きを「動いた」と語らない。
    ///
    /// 全 104 職種 × 8 区分 832 件の変化幅を測ると、90 パーセンタイルは 3.2 ポイント。
    /// 母数 20 未満に限ると 7.8 ポイント。この線を下回る動きを断定して書くと、
    /// 図の全部のバーが「よくある大きさ」なのに発見のように読まれる。
    #[test]
    fn 上位一割に入らない動きは断定して書かない() {
        let mk = |m: &str, senior: f64, tc: f64| {
            let mut pct = [Some(0.0); 8];
            pct[0] = Some(24.0);
            pct[1] = Some(senior);
            AttrMonth { month: m.to_string(), pct, term_count: Some(tc) }
        };
        // 母数 60 → 線は 3pt。動きは 5.6 → 2.8 の 2.8pt で下回る
        let small: Vec<AttrMonth> = (0..7)
            .map(|i| mk(&format!("2025-{:02}", i + 7), if i < 3 { 5.6 } else { 2.8 }, 60.0))
            .collect();
        let h = attr_section(&small);
        assert!(h.contains("目立つ動きはありませんでした"), "小さい動きを断定して書いている");
        assert!(h.contains("2.8 ポイント"), "実際の動きの大きさが出ていない");
        assert!(!h.contains("誤差"), "測っていない「誤差」という言葉を使っている");

        // はっきり超えるときは書く
        let big: Vec<AttrMonth> = (0..7)
            .map(|i| mk(&format!("2025-{:02}", i + 7), if i < 3 { 20.0 } else { 5.0 }, 60.0))
            .collect();
        let hb = attr_section(&big);
        assert!(hb.contains("いちばん動いたのは"), "超えているのに書いていない");
        assert!(hb.contains("上位 1 割"), "何と比べて大きいのかが書かれていない");
        assert!(!hb.contains("目立つ動きはありませんでした"));
    }

    /// 母数の帯で線の高さを変える。
    ///
    /// 母数が薄い職種ほど月ごとの割合が振れる。実測で、母数 20 未満の帯の
    /// 90 パーセンタイルは 7.8 ポイント、20 以上では 2.1〜3.3 ポイントだった。
    #[test]
    fn 母数の帯で線の高さを変える() {
        let mk = |m: &str, tc: f64| {
            let mut pct = [Some(0.0); 8];
            pct[0] = Some(30.0);
            pct[1] = Some(20.0);
            AttrMonth { month: m.to_string(), pct, term_count: Some(tc) }
        };
        let thin: Vec<AttrMonth> = (0..7).map(|i| mk(&format!("2025-{:02}", i + 7), 12.0)).collect();
        let thick: Vec<AttrMonth> = (0..7).map(|i| mk(&format!("2025-{:02}", i + 7), 120.0)).collect();
        assert!(attr_section(&thin).contains("8 ポイントに置いています"));
        assert!(attr_section(&thick).contains("3 ポイントに置いています"));
    }
}
