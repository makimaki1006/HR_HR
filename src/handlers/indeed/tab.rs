//! 社内タブ `/tab/indeed`。
//!
//! 顧客レポートとの違いは「どこまで見せるか」だけで、数字は同じものを使う。
//! ここでは分解（なぜそうなったか）と、言い切れない部分まで出す。

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::Html,
};
use serde::Deserialize;

use super::render::{
    arrow, category_table_html, dec1_opt, dir_class, esc, indexed_chart, line_chart, metric_card,
    num_opt, pct_opt, url_query,
};
use crate::indeed::aggregate::{
    category_table, nation_overview, pref_overview, pref_title_overviews, Overview,
};
use crate::indeed::data::{snapshot, Snapshot};
use crate::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct TabQuery {
    /// 都道府県で絞る。空なら全国
    pub pref: Option<String>,
    /// 一覧の並べ替え。指定が無ければ求人数の多い順
    pub sort: Option<String>,
}

/// 直接 URL を叩かれたときにトップへ戻す決まり文句。
/// HTMX で差し込まれた場合は nav があるので何もしない。
const DIRECT_ACCESS_GUARD: &str = r#"<script>
(function(){
  if (!document.querySelector('nav')) {
    var target = location.pathname + location.search;
    location.replace('/?tab=' + encodeURIComponent(target));
  }
})();
</script>"#;

pub async fn tab_indeed(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TabQuery>,
) -> Html<String> {
    let Some(db) = state.indeed_db.as_ref() else {
        return Html(degraded(
            "Indeed 分析データ (data/indeed_insights.db) が積まれていません。",
        ));
    };
    let snap = match snapshot(db) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("indeed snapshot failed: {e}");
            return Html(degraded("Indeed 分析データを読めませんでした。"));
        }
    };
    Html(render_tab(snap, q.pref.as_deref(), q.sort.as_deref()))
}

fn degraded(msg: &str) -> String {
    format!(
        "{DIRECT_ACCESS_GUARD}<div class=\"p-6\"><div class=\"bg-navy-800/60 border border-amber-600/40 rounded-lg p-4 text-amber-200\">{}</div></div>",
        esc(msg)
    )
}

fn render_tab(snap: &Snapshot, pref: Option<&str>, sort: Option<&str>) -> String {
    let months = &snap.meta.months;
    let prefs = snap.prefectures();
    let pref = pref.filter(|p| !p.is_empty() && prefs.iter().any(|x| x == p));

    // 全国か、選ばれた 1 県か。集計は同じ関数を通す
    // 集計は aggregate に一本化する。画面ごとに合算を書くと数字がずれる
    let overview = match pref {
        None => nation_overview(snap),
        Some(p) => pref_overview(snap, p),
    };

    let mut h = String::with_capacity(160_000);
    h.push_str(DIRECT_ACCESS_GUARD);
    h.push_str("<div class=\"space-y-6\">");

    // 見出しと出どころ
    h.push_str(&format!(
        "<div class=\"flex flex-wrap items-end justify-between gap-3\">\
         <div><h2 class=\"text-xl font-bold text-gray-100\">Indeed 採用市場（社内用）</h2>\
         <p class=\"text-slate-400 text-sm mt-1\">{period}／{n} 職種・{np} 都道府県・{src}</p>\
         {sample}</div>\
         {selector}</div>",
        period = esc(&format!(
            "{} 〜 {}",
            months.first().map(String::as_str).unwrap_or("—"),
            snap.meta.latest
        )),
        n = snap.titles.len(),
        // 合計に入っている職種の数を必ず書く。母集団が月で変わると比べられない
        sample = {
            let nc = snap.complete_titles();
            let part = snap.titles.len() - nc;
            if part == 0 {
                String::new()
            } else {
                format!(
                    "<p class=\"text-slate-500 text-xs mt-1 leading-relaxed\">\
                     合計は、全期間そろっている {nc} 職種で出しています。\
                     残り {part} 職種は月が欠けているため、下の一覧には出しますが合計には入れていません\
                     （母集団が月によって変わると、先月比が実態と関係なく動くためです）。</p>"
                )
            }
        },
        np = prefs.len(),
        src = esc(&snap.meta.source),
        selector = format!(
            "{}<span id=\"indeed-loading\" class=\"htmx-indicator text-slate-400 text-sm ml-2\">読み込み中…</span>",
            pref_selector(&prefs, pref)
        )
    ));

    // 但し書きは最初に出す。後ろに置くと読まれない
    h.push_str(&format!(
        "<div class=\"bg-navy-800/60 border-l-4 border-sky-500 rounded p-3 text-slate-300 text-sm leading-relaxed\">{}</div>",
        esc(&snap.meta.caveat)
    ));

    // 見出しの 5 指標
    h.push_str("<div class=\"grid grid-cols-2 lg:grid-cols-5 gap-3\">");
    for m in [
        &overview.job,
        &overview.ctk,
        &overview.emp,
        &overview.spp,
        &overview.ppe,
    ] {
        h.push_str(&metric_card(m, true));
    }
    h.push_str("</div>");

    // 「なぜ」の分解。推測ではなく割り算で答える
    h.push_str(&format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <h3 class=\"text-slate-100 font-bold mb-2\">なぜそうなったか（割り算での分解）</h3>\
         <p class=\"text-slate-300 text-sm leading-relaxed\">{}</p>\
         <p class=\"text-slate-400 text-xs mt-3 leading-relaxed\">\
         1 求人あたり = 求人を見た人数 ÷ 求人の数。求人の数 = 募集した企業の数 × 1 社あたりの本数。\
         この 2 段の割り算で説明が尽きるので、これ以上の理由づけは推測になります。</p></div>",
        esc(&overview.why())
    ));

    // 全体の動き
    h.push_str(&format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <h3 class=\"text-slate-100 font-bold mb-1\">{name}の動き</h3>\
         <p class=\"text-slate-400 text-xs mb-2\">最初の月を 100 とした指数。伸びの違いを同じ物差しで見るためです。</p>\
         {chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">{s1}</p>\
         <p class=\"text-slate-300 text-sm mt-1 leading-relaxed\">{s2}</p></div>",
        name = esc(&overview.name),
        chart = line_chart(
            months,
            &[
                ("求人の数".to_string(), overview.job.indexed()),
                ("求人を見た人数".to_string(), overview.ctk.indexed()),
                ("募集している企業の数".to_string(), overview.emp.indexed()),
            ],
            true,
            300
        ),
        s1 = esc(&overview.job.sentence),
        s2 = esc(&overview.spp.sentence)
    ));

    // 業界（全国のみ。県で絞ると 1 業界あたりの月次が薄くなる）
    if pref.is_none() {
        h.push_str(&industry_section(snap, months));
    }

    // 分類（全国のみ。県で絞ると分類別の月次が薄くなる）
    if pref.is_none() {
        let cats = category_table(snap);
        let top: Vec<(String, Overview)> = cats
            .iter()
            .take(7)
            .filter_map(|c| {
                snap.by_category
                    .get(&c.name)
                    .map(|s| (c.name.clone(), Overview::from_series(&c.name, s, months)))
            })
            .collect();
        let refs: Vec<(String, &Overview)> = top.iter().map(|(n, o)| (n.clone(), o)).collect();
        h.push_str(&format!(
            "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
             <h3 class=\"text-slate-100 font-bold mb-1\">分類ごとの求人数（上位 7、指数）</h3>\
             <p class=\"text-slate-400 text-xs mb-2\">同じ物差しで重ねています。分類ごとに軸を変えると、どれも同じ形に見えてしまいます。</p>\
             {chart}</div>",
            chart = indexed_chart(months, &refs, true, 320)
        ));
        h.push_str(&format!(
            "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
             <h3 class=\"text-slate-100 font-bold mb-2\">分類別の定点表（{} 分類）</h3>{}</div>",
            cats.len(),
            category_table_html(&cats, true)
        ));
    }

    // 職種の一覧
    h.push_str(&title_section(snap, pref, sort));

    h.push_str("</div>");
    h
}

fn pref_selector(prefs: &[String], current: Option<&str>) -> String {
    let mut s = String::from(
        "<select class=\"bg-navy-800 border border-slate-600 text-slate-100 rounded px-3 py-2 text-sm\" \
         hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML\" name=\"pref\" hx-trigger=\"change\" hx-include=\"[name='sort']\" \n         hx-push-url=\"true\" hx-indicator=\"#indeed-loading\" aria-label=\"都道府県\">",
    );
    s.push_str(&format!(
        "<option value=\"\"{}>全国</option>",
        if current.is_none() { " selected" } else { "" }
    ));
    for p in prefs {
        s.push_str(&format!(
            "<option value=\"{v}\"{sel}>{v}</option>",
            v = esc(p),
            sel = if current == Some(p.as_str()) {
                " selected"
            } else {
                ""
            }
        ));
    }
    s.push_str("</select>");
    s
}

/// 一覧の並べ替え。
///
/// 104 行を全部出したうえで、並び順そのもので「何を見たいのか」を示す。
/// 上位だけを抜き出すと、別の意図で見たいときに何も残らない。
struct SortSpec {
    key: &'static str,
    label: &'static str,
    /// この並びで何が見えるのかを、表の上に出す
    note: &'static str,
}

const SORTS: [SortSpec; 7] = [
    SortSpec {
        key: "size",
        label: "求人数が多い順",
        note: "案件量の大きい職種から並べています。まずここで市場の規模感を見ます。",
    },
    SortSpec {
        key: "grow",
        label: "求人が増えた順",
        note: "この期間で募集が増えた職種です。採用の競争が強まっている側から並びます。",
    },
    SortSpec {
        key: "shrink",
        label: "求人が減った順",
        note: "この期間で募集が減った職種です。撤退や採り控えが起きている可能性を見る並びです。",
    },
    SortSpec {
        key: "hard",
        label: "1求人あたりが少ない順",
        note: "1 件の求人を見た人が少ない職種です。同じ求人票でも人が集まりにくい側から並びます。",
    },
    SortSpec {
        key: "worse",
        label: "1求人あたりが減った順",
        note: "この期間で「集まりにくく」なった度合いが大きい職種です。去年と同じやり方が通じにくくなっている側です。",
    },
    SortSpec {
        key: "better",
        label: "1求人あたりが増えた順",
        note: "この期間で「集まりやすく」なった職種です。競合が引いた可能性を見る並びです。",
    },
    SortSpec {
        key: "steady",
        label: "動きが一本調子なものを上に",
        note: "月ごとの振れが小さく、傾向として読んでよい職種を上にしています。下にいくほど振れが大きく、1 か月の増減で判断してはいけません。",
    },
];

fn sort_spec(key: Option<&str>) -> &'static SortSpec {
    let k = key.unwrap_or("size");
    SORTS.iter().find(|s| s.key == k).unwrap_or(&SORTS[0])
}

fn sort_selector(current: &SortSpec) -> String {
    let mut s = String::from(
        "<select class=\"bg-navy-800 border border-slate-600 text-slate-100 rounded px-3 py-2 text-sm\" \
         hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML\" name=\"sort\" hx-trigger=\"change\" \
         hx-include=\"[name='pref']\" \n         hx-push-url=\"true\" hx-indicator=\"#indeed-loading\" aria-label=\"並べ替え\">",
    );
    for o in SORTS.iter() {
        s.push_str(&format!(
            "<option value=\"{k}\"{sel}>{l}</option>",
            k = o.key,
            l = esc(o.label),
            sel = if o.key == current.key { " selected" } else { "" }
        ));
    }
    s.push_str("</select>");
    // 都道府県は画面上のセレクト（name="pref"）を hx-include で拾う。
    // ここに隠しフィールドを足すと同じ名前が 2 つになり、二重に送られる。
    s
}

/// 職種の一覧。全国なら全職種、県を選んでいればその県の職種。
fn title_section(snap: &Snapshot, pref: Option<&str>, sort: Option<&str>) -> String {
    let months = &snap.meta.months;
    let spec = sort_spec(sort);

    // 全国と県で、行の作り方だけを変える。以降の並べ替えと描画は共通
    let mut rows: Vec<(String, String, Overview)> = match pref {
        None => snap
            .titles
            .iter()
            .filter_map(|t| {
                snap.by_title.get(&t.name).map(|s| {
                    (
                        t.name.clone(),
                        t.category.clone(),
                        Overview::from_series(&t.name, s, months),
                    )
                })
            })
            .collect(),
        // 県の絞り込みは集計側の 1 関数に閉じ込める。画面ごとに書くと
        // 片方だけ直す事故になる（このプロジェクトの過去の dedup 事故と同型）
        Some(p) => pref_title_overviews(snap, p),
    };

    // 欠測は必ず最後に置く。0 として並べると「いちばん減った」の先頭に来てしまう
    fn key_desc(v: Option<f64>) -> f64 {
        v.unwrap_or(f64::NEG_INFINITY)
    }
    fn key_asc(v: Option<f64>) -> f64 {
        v.unwrap_or(f64::INFINITY)
    }
    match spec.key {
        "grow" => rows.sort_by(|a, b| {
            key_desc(b.2.job.change_pct).total_cmp(&key_desc(a.2.job.change_pct))
        }),
        "shrink" => rows.sort_by(|a, b| {
            key_asc(a.2.job.change_pct).total_cmp(&key_asc(b.2.job.change_pct))
        }),
        "hard" => rows.sort_by(|a, b| key_asc(a.2.spp.latest).total_cmp(&key_asc(b.2.spp.latest))),
        "worse" => rows.sort_by(|a, b| {
            key_asc(a.2.spp.change_pct).total_cmp(&key_asc(b.2.spp.change_pct))
        }),
        "better" => rows.sort_by(|a, b| {
            key_desc(b.2.spp.change_pct).total_cmp(&key_desc(a.2.spp.change_pct))
        }),
        "steady" => rows.sort_by(|a, b| {
            let sa = a.2.job.fit.as_ref().map(|f| f.steady).unwrap_or(false);
            let sb = b.2.job.fit.as_ref().map(|f| f.steady).unwrap_or(false);
            sb.cmp(&sa)
                .then_with(|| key_desc(b.2.job.latest).total_cmp(&key_desc(a.2.job.latest)))
        }),
        _ => rows.sort_by(|a, b| key_desc(b.2.job.latest).total_cmp(&key_desc(a.2.job.latest))),
    }

    let mut h = String::with_capacity(80_000);
    h.push_str(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <div class=\"flex flex-wrap items-center justify-between gap-3 mb-1\">\
         <h3 class=\"text-slate-100 font-bold\">職種の一覧</h3>",
    );
    h.push_str(&sort_selector(spec));
    h.push_str("</div>");
    h.push_str(&format!(
        "<p class=\"text-slate-400 text-xs mb-3 leading-relaxed\">{}　\
         全 {n} 職種を出しています。抜き出さずに、並び順で見たいことを示しています。</p>",
        esc(spec.note),
        n = rows.len()
    ));

    h.push_str(
        "<div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\" style=\"min-width:760px\"><thead><tr>",
    );
    for (name, align) in [
        ("職種", "left"),
        ("分類", "left"),
        ("求人数（最新月）", "right"),
        ("求人数の変化", "right"),
        ("1求人あたり", "right"),
        ("その変化", "right"),
        ("動き方", "left"),
    ] {
        h.push_str(&format!(
            "<th scope=\"col\" class=\"text-slate-400 font-medium px-3 py-2 border-b border-slate-700\" style=\"text-align:{align}\">{name}</th>"
        ));
    }
    h.push_str("</tr></thead><tbody>");

    let td = "px-3 py-2 border-b border-slate-800 text-slate-200";
    for (name, cat, o) in &rows {
        h.push_str(&format!(
            "<tr><th scope=\"row\" class=\"{td} font-normal\" style=\"text-align:left\">\
             <a class=\"text-sky-400 hover:underline\" href=\"/tab/indeed/title?name={q}\" \
                hx-get=\"/tab/indeed/title?name={q}\" hx-target=\"#content\" hx-swap=\"innerHTML\" \
                hx-push-url=\"true\">{n}</a></th>\
             <td class=\"{td} text-slate-400\">{c}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{td} tabular-nums {jc}\" style=\"text-align:right\">{ja} {jp}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{s}</td>\
             <td class=\"{td} tabular-nums {sc}\" style=\"text-align:right\">{sa} {sp}</td>\
             <td class=\"{td} text-slate-300\">{t}</td></tr>",
            n = esc(name),
            q = url_query(name),
            c = esc(cat),
            j = num_opt(o.job.latest),
            jc = dir_class(o.job.change_pct, true),
            ja = arrow(o.job.change_pct),
            jp = pct_opt(o.job.change_pct),
            s = dec1_opt(o.spp.latest),
            sc = dir_class(o.spp.change_pct, true),
            sa = arrow(o.spp.change_pct),
            sp = pct_opt(o.spp.change_pct),
            t = esc(o.job.label_trend)
        ));
    }
    h.push_str("</tbody></table></div></div>");
    h
}

/// 5 業界のまとめ。
///
/// # なぜ 20 分類の前に置くか
/// 20 個並べても、見る人は自分が話す相手がどこにいるか探せない。
/// 先に 5 つで全体像を見てから、細かい分類に降りる。
///
/// # まとめ方は顧客に配る見本と同じ
/// [`crate::indeed::industry`] を使う。紙とアプリで違うまとめ方をすると、
/// 同じ会社の話が食い違う。5 つに入らないものは無理に入れず、別枠で数える。
fn industry_section(snap: &Snapshot, months: &[String]) -> String {
    use crate::indeed::aggregate::{industry_series, industry_table};
    use crate::indeed::industry;

    let rows = industry_table(snap);
    if rows.is_empty() {
        return String::new();
    }
    let series_by_industry = industry_series(snap);
    let td = "px-3 py-2 border-b border-slate-800 text-slate-200";
    let th = "text-slate-400 font-medium px-3 py-2 border-b border-slate-700";

    let mut h = String::with_capacity(30_000);
    h.push_str(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <h3 class=\"text-slate-100 font-bold mb-1\">業界ごとの動き（5 業界）</h3>\
         <p class=\"text-slate-400 text-xs mb-3 leading-relaxed\">\
         Indeed の 20 分類のうち、募集する会社が重なるものを 5 つにまとめています。\
         顧客に配る見本と同じまとめ方です。5 つに入らない職種は、無理に入れず別枠で数えています。\
         「先月比」と「前年同月比」は素の比で、「動き方」は月ごとの上下をならした線から出しています。</p>",
    );

    // --- 表 ---
    h.push_str(
        "<div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\" \
         style=\"min-width:860px\"><thead><tr>",
    );
    for (n, a) in [
        ("業界", "left"),
        ("職種数", "right"),
        ("求人数（最新月）", "right"),
        ("全体に占める割合", "right"),
        ("先月比", "right"),
        ("前年同月比", "right"),
        ("1求人あたり", "right"),
        ("動き方", "left"),
    ] {
        h.push_str(&format!(
            "<th scope=\"col\" class=\"{th}\" style=\"text-align:{a}\">{n}</th>"
        ));
    }
    h.push_str("</tr></thead><tbody>");
    for r in &rows {
        h.push_str(&format!(
            "<tr><th scope=\"row\" class=\"{td} font-normal\" style=\"text-align:left\">{n}</th>\
             <td class=\"{td}\" style=\"text-align:right\">{t}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{td} tabular-nums text-slate-400\" style=\"text-align:right\">{sh}</td>\
             <td class=\"{td} tabular-nums {mc}\" style=\"text-align:right\">{ma} {m}</td>\
             <td class=\"{td} tabular-nums {yc}\" style=\"text-align:right\">{ya} {y}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{s}</td>\
             <td class=\"{td} text-slate-300\">{tr}</td></tr>",
            n = esc(&r.name),
            t = r.titles,
            j = num_opt(r.job_latest),
            sh = match r.share_pct {
                Some(v) => format!("{v:.1}%"),
                None => "—".to_string(),
            },
            mc = dir_class(r.job_mom_pct, true),
            ma = arrow(r.job_mom_pct),
            m = pct_opt(r.job_mom_pct),
            yc = dir_class(r.job_yoy_pct, true),
            ya = arrow(r.job_yoy_pct),
            y = pct_opt(r.job_yoy_pct),
            s = dec1_opt(r.spp_latest),
            tr = esc(r.trend)
        ));
    }
    // 合計は業界を足し直さず、全国の集計をそのまま置く。
    // 足し直すと丸めの分だけ全体とずれる。
    let all = nation_overview(snap);
    h.push_str(&format!(
        "<tr class=\"border-t border-slate-600\">\
         <th scope=\"row\" class=\"{td} font-bold\" style=\"text-align:left\">全体</th>\
         <td class=\"{td}\" style=\"text-align:right\">{t}</td>\
         <td class=\"{td} tabular-nums font-bold\" style=\"text-align:right\">{j}</td>\
         <td class=\"{td} tabular-nums text-slate-400\" style=\"text-align:right\">100.0%</td>\
         <td class=\"{td} tabular-nums {mc}\" style=\"text-align:right\">{ma} {m}</td>\
         <td class=\"{td} tabular-nums {yc}\" style=\"text-align:right\">{ya} {y}</td>\
         <td class=\"{td} tabular-nums\" style=\"text-align:right\">{s}</td>\
         <td class=\"{td} text-slate-300\">{tr}</td></tr>",
        t = snap.titles.len(),
        j = num_opt(all.job.latest),
        mc = dir_class(all.job.mom_pct, true),
        ma = arrow(all.job.mom_pct),
        m = pct_opt(all.job.mom_pct),
        yc = dir_class(all.job.yoy_pct, true),
        ya = arrow(all.job.yoy_pct),
        y = pct_opt(all.job.yoy_pct),
        s = dec1_opt(all.spp.latest),
        tr = esc(all.job.label_trend)
    ));
    h.push_str("</tbody></table></div>");

    // --- 5 業界を同じ物差しで重ねた図 ---
    let series: Vec<(String, Vec<Option<f64>>)> = series_by_industry
        .iter()
        .filter(|(n, _, _)| n != industry::OUTSIDE)
        .map(|(n, s, _)| {
            let ov = Overview::from_series(n, s, months);
            (n.clone(), ov.job.indexed())
        })
        .collect();
    h.push_str(&format!(
        "<p class=\"text-slate-400 text-xs mt-4 mb-1\">\
         最初の月を 100 とした指数。業界ごとに軸を変えると、どれも同じ形に見えてしまいます。</p>{}",
        line_chart(months, &series, true, 320)
    ));

    // --- 業界ごとの一行 ---
    h.push_str("<div class=\"mt-4 space-y-3\">");
    for r in rows.iter().filter(|r| r.why.is_some()) {
        let Some((_, s, _)) = series_by_industry.iter().find(|(n, _, _)| *n == r.name) else {
            continue;
        };
        let ov = Overview::from_series(&r.name, s, months);
        h.push_str(&format!(
            "<div class=\"bg-navy-900/40 border border-slate-700 rounded p-3\">\
             <div class=\"flex flex-wrap items-baseline gap-2\">\
             <span class=\"text-slate-100 font-bold\">{n}</span>\
             <span class=\"text-slate-400 text-xs\">{t} 職種／全体の {sh}／求人数が多いのは {top}</span></div>\
             <p class=\"text-slate-300 text-sm mt-1 leading-relaxed\">{s1}</p>\
             <p class=\"text-slate-300 text-sm leading-relaxed\">{s2}</p>\
             <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">まとめ方：{why}</p></div>",
            n = esc(&r.name),
            t = r.titles,
            sh = match r.share_pct {
                Some(v) => format!("{v:.1}%"),
                None => "—".to_string(),
            },
            top = if r.top_titles.is_empty() {
                "—".to_string()
            } else {
                r.top_titles
                    .iter()
                    .map(|s| esc(s))
                    .collect::<Vec<_>>()
                    .join("、")
            },
            s1 = esc(&ov.job.sentence),
            s2 = esc(&ov.spp.sentence),
            why = esc(r.why.unwrap_or(""))
        ));
    }
    h.push_str("</div>");

    // 5 業界に入れなかった職種は、名前まで出す。黙って除くと数字が合わなくなる
    let outside: Vec<String> = snap
        .titles
        .iter()
        .filter(|t| industry::of_category(&t.category).is_none())
        .map(|t| format!("{}（{}）", t.name, t.category))
        .collect();
    if !outside.is_empty() {
        h.push_str(&format!(
            "<p class=\"text-slate-400 text-xs mt-3 leading-relaxed\">\
             5 業界に入れていない職種：{}。数字は上の「{}」の行に入っています。</p>",
            esc(&outside.join("、")),
            esc(industry::OUTSIDE)
        ));
    }

    h.push_str("</div>");
    h
}
