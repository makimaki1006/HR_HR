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
    arrow, bar_line_chart, category_table_html, dec1_opt, dir_class, esc, hbar_chart, indexed_chart,
    line_chart, metric_card, num_opt, pct_opt, scatter_chart, url_query, vbar_chart,
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
    /// どの面を出すか。指定が無ければ「全体」
    pub view: Option<String>,
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
    // 季節の波は別の出どころ（検索エンジンの検索ボリューム）。
    // 引けなければその節を出さないだけで、他は普通に出す
    let seasons = crate::indeed::season::load(db).unwrap_or_else(|e| {
        tracing::warn!("季節の波を読めませんでした: {e}");
        Vec::new()
    });
    Html(render_tab(
        snap,
        q.pref.as_deref(),
        q.sort.as_deref(),
        q.view.as_deref(),
        &seasons,
    ))
}

fn degraded(msg: &str) -> String {
    format!(
        "{DIRECT_ACCESS_GUARD}<div class=\"p-6\"><div class=\"bg-navy-800/60 border border-amber-600/40 rounded-lg p-4 text-amber-200\">{}</div></div>",
        esc(msg)
    )
}

/// 画面の面。1 枚に全部出すと 12 画面ぶんになり、上から読むしかなくなる。
///
/// # なぜサーバー側で分けるのか
/// 画面側で隠すだけだと、見ない図まで毎回作って送ることになる。
/// 面ごとに必要なものだけ組み立てる。
const VIEWS: [(&str, &str); 4] = [
    ("overview", "全体"),
    ("titles", "職種"),
    ("industry", "業界・分類"),
    ("people", "探している人"),
];

fn view_of(v: Option<&str>) -> &str {
    match v {
        Some(x) if VIEWS.iter().any(|(k, _)| *k == x) => x,
        _ => "overview",
    }
}

/// 面を切り替える帯。県と並べ替えを持ち回る。
fn view_tabs(current: &str, pref: Option<&str>, sort: Option<&str>) -> String {
    let mut h = String::from(
        "<div class=\"flex flex-wrap gap-1 border-b border-slate-700\" role=\"tablist\">",
    );
    for (key, label) in VIEWS {
        let on = key == current;
        let cls = if on {
            "px-4 py-2 text-sm font-bold text-sky-300 border-b-2 border-sky-400"
        } else {
            "px-4 py-2 text-sm text-slate-400 hover:text-slate-200 border-b-2 border-transparent"
        };
        let mut q = format!("?view={key}");
        if let Some(p) = pref.filter(|x| !x.is_empty()) {
            q.push_str(&format!("&pref={}", url_query(p)));
        }
        if let Some(x) = sort.filter(|x| !x.is_empty()) {
            q.push_str(&format!("&sort={}", url_query(x)));
        }
        h.push_str(&format!(
            "<a class=\"{cls}\" role=\"tab\" aria-selected=\"{on}\"              href=\"/tab/indeed{q}\" hx-get=\"/tab/indeed{q}\" hx-target=\"#content\"              hx-swap=\"innerHTML\" hx-push-url=\"true\" hx-indicator=\"#indeed-loading\">{l}</a>",
            l = esc(label)
        ));
    }
    h.push_str(&format!(
        "<input type=\"hidden\" name=\"view\" value=\"{}\">",
        esc(current)
    ));
    h.push_str("</div>");
    h
}

fn render_tab(
    snap: &Snapshot,
    pref: Option<&str>,
    sort: Option<&str>,
    view: Option<&str>,
    seasons: &[crate::indeed::season::TitleSeason],
) -> String {
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

    let view = view_of(view);
    h.push_str(&view_tabs(view, pref, sort));

    // 結論を先に置く。数字と図はその根拠として下に続く
    if view == "overview" {
        h.push_str(&summary_section(snap, &overview, seasons, pref));
    }

    // 見出しの 5 指標
    if view == "overview" {
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

    // 「なぜ」を図でも見せる。求人（棒）が増えると 1 求人あたり（線）が薄まる、
    // という関係は、別々の図に分けると読み手が頭の中で重ねることになる
    h.push_str(&format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\n         <h3 class=\"text-slate-100 font-bold mb-1\">求人の数と、1 求人あたりに見た人数</h3>\n         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\n         棒が求人の数（左軸）、線が 1 求人あたりに見た人数（右軸）です。\n         棒が伸びた月に線が下がっていれば、求人が増えて 1 件あたりの取り分が薄まったことになります。</p>{chart}</div>",
        chart = bar_line_chart(
            months,
            "求人の数",
            &overview.job.series,
            "1 求人あたりに見た人数",
            &overview.spp.series,
            true,
            300
        )
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

    }

    // 業界（全国のみ。県で絞ると 1 業界あたりの月次が薄くなる）
    if view == "industry" && pref.is_none() {
        h.push_str(&industry_section(snap, months));
    }

    // 職種の位置取り（全国のみ。県で絞ると点が薄くなる）
    if view == "titles" && pref.is_none() {
        h.push_str(&scatter_section(snap));
    }

    // 季節の波（全国のみ。県別の検索ボリュームは持っていない）
    if view == "people" && pref.is_none() {
        h.push_str(&season_section(seasons));
    }

    // スマホ比率（職種そのものの性質なので、県で絞っても同じ値）
    if view == "people" {
        h.push_str(&mobile_section(snap));
    }

    // 分類（全国のみ。県で絞ると分類別の月次が薄くなる）
    if view == "industry" && pref.is_none() {
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
    if view == "titles" {
        h.push_str(&title_section(snap, pref, sort));
    }

    h.push_str("</div>");
    h
}

fn pref_selector(prefs: &[String], current: Option<&str>) -> String {
    let mut s = String::from(
        "<select class=\"bg-navy-800 border border-slate-600 text-slate-100 rounded px-3 py-2 text-sm\" \
         hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML\" name=\"pref\" hx-trigger=\"change\" hx-include=\"[name='sort'],[name='view']\" \n         hx-push-url=\"true\" hx-indicator=\"#indeed-loading\" aria-label=\"都道府県\">",
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

const SORTS: [SortSpec; 8] = [
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
    SortSpec {
        key: "mobile",
        label: "スマホで探されている順",
        note: "スマホからの検索が多い職種を上にしています。求人ページと応募フォームをどちらに合わせるかの手がかりです。この値は職種そのものの性質なので、県を選んでも変わりません。",
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
         hx-include=\"[name='pref'],[name='view']\" \n         hx-push-url=\"true\" hx-indicator=\"#indeed-loading\" aria-label=\"並べ替え\">",
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

/// 職種一覧の検索欄。
///
/// # なぜ画面側で絞るのか
/// 125 行を上から目で追うのは現実的でない。並べ替えは 8 種類あるが、
/// 「この職種を見たい」という探し方には効かない。
/// サーバーに投げ直すと並べ替えと県の選択を持ち回る必要が出るので、
/// 出ている表をその場で隠す。件数も出して、何行に絞れたか分かるようにする。
///
/// htmx で差し替わるたびにこの script は読み直されるが、
/// 関数を定義し直すだけなので二重には効かない。
const SEARCH_BOX: &str = r#"<div class="mt-3 mb-2 flex items-center gap-3">
<input id="indeed-title-find" type="search" oninput="indeedFilterTitles(this.value)"
 placeholder="職種名・分類・業界で絞り込み"
 class="flex-1 px-3 py-1.5 bg-navy-900 border border-slate-700 rounded text-sm text-slate-100 placeholder-slate-500 focus:border-sky-500 focus:outline-none">
<span id="indeed-title-count" class="text-slate-400 text-xs tabular-nums"></span></div>
<script>
function indeedFilterTitles(q) {
  var rows = document.querySelectorAll('tr[data-find]');
  var needle = (q || '').trim().toLowerCase();
  var shown = 0;
  rows.forEach(function (r) {
    var hit = !needle || (r.getAttribute('data-find') || '').toLowerCase().indexOf(needle) >= 0;
    r.style.display = hit ? '' : 'none';
    if (hit) shown += 1;
  });
  var c = document.getElementById('indeed-title-count');
  if (c) c.textContent = needle ? shown + ' / ' + rows.length + ' 職種' : '';
}
</script>"#;

/// 職種の一覧。全国なら全職種、県を選んでいればその県の職種。
fn title_section(snap: &Snapshot, pref: Option<&str>, sort: Option<&str>) -> String {
    let months = &snap.meta.months;
    let spec = sort_spec(sort);

    // スマホ比率は職種そのものの性質で、県で絞っても変わらない。名前から引く
    let mobile: std::collections::HashMap<&str, f64> = snap
        .titles
        .iter()
        .filter_map(|t| t.mobile_pct.map(|v| (t.name.as_str(), v)))
        .collect();

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
        "mobile" => rows.sort_by(|a, b| {
            key_desc(mobile.get(b.0.as_str()).copied())
                .total_cmp(&key_desc(mobile.get(a.0.as_str()).copied()))
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
    h.push_str(SEARCH_BOX);
    h.push_str("</div>");
    h.push_str(&format!(
        "<p class=\"text-slate-400 text-xs mb-3 leading-relaxed\">{}　\
         全 {n} 職種を出しています。抜き出さずに、並び順で見たいことを示しています。</p>",
        esc(spec.note),
        n = rows.len()
    ));

    h.push_str(
        "<div style=\"overflow-x:auto\"><table class=\"w-full text-sm border-collapse\" style=\"min-width:940px\"><thead><tr>",
    );
    for (name, align) in [
        ("職種", "left"),
        ("分類", "left"),
        ("業界", "left"),
        ("求人数（最新月）", "right"),
        ("求人数の変化", "right"),
        ("1求人あたり", "right"),
        ("その変化", "right"),
        ("スマホ", "right"),
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
            "<tr data-find=\"{find}\"><th scope=\"row\" class=\"{td} font-normal\" style=\"text-align:left\">\
             <a class=\"text-sky-400 hover:underline\" href=\"/tab/indeed/title?name={q}\" \
                hx-get=\"/tab/indeed/title?name={q}\" hx-target=\"#content\" hx-swap=\"innerHTML\" \
                hx-push-url=\"true\">{n}</a></th>\
             <td class=\"{td} text-slate-400\">{c}</td>\
             <td class=\"{td} text-slate-400\">{ind}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{td} tabular-nums {jc}\" style=\"text-align:right\">{ja} {jp}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{s}</td>\
             <td class=\"{td} tabular-nums {sc}\" style=\"text-align:right\">{sa} {sp}</td>\
             <td class=\"{td} tabular-nums text-slate-400\" style=\"text-align:right\">{mb}</td>\
             <td class=\"{td} text-slate-300\">{t}</td></tr>",
            n = esc(name),
            // 検索欄が見る文字列。職種・分類・業界をまとめて 1 つの属性に入れる
            find = esc(&format!(
                "{name} {cat} {ind}",
                ind = crate::indeed::industry::of_category(cat)
                    .unwrap_or(crate::indeed::industry::OUTSIDE)
            )),
            q = url_query(name),
            c = esc(cat),
            // 散布図の色はこの業界。色が読み取れない人も、表から同じ区分けを追える
            ind = esc(
                crate::indeed::industry::of_category(cat)
                    .unwrap_or(crate::indeed::industry::OUTSIDE)
            ),
            j = num_opt(o.job.latest),
            jc = dir_class(o.job.change_pct, true),
            ja = arrow(o.job.change_pct),
            jp = pct_opt(o.job.change_pct),
            s = dec1_opt(o.spp.latest),
            sc = dir_class(o.spp.change_pct, true),
            sa = arrow(o.spp.change_pct),
            sp = pct_opt(o.spp.change_pct),
            mb = match mobile.get(name.as_str()) {
                Some(v) => format!("{v:.1}%"),
                None => "—".to_string(),
            },
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
    use crate::indeed::aggregate::industry_table;
    use crate::indeed::industry;

    let rows = industry_table(snap);
    if rows.is_empty() {
        return String::new();
    }
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
    let series: Vec<(String, Vec<Option<f64>>)> = rows
        .iter()
        .filter(|r| r.name != industry::OUTSIDE)
        .map(|r| (r.name.clone(), r.ov.job.indexed()))
        .collect();
    h.push_str(&format!(
        "<p class=\"text-slate-400 text-xs mt-4 mb-1\">\
         最初の月を 100 とした指数。業界ごとに軸を変えると、どれも同じ形に見えてしまいます。</p>{}",
        line_chart(months, &series, true, 320)
    ));

    // --- 業界ごとの一行 ---
    h.push_str("<div class=\"mt-4 space-y-3\">");
    for r in rows.iter().filter(|r| r.why.is_some()) {
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
            s1 = esc(&r.ov.job.sentence),
            s2 = esc(&r.ov.spp.sentence),
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

/// 職種の位置取りを 1 枚で見る散布図。
///
/// # なぜ表ではなく散布図か
/// 126 行の表は上から読むしかない。「募集は多いのに人が集まっていない」職種を
/// 探すには、求人数の列と 1 求人あたりの列を目で往復することになる。
/// 位置に置けば、右下を見るだけで済む。
/// 先頭に置く「要点」。
///
/// # なぜ足したのか
/// 数字と図は出していたが、「だから何をすればいいのか」が無かった。
/// 12 画面ぶんを上から読ませて、読み手に結論を組み立てさせていた。
/// 材料は散らばっていただけなので、答えの形にして先頭に置く。
///
/// # 書いてよいことの線引き
/// ここに書くのは**この画面の数字から出せることだけ**。
/// 向きが定まらない指標を「増えています」と断定しない
/// （期間全体の変化は月ごとの上下をならした線に沿った値なので、そう断る）。
/// 打ち手は「やれ」ではなく「見直す候補になる」に留める。
fn summary_section(
    snap: &Snapshot,
    ov: &Overview,
    seasons: &[crate::indeed::season::TitleSeason],
    pref: Option<&str>,
) -> String {
    let months = &snap.meta.months;
    let where_ = pref.unwrap_or("全国");

    // --- 1. いま何が起きているか ---
    let state = match (ov.job.change_pct, ov.spp.change_pct) {
        (Some(j), Some(s)) if j > 0.0 && s < 0.0 => format!(
            "募集は <strong>{}</strong> 増えたのに、1 求人あたりに見た人数は <strong>{}</strong> 減りました。\
             <strong>同じ求人票なら、期間の初めより人が集まりにくくなっています。</strong>",
            pct_opt(Some(j)),
            pct_opt(Some(s))
        ),
        (Some(j), Some(s)) if j < 0.0 && s > 0.0 => format!(
            "募集が <strong>{}</strong> 減り、1 求人あたりに見た人数は <strong>{}</strong> 増えました。\
             <strong>競合が引いて、集まりやすくなっています。</strong>",
            pct_opt(Some(j)),
            pct_opt(Some(s))
        ),
        (Some(j), Some(s)) => format!(
            "募集は {}、1 求人あたりに見た人数は {} でした。",
            pct_opt(Some(j)),
            pct_opt(Some(s))
        ),
        _ => "この期間の変化を出せるだけの月数がそろっていません。".to_string(),
    };
    let unsure = [&ov.job, &ov.spp]
        .iter()
        .filter(|m| {
            m.fit
                .as_ref()
                .map(|f| f.level == crate::indeed::trend::Level::None)
                .unwrap_or(false)
        })
        .count();
    let hedge = if unsure > 0 {
        "（月ごとの上下が大きく、一本調子ではありません。上の % はならした線に沿った変化です）"
    } else {
        ""
    };

    // --- 2. どの職種を見るか ---
    let mut rows: Vec<(&str, Overview)> = snap
        .titles
        .iter()
        .filter(|t| t.complete)
        .filter_map(|t| {
            snap.by_title
                .get(&t.name)
                .map(|s| (t.name.as_str(), Overview::from_series(&t.name, s, months)))
        })
        .collect();
    rows.sort_by(|a, b| {
        a.1.spp
            .change_pct
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b.1.spp.change_pct.unwrap_or(f64::INFINITY))
    });
    let harder: Vec<String> = rows
        .iter()
        .filter(|(_, o)| o.job.latest.unwrap_or(0.0) >= 5_000.0)
        .take(3)
        .map(|(n, o)| format!("{}（{}）", esc(n), pct_opt(o.spp.change_pct)))
        .collect();
    let mut big: Vec<&(&str, Overview)> = rows
        .iter()
        .filter(|(_, o)| o.job.latest.unwrap_or(0.0) >= 20_000.0)
        .collect();
    big.sort_by(|a, b| {
        a.1.spp
            .latest
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b.1.spp.latest.unwrap_or(f64::INFINITY))
    });
    let crowded: Vec<String> = big
        .iter()
        .take(3)
        .map(|(n, o)| format!("{}（{}）", esc(n), dec1_opt(o.spp.latest)))
        .collect();

    // --- 3. いつ動くか ---
    let when = if pref.is_some() || seasons.len() < 20 {
        String::new()
    } else {
        let idx = crate::indeed::season::overall(seasons);
        let pick = |max: bool| {
            let mut best: Option<(usize, f64)> = None;
            for (i, v) in idx.iter().enumerate() {
                let Some(x) = v else { continue };
                let better = match best {
                    None => true,
                    Some((_, b)) => {
                        if max {
                            *x > b
                        } else {
                            *x < b
                        }
                    }
                };
                if better {
                    best = Some((i, *x));
                }
            }
            best
        };
        match (pick(true), pick(false)) {
            (Some((hi, hv)), Some((lo, lv))) => format!(
                "<li>探している人がいちばん多いのは <strong>{} 月</strong>（年間平均の {:.2} 倍）、\
                 少ないのは <strong>{} 月</strong>（{:.2} 倍）です。\
                 動くなら年明けから {} 月に向けてで、{} 月に出しても人は少なめです。\
                 <span class=\"text-slate-500 text-xs\">※ 検索エンジンの検索ボリューム。\
                 Indeed の求人数とは別のデータです</span></li>",
                hi + 1,
                hv,
                lo + 1,
                lv,
                hi + 1,
                lo + 1
            ),
            _ => String::new(),
        }
    };

    format!(
        "<div class=\"bg-sky-900/30 border-l-4 border-sky-400 rounded-r-lg p-4\">\
         <h3 class=\"text-sky-100 text-base font-bold mb-2\">要点（{w}）</h3>\
         <ul class=\"text-slate-200 text-sm leading-relaxed list-disc pl-5 space-y-2\">\
         <li>{state}<span class=\"text-slate-500 text-xs\">{hedge}</span></li>\
         {harder}{crowded}{when}</ul>\
         <p class=\"text-slate-500 text-xs mt-3 leading-relaxed\">\
         ここに書いたのは、下の図と表から出せることだけです。根拠は各図の下にあります。</p></div>",
        w = esc(where_),
        state = state,
        hedge = hedge,
        harder = if harder.is_empty() {
            String::new()
        } else {
            format!(
                "<li><strong>去年と同じやり方が通じにくくなっている職種</strong>は {}。\
                 1 求人あたりに見た人数がいちばん減った順です（求人 5,000 件以上）。\
                 求人票の書き方か、出す媒体を見直す候補になります。</li>",
                harder.join("、")
            )
        },
        crowded = if crowded.is_empty() {
            String::new()
        } else {
            format!(
                "<li><strong>募集は多いのに人が集まっていない職種</strong>は {}。\
                 かっこ内は 1 求人あたりに見た人数で、求人 20,000 件以上の中で少ない順です。\
                 求人票を出すだけでは埋まりにくい見込みです。</li>",
                crowded.join("、")
            )
        },
        when = when
    )
}

/// 季節の波。1 年のうち、いつ求職者が動くか。
///
/// # なぜ職種ごとに出さないのか
/// 検索ボリュームは粗いきざみで報告される。検索数が少ない職種ほど
/// きざみの影響が大きく出て、**季節の波が大きく見える**。
///
///     月あたりの検索数   職種数   peak_ratio の中央
///          〜50           16          1.28
///        1000〜           20          1.10
///
/// データが良いほど波が小さいので、これは季節ではなくきざみの粗さである。
/// 施工管理技術者（月 7 回）は 48 か月の値が 0 か 10 しか無く、
/// 0 を除くと全月 1.00 になる。職種ごとに出すと、この見せかけを
/// 「3 月に動くべき職種」と読ませてしまう。
///
/// 職種をまたいでならすとぶれが打ち消し合い、3 月が山・12 月が谷という
/// 形が残る。しかも検索数の多い職種に絞るほどはっきりする
/// （12 月が最小の職種は全体で 42%、月 1000 回以上では 65%）。
fn season_section(seasons: &[crate::indeed::season::TitleSeason]) -> String {
    use crate::indeed::season;
    if seasons.len() < 20 {
        return String::new();
    }
    let idx = season::overall(seasons);
    if idx.iter().any(|v| v.is_none()) {
        return String::new();
    }
    let labels: Vec<String> = (1..=12).map(|m| format!("{m}月")).collect();
    let pick = |want_max: bool| -> (String, String) {
        let mut best: Option<(usize, f64)> = None;
        for (i, v) in idx.iter().enumerate() {
            let Some(x) = v else { continue };
            let better = match best {
                None => true,
                Some((_, b)) => {
                    if want_max {
                        *x > b
                    } else {
                        *x < b
                    }
                }
            };
            if better {
                best = Some((i, *x));
            }
        }
        match best {
            Some((i, x)) => (format!("{} 月", i + 1), format!("{x:.2}")),
            None => ("—".to_string(), "—".to_string()),
        }
    };
    let (hm, hv) = pick(true);
    let (lm, lv) = pick(false);
    let years = seasons.first().map(|s| s.years).unwrap_or(0);
    let solid = seasons.iter().filter(|s| s.avg_monthly >= 1000.0).count();

    format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <h3 class=\"text-slate-100 font-bold mb-1\">1 年のうち、いつ動くか（過去 {y} 年）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         検索エンジンで職種名がどれだけ検索されたかを、{n} 職種ぶんならして\
         暦月ごとに平均したものです。<strong>1.00 が年間の平均</strong>で、\
         1.10 なら平均より 1 割多い月という意味です。\
         上の求人数とは<strong>別の出どころ</strong>で、求人の数ではなく\
         「探している人の動き」を表します。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         いちばん多いのは{hm}（{hv}）、少ないのは{lm}（{lv}）です。\
         差は 2 割ほどで、大きくはありません。</p>\
         <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">\
         職種ごとには出していません。検索数が少ない職種ほど月ごとのきざみが粗く、\
         波が大きく見えてしまうためです（月 50 回未満の 16 職種では山が年間平均の \
         1.28 倍、月 1000 回以上の {solid} 職種では 1.10 倍）。\
         ならすとぶれが打ち消し合い、検索数の多い職種に絞るほどこの形がはっきりします。</p></div>",
        y = years,
        n = seasons.len(),
        chart = vbar_chart(
            &labels,
            &idx,
            "年間平均を 1.00 としたときの比",
            Some((1.0, "年間平均")),
            true,
            300
        ),
        hm = hm,
        hv = hv,
        lm = lm,
        lv = lv,
        solid = solid
    )
}

/// スマホで探されている職種と、PC で探されている職種。
///
/// # なぜ図にするのか
/// 求人ページと応募フォームをどちらに合わせるかは、作り直しの費用が大きい割に
/// 「なんとなくスマホ」で決められがち。職種によって 44.5% から 84.4% まで
/// 40 ポイント近く違うので、職種を決めてから話せる材料になる。
///
/// # 125 職種を全部は出さない
/// 横棒 125 本は 2000px 近くになり、上下を見比べられない。
/// 高いほう 10 と低いほう 10 だけを出し、残りは下の表で見てもらう。
fn mobile_section(snap: &Snapshot) -> String {
    let mut rows: Vec<(&str, f64)> = snap
        .titles
        .iter()
        .filter_map(|t| t.mobile_pct.map(|v| (t.name.as_str(), v)))
        .collect();
    if rows.len() < 20 {
        return String::new();
    }
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let med = {
        let mut v: Vec<f64> = rows.iter().map(|r| r.1).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    let n = rows.len();
    let top: Vec<&(&str, f64)> = rows.iter().take(10).collect();
    let bottom: Vec<&(&str, f64)> = rows.iter().skip(n - 10).collect();
    // 上と下をそのまま繋げると、20 職種が連続した順位に見える。
    // 間に棒の無い行を 1 つ挟んで、抜けていることを図の側でも示す。
    // 抜けが無いとき（20 職種ちょうど）は挟まない。「ほか 0 職種」は嘘になる
    let hidden = n.saturating_sub(20);
    let mut labels: Vec<String> = top.iter().map(|r| r.0.to_string()).collect();
    let mut values: Vec<Option<f64>> = top.iter().map(|r| Some(r.1)).collect();
    if hidden > 0 {
        labels.push(format!("\u{2500}\u{2500} ほか {hidden} 職種 \u{2500}\u{2500}"));
        values.push(None);
    }
    labels.extend(bottom.iter().map(|r| r.0.to_string()));
    values.extend(bottom.iter().map(|r| Some(r.1)));

    format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <h3 class=\"text-slate-100 font-bold mb-1\">スマホで探されている職種・PC で探されている職種</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         求人ページと応募フォームをどちらに合わせるかの手がかりです。\
         {n} 職種のうち、<strong>高いほう 10 と低いほう 10</strong>だけを出しています{mid}\
         。破線は全体の真ん中（{md}）です。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         いちばん高いのは{t}（{tv}）、いちばん低いのは{b}（{bv}）で、差は {gap} ポイントあります。\
         「スマホで探す人は条件で絞り込む」という見方は、この数字と条件検索率の間には\
         ほとんど関係が無く（相関 0.04）、裏づけられません。</p></div>",
        n = n,
        mid = if hidden > 0 {
            format!("（間の {hidden} 職種は下の表で見てください）")
        } else {
            String::new()
        },
        md = dec1_opt(Some(med)),
        chart = hbar_chart(
            &labels,
            &values,
            "スマホからの検索の割合（%）",
            Some((med, "全体の真ん中")),
            true,
            140 + (labels.len() as u32) * 22
        ),
        t = esc(top[0].0),
        tv = dec1_opt(Some(top[0].1)),
        b = esc(bottom[9].0),
        bv = dec1_opt(Some(bottom[9].1)),
        gap = dec1_opt(Some(top[0].1 - bottom[9].1))
    )
}

fn scatter_section(snap: &Snapshot) -> String {
    use crate::indeed::industry;

    let months = &snap.meta.months;
    let mut points: Vec<(String, f64, f64, String)> = Vec::new();
    for t in snap.titles.iter().filter(|t| t.complete) {
        let Some(s) = snap.by_title.get(&t.name) else {
            continue;
        };
        let ov = Overview::from_series(&t.name, s, months);
        let (Some(j), Some(spp)) = (ov.job.latest, ov.spp.latest) else {
            continue;
        };
        if j <= 0.0 {
            continue;
        }
        points.push((
            t.name.clone(),
            j,
            spp,
            industry::of_category(&t.category)
                .unwrap_or(industry::OUTSIDE)
                .to_string(),
        ));
    }
    if points.is_empty() {
        return String::new();
    }
    let mut groups: Vec<String> = industry::INDUSTRIES
        .iter()
        .map(|i| i.name.to_string())
        .collect();
    if points.iter().any(|p| p.3 == industry::OUTSIDE) {
        groups.push(industry::OUTSIDE.to_string());
    }

    // 目安になる値を文章で添える。図だけでは「どこから右下か」が決められない
    let med = {
        let mut v: Vec<f64> = points.iter().map(|p| p.2).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    let hard: Vec<&str> = {
        let mut v: Vec<&(String, f64, f64, String)> =
            points.iter().filter(|p| p.1 >= 20_000.0).collect();
        v.sort_by(|a, b| a.2.total_cmp(&b.2));
        v.iter().take(5).map(|p| p.0.as_str()).collect()
    };

    format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <h3 class=\"text-slate-100 font-bold mb-1\">職種の位置取り（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         横は求人数（対数）、縦は 1 求人あたりに見た人数です。\
         <strong>右下ほど「募集は多いのに人が集まっていない」</strong>職種になります。\
         色は業界です。合計に入る {n} 職種を出しています。</p>\
         {chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">\
         1 求人あたりの真ん中は {med} です。\
         求人が 20,000 件以上ある職種のうち、1 求人あたりが少ないのは {hard} の順でした。</p></div>",
        m = esc(&snap.meta.latest),
        n = points.len(),
        chart = scatter_chart(&points, &groups, true, 380),
        med = dec1_opt(Some(med)),
        hard = if hard.is_empty() {
            "—".to_string()
        } else {
            hard.iter().map(|s| esc(s)).collect::<Vec<_>>().join("、")
        }
    )
}

#[cfg(test)]
mod season_tests {
    use super::*;
    use crate::indeed::season::TitleSeason;

    fn t(name: &str, index: [Option<f64>; 12], avg: f64) -> TitleSeason {
        TitleSeason {
            title: name.to_string(),
            index,
            peak_month: Some(3),
            trough_month: Some(12),
            peak_ratio: Some(1.1),
            years: 4,
            months: Vec::new(),
            series: Vec::new(),
            avg_monthly: avg,
        }
    }

    fn many(n: usize) -> Vec<TitleSeason> {
        // 3 月が山、12 月が谷の形
        let v = [
            1.03, 1.01, 1.09, 1.06, 1.05, 1.00, 0.95, 0.98, 1.03, 0.99, 0.95, 0.86,
        ];
        let mut idx = [None; 12];
        for (i, x) in v.iter().enumerate() {
            idx[i] = Some(*x);
        }
        (0..n)
            .map(|i| t(&format!("職種{i}"), idx, if i < 20 { 2000.0 } else { 30.0 }))
            .collect()
    }

    /// 職種が少なければ出さない。ならして初めて意味が出る図なので、
    /// 数が足りないうちに出すと 1 職種のぶれがそのまま形になる。
    #[test]
    fn 職種が少ないときは季節の図を出さない() {
        assert_eq!(season_section(&many(19)), "");
        assert!(!season_section(&many(20)).is_empty());
    }

    /// 職種ごとに出さない理由を必ず書く。
    ///
    /// 検索数が少ない職種ほど月ごとのきざみが粗く、波が大きく見える。
    /// 実測で 月 50 回未満は中央 1.28、月 1000 回以上は 1.10 だった。
    /// この断りが消えると、読み手は「職種ごとの波も出せるはず」と考える。
    #[test]
    fn 職種ごとに出さない理由を書いている() {
        let h = season_section(&many(84));
        assert!(h.contains("職種ごとには出していません"));
        assert!(h.contains("きざみが粗く"));
        // 出どころが違うことも書く
        assert!(h.contains("別の出どころ"));
        assert!(h.contains("検索エンジン"));
        assert!(!h.contains("Google"));
    }

    /// 山と谷が本文と図で一致すること。
    #[test]
    fn 山と谷が本文と図で一致する() {
        let h = season_section(&many(84));
        assert!(h.contains("3 月（1.09）"), "山が本文に出ていない");
        assert!(h.contains("12 月（0.86）"), "谷が本文に出ていない");
        assert!(h.contains("1.090"), "図に山の値が渡っていない");
        // 検索数が多い職種の数を数えて書く
        assert!(h.contains("月 1000 回以上の 20 職種"));
    }
}
