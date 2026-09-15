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
    http::HeaderMap,
    response::Html,
};
use serde::Deserialize;

use super::render::{
    arrow, dec1_opt, dir_class, dumbbell_chart, esc, hbar_chart, json_str, line_chart, metric_card,
    dual_line_chart, num_opt, pct_opt, small_multiples, url_query,
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
    /// 都道府県で絞る。空なら全国
    pub pref: Option<String>,
    /// 一覧で選ばれていた並べ替え。この画面では使わず、戻りリンクに載せ直すためだけに受け取る。
    ///
    /// 2026-09 時点の一覧（`tab.rs` の `title_section`）は詳細へのリンクに
    /// `name` と `pref` しか付けていないので、ここには届かない。
    /// 届かないときは [`sort_from_hx_url`] で拾う。
    /// 一覧側が `&sort=` を足したら、そのままこちらが優先される。
    pub sort: Option<String>,
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

/// 県の行を開いたときに差し込む推移。
///
/// `close=1` が付いていれば空文字を返す。閉じるための別ルートを作らずに済み、
/// JavaScript も要らない。
pub async fn tab_indeed_title_pref(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PrefQuery>,
) -> Html<String> {
    if q.close.as_deref() == Some("1") {
        return Html(String::new());
    }
    let (Some(name), Some(pref)) = (q.name.as_deref(), q.pref.as_deref()) else {
        return Html(String::new());
    };
    let Some(db) = state.indeed_db.as_ref() else {
        return Html(String::new());
    };
    let series = match crate::indeed::detail::pref_series(db, name, pref) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Html(format!(
                "<p class=\"text-slate-400 text-sm p-3\">{} の推移は取れませんでした。</p>",
                esc(pref)
            ))
        }
        Err(e) => {
            tracing::error!("pref_series failed: {e}");
            return Html(String::new());
        }
    };
    Html(pref_series_html(&series))
}

#[derive(serde::Deserialize)]
pub struct PrefQuery {
    pub name: Option<String>,
    pub pref: Option<String>,
    /// "1" なら閉じる（空を返す）
    pub close: Option<String>,
}

/// 1 県ぶんの推移を、3 つの図として横に並べる。
///
/// # なぜ重ねないのか
/// 見た人数・求人数・企業数は桁が 2 つ違う（東京都の販売スタッフで
/// 30 万人 / 3 万件 / 4.5 千社）。1 つの軸に重ねると企業数が平らな線になる。
/// 全国の面と同じく、桁が違うものは重ねずに並べる。
fn pref_series_html(s: &crate::indeed::detail::PrefSeries) -> String {
    use crate::handlers::indeed::render::{num_opt, raw_line_chart_colored};
    let n = s.months.len();
    if n < 4 {
        return format!(
            "<p class=\"text-slate-400 text-sm p-3\">{} は {} か月ぶんしか無く、推移として出せません。</p>",
            esc(&s.prefecture),
            n
        );
    }
    let first = |v: &[Option<f64>]| v.iter().flatten().next().copied();
    let last = |v: &[Option<f64>]| v.iter().flatten().next_back().copied();
    // 図 1 枚ぶん。全幅で縦に積む。
    //
    // # なぜ横に並べないのか
    // 最初は `small_multiples` で 3 枚を横に並べていた。1 枚あたりの幅が
    // 画面の 3 分の 1 になり、14 か月ぶんの折れ線が小さくて読めなかった
    // （2026-09-15 に指摘）。県を一度に何枚も開く使い方ではないので、
    // 1 枚ずつ全幅を使い、縦に積むほうが読める。
    let one = |name: &str, v: &[Option<f64>], unit: &str, ci: usize| -> String {
        let 増減 = match (first(v), last(v)) {
            (Some(a), Some(b)) if a > 0.0 => format!(
                "{} → {} {}（{:+.1}%）",
                num_opt(Some(a)),
                num_opt(Some(b)),
                unit,
                (b / a - 1.0) * 100.0
            ),
            _ => "—".to_string(),
        };
        format!(
            "<div class=\"mb-3\">\
             <div class=\"flex flex-wrap gap-3 items-baseline mb-1\">\
             <span class=\"text-slate-200 text-sm font-bold\">{nm}</span>\
             <span class=\"text-slate-400 text-xs tabular-nums\">{d}</span></div>\
             {c}</div>",
            nm = esc(name),
            d = 増減,
            c = raw_line_chart_colored(
                &s.months,
                &[(name.to_string(), v.to_vec())],
                true,
                230,
                unit,
                Some(ci)
            )
        )
    };
    format!(
        "<div class=\"border border-slate-600 rounded-lg p-4 mt-1 mb-2\">\
         <div class=\"flex flex-wrap gap-4 items-baseline mb-3\">\
         <span class=\"text-slate-100 text-base font-bold\">{p} の推移（{m} か月）</span>\
         <a class=\"text-blue-400 text-xs\" href=\"#\"\
            hx-get=\"/tab/indeed/title/pref?close=1\"\
            hx-target=\"closest div.pref-open\" hx-swap=\"innerHTML\">閉じる</a></div>\
         {c1}{c2}{c3}\
         <p class=\"text-slate-400 text-xs leading-relaxed\">\
         見た人数は<strong>求人が開かれた回数</strong>で、応募数ではありません。\
         縦軸はドラッグで目盛りの幅を変えられます（ダブルクリックで戻ります）。</p></div>",
        p = esc(&s.prefecture),
        m = n,
        c1 = one("求人を見た人数", &s.ctk, "人", 2),
        c2 = one("求人の数", &s.job, "件", 0),
        c3 = one("募集している企業の数", &s.employers, "社", 3),
    )
}

pub async fn tab_indeed_title(
    State(state): State<Arc<AppState>>,
    session: tower_sessions::Session,
    headers: HeaderMap,
    Query(q): Query<TitleQuery>,
) -> Html<String> {
    // 都道府県は画面いちばん上の絞り込み（セッション）を使う。社内タブと同じ流儀
    let session_pref: String = session
        .get(crate::auth::SESSION_PREFECTURE_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    // 県を選んでいれば、その県の系列で見立てる。
    // 全国の数字のまま「東京都」と名乗らせると、まるごと嘘になる。
    //
    // データを読む前に決めておく。読めなかったときの断り（[`note`]）にも
    // 同じ戻り先を付けたいので、県と並べ替えはいちばん先に確定させる
    let pref = q
        .pref
        .as_deref()
        .filter(|x| !x.is_empty())
        .or(Some(session_pref.as_str()))
        .filter(|x| !x.is_empty());
    // 一覧で選ばれていた並べ替え。クエリで来ていればそれ、来ていなければ
    // htmx が送ってくる「いま見えている URL」から拾う
    let sort = q
        .sort
        .as_deref()
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .or_else(|| sort_from_hx_url(&headers));
    let back = back_href(pref, sort.as_deref());

    let Some(db) = state.indeed_db.as_ref() else {
        return Html(note("Indeed 分析データが積まれていません。", &back));
    };
    let Some(name) = q.name.as_deref().filter(|s| !s.is_empty()) else {
        return Html(note(
            "職種が指定されていません。一覧から選んでください。",
            &back,
        ));
    };
    let snap = match snapshot(db) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("indeed snapshot failed: {e}");
            return Html(note("Indeed 分析データを読めませんでした。", &back));
        }
    };
    let d = match detail::load(db, name) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return Html(note(
                &format!("「{name}」のデータが見つかりませんでした。"),
                &back,
            ))
        }
        Err(e) => {
            tracing::error!("indeed detail failed: {e}");
            return Html(note("この職種のデータを読めませんでした。", &back));
        }
    };
    let overview = match pref {
        None => snap
            .by_title
            .get(name)
            .map(|s| Overview::from_series(name, s, &snap.meta.months)),
        Some(p) => crate::indeed::aggregate::pref_rows(&snap, p)
            .iter()
            .find(|r| r.title == name)
            .map(|r| Overview::from_series(name, &r.series, &snap.meta.months)),
    };

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

    // 県を選んでいればその県のスマホ率。全国の値のまま
    // 「東京都」と名乗らせると、この行だけ嘘になる
    let mobile = match pref {
        None => snap
            .titles
            .iter()
            .find(|t| t.name == name)
            .and_then(|t| t.mobile_pct),
        Some(p) => crate::indeed::aggregate::pref_rows(&snap, p)
            .iter()
            .find(|r| r.title == name)
            .and_then(|r| r.mobile_pct),
    };
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
        pref,
        sort.as_deref(),
    ))
}

/// 一覧に戻るリンクの URL。
///
/// # 何を持ち帰るのか
/// * `view=titles` … 素の `/tab/indeed` は「全体」の面。職種の一覧に戻す
/// * `pref` …… 県を選んでいた人を全国に落とさない
/// * `sort` …… 一覧で選んでいた並べ替え。`tab.rs` の `TabQuery` がそのまま受ける
///
/// 「スマホで探されている順・東京都」で一覧を見ていた人が、職種を 1 つ開いて
/// 戻ったときに並べ替えだけ消える、という状態だった。
fn back_href(pref: Option<&str>, sort: Option<&str>) -> String {
    let mut h = String::from("/tab/indeed?view=titles");
    if let Some(p) = pref.filter(|x| !x.is_empty()) {
        h.push_str(&format!("&pref={}", url_query(p)));
    }
    if let Some(k) = sort.filter(|x| !x.is_empty()) {
        h.push_str(&format!("&sort={}", url_query(k)));
    }
    h
}

/// htmx が送ってくる「いま見えている URL」から、一覧の並べ替えを拾う。
///
/// # なぜヘッダを見るのか
/// 一覧（`tab.rs` の `title_section`）が職種名に張るリンクは
/// `/tab/indeed/title?name=…&pref=…` で、**`sort` が入っていない**。
/// この画面はクエリからは並べ替えを知りようがない。
/// htmx はリクエストのたびに `HX-Current-URL` を送るので、
/// 一覧から飛んできた直後であれば、そこに一覧の URL がまるごと残っている。
///
/// # 拾いすぎないための決まり
/// * パスが `/tab/indeed` のときだけ見る。別の画面の `sort` を持ち込まない
/// * 値は小文字の英字だけ受ける（実在するキーは size/grow/shrink/hard/worse/
///   better/steady/mobile の 8 つ）。知らない値は `tab.rs` 側で既定に落ちるが、
///   ここで形を絞っておけば、URL に何を入れられてもリンクに素通りしない
///
/// 直接 URL を叩かれた場合は `HX-Current-URL` がこの画面自身になり、
/// `sort` は入っていない。そのときは並べ替え無し（既定＝求人数の多い順）に戻る。
fn sort_from_hx_url(headers: &HeaderMap) -> Option<String> {
    let url = headers.get("hx-current-url")?.to_str().ok()?;
    let (path, query) = url.split_once('?')?;
    if !path.ends_with("/tab/indeed") {
        return None;
    }
    let v = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("sort="))?;
    if !v.is_empty() && v.len() <= 16 && v.bytes().all(|b| b.is_ascii_lowercase()) {
        Some(v.to_string())
    } else {
        None
    }
}

/// 出せないときの断り。
///
/// 戻り先は職種の面・県・並べ替えを持ったままにする（[`back_href`]）。
/// 素の `/tab/indeed` だと「全体」の面が出て、一覧から選び直そうとした人が
/// もう一度職種の面を開き直すことになる。
fn note(msg: &str, back: &str) -> String {
    format!(
        "{GUARD}<div class=\"p-6\"><div class=\"bg-navy-800/60 border border-amber-500 rounded-lg p-4 text-amber-300\">{m}</div>\
         <p class=\"mt-3\"><a class=\"text-blue-400\" href=\"{b}\" hx-get=\"{b}\" hx-target=\"#content\" hx-swap=\"innerHTML show:top\" hx-push-url=\"true\">一覧に戻る</a></p></div>",
        m = esc(msg),
        b = back
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
    pref: Option<&str>,
    sort: Option<&str>,
) -> String {
    let mut h = String::with_capacity(120_000);
    h.push_str(GUARD);
    h.push_str("<div class=\"space-y-6\">");

    // 見出しと戻り道。
    //
    // 戻り先には面・県・並べ替えを持たせる（[`back_href`]）。素の `/tab/indeed` に
    // 戻すと面が「全体」に、県が全国に、並べ替えが求人数順に落ちる。
    // この画面へは職種の一覧からしか来ないので view=titles が戻り先。
    // hx-push-url を付けないと URL だけ詳細のまま残り、再読み込みで詳細に戻ってしまう。
    let back = back_href(pref, sort);
    h.push_str(&format!(
        "<div>\
         <p class=\"text-slate-400 text-sm\">\
         <a class=\"text-blue-400 hover:underline\" href=\"{b}\" \
            hx-get=\"{b}\" hx-target=\"#content\" hx-swap=\"innerHTML show:top\" \
            hx-push-url=\"true\">← 採用市場の一覧</a></p>\
         <h2 class=\"text-xl font-bold text-gray-100 mt-1\">{t}</h2>\
         <p class=\"text-slate-400 text-sm\">{c}／最新月 {m}／<strong>{w}</strong></p></div>",
        b = back,
        t = esc(&d.title),
        c = esc(&d.category),
        m = esc(&d.month),
        // いま全国を見ているのか、県を見ているのかを見出しに出す。
        // 県を選んだのに数字が全国のままだと気づけない
        w = esc(pref.unwrap_or("全国"))
    ));



    // 結論を先に置く。図と表はその根拠として下に続く
    h.push_str(&takeaway_section(d, ov, w, terms, mobile));

    // 全国の姿
    if let Some(o) = ov {
        h.push_str("<div class=\"grid grid-cols-2 lg:grid-cols-5 gap-3\">");
        // 件数・人数・社数は整数、1 件あたりの比は小数第 1 位。
        // 共通の `metric_card` は 1000 未満をすべて小数第 1 位で出すので、
        // 自動車設計のように求人 937 件・企業 307 社の職種だと
        // 本文の「いま求人 937 件」に対してカードだけ「937.0」になっていた。
        for m in [&o.job, &o.ctk, &o.emp] {
            h.push_str(&count_card(m));
        }
        for m in [&o.spp, &o.ppe] {
            h.push_str(&metric_card(m, true));
        }
        h.push_str("</div>");
        h.push_str(&format!(
            "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">全国の動き</h3>\
             <p class=\"text-slate-400 text-xs mb-2\">実数です。3 つは桁が違うので、重ねずに並べています。</p>{chart}\
             <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">{s}</p>{bd}</div>",
            bd = crate::handlers::indeed::render::breakdown_html(
                crate::indeed::aggregate::growth_breakdown(&o.job.series, &o.emp.series)
            ),
            chart = small_multiples(
                months,
                &[
                    ("求人の数".to_string(), o.job.series.clone(), "件", 0),
                    ("求人を見た人数".to_string(), o.ctk.series.clone(), "人", 2),
                    ("募集している企業の数".to_string(), o.emp.series.clone(), "社", 3),
                ],
                true,
                180
            ),
            s = esc(&o.spp.sentence)
        ));
    }

    // 図を先、表を後にする。先に形で掴んでから数字を確かめる順。
    //
    // # なぜ職種の話を先、地域の話を後にするのか
    // 以前は都道府県別の図（`pref_bar`）が 3 枚目に来ていて、
    // 「この職種がどう動いているか」を読み切る前に地域の話が割り込んでいた。
    // 検索のされ方・探し方の言葉・求職者の内訳は**職種そのものの話**なので
    // 先にまとめ、地域の話（都道府県別の図と表）はそのあとに続ける。
    // 2026-09-15 にユーザーから並べ替えの指示。
    h.push_str(&source_compare_section(d, ov, months, season));
    h.push_str(&term_series_section(term_months, terms));
    h.push_str(&attr_section(attrs));
    h.push_str(&wage_gap_chart(d, w));
    h.push_str(&pref_bar(d, ov.and_then(|o| o.spp.latest)));
    h.push_str(&pref_table(d, w));
    h.push_str(&keywords_block(d));
    h.push_str(&attrs_block(d, attrs));
    h.push_str(&volume_block(d));

    h.push_str("</div>");
    h
}

/// 件数を出すカード。見た目は [`metric_card`] と同じで、値だけ整数にする。
///
/// # なぜ共通のカードを使わないか
/// `metric_card` は 1000 未満を小数第 1 位で出す。1 求人あたりの人数のような
/// 比では前月からの動きが見えるので正しいが、求人の数・見た人数・企業の数では
/// 「937.0 件」「307.0 社」になり、同じ画面の本文の「937 件」と食い違う。
/// `render.rs` は他のタブも使うので、こちら側で値の出し方だけ変える。
fn count_card(m: &crate::indeed::aggregate::Metric) -> String {
    format!(
        "<div class=\"bg-navy-800/60 border border-slate-700 rounded-lg p-4\">\
         <div class=\"text-slate-400 text-xs\">{name}</div>\
         <div class=\"text-slate-100 text-2xl font-bold tabular-nums\">{v}</div>\
         <div class=\"{dc} text-sm mt-1 tabular-nums\">{ar} {p}</div>\
         <div class=\"text-slate-400 text-xs mt-1\">{t}</div></div>",
        name = esc(&m.label),
        v = num_opt(m.latest),
        dc = dir_class(m.change_pct, true),
        ar = arrow(m.change_pct),
        p = pct_opt(m.change_pct),
        t = esc(m.label_trend)
    )
}

/// 都道府県別。順位と全国比は、見出しに意味を書いてから出す。
fn pref_table(d: &TitleDetail, w: &MinWages) -> String {
    let of = d.prefs.iter().filter_map(|p| p.of).max().unwrap_or(0);
    let mut h = format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">都道府県別（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         「順位」は 1 求人あたりに見た人が多い順です。<strong>1 位がいちばん集まりやすい県</strong>で、\
         {of} 位がいちばん集まりにくい県になります。\
         「全国比」は全国平均を 1.00 としたときの比です（0.59 なら全国の 59%）。\
         時給は求人票に書かれた金額の中央値で、取れていない県もあります。{wage_note}</p>\
         <div style=\"overflow-x:auto\"><table class=\"w-full text-sm\" style=\"min-width:980px\"><thead><tr>",
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
    for (i, p) in d.prefs.iter().enumerate() {
        h.push_str(&format!(
            "<tr><th scope=\"row\" class=\"{TD} font-normal\" style=\"text-align:left\">\
             <a class=\"text-blue-400 hover:underline\" href=\"#\" \
                hx-get=\"/tab/indeed/title/pref?name={qn}&pref={qp}\" \
                hx-target=\"#pref-open-{slot}\" hx-swap=\"innerHTML\" \
                title=\"この県の推移を開く\">{pf}</a></th>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{c}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{e}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{s}</td>\
             <td class=\"{TD} tabular-nums text-slate-400\" style=\"text-align:right\">{r}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{v}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{d}</td>\
             <td class=\"{TD} tabular-nums\" style=\"text-align:right\">{wg}</td>{extra}</tr>\
             <tr><td colspan=\"11\" class=\"p-0\">\
             <div class=\"pref-open\" id=\"pref-open-{slot}\"></div></td></tr>",
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
            qn = url_query(&d.title),
            qp = url_query(&p.prefecture),
            slot = i,
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
                <div style=\"overflow-x:auto\"><table class=\"w-full text-sm\">");
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
                <div style=\"overflow-x:auto\"><table class=\"w-full text-sm\">");
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

/// 枠に出すと「0.0%」としか読めない値か。
///
/// 表示は `{:.1}%` なので、0.05 未満はすべて「0.0%」になる。
/// 判定も同じ丸めで行う。生の 0.032 を「値がある」として枠に出すと、
/// 画面には 0.0% と出る。
fn displays_as_zero(v: Option<f64>) -> bool {
    matches!(v, Some(x) if (x * 10.0).round() == 0.0)
}

/// 求職者の傾向。
///
/// # 中身の無い枠は並べず、名前だけ断る
/// すぐ上の図（[`attr_section`]）は、線を引いても何も言えない区分を落として
/// 「…は全期間 0% なので出していません」と名前だけ挙げている。
/// ここも同じ考え方に揃える。落とす理由は 2 つある。
///
/// 1. 全期間 0% … 図にも最初から出ていない区分。実データの自動車設計では
///    主婦・主夫／学生／資格の 3 つが 14 か月ずっと 0% だった
/// 2. この月が 0% … 図では線として意味があるが、最新月 1 点のこの枠では
///    「0.0%」としか出ない。2026-08 のデータで 125 職種中 95 職種に残っていた
///
/// どちらも枠にはせず、名前を断り書きに出す。並びは図に合わせて
/// 直近の割合が大きい順にし、図の凡例と目で突き合わせられるようにする。
fn attrs_block(d: &TitleDetail, trend: &[crate::indeed::keywords::AttrMonth]) -> String {
    let Some(a) = &d.attrs else {
        return String::new();
    };
    // `shares` と `AttrMonth::pct` は同じ並びなので添字で照合できる
    let all_zero = |i: usize| {
        // 月次が読めていないときは判断材料が無い。落とすと黙って情報が消えるので残す
        !trend.is_empty()
            && trend
                .iter()
                .all(|m| m.pct.get(i).copied().flatten().unwrap_or(0.0) == 0.0)
    };
    let live: Vec<usize> = (0..a.shares.len()).filter(|i| !all_zero(*i)).collect();
    // 図と同じ並び（区分の定義順）で断り書きに出す
    let dead: Vec<&str> = (0..a.shares.len())
        .filter(|i| all_zero(*i))
        .map(|i| a.shares[i].0)
        .collect();

    // この月だけ 0% の区分も枠にしない。
    //
    // 図は 14 か月ぶんを線で見せるので「いまは 0 だが去年は出ていた」ことに意味がある。
    // ここは最新月 1 点の枠なので、同じ区分が「0.0%」としか出ない。
    // すぐ上の図が「全期間 0% の区分は出していません」と省いているのに、
    // この枠だけ 0.0% を並べるのは同じ画面で扱いが食い違う。
    //
    // 実データ（2026-08、`insight_kw_attr_trend`）では 125 職種のうち 95 職種に
    // この枠が残っていて、合計 202 枠あった。タクシードライバーは 7 枠のうち
    // 5 枠（主婦・主夫／外国人／未経験／語学／資格）が 0.0%、警備員も 3 枠のうち
    // 1 枠（学生）が 0.0% だった。落とした区分は図と同じく名前だけ断り書きに出す。
    let zero_now: Vec<&str> = live
        .iter()
        .filter(|i| displays_as_zero(a.shares[**i].1))
        .map(|i| a.shares[*i].0)
        .collect();
    let mut shown: Vec<usize> = live
        .iter()
        .copied()
        .filter(|i| !displays_as_zero(a.shares[*i].1))
        .collect();

    // 出す枠が 1 つも無いなら並べる意味が無い。1 文で済ませる
    if shown.is_empty() {
        return format!(
            "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">どんな人が探しているか（{m}）</h3>\
             <p class=\"text-slate-300 text-sm leading-relaxed\">{msg}</p></div>",
            m = esc(&a.month),
            // 一度も出てこなかったのか、この月だけ出てこなかったのかで言い方を変える。
            // 2026-08 のデータで前者は建具設計の 1 職種、後者は 0 職種
            msg = if live.is_empty() {
                "この職種では、どの区分も検索語に出てきませんでした。"
            } else {
                "この月は、どの区分も検索語に出てきませんでした。"
            }
        );
    }
    shown.sort_by(|x, y| {
        a.shares[*y]
            .1
            .unwrap_or(0.0)
            .total_cmp(&a.shares[*x].1.unwrap_or(0.0))
    });

    // 枠に出す数。下の for で `shown` を消費するので先に控える
    let shown_n = shown.len();
    let mut h = format!(
        "<div class=\"{CARD}\"><h3 class=\"text-slate-100 font-bold mb-1\">どんな人が探しているか（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-3 leading-relaxed\">\
         入力された語に、その言葉が含まれていた割合です。1 つの語が複数に当てはまることがあるので、\
         <strong>足しても 100% にはなりません</strong>。語の種類 {n} 件、流入 {c} 件が対象です。{dead}</p>\
         <div class=\"grid grid-cols-2 gap-3\">",
        m = esc(&a.month),
        n = a.terms.map(|v| v.to_string()).unwrap_or("—".into()),
        c = num_opt(a.total_clicks),
        // 上の図と同じ断り書き。枠が 8 つ無い理由をここでも書く。
        //
        // # まず数を出す
        // 枠の数は職種によって変わる（実データで 0〜6 枠）。数を先に書かないと、
        // 別の職種を見た人が「こちらは枠が少ない＝データが足りない画面」と読む。
        // 全部出ているときは書かない（「8 つのうち 8 つ」は読む意味が無い）。
        //
        // # 落とした理由は 2 つに分ける
        // 「一度も出てこなかった」と「この月は出てこなかった」は別の話
        dead = format!(
            "{count}{all_zero}{zero_now}",
            count = if shown_n < a.shares.len() {
                format!(
                    "区分は全部で {all} つあり、このうち {k} つを出しています。",
                    all = a.shares.len(),
                    k = shown_n
                )
            } else {
                String::new()
            },
            all_zero = if dead.is_empty() {
                String::new()
            } else {
                format!(
                    "{}は全期間 0% なので出していません。",
                    dead.iter().map(|s| esc(s)).collect::<Vec<_>>().join("・")
                )
            },
            zero_now = if zero_now.is_empty() {
                String::new()
            } else {
                format!(
                    "{}はこの月 0% だったので枠にしていません。",
                    zero_now.iter().map(|s| esc(s)).collect::<Vec<_>>().join("・")
                )
            }
        )
    );
    for i in shown {
        let (name, v) = &a.shares[i];
        h.push_str(&format!(
            "<div class=\"bg-navy-800/60 border border-slate-700 rounded p-3\">\
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

/// 広告枠の競合の多さを日本語にする。
///
/// # なぜ変換するか
/// 出どころは検索エンジンの広告側の区分で、`HIGH` / `MEDIUM` / `LOW` の 3 段階が
/// そのまま入っている。画面に英語の大文字を出さない方針なので、ここで言い換える。
/// 実データの内訳は LOW 93 行・MEDIUM 69 行・HIGH 21 行・空 2 行（`insight_search_trend`）。
///
/// 知らない値が来たら、隠さずそのまま出す。黙って「ふつう」に寄せると、
/// 区分が増えたときに気づけない。
fn competition_ja(v: &str) -> &str {
    match v {
        "HIGH" => "多い",
        "MEDIUM" => "ふつう",
        "LOW" => "少ない",
        "" => "—",
        other => other,
    }
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
         <div style=\"overflow-x:auto\"><table class=\"w-full text-sm\" style=\"min-width:640px\"><thead><tr>"
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
            cp = esc(competition_ja(&v.competition)),
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

    /// 片方にしか無い月を落とさない。
    ///
    /// # 何を防いでいるか
    /// 検索エンジン側の月だけで軸を作っていたため、Indeed 側の 2026-08 が
    /// 黙って落ちていた。本番で「8 月のデータはあるのに 7 月で止まっている」
    /// と指摘されて分かった（2026-09-15）。
    /// **どちらか一方にしか無い月**を必ず含むことを、データを使わずに確かめる。
    #[test]
    fn 軸は両方の月を落とさない() {
        let m = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // 実際に起きた形: 検索側が先に終わり、Indeed 側だけ 1 か月多い
        let 検索 = m(&["2026-05", "2026-06", "2026-07"]);
        let indeed = m(&["2026-06", "2026-07", "2026-08"]);
        assert_eq!(
            merged_axis(&検索, &indeed),
            m(&["2026-05", "2026-06", "2026-07", "2026-08"]),
            "片方にしか無い月が落ちている"
        );

        // 逆向き（Indeed 側が先に終わる）でも同じ
        assert_eq!(
            merged_axis(&indeed, &検索),
            m(&["2026-05", "2026-06", "2026-07", "2026-08"])
        );

        // 並びが崩れていても時系列に直す
        assert_eq!(
            merged_axis(&m(&["2026-07", "2026-05"]), &m(&["2026-06"])),
            m(&["2026-05", "2026-06", "2026-07"])
        );

        // 重複を増やさない
        assert_eq!(merged_axis(&検索, &検索), 検索);

        // 片方が空
        assert_eq!(merged_axis(&検索, &[]), 検索);
        assert_eq!(merged_axis(&[], &indeed), indeed);

        // 年をまたいでも時系列（文字列の並びが時系列になることの確認）
        assert_eq!(
            merged_axis(&m(&["2025-12"]), &m(&["2026-01", "2025-09"])),
            m(&["2025-09", "2025-12", "2026-01"])
        );
    }

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

    /// 掲示時給の図に出す 1 県ぶん。実データ（警備員・2026-08）の値を使う。
    ///
    /// 掲示時給は `insight_salary`（HOURLY・最新スナップショット）の中央値、
    /// 最低賃金は `v2_external_minimum_wage`。どちらも稼働中のサーバが
    /// 実際に描いた図から採った値で、こちらで作った推定値ではない。
    fn wage_row(pref: &str, wage: f64) -> PrefRow {
        row(pref, Some(wage))
    }

    /// 図の設定（`data-chart-config`）を取り出して読む。
    fn chart_config(html: &str) -> serde_json::Value {
        let cfg = html
            .split("data-chart-config='")
            .nth(1)
            .and_then(|x| x.split('\'').next())
            .expect("図が出ていない");
        serde_json::from_str(cfg).expect("図の JSON が壊れている")
    }

    /// 掲示時給の図は、上乗せ（掲示時給 − 最低賃金）の大きい順に並ぶ。
    ///
    /// # なぜ帯の長さ順ではないのか
    /// 下限を下回る県は差が負になるので、橙の帯だけ下へ行くほど長くなる。
    /// 帯の長さだけを追うと順番が読み取れないが、長さで並べ直すと
    /// 「+150 円の県」と「−150 円の県」が隣に来る。並びは符号つきのままにして、
    /// 意味は図の上の説明で伝える方針（`wage_gap_chart` のコメント参照）。
    #[test]
    fn 掲示時給の図は上乗せの大きい順に並ぶ() {
        let d = detail(vec![
            wage_row("大分県", 1496.700),
            wage_row("佐賀県", 1033.268),
            wage_row("京都府", 1108.287),
            wage_row("大阪府", 1159.002),
            wage_row("静岡県", 1067.784),
            wage_row("山形県", 983.626),
            wage_row("埼玉県", 1096.550),
            wage_row("神奈川県", 1176.598),
            wage_row("東京都", 1146.089),
            wage_row("徳島県", 904.197),
            wage_row("奈良県", 903.358),
        ]);
        let w = MinWages {
            by_pref: [
                ("大分県", 1024.0),
                ("佐賀県", 1030.0),
                ("京都府", 1122.0),
                ("大阪府", 1177.0),
                ("静岡県", 1097.0),
                ("山形県", 1023.0),
                ("埼玉県", 1141.0),
                ("神奈川県", 1225.0),
                ("東京都", 1226.0),
                ("徳島県", 1046.0),
                ("奈良県", 1051.0),
            ]
            .into_iter()
            .map(|(p, v)| (p.to_string(), v))
            .collect(),
            fiscal_year: Some(2025),
        };
        let h = wage_gap_chart(&d, &w);
        let v = chart_config(&h);

        // ECharts の縦軸は下から上なので、並びは逆順に入っている。
        // 上から読むと 大分(+472.7) → 佐賀(+3.3) → 京都(-13.7) → 大阪(-18.0) →
        // 静岡(-29.2) → 山形(-39.4) → 埼玉(-44.5) → 神奈川(-48.4) →
        // 東京(-79.9) → 徳島(-141.8) → 奈良(-147.6)
        let got: Vec<String> = v["yAxis"]["data"]
            .as_array()
            .expect("県が入っていない")
            .iter()
            .map(|x| x.as_str().unwrap_or_default().to_string())
            .collect();
        let want = [
            "奈良県", "徳島県", "東京都", "神奈川県", "埼玉県", "山形県",
            "静岡県", "大阪府", "京都府", "佐賀県", "大分県",
        ];
        assert_eq!(got, want, "上乗せの大きい順になっていない");

        // 横軸の左端は、いちばん低い値（奈良県の 903.358 円）を 100 円単位に切り下げた値。
        // 「900 始まり」に意味が無いことは図の上の説明で断る
        assert_eq!(v["xAxis"]["min"], 900, "横軸の左端が変わっている");
        assert!(
            h.contains("上乗せの大きい順"),
            "並び順の説明が無い"
        );
        assert!(
            h.contains("いちばん下が、下回る幅のいちばん大きい県"),
            "下端が何かの説明が無い"
        );
        assert!(
            h.contains("横軸は 0 円からではなく"),
            "横軸の左端についての断りが無い"
        );
        assert!(
            h.contains("左端の位置そのものに意味はありません"),
            "左端の読み方が書かれていない"
        );
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
        assert!(html.contains("全国の 59%"), "全国比が比だという断りが無い");
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
            // 変化が出せない職種（月が欠けている）で「—」と出したうえに
            // 「ほぼ横ばいです」と断定していた。出せないことを言う
            let dir = match ch {
                None => "この期間の変化は、月が欠けているため出せません。",
                Some(c) if c < -5.0 => "集まりにくさが強まっています。求人票か媒体を見直す候補です。",
                Some(c) if c > 5.0 => "集まりやすくなっています。競合が引いた可能性があります。",
                _ => "集まりやすさはこの期間ほぼ横ばいです。",
            };
            items.push(format!(
                "<li>いま求人 <strong>{}</strong> 件、1 求人あたりに見た人は <strong>{}</strong> 人。\
                 {}{}</li>",
                num_opt(Some(j)),
                dec1_opt(Some(sp)),
                match ch {
                    Some(_) => format!("1 求人あたりはこの期間で {}。", pct_opt(ch)),
                    None => String::new(),
                },
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
        // 県が少ない職種だと上位 3 と下位 3 に同じ県が入る。
        // 「集まりやすいのは愛知県、集まりにくいのは愛知県」になるので出さない
        let overlap = easy.iter().any(|x| hard.contains(x));
        if !overlap {
        items.push(format!(
            "<li>人が集まりやすいのは <strong>{}</strong>、集まりにくいのは <strong>{}</strong>。\
             同じ求人票でも、県によって手応えが変わります。</li>",
            easy.join("・"),
            hard.join("・")
        ));
        }
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
                "<li><strong>{}で、掲示時給の中央値が最低賃金を下回っています</strong>{}。\
                 業務委託や基本給だけの掲示が混ざるとこうなります。\
                 提示額を決めるときは、この相場をそのまま使わないでください。</li>",
                // 1 県のときに「1 県で…（熊本県など）」と出ていた。
                // 少ないときは県名をそのまま書く
                if below.len() <= 3 {
                    below.iter().map(|s| esc(s)).collect::<Vec<_>>().join("・")
                } else {
                    format!("{} 県", below.len())
                },
                if below.len() <= 3 {
                    String::new()
                } else {
                    format!(
                        "（{}など）",
                        below
                            .iter()
                            .take(3)
                            .map(|s| esc(s))
                            .collect::<Vec<_>>()
                            .join("・")
                    )
                }
            ));
        }
    }

    if items.is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"bg-blue-900/50 border-l-4 border-blue-400 rounded-r-lg p-4\">\
         <h3 class=\"text-blue-300 text-base font-bold mb-2\">この職種で言えること</h3>\
         <ul class=\"text-slate-200 text-sm leading-relaxed list-disc pl-4 space-y-2\">{}</ul>\
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
/// 前者は「そもそもこの仕事を求職者がどれだけいるか」、
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
/// 2 つの系列の月を合わせて、時系列に並べた軸を返す。
///
/// # なぜ要るのか
/// 片方の月だけで軸を作ると、もう片方にしか無い月が**黙って落ちる**。
/// 実際に起きた: 検索エンジン側は 2026-07 までしか返さないのに、その月並びを
/// そのまま軸にしていたため、Indeed 側の 2026-08 が描かれなかった。
/// 同じページの他の図が 8 月まで出ているのに、この図だけ 7 月で終わっていた。
///
/// 月は "YYYY-MM" なので、文字列のまま並べれば時系列になる。
fn merged_axis(a: &[String], b: &[String]) -> Vec<String> {
    let mut v: Vec<String> = a.to_vec();
    for m in b {
        if !v.contains(m) {
            v.push(m.clone());
        }
    }
    v.sort();
    v
}

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
    // 軸は「両方の月をあわせたもの」にする。
    //
    // # なぜ片方の月だけで作ってはいけないか
    // 以前は検索エンジン側の月（`sn.months`）をそのまま軸にしていた。
    // 検索エンジン側は 2026-07 までしか返さないため、**Indeed 側の 2026-08 は
    // 置き場所が無く、黙って落ちていた**。同じページの他の図が 8 月まで
    // 描いているのに、この図だけ 7 月で終わっていた（本番で実測）。
    // この図の説明文は「重なっていない区間では線は 1 本だけです」と書いており、
    // 左側（検索だけある月）では守られていたが右側では守られていなかった。
    //
    // 両方の月を合わせて並べ、それぞれ値のある月にだけ置く。
    // 月は "YYYY-MM" なので、文字列のまま並べれば時系列になる。
    let axis: Vec<String> = merged_axis(&sn.months, months);
    let idx: std::collections::HashMap<&str, usize> = months
        .iter()
        .enumerate()
        .map(|(i, m)| (m.as_str(), i))
        .collect();
    let sidx: std::collections::HashMap<&str, usize> = sn
        .months
        .iter()
        .enumerate()
        .map(|(i, m)| (m.as_str(), i))
        .collect();
    let indeed: Vec<Option<f64>> = axis
        .iter()
        .map(|m| idx.get(m.as_str()).and_then(|i| o.ctk.series.get(*i).copied().flatten()))
        .collect();
    // 検索側も、合わせた軸に置き直す
    let search: Vec<Option<f64>> = axis
        .iter()
        .map(|m| sidx.get(m.as_str()).and_then(|i| sn.series.get(*i).copied().flatten()))
        .collect();
    // 2 つを区別する。軸を両方の月の和集合にしたので、
    // 「Indeed にある月」と「両方にある月」は同じ数にならない。
    // 相関も向きの差も**両方そろっている月**の上でしか計算できないので、
    // 本文に出す数も overlap のほうを使う。
    let indeed_months = indeed.iter().filter(|v| v.is_some()).count();
    let overlap = axis
        .iter()
        .enumerate()
        .filter(|(i, _)| indeed[*i].is_some() && search[*i].is_some())
        .count();
    if overlap < 6 {
        return String::new();
    }
    // 実数のまま使う。以前はここで両方を指数にしていたが、その基準は
    // 「Indeed 側にデータがある最初の月」だった。取得できる月が 1 つ動くだけで
    // 両系列の値が全部ずれ、先月出した図と比べられなくなる。
    // 相関は尺度によらないので、指数をやめても下の r は変わらない。
    let g = search.clone();
    let i2 = indeed.clone();
    if g.iter().all(|x| x.is_none()) || i2.iter().all(|x| x.is_none()) {
        return String::new();
    }

/// 検索の関心と Indeed 内の閲覧が、重なる期間でどちらへ動いたかを返す。
///
/// 返すのは `(検索の変化率%, 閲覧の変化率%, 丸め 1 段の%)`。
/// **測れないときは `None`** で、呼び出し側は数字を出さない。
///
/// # なぜ前月比の向きを数えないのか
/// 最初は「前月比で向きが違った月の割合」を考えたが、実データで成立しない。
/// 検索エンジン側は 110 / 170 / 210 / 260 / 320 / 390 のような**粗い刻みの推定値**しか
/// 返さず、13 か月の重なりのうち **12 回中 3〜10 回が前月と同値**になる。
/// 実測では横ばいの内訳が「検索側 20 回 / Indeed 側 0 回」で、
/// 割合の分母が職種ごとに 2〜9 とばらついた。清掃スタッフの「乖離 0%」は
/// **n=2 の上に乗っていた**。これは向きではなく丸めの粒度を測っている。
///
/// # 代わりに何を見るか
/// 重なる区間の**両端 3 か月ずつの平均**を比べ、期間全体でどちらへ動いたかを出す。
/// 3 か月にするのは、端の 1 か月が刻みの境目に当たると符号が反転するため。
///
/// # 測れない条件
/// 検索側の変化が「丸め 1 段」に届かないものは出さない。1 段は実際に現れた
/// 隣り合う値の最小の比で決める（実測で約 13〜24%）。この関門で、
/// 手元の 15 職種のうち 12 職種は数字が出ない。**出ないことが正しい**。
fn divergence(search: &[Option<f64>], views: &[Option<f64>]) -> Option<(f64, f64, f64)> {
    let pairs: Vec<(f64, f64)> = search
        .iter()
        .zip(views.iter())
        .filter_map(|(a, b)| match (a, b) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some((*x, *y)),
            _ => None,
        })
        .collect();
    // 両端 3 か月ずつを重ならせないため 6 か月以上を要求する
    const K: usize = 3;
    if pairs.len() < K * 2 {
        return None;
    }
    let mean = |v: &[(f64, f64)], f: fn(&(f64, f64)) -> f64| {
        v.iter().map(f).sum::<f64>() / v.len() as f64
    };
    let head = &pairs[..K];
    let tail = &pairs[pairs.len() - K..];
    let (ha, hb) = (mean(head, |p| p.0), mean(head, |p| p.1));
    let (ta, tb) = (mean(tail, |p| p.0), mean(tail, |p| p.1));
    if ha <= 0.0 || hb <= 0.0 {
        return None;
    }
    let a_pct = (ta / ha - 1.0) * 100.0;
    let b_pct = (tb / hb - 1.0) * 100.0;

    // 検索側の「丸め 1 段」= その期間に実際に現れた隣り合う値の最小の比
    let mut v: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if v.len() < 2 {
        return None; // 1 種類しか出ていない＝刻みを測れない
    }
    let mut step = f64::INFINITY;
    for w in v.windows(2) {
        if w[0] > 0.0 {
            step = step.min((w[1] / w[0] - 1.0) * 100.0);
        }
    }
    if !step.is_finite() || a_pct.abs() < step {
        return None;
    }
    Some((a_pct, b_pct, step))
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
    // 期間全体でどちらへ動いたか。測れない職種では数字を出さない
    let gap_html = match divergence(&g, &i2) {
        Some((a, b, _)) => format!(
            "<div class=\"mt-3 mb-1 border border-slate-600 rounded-lg p-3\">\
             <p class=\"text-slate-400 text-xs mb-2\">重なる {ov} か月で、\
             両端 3 か月ずつの平均を比べています。</p>\
             <div class=\"flex flex-wrap gap-4 items-baseline\">\
             <span class=\"text-slate-300 text-sm\">検索エンジンでの関心 \
             <strong class=\"{ca} text-base\">{aa} {a:+.1}%</strong></span>\
             <span class=\"text-slate-300 text-sm\">Indeed で見られた数 \
             <strong class=\"{cb} text-base\">{ab} {b:+.1}%</strong></span>\
             <span class=\"text-slate-100 text-sm font-bold\">{mk}・差 {d:.0} ポイント</span>\
             </div></div>",
            ov = overlap,
            a = a,
            b = b,
            d = (a - b).abs(),
            aa = arrow(Some(a)),
            ab = arrow(Some(b)),
            ca = dir_class(Some(a), true),
            cb = dir_class(Some(b), true),
            mk = if a.signum() == b.signum() { "同じ向き" } else { "逆の向き" },
        ),
        None => format!(
            "<div class=\"mt-3 mb-1 border border-slate-700 rounded-lg p-3\">\
             <p class=\"text-slate-400 text-sm leading-relaxed\">\
             <strong>この職種は、向きの差を数字にできません。</strong>\
             検索エンジン側は粗い刻みの推定値しか返さず、重なる {ov} か月のあいだの\
             検索数の動きが、その刻み 1 段ぶんに届いていないためです。\
             刻みの中の上下を差として出すと、実際には無い動きを読むことになります。\
             上の図で形だけを見てください。</p></div>",
            ov = overlap
        ),
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
         量が比べられないので<strong>左右で別々の目盛り</strong>にしています。\
         2 本が交わる位置や上下関係に意味はありません。見るのは形だけです。\n         <strong>縦軸は 0 から始まっていません。</strong>動いた形が見えるように、\n         その期間の値を包む範囲だけを映しています。目盛り 1 本ぶんの大きさは\n         軸の数字で確かめてください。</p>{chart}{gap_html}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed\">月ごとの上下について: {say}</p>\
         <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">\
         <strong>青い線が階段状なのは、検索エンジン側が粗い刻みで報告するため</strong>です\
         （実際に「5,600」「4,400」のような値しか返りません）。細かい上下は刻みの影響で、\
         意味のある動きではありません。長い期間の傾きだけを見てください。<br>\
         <strong>2 本の期間は揃っていません。</strong>左の {y} か月に対し、\
         Indeed 側は右端の {n} か月ぶんしかありません。重なっていない区間では線は 1 本だけです。<br>\
         検索エンジン側は職種名の 1 語だけで、Indeed 側のような語ごとの内訳は取れません。\
         そのため「どの言葉で探しているか」の比較はできず、動きの向きだけを見ています。</p></div>",
        t = esc(&d.title),
        y = sn.months.len(),
        n = indeed_months,
        chart = dual_line_chart(
            &axis,
            ("検索エンジンでの検索数", &g, "回"),
            ("Indeed で求人を見た人数", &i2, "人"),
            true,
            340
        ),
        gap_html = gap_html,
        say = say
    )
}

/// [`line_chart`] が描く縦軸に、単位の名前を足す。
///
/// # なぜ後から差し込むのか
/// この画面のほかの図は縦軸に単位が入っている（`raw_line_chart` は `unit` を
/// 受け取り、`dumbbell_chart` は「円」を出す）。ところが `render.rs::line_chart`
/// だけは単位を受け取らない作りで、「探し方の言葉の移り変わり」と
/// 「求職者の内訳の移り変わり」の 2 枚が、目盛りの数字だけの図になっていた。
/// 0〜50 と出ていても、それが % なのか件数なのか図の中に手がかりが無い。
/// `render.rs` は他のタブも使うので、こちら側で軸名だけを足す。
///
/// # 単位は実データで確かめた
/// * 検索語 … `insight_kw_term_monthly.share_pct` を 1 か月ぶん足すと 97.5〜99.4
///   （事務の 14 か月で実測）。0〜1 の比ではなく % だった
/// * 属性 … `insight_kw_attr_trend.pct_*` は全 1,477 行で最大 88.6（`pct_condition`）。
///   8 区分は排他ではないので合計は 100 にならないが、列ごとの単位は % だった
///
/// # 差し込めなかったときは
/// 目印が見つからなければ元の図をそのまま返す（軸名が無いだけで図は出る）。
/// 黙って劣化しないように警告を残し、`縦軸に単位が入る` テストが落ちるようにしてある。
fn with_y_unit(chart: String, unit: &str) -> String {
    // `render.rs::line_chart` が出す yAxis の書き出し。この直後に名前を挟む
    const ANCHOR: &str = r#""yAxis":{"type":"value","scale":true,"#;
    if !chart.contains(ANCHOR) {
        tracing::warn!(
            "縦軸に単位を入れられませんでした。render.rs::line_chart の形が変わっています"
        );
        return chart;
    }
    // 色は同じ図の軸ラベルと合わせる（`render.rs::axis_color(true)` の値）。
    // ECharts の既定色は濃い灰で、暗い背景では読めない
    chart.replacen(
        ANCHOR,
        &format!(
            r##""yAxis":{{"type":"value","scale":true,"name":"{u}","nameTextStyle":{{"color":"#94a3b8","fontSize":10,"align":"left"}},"nameGap":8,"##,
            u = json_str(unit)
        ),
        1,
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
        chart = with_y_unit(line_chart(months, &lines, true, 340), "%"),
        u = say(up),
        d = say(down)
    )
}

/// 求職者の内訳の月次推移。
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
         求職者の内訳の移り変わり</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed\">\
         検索語に出てくる言葉を 8 つに区分し、<strong>月ごとの割合</strong>を出しています。\
         単位は %。{dead}</p>{chart}\
         <p class=\"text-slate-500 text-xs mt-2 leading-relaxed\">\
         この職種は 1 か月あたりの検索語が中央値で {tc} 語しかありません。\
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
        chart = with_y_unit(line_chart(&months, &lines, true, 340), "%"),
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
            None,
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
    // 上乗せ（掲示時給 − 最低賃金）の大きい順。符号つきのまま並べる。
    //
    // # 帯の長さでは並んでいない
    // 下限を下回る県はこの差が負になる。図では下回る分を橙の帯として
    // 長さ |差| で描くので、青の帯は上へ行くほど長く、橙の帯は下へ行くほど長い。
    // 帯の長さだけを追うと、真ん中でいったん 0 になってまた伸びる形に見え、
    // 「並び順が読み取れない」という読み方になる。
    //
    // 長さ（＝差の絶対値）で並べ直すことも考えたが、それだと「+150 円の県」と
    // 「−150 円の県」が隣に来る。上乗せがあるのか下回っているのかは正反対の話で、
    // 隣り合わせるほうが誤解を招く。並びは符号つきの差のままにして、
    // 代わりに並びの意味を図の上の説明に書いた。
    //
    // 実データ（警備員・46 県。掲示時給は `insight_salary` の HOURLY 中央値、
    // 最低賃金は Turso の `v2_external_minimum_wage`。稼働中のサーバが返した
    // 図の設定から採った実値）:
    //   大分 +472.7 → 岩手 +297.9 → …（37 県）… → 佐賀 +3.3 →
    //   京都 −13.7 → 大阪 −18.0 → 静岡 −29.2 → 山形 −39.4 → 埼玉 −44.5 →
    //   神奈川 −48.4 → 東京 −79.9 → 徳島 −141.8 → 奈良 −147.6
    // 46 県すべてで差は単調に減っている（下回るのは 9 県）。
    //
    // それでも「並び順が読み取れない」と見えるのは、橙の帯だけ下へ行くほど
    // 長くなるため。帯の長さを目で読んだ観測では、並びの順番そのものは合っていたが
    // 埼玉 −44.5 が「85」、山形 −39.4 が「55」と取られていた。
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
         並びは<strong>上乗せの大きい順</strong>です。上から下へ上乗せが小さくなり、\
         下限を下回る県（オレンジ）が下にまとまります。そのため\
         <strong>いちばん下が、下回る幅のいちばん大きい県</strong>になります。\
         横軸は 0 円からではなく、いちばん低い値の手前から始めています。\
         読むのは帯の長さで、左端の位置そのものに意味はありません。\
         時給で出ている求人だけが対象で、月給・日給の求人は含みません。\n         {cover}</p>{chart}\
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
        cover = if rows.len() >= 47 {
            String::new()
        } else {
            format!(
                "そのため出ている県は 47 県のうち <strong>{} 県</strong>です。",
                rows.len()
            )
        },
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

    /// 折れ線の縦軸に単位を出す。
    ///
    /// この 2 枚だけが目盛りの数字しか無い図になっていた。0〜50 と出ていても、
    /// それが % なのか件数なのか図の中に手がかりが無い。
    /// 単位は実データで確かめた（`with_y_unit` のコメント参照）。
    ///
    /// 差し込み先は `render.rs::line_chart` が出す JSON なので、
    /// 向こうの形が変わったらこのテストで気づけるようにしておく。
    #[test]
    fn 縦軸に単位が入る() {
        let months: Vec<String> = (7..=12).map(|m| format!("2025-{m:02}")).collect();
        let terms = vec![
            series("事務", &[45.3, 46.4, 46.3, 44.8, 41.4, 40.8]),
            series("一般事務", &[5.3, 4.9, 5.0, 7.4, 10.1, 7.7]),
        ];
        let attrs: Vec<AttrMonth> = (0..7)
            .map(|i| month(&format!("2025-{:02}", i + 7), 5.0, 30.0, 80.0))
            .collect();

        for (name, h) in [
            ("探し方の言葉の移り変わり", term_series_section(&months, &terms)),
            ("求職者の内訳の移り変わり", attr_section(&attrs)),
        ] {
            let cfg = h
                .split("data-chart-config='")
                .nth(1)
                .and_then(|x| x.split('\'').next())
                .unwrap_or_else(|| panic!("{name} の図が出ていない"));
            let v: serde_json::Value = serde_json::from_str(cfg)
                .unwrap_or_else(|e| panic!("{name} の図の JSON が壊れた: {e}"));
            assert_eq!(v["yAxis"]["name"], "%", "{name} の縦軸に単位が無い");
            // 軸ラベルと同じ色にする。ECharts の既定色は暗い背景で読めない
            assert_eq!(
                v["yAxis"]["nameTextStyle"]["color"], v["yAxis"]["axisLabel"]["color"],
                "{name} の軸名が軸ラベルと違う色になっている"
            );
        }
    }
    /// 母数を出して、1 か月の上下で判断しないよう書く。
    #[test]
    fn 母数と読み方の注意を出す() {
        let v: Vec<AttrMonth> = (0..7)
            .map(|i| month(&format!("2025-{:02}", i + 7), 5.0, 30.0, 42.0))
            .collect();
        let h = attr_section(&v);
        assert!(h.contains("中央値で 42 語"), "母数が出ていない");
        assert!(h.contains("12.2 ポイント") && h.contains("3.1 ポイント"), "振れ幅の実測が出ていない");
    }
}

#[cfg(test)]
mod detail_display_tests {
    use super::*;
    use crate::indeed::detail::{Attrs, SearchVolume};
    use crate::indeed::keywords::AttrMonth;

    /// 8 区分ぶんの月。`live` に入れた添字だけ値を持たせる。
    fn attr_month(m: &str, live: &[(usize, f64)]) -> AttrMonth {
        let mut pct = [Some(0.0); 8];
        for (i, v) in live {
            pct[*i] = Some(*v);
        }
        AttrMonth {
            month: m.to_string(),
            pct,
            term_count: Some(27.0),
        }
    }

    /// 最新月の枠。`live` に入れた添字だけ値を持たせる。
    fn attrs(live: &[(usize, f64)]) -> Attrs {
        let mut shares: Vec<(&'static str, Option<f64>)> = [
            "働き方の条件",
            "シニア",
            "主婦・主夫",
            "学生",
            "外国人",
            "未経験",
            "語学",
            "資格",
        ]
        .iter()
        .map(|n| (*n, Some(0.0)))
        .collect();
        for (i, v) in live {
            shares[*i].1 = Some(*v);
        }
        Attrs {
            month: "2026-08".to_string(),
            total_clicks: Some(1200.0),
            terms: Some(27),
            shares,
        }
    }

    fn detail_with_attrs(a: Attrs) -> TitleDetail {
        TitleDetail {
            title: "自動車設計".to_string(),
            attrs: Some(a),
            ..Default::default()
        }
    }

    /// 全期間 0% の区分は枠ごと出さない。
    ///
    /// すぐ上の図は同じ区分を線ごと落として「全期間 0% なので出していません」と
    /// 断っている。ここだけ 8 枠を並べると同じ画面で扱いが食い違う。
    /// 実データの自動車設計は 14 か月ぶんで主婦・主夫／学生／資格がずっと 0%、
    /// 最新月 2026-08 では 8 枠中 7 枠が 0.0% になっていた。
    #[test]
    fn 全期間ゼロの区分は枠ごと出さない() {
        // 働き方の条件・シニア・外国人・未経験・語学は期間中どこかで値がある
        let trend: Vec<AttrMonth> = vec![
            attr_month("2026-06", &[(0, 16.2), (1, 2.7), (5, 2.8)]),
            attr_month("2026-07", &[(0, 20.3), (1, 2.7), (4, 1.7), (5, 1.1), (6, 1.1)]),
            attr_month("2026-08", &[(0, 15.2)]),
        ];
        let d = detail_with_attrs(attrs(&[(0, 15.2)]));
        let h = attrs_block(&d, &trend);

        assert!(h.contains("働き方の条件"), "値のある区分が消えている");
        assert!(h.contains("シニア"), "期間中に値のある区分を落としている");
        for dead in ["主婦・主夫", "学生", "資格"] {
            let frames = h.matches(dead).count();
            // 断り書きに 1 回だけ出る。枠としては出さない
            assert_eq!(frames, 1, "{dead} の枠が残っている");
        }
        assert!(h.contains("全期間 0% なので出していません"), "落とした理由が書いていない");
    }

    /// 全部ゼロなら枠を並べず 1 文で済ませる。
    #[test]
    fn どの区分も出てこなければ一文にする() {
        let trend: Vec<AttrMonth> = vec![attr_month("2026-08", &[])];
        let d = detail_with_attrs(attrs(&[]));
        let h = attrs_block(&d, &trend);
        assert!(h.contains("どの区分も検索語に出てきませんでした"));
        assert!(!h.contains("0.0%"), "枠を並べたままになっている");
    }

    /// 枠の数が職種によって変わる理由を、枠の上に書く。
    ///
    /// 実データでは枠の数が 0〜6 とばらつく（125 職種）。数を書かないと、
    /// 枠が少ない職種を見た人が「この画面はデータが足りない」と読んでしまう。
    /// 全部出ているときは書かない。
    #[test]
    fn 出している区分の数を書く() {
        // 実データの自動車設計。主婦・主夫／学生／資格は 14 か月ずっと 0%、
        // 最新月 2026-08 は働き方の条件 15.2% だけで、残る 4 区分は 0%
        let trend: Vec<AttrMonth> = vec![
            attr_month("2026-06", &[(0, 16.2), (1, 2.7), (5, 2.8)]),
            attr_month("2026-07", &[(0, 20.3), (1, 2.7), (4, 1.7), (5, 1.1), (6, 1.1)]),
            attr_month("2026-08", &[(0, 15.2)]),
        ];
        let d = detail_with_attrs(attrs(&[(0, 15.2)]));
        let h = attrs_block(&d, &trend);
        assert!(
            h.contains("区分は全部で 8 つあり、このうち 1 つを出しています。"),
            "出している区分の数が書かれていない"
        );

        // 8 区分すべてに値があるときは、数を書かない（読む意味が無い）
        let all = [
            (0, 30.0), (1, 5.0), (2, 4.0), (3, 3.0),
            (4, 2.0), (5, 1.5), (6, 1.0), (7, 0.5),
        ];
        let trend: Vec<AttrMonth> = vec![attr_month("2026-08", &all)];
        let d = detail_with_attrs(attrs(&all));
        let h = attrs_block(&d, &trend);
        assert!(!h.contains("区分は全部で"), "全部出ているのに数を書いている");
        assert!(!h.contains("出していません") && !h.contains("枠にしていません"));
    }
    /// この月だけ 0% の区分も枠にしない。
    ///
    /// 図は 14 か月ぶんの線なので「いまは 0 でも去年は出ていた」ことに意味があるが、
    /// この枠は最新月 1 点なので「0.0%」としか出ない。すぐ上の図が
    /// 「全期間 0% の区分は出していません」と省いているのに、ここだけ 0.0% を
    /// 並べると同じ画面で扱いが食い違う。
    ///
    /// 実データ（2026-08）のタクシードライバーは 7 枠のうち 5 枠が 0.0% だった。
    #[test]
    fn この月だけゼロの区分も枠にしない() {
        // 主婦・主夫と資格は過去の月には出ている。学生などは全期間 0%
        let trend: Vec<AttrMonth> = vec![
            attr_month("2026-06", &[(0, 14.7), (1, 4.7), (2, 1.9), (7, 0.6)]),
            attr_month("2026-07", &[(0, 14.5), (1, 4.9), (2, 0.4), (7, 0.2)]),
            attr_month("2026-08", &[(0, 14.7), (1, 4.7), (7, 0.032)]),
        ];
        let d = detail_with_attrs(attrs(&[(0, 14.7), (1, 4.7), (7, 0.032)]));
        let h = attrs_block(&d, &trend);

        // 「10.0%」にも "0.0%" が含まれるので、枠の中身そのものの形で見る
        assert!(!h.contains(">0.0%<"), "0.0% の枠が残っている");
        assert!(h.contains("14.7%") && h.contains("4.7%"), "値のある枠が消えている");
        assert!(
            h.contains("この月 0% だったので枠にしていません"),
            "落とした理由が書かれていない"
        );
        // 落とした区分は名前だけ断り書きに 1 回だけ出す
        for n in ["主婦・主夫", "資格"] {
            assert_eq!(h.matches(n).count(), 1, "{n} の枠が残っている");
        }
        assert!(h.contains("全期間 0% なので出していません"), "全期間 0% の断りが消えている");
    }

    /// 月次が読めなくても、区分の名前は黙って消さない。
    ///
    /// 全期間 0% かどうかは月次が無いと判断できないので、そちらの断りは出さない。
    /// ただし「この月 0%」はその月の値だけで分かるので、枠にはせず名前を出す。
    #[test]
    fn 月次が無くても名前は消さない() {
        let d = detail_with_attrs(attrs(&[(0, 15.2)]));
        let h = attrs_block(&d, &[]);
        for name in ["働き方の条件", "シニア", "主婦・主夫", "学生", "資格"] {
            assert!(h.contains(name), "{name} が黙って消えている");
        }
        assert!(!h.contains("全期間 0% なので出していません"));
        // 「10.0%」にも "0.0%" が含まれるので、枠の中身そのものの形で見る
        assert!(!h.contains(">0.0%<"), "0.0% の枠が残っている");
    }

    /// 件数のカードは整数で出す。
    ///
    /// 共通の `metric_card` は 1000 未満を小数第 1 位にする。1 求人あたりの
    /// 人数では正しいが、実データの自動車設計（求人 937 件・企業 307 社）では
    /// 「937.0」「307.0」になり、同じ画面の本文の「937 件」と食い違っていた。
    #[test]
    fn 件数のカードは整数で出す() {
        let months: Vec<String> = ["2026-07", "2026-08"].iter().map(|s| s.to_string()).collect();
        let o = Overview::from_series(
            "自動車設計",
            &crate::indeed::data::Series {
                job: vec![Some(950.0), Some(937.0)],
                ctk: vec![Some(4000.0), Some(4047.0)],
                emp: vec![Some(310.0), Some(307.0)],
            },
            &months,
        );
        let h = count_card(&o.job) + &count_card(&o.emp);
        assert!(h.contains(">937<"), "求人数が整数で出ていない");
        assert!(h.contains(">307<"), "企業数が整数で出ていない");
        assert!(!h.contains("937.0") && !h.contains("307.0"), "小数のまま出ている");
        // 比のカードは小数第 1 位のままであること
        assert!(metric_card(&o.spp, true).contains("4.3"), "1 求人あたりが整数に丸まっている");
    }

    /// 競合の多さは日本語で出す。
    #[test]
    fn 競合の多さは日本語で出す() {
        let vols = ["HIGH", "MEDIUM", "LOW"]
            .iter()
            .map(|c| SearchVolume {
                variant: "job".to_string(),
                avg_monthly: Some(140),
                latest: Some(90),
                yoy_pct: Some(-18.0),
                competition: c.to_string(),
                low_bid_yen: Some(229.9),
                high_bid_yen: Some(677.6),
            })
            .collect();
        let d = TitleDetail {
            title: "自動車設計".to_string(),
            volumes: vols,
            ..Default::default()
        };
        let h = volume_block(&d);
        for w in ["多い", "ふつう", "少ない"] {
            assert!(h.contains(w), "{w} が出ていない");
        }
        for w in ["HIGH", "MEDIUM", "LOW"] {
            assert!(!h.contains(w), "{w} が英語のまま残っている");
        }
        // 取れていない行は空欄ではなく欠測として出す
        assert_eq!(competition_ja(""), "—");
    }

    /// 戻り先は面と県と並べ替えを持つ。
    ///
    /// 素の `/tab/indeed` に戻すと面が「全体」に、県が全国に、
    /// 並べ替えが求人数の多い順に落ちる。
    /// この画面へは職種の一覧からしか来ないので view=titles が戻り先。
    #[test]
    fn 戻り先は面と県を持つ() {
        let d = TitleDetail {
            title: "自動車設計".to_string(),
            category: "技術".to_string(),
            month: "2026-08".to_string(),
            ..Default::default()
        };
        let w = MinWages {
            by_pref: Default::default(),
            fiscal_year: None,
        };
        let h = render(
            &d,
            None,
            &[],
            &w,
            &[],
            &[],
            &[],
            None,
            None,
            Some("東京都"),
            Some("mobile"),
        );
        assert!(h.contains("view=titles"), "戻り先が面を持っていない");
        assert!(
            h.contains("pref=%E6%9D%B1%E4%BA%AC%E9%83%BD"),
            "戻り先が県を持っていない"
        );
        assert!(h.contains("&sort=mobile"), "戻り先が並べ替えを持っていない");
        assert!(h.contains("hx-push-url=\"true\""), "URL が更新されない");

        // 全国・並べ替え無しのときは余計なものを付けない
        let h = render(&d, None, &[], &w, &[], &[], &[], None, None, None, None);
        assert!(h.contains("view=titles"));
        assert!(!h.contains("&pref="), "全国なのに県が付いている");
        assert!(!h.contains("&sort="), "選んでいない並べ替えが付いている");
    }

    /// 断り書きの画面からも、面・県・並べ替えを持って戻れる。
    ///
    /// 職種名を打ち間違えた URL を開いたときなど、ここからしか一覧に戻れない。
    #[test]
    fn 断り書きからの戻り先も持ち物を落とさない() {
        let h = note(
            "「あ」のデータが見つかりませんでした。",
            &back_href(Some("東京都"), Some("mobile")),
        );
        assert!(h.contains("view=titles"));
        assert!(h.contains("pref=%E6%9D%B1%E4%BA%AC%E9%83%BD"));
        assert!(h.contains("&sort=mobile"));
        assert!(h.contains("hx-push-url=\"true\""));
    }

    /// 一覧のリンクに `sort` が無くても、htmx のヘッダから拾う。
    ///
    /// 2026-09 時点の `tab.rs` は詳細へのリンクに `name` と `pref` しか付けない。
    /// それでも「スマホで探されている順」で見ていた人が戻れるようにする。
    #[test]
    fn 並べ替えはヘッダからも拾う() {
        let mut h = HeaderMap::new();
        h.insert(
            "hx-current-url",
            "http://127.0.0.1:8080/tab/indeed?view=titles&sort=mobile&pref=%E6%9D%B1%E4%BA%AC%E9%83%BD"
                .parse()
                .unwrap(),
        );
        assert_eq!(sort_from_hx_url(&h).as_deref(), Some("mobile"));

        // 一覧以外の画面から来たものは持ち込まない
        let mut h = HeaderMap::new();
        h.insert(
            "hx-current-url",
            "http://127.0.0.1:8080/tab/analysis?sort=mobile".parse().unwrap(),
        );
        assert_eq!(sort_from_hx_url(&h), None);

        // 詳細の URL を直接開いた場合。並べ替えは入っていない
        let mut h = HeaderMap::new();
        h.insert(
            "hx-current-url",
            "http://127.0.0.1:8080/tab/indeed/title?name=%E4%BA%8B%E5%8B%99"
                .parse()
                .unwrap(),
        );
        assert_eq!(sort_from_hx_url(&h), None);

        // ヘッダが無いとき（htmx を通らない素のアクセス）
        assert_eq!(sort_from_hx_url(&HeaderMap::new()), None);
    }

    /// 並べ替えの値は形を絞ってからリンクに載せる。
    ///
    /// `HX-Current-URL` は呼び出し側が自由に書けるヘッダなので、
    /// 拾った値をそのまま href に入れない。
    #[test]
    fn 知らない形の並べ替えは載せない() {
        for bad in [
            "\"><script>",
            "MOBILE",
            "mobile1",
            "verylongsortkeyname_that_is_not_real",
        ] {
            let mut h = HeaderMap::new();
            let Ok(v) = format!("http://x/tab/indeed?sort={bad}").parse() else {
                continue;
            };
            h.insert("hx-current-url", v);
            assert_eq!(sort_from_hx_url(&h), None, "{bad} を通している");
        }

        // 後ろに別のパラメータが続いても、sort の値だけを取る
        let mut h = HeaderMap::new();
        h.insert(
            "hx-current-url",
            "http://x/tab/indeed?sort=mobile&pref=x".parse().unwrap(),
        );
        assert_eq!(sort_from_hx_url(&h).as_deref(), Some("mobile"));
    }
}

/// 同梱の実データで確かめる。作り物のデータだけだと、
/// 「この職種ではこうなる」という思い込みのまま通ってしまう。
#[cfg(test)]
mod real_data_tests {
    use super::*;
    use crate::db::local_sqlite::LocalDb;
    use crate::handlers::helpers::get_str;
    use std::path::Path;

    const DB: &str = "data/indeed_insights.db";
    const GZ: &str = "data/indeed_insights.db.gz";

    /// 同梱の gz から実体を用意する。本番の起動時と同じ手順
    /// （`indeed::keywords` のテストと同じ）。
    fn open_db() -> LocalDb {
        assert!(Path::new(GZ).exists(), "{GZ} がありません");
        crate::ensure_db_from_gz(DB);
        LocalDb::new(DB).expect("Indeed 分析 DB を開けませんでした")
    }

    fn titles(db: &LocalDb) -> Vec<String> {
        db.query(
            "SELECT DISTINCT norm_title FROM insight_kw_attr_trend ORDER BY norm_title",
            &[],
        )
        .expect("職種名を読めませんでした")
        .iter()
        .map(|r| get_str(r, "norm_title"))
        .collect()
    }

    fn chart_config(html: &str) -> Option<serde_json::Value> {
        let cfg = html
            .split("data-chart-config='")
            .nth(1)?
            .split('\'')
            .next()?;
        serde_json::from_str(cfg).ok()
    }

    /// 「どんな人が探しているか」に 0.0% の枠を出さない。
    ///
    /// この検査を入れる前は、125 職種のうち 95 職種に 0.0% の枠が残っていて、
    /// 合計 202 枠あった（タクシードライバーは 7 枠中 5 枠）。
    #[test]
    fn 実データのどの職種にも0パーセントの枠が無い() {
        let db = open_db();
        let mut checked = 0;
        for t in titles(&db) {
            let Ok(Some(d)) = detail::load(&db, &t) else {
                continue;
            };
            let trend = crate::indeed::keywords::attr_months(&db, &t)
                .expect("属性の内訳を読めませんでした");
            let h = attrs_block(&d, &trend);
            if h.is_empty() {
                continue;
            }
            // 「10.0%」にも "0.0%" が含まれるので、枠の中身そのものの形で見る
            assert!(
                !h.contains(">0.0%<"),
                "{t} に 0.0% の枠が残っています"
            );
            checked += 1;
        }
        assert!(checked >= 100, "{checked} 職種しか見ていません（125 のはず）");
    }

    /// 2 つの折れ線の縦軸に、実データでも単位が入る。
    ///
    /// 差し込み先は `render.rs::line_chart` が出す JSON なので、
    /// 向こうの形が変われば軸名が消える。実データの全職種で確かめる。
    #[test]
    fn 実データでも折れ線の縦軸に単位が入る() {
        let db = open_db();
        let mut terms_checked = 0;
        let mut attrs_checked = 0;
        for t in titles(&db) {
            let (months, series) = crate::indeed::keywords::term_monthly(&db, &t)
                .expect("検索語の月次を読めませんでした");
            let h = term_series_section(&months, &series);
            if !h.is_empty() {
                let v = chart_config(&h)
                    .unwrap_or_else(|| panic!("{t}: 探し方の言葉の図の JSON が壊れています"));
                assert_eq!(v["yAxis"]["name"], "%", "{t}: 縦軸に単位が無い");
                terms_checked += 1;
            }
            let attrs = crate::indeed::keywords::attr_months(&db, &t)
                .expect("属性の内訳を読めませんでした");
            let h = attr_section(&attrs);
            if !h.is_empty() {
                let v = chart_config(&h)
                    .unwrap_or_else(|| panic!("{t}: 求職者の内訳の図の JSON が壊れています"));
                assert_eq!(v["yAxis"]["name"], "%", "{t}: 縦軸に単位が無い");
                attrs_checked += 1;
            }
        }
        assert!(
            terms_checked >= 50 && attrs_checked >= 50,
            "図が出た職種が少なすぎます（検索語 {terms_checked} / 属性 {attrs_checked}）"
        );
    }
}
