//! 社内タブ `/tab/indeed`。
//!
//! 顧客レポートとの違いは「どこまで見せるか」だけで、数字は同じものを使う。
//! ここでは分解（なぜそうなったか）と、言い切れない部分まで出す。
//!
//! # 本文の行長
//! 日本語は 1 行 35〜45 字あたりが読みやすい。1280px では本文が 85 字、
//! 注記が 100 字あり、行を折り返すたびに目が左端を探し直す長さになっていた。
//! text-sm(14px) の段落に `max-w-xl`(576px = 約 41 字)、
//! text-xs(12px) の注記に `max-w-lg`(512px = 約 42 字) を掛けてある。
//! 全角は font-size と同じ幅なので、字数は幅 ÷ font-size でそのまま出る。
//! どちらも tailwind-precompiled.css に実在するクラス。
//! 表・図・帯には掛けない（列や軸は幅いっぱい使うほうが読みやすい）。

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::Html,
};
use serde::Deserialize;

use super::render::{
    arrow, bar_line_chart, category_table_html, dec1_opt, dir_class, esc, hbar_chart, metric_card,
    num_opt, pct_opt, raw_line_chart, scatter_chart, small_multiples, url_query, vbar_chart,
};
use crate::indeed::aggregate::{
    category_table_at, nation_overview, pref_overview, pref_title_overviews, Overview,
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
    session: tower_sessions::Session,
    Query(q): Query<TabQuery>,
) -> Html<String> {
    // 都道府県は画面いちばん上の絞り込み（セッションに入る）を使う。
    // このアプリの他タブと同じ流儀。タブ内にもう 1 つ置いていたのをやめた
    let session_pref: String = session
        .get(crate::auth::SESSION_PREFECTURE_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
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
    // クエリの pref はリンクから来たとき用。無ければセッションの値を使う。
    //
    // クエリで来た県は、**セッションにも書き戻す**。表示にだけ使って書かないと
    //   - ヘッダのプルダウン（セッション由来）と画面の県が食い違う
    //   - 再読込した瞬間に、URL の県ではなくセッションの県に黙って変わる
    //     （東京都 122 行の URL を開いて F5 → 全国 126 行）
    // という状態になる。人に URL を送って見てもらう使い方と噛み合わない。
    // URL を開くことも「県を選んだ」とみなす。
    //
    // 「全国に戻す」は ?pref=all で明示する。
    // 空の ?pref= はセッションの県にフォールバックするので、URL では全国に戻せなかった。
    // 東京都を見ている人に全国の URL を送っても、相手のセッションに県が残っていれば
    // その県が出る。47 都道府県に "all" という名前は無いので、県名と取り違えない。
    let want_all = q.pref.as_deref() == Some("all");
    let pref = if want_all {
        None
    } else {
        q.pref
            .as_deref()
            .filter(|x| !x.is_empty())
            .or(Some(session_pref.as_str()))
            .filter(|x| !x.is_empty())
    };
    // 全国に戻すときはセッションにも空を書き戻す。書かないと、再読込した瞬間に
    // セッションの県へ黙って戻り、画面いちばん上のプルダウンとも食い違う。
    let to_store: Option<String> = if want_all {
        Some(String::new())
    } else {
        q.pref
            .as_deref()
            .filter(|x| !x.is_empty())
            .map(str::to_string)
    };
    if let Some(v) = to_store {
        if v != session_pref {
            if let Err(e) = session.insert(crate::auth::SESSION_PREFECTURE_KEY, v).await {
                tracing::warn!("県をセッションに書けませんでした: {e}");
            }
        }
    }
    Html(render_tab(
        snap,
        pref,
        q.sort.as_deref(),
        q.view.as_deref(),
        &seasons,
    ))
}

fn degraded(msg: &str) -> String {
    format!(
        "{DIRECT_ACCESS_GUARD}<div class=\"p-6\"><div class=\"bg-navy-800/60 border border-amber-500 rounded-lg p-4 text-amber-300\">{}</div></div>",
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
    ("people", "求職者"),
];

fn view_of(v: Option<&str>) -> &str {
    match v {
        Some(x) if VIEWS.iter().any(|(k, _)| *k == x) => x,
        _ => "overview",
    }
}

/// 左の列。面の選択と、いま見ている面の節を 1 本にまとめる（試作 2026-09-24）。
///
/// # なぜ節だけの並びでは足りなかったか
/// 最初は「面の中の節へ飛ぶ並び」として作った。実測すると:
///
/// ```text
/// 面          高さ    画面数  節  図  表の行  文字
/// 全体       2139px   2.4    4   4      0   1896
/// 職種       5978px   6.6    2   1    127   7175   ← いちばん長いのに節が最少
/// 業界・分類  4035px   4.5    3   6     29   4065
/// 求職者     1708px   1.9    2   2      0    781
/// ```
///
/// 長さと節の数が逆だった。いちばん辛い「職種」は節が 2 つで、
/// 中身は 127 行の表 1 枚。節へ飛ぶ並びでは何も解決しない。
/// また節が 3 つ未満の面では並びが出ないので、面を移るたびに
/// 左の列が出たり消えたりして、画面の幅まで変わっていた。
///
/// # 作り
/// 上の帯（媒体分析／キーワード需要／求人票作成／調べる）は機能の選択。
/// その中の「どの面を見るか」は詳細の選択なので、左に移す。
/// 左の列は常に 4 面ぶん出るので、面を移っても幅が変わらない。
///
/// ```text
/// ┌─────────┬────────────────────────┐
/// │ 見るもの  │                         │
/// │ ● 全体   │   選んだ面の中身          │
/// │   ├ 要点 │                         │
/// │   ├ なぜ │                         │
/// │   └ 動き │                         │
/// │ ○ 職種   │                         │
/// │ ○ 業界   │                         │
/// │ ○ 求職者 │                         │
/// └─────────┴────────────────────────┘
/// ```
///
/// # 節は「飛ぶ」だけで「隠さない」
/// 隠すと `offsetHeight === 0` になり app.js が ECharts を作らない
/// （`0 !== t.offsetHeight` の判定）。表示は全部出したまま、印へ飛ばす。
///
/// # 使えるクラスだけ
/// `pl-3` は配布 CSS に無い（あるのは pl-2 / pl-4 / pl-6）。
/// `hover:bg-slate-600` `hover:text-slate-200` は dashboard.css 側に定義がある。
/// 節を押したあとの URL を、人に送れる形にする script。
///
/// # なぜ要るか
/// `?tab=` は読み込み直後に `history.replaceState` で消される
/// （`templates/dashboard_inline.html`。履歴を汚さないため）。
/// どのタブを見ていたかは `sessionStorage` が覚えているので画面としては困らない。
///
/// ただし**その URL を人に送ると困る**。実測（2026-09-26）:
///
/// ```text
/// 節を押した後の URL   /#sec-1
/// 別の文脈で開くと      媒体分析が出る（既定のタブ）
///                     sec-1 の印が無いので hash は空振り
/// ```
///
/// 節へ飛べるようにした以上、押したあとの URL は送れる形であるべきなので、
/// 押したときだけ `?tab=` を書き戻す。
///
/// # pushState ではなく replaceState
/// 節を 5 つ見て戻るときに 5 回戻らされるのは煩わしい。節の行き来は履歴に積まない。
fn sec_link_script(tab_url: &str) -> String {
    let t = serde_json::to_string(tab_url).unwrap_or_else(|_| "\"/tab/indeed\"".to_string());
    format!(
        "<script>(function(){{var t={t};\
         document.addEventListener('click',function(e){{\
         var a=e.target&&e.target.closest?e.target.closest('a.indeed-sec-link'):null;\
         if(!a)return;var h=a.getAttribute('href')||'';if(h.charAt(0)!=='#')return;\
         setTimeout(function(){{try{{history.replaceState(history.state,'',\
         '/?tab='+encodeURIComponent(t)+h);}}catch(_){{}}}},0);}});}})();</script>"
    )
}

fn side_nav(
    current: &str,
    pref: Option<&str>,
    sort: Option<&str>,
    sections: &[(String, String)],
) -> String {
    let mut h = String::from(
        "<nav class=\"indeed-sidenav\" aria-label=\"見るもの\">\
         <div>\
         <div class=\"indeed-nav-head text-slate-400 text-xs px-2 pb-1\">見るもの</div>",
    );
    // 県はここ 1 か所だけに出す。左の列は sticky なので、
    // 職種の一覧 127 行のどこを見ていても画面に残る。
    h.push_str(&format!(
        "<div class=\"indeed-nav-head flex items-center gap-2 px-2 pb-2\">\
         <span class=\"px-2 py-0.5 rounded bg-blue-500/20 text-blue-300 text-sm font-bold\">{w}</span>\
         <span id=\"indeed-loading\" class=\"htmx-indicator text-slate-400 text-xs\">読み込み中…</span></div>",
        w = esc(pref.unwrap_or("全国"))
    ));
    for (key, label) in VIEWS {
        let on = key == current;
        let cls = if on {
            "px-3 py-2 text-sm font-bold text-blue-300 bg-navy-700 rounded"
        } else {
            "px-3 py-2 text-sm text-slate-300 hover:bg-slate-600 hover:text-slate-200 rounded"
        };
        let mut q = format!("?view={key}");
        if let Some(p) = pref.filter(|x| !x.is_empty()) {
            q.push_str(&format!("&pref={}", url_query(p)));
        }
        if let Some(x) = sort.filter(|x| !x.is_empty()) {
            q.push_str(&format!("&sort={}", url_query(x)));
        }
        h.push_str(&format!(
            "<a class=\"{cls}\" href=\"/tab/indeed{q}\" hx-get=\"/tab/indeed{q}\" \
             hx-target=\"#content\" hx-swap=\"innerHTML show:top\" hx-push-url=\"true\" \
             hx-indicator=\"#indeed-loading\" aria-current=\"{ac}\">{l}</a>",
            ac = if on { "page" } else { "false" },
            l = esc(label)
        ));
        // いま見ている面の下にだけ、節をぶら下げる
        if on {
            for (id, s) in sections {
                h.push_str(&format!(
                    "<a class=\"indeed-sec-link ml-2 pl-2 border-l border-slate-600 px-2 py-1 text-xs \
                     text-slate-400 hover:bg-slate-600 hover:text-slate-200 rounded truncate\" \
                     href=\"#{id}\">{s}</a>",
                    id = esc(id),
                    s = esc(s)
                ));
            }
        }
    }
    h.push_str("</div></nav>");
    h
}

/// 面を切り替える帯。県と並べ替えを持ち回る。
fn view_tabs(current: &str, pref: Option<&str>, sort: Option<&str>) -> String {
    // 面の切り替えと「いまどこを見ているか」を 1 本の帯にまとめ、画面上部に貼る。
    //
    // 以前は帯の下に「いま見ているのは 東京都（変えるには画面いちばん上の…）」という
    // 別行があった。同じ事実がヘッダのプルダウン・パンくず・この行・各見出しの
    // 4 か所に散り、食い違うと画面に矛盾が出ていた。
    // 加えて、この行は画面上部 457px にしか無いので、職種の表 122 行のうち
    // 113 行（93%）では「東京都」の文字が画面のどこにも無い状態だった。
    // 帯に寄せて sticky にすれば、1 か所で、かつスクロールしても見える。
    //
    // クラスは static/css/tailwind-precompiled.css に実在するものだけを使う。
    // 最初 z-20 / bg-navy-900/95 / backdrop-blur を指定したが 3 つとも precompiled に
    // 無く、実測すると z-index=auto、背景 rgba(0,0,0,0) だった。sticky 自体は効くので
    // 透明な帯の下を本文が素通りする状態になる。用意があるのは z-10 / z-50 /
    // bg-navy-900 / shadow-md。
    //
    // 地色は bg-navy-900 をやめた。body も bg-navy-900 (#0d1525) なので、帯と本文が
    // **まったく同じ色**だった。shadow-md は黒 10% でこの暗さでは見えず、
    // 下罫線の border-slate-700 (#334155) も地に対して 1.76:1 で境目にならない。
    // 本文が帯の下に潜っても、どこからが帯なのか分からない状態だった。
    //
    // 使うのは bg-slate-700 (#334155)。カードを bg-navy-700 (#1e293b) に寄せたので、
    // 帯まで navy-700 にすると**今度は帯とカードが同じ色**になり、同じ問題が
    // body からカードに移るだけになる。slate-700 なら body に対して 1.76:1、
    // カードに対して 1.41:1 で、どちらの上に来ても 1 段明るい面として残る。
    // 下罫線は slate-400 (#94a3b8) を直に指定する。帯に対して 4.04:1、
    // カードに対して 5.71:1 で、どちらから見ても 3:1 を超える
    // （UI 部品の境界は WCAG 1.4.11 で 3:1）。precompiled に border-slate-400 が
    // 無いので色だけ style 属性で上書きしている（太さは border-b のまま）。
    // 帯は畳んだ。面の選択は左の列へ移り、残ったのは「全国」の札ひとつだけで、
    // 同じ事実が画面上部の「現在の表示: 全国」にも出ていた（実測 y=192 と y=444）。
    // 高さ 25px の空の棒が 1 本増えるだけだったので消す。
    // 県と読み込み表示は左の列の頭へ移した（左の列は sticky なので、
    // 職種の一覧 127 行を送っても見え続ける。帯と同じ役目を果たす）。
    let mut h = String::new();
    h.push_str(&format!(
        "<input type=\"hidden\" name=\"view\" value=\"{}\">",
        esc(current)
    ));
    // 並べ替えの hx-include が拾う先。タブ内の県プルダウンを撤去したときに
    // 参照先が消え、並べ替えるたびに URL から pref が落ちていた。
    // 自分の画面はセッションに県が残るので気づけないが、その URL を人に送ると
    // 相手には全国が出る（東京都 122 行 → 全国 126 行）。
    if let Some(p) = pref.filter(|x| !x.is_empty()) {
        h.push_str(&format!(
            "<input type=\"hidden\" name=\"pref\" value=\"{}\">",
            esc(p)
        ));
    }
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
    //
    // 職種数は見ている地域のもの。以前は県を選んでも全国の 126 のままで、
    // 一覧の注記だけ「全 86 職種」に追随していたため、同じ画面に
    // 126 / 104 / 86 の 3 つが並んでいた。
    let (n_titles, n_complete) = match pref {
        None => (snap.titles.len(), snap.complete_titles()),
        Some(p) => {
            let rows = crate::indeed::aggregate::pref_rows(snap, p);
            let complete = rows
                .iter()
                .filter(|r| r.series.job.iter().all(|v| v.is_some()))
                .count();
            (rows.len(), complete)
        }
    };
    h.push_str(&format!(
        "<div>\
         <h2 class=\"text-2xl font-bold text-gray-100\">Indeed 採用市場（社内用）</h2>\
         <p class=\"text-slate-400 text-sm mt-1\">{period}／{n} 職種・{np}・{src}</p>\
         {sample}</div>",
        period = esc(&format!(
            "{} 〜 {}",
            months.first().map(String::as_str).unwrap_or("—"),
            snap.meta.latest
        )),
        n = n_titles,
        // 合計に入っている職種の数を必ず書く。母集団が月で変わると比べられない
        sample = {
            let nc = n_complete;
            let part = n_titles - nc;
            if part == 0 {
                String::new()
            } else {
                format!(
                    "<p class=\"text-slate-400 text-xs mt-1 leading-relaxed max-w-lg\">\
                     合計は、全期間そろっている {nc} 職種で出しています。\
                     残り {part} 職種は月が欠けているため、下の一覧には出しますが合計には入れていません\
                     （母集団が月によって変わると、先月比が実態と関係なく動くためです）。</p>"
                )
            }
        },
        np = match pref {
            None => format!("{} 都道府県", prefs.len()),
            Some(p) => esc(p),
        },
        src = esc(&snap.meta.source),
    ));

    // 但し書きは最初に出す。後ろに置くと読まれない
    h.push_str(&format!(
        "<div class=\"bg-navy-700 border-l-4 border-slate-500 rounded p-3 max-w-2xl\"><p class=\"text-slate-300 text-sm leading-relaxed\">{}</p></div>",
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
        // 間を内側より広くする。gap-3(12px) は内側の p-4(16px) より狭く、
        // 罫線もカード地に対して 1.74:1 でほぼ見えないため、5 枚が 1 本の帯に
        // 見えていた（ux-visual の実測）
        h.push_str("<div class=\"grid grid-cols-2 lg:grid-cols-5 gap-6\">");
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
            // 文字だけの箱なので、箱ごと絞る。カードを全幅のまま中の文だけ絞ると、
            // 1280px で右に 600px 以上の空白が残って間延びして見える。
            "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5 max-w-2xl\">\
         <h3 class=\"text-slate-100 text-lg font-bold mb-2\">なぜそうなったか（数字の内訳）</h3>\
         <p class=\"text-slate-300 text-sm leading-relaxed\">{}</p>\
         <p class=\"text-slate-400 text-xs mt-3 leading-relaxed max-w-lg\">\
         1 求人あたり = 求人を見た人数 ÷ 求人の数。求人の数 = 募集した企業の数 × 1 社あたりの本数。\
         この 2 つの内訳で説明は終わりです。これ以上は推測になります。</p></div>",
            esc(&overview.why())
        ));

        // 「なぜ」を図でも見せる。求人（棒）が増えると 1 求人あたり（線）が薄まる、
        // という関係は、別々の図に分けると読み手が頭の中で重ねることになる
        h.push_str(&format!(
        "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\n         <h3 class=\"text-slate-100 text-lg font-bold mb-1\">求人の数と、1 求人あたりに見た人数</h3>\n         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed max-w-lg\">\n         棒が求人の数（左軸）、線が 1 求人あたりに見た人数（右軸）です。\n         棒が伸びた月に線が下がっていれば、求人が増えて 1 件あたりの取り分が薄まったことになります。</p>{chart}</div>",
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
        "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
         <h3 class=\"text-slate-100 text-lg font-bold mb-1\">{name}の動き</h3>\
         <p class=\"text-slate-400 text-xs mb-2 max-w-lg\">実数です。求人数と見た人数は 40 倍ほど桁が違うので、重ねずに並べています。</p>\
         {chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed max-w-xl\">{s1}</p>\
         <p class=\"text-slate-300 text-sm mt-1 leading-relaxed max-w-xl\">{s2}</p>{bd}</div>",
        bd = crate::handlers::indeed::render::breakdown_html(
            crate::indeed::aggregate::growth_breakdown(&overview.job.series, &overview.emp.series)
        ),
        name = esc(&overview.name),
        chart = small_multiples(
            months,
            &[
                ("求人の数".to_string(), overview.job.series.clone(), "件", 0),
                ("求人を見た人数".to_string(), overview.ctk.series.clone(), "人", 2),
                ("募集している企業の数".to_string(), overview.emp.series.clone(), "社", 3),
            ],
            true,
            200
        ),
        s1 = esc(&overview.job.sentence),
        s2 = esc(&overview.spp.sentence)
    ));
    }

    // 業界（全国のみ。県で絞ると 1 業界あたりの月次が薄くなる）
    if view == "industry" {
        h.push_str(&industry_section(snap, months, pref));
    }

    // 職種の位置取り（全国のみ。県で絞ると点が薄くなる）
    if view == "titles" {
        h.push_str(&scatter_section(snap, pref));
    }

    // 季節の波（全国のみ。県別の検索ボリュームは持っていない）
    if view == "people" {
        if pref.is_none() {
            h.push_str(&season_section(seasons));
        } else {
            // 検索ボリュームは全国ぶんしか無い（geo_id が 1 種類）。
            // 県を選ぶと黙って消えていたので、出せない理由を書く
            h.push_str(
                // 見出しに「全国のみ」を入れる。県を選んだ人がこの面を開いた時点で
                // 分かるようにする。本文の 1 段目は 2 行に収め、取っていない理由の
                // 内訳（259 字あった）は下の小さい注記に送る。
                // 文字だけの箱なので箱ごと絞る
                "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5 max-w-2xl\">\
                 <h3 class=\"text-slate-100 text-lg font-bold mb-1\">1 年のうち、いつ動くか（全国のみ）</h3>\
                 <p class=\"text-slate-400 text-sm leading-relaxed\">\
                 <strong>この図は全国のぶんしか出せません。</strong>元にしている検索エンジンの\
                 検索ボリュームを、日本全体としてしか取得していないためです。\
                 県を「全国」に戻すと出ます。</p>\
                 <p class=\"text-slate-400 text-xs mt-2 leading-relaxed max-w-lg\">\
                 県別に取ること自体はできます（確認済み）。取っていないのは量が足りないためです。\
                 配送ドライバー（全国で月 1,300 回と多いほう）でも東京都 210 回・鳥取県 10 回で、\
                 6 月は鳥取県が 0 でした。全国の中央値は月 210 回なので、大半の職種は県別にすると\
                 0 か 10 しか返りません。その量では月ごとの動きが読めません。</p></div>",
            );
        }
    }

    // スマホ比率（職種そのものの性質なので、県で絞っても同じ値）
    if view == "people" {
        h.push_str(&mobile_section(snap, pref));
    }

    // 分類
    if view == "industry" {
        let cats = category_table_at(snap, pref);
        let top: Vec<(String, Overview)> = cats
            .iter()
            // 6 本。7 本だと、色だけでは見分けにくい 6 組を線種 3 種類で
            // さばききれず、分類 → 色/線種 の割り当てが成立しない
            // （`render::CATEGORY_STYLE` のコメント参照）
            .take(6)
            .filter_map(|c| {
                snap.by_category
                    .get(&c.name)
                    .map(|s| (c.name.clone(), Overview::from_series(&c.name, s, months)))
            })
            .collect();
        let refs: Vec<(String, &Overview)> = top.iter().map(|(n, o)| (n.clone(), o)).collect();
        h.push_str(&format!(
            "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
             <h3 class=\"text-slate-100 text-lg font-bold mb-1\">分類ごとの求人数（上位 6）</h3>\
             <p class=\"text-slate-400 text-xs mb-2 max-w-lg\">実数です。6 つの差は 4 倍ほどなので、そのまま重ねられます。</p>\
             {chart}</div>",
            chart = raw_line_chart(
                months,
                &refs
                    .iter()
                    .map(|(n, o)| (n.clone(), o.job.series.clone()))
                    .collect::<Vec<_>>(),
                true,
                320,
                "件"
            )
        ));
        // 上の業界の表と「変化」の基準が違うことを、表の前に書く。
        //
        // 業界の表は先月比・前年同月比（2 か月だけの素の比）で、実データでは全行マイナス。
        // この表の「求人数の変化」「見た人数の変化」「その変化」は期間全体
        //（最初の月から最新月までを、月ごとの上下をならした線で見た変化）で全行プラス。
        // 同じ「変化」に見えて逆を向く。列名は render.rs 側にあって触れないので、
        // 見出しの下に基準を書いておく。
        // この表の変化はどれも「期間を通して」のもので、上の業界の表の「先月比（直近 1 か月）」
        // とは別のものを測っている。どちらも要る数字なので基準は揃えず、見出しで期間を示す。
        // 列名そのもの（「求人数の変化」など）は render.rs 側にあって触れない。
        //
        // 罫線の上書きについて: render.rs の td は border-slate-800 (#1e293b) で、
        // カードを bg-navy-700 (#1e293b) に寄せたことで**地とまったく同じ色**になり
        // 行の区切りが消える。ここだけ色を差し替える（本筋は render.rs 側の修正）。
        h.push_str(&format!(
            "<style>#indeed-cat-table tbody td{{border-bottom-color:#334155}}
             #indeed-cat-table thead th{{border-bottom-color:#64748b}}#indeed-cat-table tbody td:last-child{{white-space:nowrap}}</style>\
             <div id=\"indeed-cat-table\" class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
             <h3 class=\"text-slate-100 text-lg font-bold mb-1\">分類別の定点表（{n} 分類・変化はすべて期間を通して）</h3>\
             <p class=\"text-slate-400 text-xs mb-3 leading-relaxed max-w-lg\">\
             この表の変化は {first} から {latest} までを通して見たものです。\
             上の業界の表の「先月比（直近 1 か月）」とは別のものを測っています。</p>{tbl}</div>",
            n = cats.len(),
            first = esc(months.first().map(String::as_str).unwrap_or("—")),
            latest = esc(&snap.meta.latest),
            tbl = category_table_html(&cats, true)
        ));
    }

    // 職種の一覧
    if view == "titles" {
        h.push_str(&title_section(snap, pref, sort));
    }

    h.push_str("</div>");

    // 面の中の節に id を振り、左の並びから飛べるようにする（2026-09-24 試作）。
    //
    // 生成側の 11 か所を書き換えず、出来上がりを 1 回通す。
    // 節が 3 つ未満の面では並びを出さない（1 本の列のまま）。
    let (h, sections) = crate::handlers::indeed::render::add_section_ids(&h);
    let nav = side_nav(view, pref, sort, &sections);
    // 節を押したあとの URL に載せる「いまの面」。side_nav のリンクと同じ形にする。
    let mut tab_url = format!("/tab/indeed?view={view}");
    if let Some(p) = pref.filter(|x| !x.is_empty()) {
        tab_url.push_str(&format!("&pref={}", url_query(p)));
    }
    if let Some(x) = sort.filter(|x| !x.is_empty()) {
        tab_url.push_str(&format!("&sort={}", url_query(x)));
    }
    let script = sec_link_script(&tab_url);
    format!(
        "<div class=\"indeed-with-nav flex gap-4\">{nav}\
         <div class=\"flex-1 min-w-0\">{h}</div></div>{script}"
    )
}

fn pref_selector(prefs: &[String], current: Option<&str>) -> String {
    let mut s = String::from(
        "<select class=\"bg-navy-800 border border-slate-600 text-slate-100 rounded px-3 py-2 text-sm\" \
         hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML show:top\" name=\"pref\" hx-trigger=\"change\" hx-include=\"[name='sort'],[name='view']\" \n         hx-push-url=\"true\" hx-indicator=\"#indeed-loading\" aria-label=\"都道府県\">",
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
        note: "スマホからの検索が多い職種を上にしています。求人ページと応募フォームをどちらに合わせるかの手がかりです。県を選ぶと、その県での比率になります。",
    },
];

fn sort_spec(key: Option<&str>) -> &'static SortSpec {
    let k = key.unwrap_or("size");
    SORTS.iter().find(|s| s.key == k).unwrap_or(&SORTS[0])
}

fn sort_selector(current: &SortSpec) -> String {
    let mut s = String::from(
        "<select class=\"bg-navy-800 border border-slate-600 text-slate-100 rounded px-3 py-2 text-sm\" \
         hx-get=\"/tab/indeed\" hx-target=\"#content\" hx-swap=\"innerHTML show:top\" name=\"sort\" hx-trigger=\"change\" \
         hx-include=\"[name='pref'],[name='view']\" \n         hx-push-url=\"true\" hx-indicator=\"#indeed-loading\" aria-label=\"並べ替え\">",
    );
    for o in SORTS.iter() {
        s.push_str(&format!(
            "<option value=\"{k}\"{sel}>{l}</option>",
            k = o.key,
            l = esc(o.label),
            sel = if o.key == current.key {
                " selected"
            } else {
                ""
            }
        ));
    }
    s.push_str("</select>");
    // 都道府県は画面上のセレクト（name="pref"）を hx-include で拾う。
    // ここに隠しフィールドを足すと同じ名前が 2 つになり、二重に送られる。
    s
}

/// 職種一覧の検索欄（入力欄・件数・0 件メッセージ）。
///
/// # なぜ画面側で絞るのか
/// 125 行を上から目で追うのは現実的でない。並べ替えは 8 種類あるが、
/// 「この職種を見たい」という探し方には効かない。
/// サーバーに投げ直すと並べ替えと県の選択を持ち回る必要が出るので、
/// 出ている表をその場で隠す。件数も出して、何行に絞れたか分かるようにする。
///
/// 動かすほうは [`SEARCH_BOX_SCRIPT`]。**表より後ろ**に出す。
const SEARCH_BOX: &str = r#"<div class="mt-3 mb-2 flex items-center gap-3">
<input id="indeed-title-find" data-indeed-find type="search" oninput="indeedFilterTitles(this.value, this)"
 placeholder="職種名・分類・業界で絞り込み"
 class="flex-1 px-3 py-1.5 bg-navy-900 border border-slate-700 rounded text-sm text-slate-100 placeholder-slate-500 focus:border-blue-500 focus:outline-none">
<span id="indeed-title-count" data-indeed-count class="text-slate-400 text-xs tabular-nums"></span></div>
<p id="indeed-title-empty" data-indeed-empty class="text-amber-300 text-sm mb-2"></p>"#;

/// 検索欄を動かす script。
///
/// # 置き場所は表の後ろ
/// 入力欄のすぐ下に置くと、素の読み込みでは**表がまだ組み立てられていない**うちに走る。
/// 実測では、絞り込みの語を当て直したのに数えられた行が 0 で、件数が「0 / 0 職種」と出て
/// 122 行はそのまま全部見えていた。表の後ろに出せば、どの経路でも行がそろってから走る。
///
/// # 描き直しごとに違う印（token）を振る理由
/// htmx が #content を入れ替えている最中は、**古いカードと新しいカードが両方**
/// DOM にある。その状態でこの script が走るので、document 全体を数えると
///   tr[data-find] が 244 行（122 行の 2 倍）
///   入力欄・件数・0 件メッセージが 2 個ずつ
/// になる。実測（東京都で「ドライバー」）では、表示 8 行・DOM 122 行なのに
/// 件数が「16 / 244 職種」と出て、並べ替えを何回繰り返しても倍のままだった。
/// 0 件のときは「当てはまる職種はありません」が消えるほうの控えに書かれ、
/// 画面には列見出しだけの空の表が残っていた。
///
/// そこでカードに 1 回きりの印を付け、script は**自分の印のカードだけ**を見る。
/// 「新しいほうが DOM の先頭に入る」といった htmx の実装の順番に頼らずに済む。
const SEARCH_BOX_SCRIPT: &str = r#"<script>
// 絞り込みの語は、並べ替えをまたいでも保つ。
//
// 並べ替えは表ごと取り直すので、素直に書くと入力欄が空になり、126 行が黙って全部出てくる。
// 「ドライバーで絞ってから増えた順で見る」という、この面でいちばん自然な使い方が
// 毎回リセットされていた。語は表の外（window）に置いて、描き直したあとに当て直す。
//
// 数えるのは自分のカードの中だけ。document 全体を見ると、入れ替え中に残っている
// 古い表まで数えて件数が倍になる（この定数の上のコメント参照）。
function indeedFilterTitles(q, el) {
  var root = el && el.closest ? el.closest('[data-indeed-titles]') : null;
  if (!root) return;
  var rows = root.querySelectorAll('tr[data-find]');
  var needle = (q || '').trim().toLowerCase();
  window._indeedFind = q || '';
  var shown = 0;
  rows.forEach(function (r) {
    var hit = !needle || (r.getAttribute('data-find') || '').toLowerCase().indexOf(needle) >= 0;
    r.style.display = hit ? '' : 'none';
    if (hit) shown += 1;
  });
  var c = root.querySelector('[data-indeed-count]');
  if (c) c.textContent = needle ? shown + ' / ' + rows.length + ' 職種' : '';
  // 0 件のときは列見出しだけの空の表が残り、壊れたように見える。
  //
  // 出し分けは**文字を入れるかどうかだけ**で行う。style も class も触らない。
  // htmx は差し替えのとき、新旧で同じ id を持つ要素の**属性を元に戻す**
  // （attribute settling）。実測では、この script が style を消した 24ms 後に
  // htmx がサーバーの返した style="display:none" を書き戻しており、
  // メッセージだけが消えていた。件数の文字は属性ではないので生き残る、という
  // 食い違い方をしていたのはこのため。実機で古い要素から id を外すと現象が消えることも
  // 確認済み（scratchpad/uxtab/exp.py）。属性を触らなければ戻されようがない。
  // 見せる／隠すは、文字が入っているかどうかで決まる :empty の CSS に任せる。
  var e = root.querySelector('[data-indeed-empty]');
  if (e) {
    e.textContent = (needle && shown === 0)
      ? '「' + q + '」に当てはまる職種はありません。別の言葉で探すか、絞り込みを消してください。'
      : '';
  }
}
(function () {
  // 自分の印のカードだけを見る。入れ替え中は古いカードも DOM に残っているので、
  // document 全体から拾うと 2 枚のうちどちらに書いたか分からなくなる。
  var root = document.querySelector('[data-indeed-titles="__INDEED_TOKEN__"]');
  if (!root) return;
  var box = root.querySelector('[data-indeed-find]');
  if (box && window._indeedFind) {
    box.value = window._indeedFind;
    indeedFilterTitles(window._indeedFind, box);
  }
})();
</script>"#;

/// 描き直しごとの印を埋め込んだ script を返す。
fn search_box_script(token: u64) -> String {
    SEARCH_BOX_SCRIPT.replace("__INDEED_TOKEN__", &token.to_string())
}

/// 職種の一覧。全国なら全職種、県を選んでいればその県の職種。
fn title_section(snap: &Snapshot, pref: Option<&str>, sort: Option<&str>) -> String {
    let months = &snap.meta.months;
    let spec = sort_spec(sort);
    // 職種リンクに付ける県と並べ替え。
    //
    // 県を付けないと、人から送られた URL 経由で詳細を開いたとき
    // 一覧は東京都なのに詳細だけ全国、という食い違いが黙って起きる。
    // 並べ替えを付けないと、title.rs 側が HX-Current-URL ヘッダから拾う回り道に頼ることになり、
    // htmx を通らない素のアクセス（人に送った URL を直接開く）では並べ替えが落ちる。
    // spec.key は SORTS の固定値なので、そのまま URL に載せてよい。
    let pq = {
        let mut q = format!("&sort={}", spec.key);
        if let Some(p) = pref.filter(|x| !x.is_empty()) {
            q.push_str(&format!("&pref={}", url_query(p)));
        }
        q
    };

    // スマホ比率は職種そのものの性質で、県で絞っても変わらない。名前から引く
    // 県を選んでいればその県のスマホ率。図（求職者タブ）は県別に直したのに
    // この表だけ全国のままで、同じ職種に 2 つの数字が出ていた
    let mobile: std::collections::HashMap<&str, f64> = match pref {
        None => snap
            .titles
            .iter()
            .filter_map(|t| t.mobile_pct.map(|v| (t.name.as_str(), v)))
            .collect(),
        Some(p) => crate::indeed::aggregate::pref_rows(snap, p)
            .iter()
            .filter_map(|r| r.mobile_pct.map(|v| (r.title.as_str(), v)))
            .collect(),
    };

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
        "grow" => rows
            .sort_by(|a, b| key_desc(b.2.job.change_pct).total_cmp(&key_desc(a.2.job.change_pct))),
        "shrink" => {
            rows.sort_by(|a, b| key_asc(a.2.job.change_pct).total_cmp(&key_asc(b.2.job.change_pct)))
        }
        "hard" => rows.sort_by(|a, b| key_asc(a.2.spp.latest).total_cmp(&key_asc(b.2.spp.latest))),
        "worse" => {
            rows.sort_by(|a, b| key_asc(a.2.spp.change_pct).total_cmp(&key_asc(b.2.spp.change_pct)))
        }
        "better" => rows
            .sort_by(|a, b| key_desc(b.2.spp.change_pct).total_cmp(&key_desc(a.2.spp.change_pct))),
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
    // 描き直しごとに違う印を振る。htmx が #content を入れ替えている最中は
    // 古いカードと新しいカードが**両方** DOM にあるので、検索欄の script が
    // 「自分のカード」を名指しできるようにしておく（理由は SEARCH_BOX のコメント）。
    static NEXT_TOKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let token = NEXT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    h.push_str(&format!(
        "<div data-indeed-titles=\"{token}\" class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
         <div class=\"flex flex-wrap items-center justify-between gap-3 mb-1\">\
         <h3 class=\"text-slate-100 text-lg font-bold\">職種の一覧</h3>"
    ));
    h.push_str(&sort_selector(spec));
    h.push_str(SEARCH_BOX);
    h.push_str("</div>");
    h.push_str(&format!(
        "<p class=\"text-slate-400 text-xs mb-3 leading-relaxed max-w-lg\">{}　\
         全 {n} 職種を出しています。</p>",
        esc(spec.note),
        n = rows.len()
    ));

    // 列見出しを、サブタブの帯のすぐ下に貼り付ける。
    //
    // # なぜ要るか
    // この表は 1280px で縦 6,344px ある。スクロールすると 9 列のうちどれを見ているか
    // 分からなくなり、「求人数の変化」と「その変化」のように名前が似た列を取り違える。
    //
    // # なぜ狭い幅では列を減らすのか（ここが肝）
    // CSS の仕様上、overflow-x:auto を指定すると overflow-y は visible のままでいられず
    // auto に計算される。つまりラッパーが**縦にもスクロール枠になり**、中の thead は
    // viewport ではなくラッパー基準で貼られる。Chrome で実測したところ、ページを
    // 400 / 1000 / 2000px 送ると列見出しの top は -212 / -812 / -1812 と素通りしていた。
    // ラッパーに max-height を与えて縦にスクロールさせると貼り付きはするが、今度は
    // **ラッパーごと帯の下をくぐる**ので、800px で見出しが 45px 潜って 2 行目の断片しか
    // 残らなかった（実測。620px でも 33px 潜る）。ラッパー自身を sticky にしても効かない。
    // sticky はカードの中しか動けず、ラッパーはカードのほぼ全高を占めていて余地が無いため。
    //
    // 結局、**横スクロールを作らない**のが唯一きれいに解ける。
    // 幅が足りないときは列を落として収める。そうすればどの幅でも overflow は visible のままで、
    // 列見出しは viewport 基準で帯の直下（39px）に貼れる。
    // 落とした列は黙って消さず、上に「どの列を省いたか」を出す。
    //
    // 幅で 2 通りに分ける。
    //
    // ## 1240px 以上（既定の画面）
    // 内側の幅が表の min-width (1100px) を超えるなら、横スクロール自体が要らない。
    // overflow-x を visible に戻してラッパーをスクロール枠から外し、**viewport 基準**で
    // 帯の下に貼る。実測で top は 39px に貼り付き、ページ側に横スクロールバーも出ない
    // （scrollWidth == clientWidth == 1280）。
    // 境目を 1240px にしている内訳: main の p-6 が左右 24px、カードの border 1px + p-5 20px、
    // それに縦スクロールバー約 15px。1240 - 15 - 48 - 2 - 40 = 1135px 残るので
    // 1100px の表が収まる。ここを下げると、表がカードからはみ出して**ページごと**
    // 横スクロールする（帯まで横に流れる）ので下げないこと。
    //
    // 表の min-width を 940px から 1100px に上げた理由:
    // 940px だと 620/800px で **126 行中 73 行**が 2 行に折り返していた（実測）。
    // 1100px にすると 21 行まで減る（1190px なら 8 行だが、それだと上の境目が
    // 1295px 必要になり、1280px の画面で viewport 基準の貼り付けが効かなくなる）。
    // 1280px では表はもともと 1190px 幅なので、折り返しは 8 行のまま変わらない。
    //
    // ## 1240px 未満（表が入りきらない画面）
    // 横スクロールは使わない。列を落として表そのものを縮める。
    // min-width を 0 に戻したうえで、幅に応じて 3 段階で列を隠す。
    //
    //   1239px 以下 … 「業界」「スマホ」を隠す
    //   1049px 以下 … さらに「分類」を隠す
    //    899px 以下 … さらに「その変化」「動き方」を隠す（残り 4 列）
    //
    // こうすると overflow-x は visible のままでよく、ラッパーがスクロール枠に
    // ならないので、thead は**どの幅でも viewport 基準**で帯の下に貼れる。
    // 幅ごとに貼り付けの仕組みを変えずに済む。
    //
    // ラッパーに max-height を与えて中だけスクロールさせる手もあるが、採っていない。
    // 表の下に内容を足した日に前提が崩れるうえ、行は見えているのに表の外が
    // 動かない、という読み方になるため。列を落とすほうは、隠した列を
    // 直後の <p> で必ず名指しするので、黙って消えることにはならない。
    //
    // 🔴 overflow-x が visible なので、落としても収まらない幅があると
    //    **ページごと**横に流れる（帯まで横に動く）。列の構成を変えるときは、
    //    いちばん狭い段でも表が収まることを実測で確かめること。
    //
    // 実測（Chrome、ページを下まで送った状態）:
    //   1400 / 1280 / 1240 / 1239 / 1100 / 900 / 899 / 800 / 620px の 9 幅とも
    //   列見出しの top = 39px、ページも受け皿も横スクロールなし。
    //   行の折り返しは順に 0 / 8 / 9 / 0 / 8 / 0 / 0 / 0 / 4 行。
    //
    //   4 列まで落としたときの表の最小幅は 268px（数字と「▲ +106.8%」が
    //   折り返せないため）。表の左端は main の p-6 とカードの p-5 で 45px の位置に
    //   あるので、**viewport が 315px あたりを切ると**カードの外へはみ出し、
    //   最後はページごと横に流れる。620 / 560 / 500 / 480 / 440 / 400 / 375 /
    //   360 / 320px を実測した範囲では、ページの横スクロールは出ていない
    //   （320px で表がカードの右端を 17px 越えるが、viewport の内側には収まる）。
    //
    // # top:39px の根拠
    // 帯の高さは py-2 (8+8) + text-sm の行高 20px + リンクの border-b-2 2px = 38px、
    // 帯自身の border-b 1px を足して 39px（Chrome 実測でも 39px）。
    // 0 にすると帯の裏に完全に隠れる。帯は z-10、見出しは z-5 で帯が必ず上に来る。
    //
    // # 背景と罫線
    // th は透明なので、貼り付いた状態だと下の行が透けて重なる。カードの地と同じ
    // #1e293b を直に指定する（カードは bg-navy-700 で不透明）。
    // border-collapse:collapse の表では sticky な th の下罫線が消えることがあるため、
    // 罫線は box-shadow でも引いておく（色は border-slate-500 と同じ #64748b）。
    //
    // precompiled CSS には sticky な thead 用の指定も top 値も無いので、
    // クラス名ではなくこの表専用の <style> をその場で吐く。
    // htmx は #content を innerHTML ごと入れ替えるので、重複して溜まることはない。
    h.push_str(
        "<style>[data-indeed-empty]:empty{display:none}#indeed-title-wrap{overflow-x:visible}#indeed-title-wrap table{min-width:1100px}#indeed-title-wrap thead th{position:sticky;top:0;z-index:5;background:#1e293b;box-shadow:inset 0 -1px 0 #64748b}[data-indeed-narrow],[data-indeed-narrow-2],[data-indeed-narrow-3]{display:none}@container indeedbody (max-width:1191px){#indeed-title-wrap table{min-width:0}#indeed-title-wrap tr>:nth-child(3),#indeed-title-wrap tr>:nth-child(8){display:none}[data-indeed-narrow]{display:block}}@container indeedbody (max-width:1001px){#indeed-title-wrap tr>:nth-child(2){display:none}[data-indeed-narrow-2]{display:inline}}@container indeedbody (max-width:851px){#indeed-title-wrap tr>:nth-child(7),#indeed-title-wrap tr>:nth-child(9){display:none}[data-indeed-narrow-3]{display:inline}}</style><p data-indeed-narrow class=\"text-amber-300 text-xs mb-2 leading-relaxed max-w-lg\">画面の幅が足りないので「業界」「スマホ」<span data-indeed-narrow-2>「分類」</span><span data-indeed-narrow-3>「その変化」「動き方」</span>の列を省いています。全部見るには画面を広げてください。</p><div id=\"indeed-title-wrap\"><table class=\"w-full text-sm\"><thead><tr>",
    );
    for (name, align) in [
        ("職種", "left"),
        ("分類", "left"),
        ("業界", "left"),
        ("求人数（最新月）", "right"),
        ("求人数の変化", "right"),
        ("1 求人あたりに見た人数", "right"),
        ("その変化", "right"),
        ("スマホ", "right"),
        ("動き方", "left"),
    ] {
        h.push_str(&format!(
            "<th scope=\"col\" class=\"text-slate-400 font-medium px-3 py-2 border-b border-slate-500\" style=\"text-align:{align}\">{name}</th>"
        ));
    }
    h.push_str("</tr></thead><tbody>");

    let td = "px-3 py-2 border-b border-slate-700 text-slate-200";
    for (name, cat, o) in &rows {
        h.push_str(&format!(
            "<tr data-find=\"{find}\"><th scope=\"row\" class=\"{td} font-normal\" style=\"text-align:left\">\
             <a class=\"text-blue-400 hover:underline\" href=\"/tab/indeed/title?name={q}{pq}\" \
                hx-get=\"/tab/indeed/title?name={q}{pq}\" hx-target=\"#content\" hx-swap=\"innerHTML show:top\" \
                hx-push-url=\"true\">{n}</a></th>\
             <td class=\"{td} text-slate-400\">{c}</td>\
             <td class=\"{td} text-slate-400 whitespace-nowrap\">{ind}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{td} tabular-nums whitespace-nowrap {jc}\" style=\"text-align:right\">{ja} {jp}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{s}</td>\
             <td class=\"{td} tabular-nums whitespace-nowrap {sc}\" style=\"text-align:right\">{sa} {sp}</td>\
             <td class=\"{td} tabular-nums text-slate-400\" style=\"text-align:right\">{mb}</td>\
             <td class=\"{td} text-slate-300 whitespace-nowrap\">{t}</td></tr>",
            n = esc(name),
            // 検索欄が見る文字列。職種・分類・業界をまとめて 1 つの属性に入れる
            find = esc(&format!(
                "{name} {cat} {ind}",
                ind = crate::indeed::industry::of_category(cat)
                    .unwrap_or(crate::indeed::industry::OUTSIDE)
            )),
            q = url_query(name),
            pq = pq,
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
    h.push_str("</tbody></table></div>");
    // 絞り込みを動かす script は**表の後ろ**。入力欄のすぐ下に置くと、
    // 素の読み込みでは表が組み立てられる前に走り、0 行を数えてしまう。
    h.push_str(&search_box_script(token));
    h.push_str("</div>");
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
fn industry_section(snap: &Snapshot, months: &[String], pref: Option<&str>) -> String {
    use crate::indeed::aggregate::industry_table_at;
    use crate::indeed::industry;

    let rows = industry_table_at(snap, pref);
    if rows.is_empty() {
        return String::new();
    }

    // 表の行と同じ母集団を、ここでも作る。
    //
    // # なぜ要るか
    // 1 つの画面に職種数が 4 つ並び、どれとどれが対応するのか読めなかった。
    //   - 5 業界＋外の「職種数」を足すと 104（全期間そろっている職種だけ）
    //   - 「全体」行の職種数は 126（名簿そのままの数）
    //   - 表の「5 業界の外」の行は 4 職種
    //   - なのに下の「5 業界に入れていない職種」の一覧は 22 職種
    // 全国・2025-07〜2026-08 の実データで確認した数。
    // industry_series_at は「全期間そろっている職種」だけを足しているので、
    // 表に出る数はすべてその母集団。ここも同じ分け方で数え直す。
    // 全国は Title::complete、県はその県の並びが全月そろっているかで決まる。
    let cat_of = |name: &str| -> String {
        snap.titles
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.category.clone())
            .unwrap_or_default()
    };
    // (職種名, 分類, 合計に入っているか)
    let members: Vec<(String, String, bool)> = match pref {
        None => snap
            .titles
            .iter()
            .map(|t| (t.name.clone(), t.category.clone(), t.complete))
            .collect(),
        Some(p) => crate::indeed::aggregate::pref_rows(snap, p)
            .iter()
            .map(|r| {
                (
                    r.title.clone(),
                    cat_of(&r.title),
                    r.series.job.iter().all(|v| v.is_some()),
                )
            })
            .collect(),
    };
    let counted_titles = members.iter().filter(|(_, _, ok)| *ok).count();
    let skipped_titles = members.len() - counted_titles;
    let td = "px-3 py-2 border-b border-slate-700 text-slate-200";
    let th = "text-slate-400 font-medium px-3 py-2 border-b border-slate-500";

    let mut h = String::with_capacity(30_000);
    h.push_str(
        "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
         <h3 class=\"text-slate-100 text-lg font-bold mb-1\">業界ごとの動き（5 業界）</h3>\
         <p class=\"text-slate-400 text-xs mb-3 leading-relaxed max-w-lg\">\
         Indeed の 20 分類のうち、募集する会社が重なるものを 5 つにまとめています。\
         5 つに入らない職種は、無理に入れず別枠で数えています。\
         「動き方」は月ごとの上下をならした線から出しています。</p>",
    );

    // --- 表 ---
    h.push_str(
        "<div style=\"overflow-x:auto\"><table class=\"w-full text-sm\" \
         style=\"min-width:960px\"><thead><tr>",
    );
    for (n, a) in [
        ("業界", "left"),
        // 表に出る数はすべて「合計に入れた職種」で数えている。ただの「職種数」だと、
        // 見出しの「126 職種」と同じものに見える
        ("合計に入れた職種数", "right"),
        ("求人数（最新月）", "right"),
        ("全体に占める割合", "right"),
        // 下の「分類別の定点表」の「◯◯の変化」は期間を通した変化で、この 2 列とは
        // 別のものを測っている（こちらは 2 か月だけを見た素の比）。どちらも要る数字なので
        // 揃えず、どの期間の話かを列名に持たせて見分けがつくようにする。
        ("先月比（直近 1 か月）", "right"),
        ("前年同月比（1 年前の同月）", "right"),
        ("1求人あたり（人）", "right"),
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
             <td class=\"{td} text-slate-300 whitespace-nowrap\">{tr}</td></tr>",
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
    // 「全体」行も見ている範囲に合わせる。県を選んでいるのに
    // 全国の 1,624,095 が出ていた
    let all = match pref {
        None => nation_overview(snap),
        Some(p) => pref_overview(snap, p),
    };
    h.push_str(&format!(
        "<tr class=\"border-t border-slate-600\">\
         <th scope=\"row\" class=\"{td} font-bold\" style=\"text-align:left\">全体</th>\
         <td class=\"{td}\" style=\"text-align:right\">{t}</td>\
         <td class=\"{td} tabular-nums font-bold\" style=\"text-align:right\">{j}</td>\
         <td class=\"{td} tabular-nums text-slate-400\" style=\"text-align:right\">100.0%</td>\
         <td class=\"{td} tabular-nums {mc}\" style=\"text-align:right\">{ma} {m}</td>\
         <td class=\"{td} tabular-nums {yc}\" style=\"text-align:right\">{ya} {y}</td>\
         <td class=\"{td} tabular-nums\" style=\"text-align:right\">{s}</td>\
         <td class=\"{td} text-slate-300 whitespace-nowrap\">{tr}</td></tr>",
        // 上の行と同じ母集団で数える。名簿そのままの数（全国 126）を置くと、
        // 5 業界＋外を足した 104 と合わない
        t = counted_titles,
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

    // --- 5 業界。実数のまま 1 つずつ並べる ---
    //
    // 重ねると建設・設備・整備（7.5 万〜10.6 万）が、サービス・販売の最大 60.1 万に対して
    // 12% の高さになる。この業界は期間中に 42% 動いているのに、平らな線に見えてしまう。
    // 8.1 倍は重ねられる限界（5 倍）を超えている。
    // 色は散布図と揃える。散布図は INDUSTRIES の並び順で palette を引いているので、
    // ここも同じ並び順の位置を渡す。名前で引くと「製造・生産」が分類と衝突する。
    let series: Vec<(String, Vec<Option<f64>>, &str, usize)> = rows
        .iter()
        .filter(|r| r.name != industry::OUTSIDE)
        .map(|r| {
            let ci = industry::INDUSTRIES
                .iter()
                .position(|i| i.name == r.name)
                .unwrap_or(0);
            (r.name.clone(), r.ov.job.series.clone(), "件", ci)
        })
        .collect();
    h.push_str(&format!(
        "<p class=\"text-slate-400 text-xs mt-4 mb-2 max-w-lg\">\
         求人数の実数です。業界どうしで規模が 8 倍ちがうので、重ねずに 1 つずつ出しています。</p>{}",
        small_multiples(months, &series, true, 190)
    ));

    // --- 業界ごとの一行 ---
    //
    // 5 業界とも「月ごとの上下が毎月の動きより大きい」形になると、同じ断り書きが
    // 5 行そろって並ぶ（実データでは「毎月少しずつ増えたわけではありません」が
    // 求人数で 5 回、1 求人あたりでさらに 2 回）。読み手には壊れているように見えるが、
    // これは 5 業界に共通の性質で、業界ごとの違いではない。先に 1 回まとめて断る。
    // 文そのものは src/indeed/wording.rs の describe_trend が作るので、ここでは変えられない。
    let shown = rows.iter().filter(|r| r.why.is_some()).count();
    let unsteady = rows
        .iter()
        .filter(|r| r.why.is_some())
        .filter(|r| !r.ov.job.fit.as_ref().map(|f| f.steady).unwrap_or(false))
        .count();
    let all_unsteady = shown > 1 && unsteady == shown;
    if all_unsteady {
        h.push_str(&format!(
            "<p class=\"text-slate-400 text-xs mt-4 mb-2 leading-relaxed max-w-lg\">\
             下の {n} 業界は<strong>どれも</strong>月ごとの上下が毎月の動きより大きく、\
             「毎月少しずつ増えた（減った）」という形ではありません。\
             同じ断りが業界ごとに繰り返し出ますが、5 業界に共通の性質であって、\
             業界ごとの違いではありません。</p>",
            n = shown
        ));
    }
    let list_cls = if all_unsteady {
        "space-y-3"
    } else {
        "mt-4 space-y-3"
    };
    h.push_str(&format!("<div class=\"{list_cls}\">"));
    for r in rows.iter().filter(|r| r.why.is_some()) {
        h.push_str(&format!(
            "<div class=\"bg-navy-800/60 border border-slate-700 rounded p-3\">\
             <div class=\"flex flex-wrap items-baseline gap-2\">\
             <span class=\"text-slate-100 font-bold\">{n}</span>\
             <span class=\"text-slate-400 text-xs\">{t} 職種／全体の {sh}／求人数が多いのは {top}</span></div>\
             <p class=\"text-slate-300 text-sm mt-1 leading-relaxed max-w-xl\">{s1}</p>\
             <p class=\"text-slate-300 text-sm leading-relaxed max-w-xl\">{s2}</p>\
             <p class=\"text-slate-400 text-xs mt-2 leading-relaxed max-w-lg\">まとめ方：{why}</p></div>",
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

    // 5 業界に入れなかった職種は、名前まで出す。黙って除くと数字が合わなくなる。
    //
    // 以前はここだけ名簿（snap.titles）全部から拾っていたので、表の「5 業界の外」の行は
    // 4 職種なのに、この一覧には 22 職種が並んでいた（全国の実データ）。しかも県を
    // 選んでも全国の名前が出ていた。表の行と同じ母集団（members）から拾い直す。
    let outside_in: Vec<String> = members
        .iter()
        .filter(|(_, c, ok)| *ok && industry::of_category(c).is_none())
        .map(|(n, c, _)| format!("{n}（{c}）"))
        .collect();
    if !outside_in.is_empty() {
        h.push_str(&format!(
            "<p class=\"text-slate-400 text-xs mt-3 leading-relaxed max-w-lg\">\
             上の「{o}」の行に入っている {n} 職種：{names}。</p>",
            o = esc(industry::OUTSIDE),
            n = outside_in.len(),
            names = esc(&outside_in.join("、"))
        ));
    }

    // 合計に入れていない職種も、数と名前を出す。出さないと「見出しの 126 職種と
    // 表の 104 職種の差は何か」が画面から追えない。
    if skipped_titles > 0 {
        let skipped: Vec<String> = members
            .iter()
            .filter(|(_, _, ok)| !*ok)
            .map(|(n, c, _)| format!("{n}（{c}）"))
            .collect();
        h.push_str(&format!(
            "<p class=\"text-slate-400 text-xs mt-2 leading-relaxed max-w-lg\">\
             この表に入っているのは、{first} から {latest} まで毎月そろっている {c} 職種です。\
             残り {s} 職種は月が欠けているため、どの行にも入れていません\
             （母集団が月によって変わると、先月比が実態と関係なく動くためです）：{names}。</p>",
            first = esc(months.first().map(String::as_str).unwrap_or("—")),
            latest = esc(&snap.meta.latest),
            c = counted_titles,
            s = skipped_titles,
            names = esc(&skipped.join("、"))
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
    // 県を選んでいればその県の職種で並べる。以前は見出しだけ「要点（沖縄県）」に
    // 変わり、中身は全国の順位のままだった
    let mut rows: Vec<(&str, Overview)> = match pref {
        None => snap
            .titles
            .iter()
            .filter(|t| t.complete)
            .filter_map(|t| {
                snap.by_title
                    .get(&t.name)
                    .map(|s| (t.name.as_str(), Overview::from_series(&t.name, s, months)))
            })
            .collect(),
        Some(p) => crate::indeed::aggregate::pref_rows(snap, p)
            .iter()
            .filter(|r| r.series.job.iter().all(|v| v.is_some()))
            .map(|r| {
                (
                    r.title.as_str(),
                    Overview::from_series(&r.title, &r.series, months),
                )
            })
            .collect(),
    };
    rows.sort_by(|a, b| {
        a.1.spp
            .change_pct
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b.1.spp.change_pct.unwrap_or(f64::INFINITY))
    });
    // 「求人数が多いほう」は、その県の中で決める。
    //
    // 以前は「5,000 件以上」「20,000 件以上」という全国基準の絶対値だった。
    // 実測すると、5,000 件以上の職種が 1 つも無い県が 38、20,000 件以上が
    // 無い県が 46 ある。鳥取県では最大の職種でも 425 件しかない。
    // このままだと県を選んだ時点でこの 2 項目が黙って消える。
    let cutoff = {
        let mut v: Vec<f64> = rows.iter().filter_map(|(_, o)| o.job.latest).collect();
        v.sort_by(|a, b| b.total_cmp(a));
        // 上位 3 分の 1。少ない県でも 5 職種は残す
        let n = (v.len() / 3).max(5).min(v.len());
        v.get(n.saturating_sub(1)).copied().unwrap_or(0.0)
    };
    let harder: Vec<String> = rows
        .iter()
        .filter(|(_, o)| o.job.latest.unwrap_or(0.0) >= cutoff)
        .take(3)
        .map(|(n, o)| format!("{}（{}）", esc(n), pct_opt(o.spp.change_pct)))
        .collect();
    let mut big: Vec<&(&str, Overview)> = rows
        .iter()
        .filter(|(_, o)| o.job.latest.unwrap_or(0.0) >= cutoff)
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
            // 「求職者」の面の同じ図には「差は 2 割ほどで、大きくはありません」と書いてある。
            // ここだけ「12 月に出しても人は少なめです」と打ち手の形で書くと、
            // 2 つの画面で逆の結論を読むことになる。同じ大きさの話に寄せ、
            // 差の大きさを数字でそのまま出す。
            (Some((hi, hv)), Some((lo, lv))) => format!(
                "<li>求職者がいちばん多いのは <strong>{hi} 月</strong>（年間平均の {hv:.2} 倍）、\
                 少ないのは <strong>{lo} 月</strong>（{lv:.2} 倍）です。\
                 ただし山と谷の差は年間平均の {gap:.0}% 分で、月を選び直すほどの開きではありません\
                 （同じ図が「求職者」の面にあります）。\
                 <span class=\"text-slate-400 text-xs\">※ 検索エンジンの検索ボリューム。\
                 Indeed の求人数とは別のデータです</span></li>",
                hi = hi + 1,
                hv = hv,
                lo = lo + 1,
                lv = lv,
                gap = (hv - lv) * 100.0
            ),
            _ => String::new(),
        }
    };

    // 要点の箱だけ、画面の他の青から teal に移す。
    //
    // # 3 つの役割を色で分ける
    // 画面の青が「重要な結論（この箱）」「現在地（県バッジ・サブタブ・パンくず・上部ナビ）」
    // 「注意（但し書きの左罫線）」の 3 役を兼ねていた。とくに**要点と但し書きが
    // 同じ border-blue-500 の左罫線**で、色では結論と注意書きが見分けられなかった。
    // 要点=teal / 現在地=blue / 免責=slate の 3 本に分ける。
    //
    // # なぜ teal か（emerald と amber は使えない）
    // - amber #f59e0b … 図の「1 求人あたりに見た人数」と「製造・生産」、さらに**全図の基準線**
    // - emerald #34d399 … 表と図の**「▲ 増加」**、「建設・設備・整備」「接客・販売」
    //   増加の色を結論の箱に使うと、中身が減少の話でも良い知らせに見える。
    //   実際この箱の 1 行目は「1 求人あたりに見た人数は -6.1% 減りました」。
    // - teal … どの図でも使っていない。だから teal にした。
    //
    // # なぜクラスではなくインライン style か
    // precompiled CSS に border-teal-* も bg-teal-900/50 も無い
    // （あるのは bg-teal-500・bg-teal-600・text-teal-400 だけ）。
    // 無いクラス名を書いても何も起きないので、この 2 色だけ style 属性で指定する。
    // 値は Tailwind と同じ尺度の teal-900 50%（body #0d1525 と混ざって #103238）と
    // teal-500 (#14b8a6)。この地に対して slate-400 は 5.33:1、slate-200 は 11.09:1、
    // 見出しの text-teal-400 は 7.35:1 で、いずれも本文の AA (4.5:1) を満たす。
    format!(
        "<div class=\"border border-l-4 rounded-xl p-5 shadow-md max-w-2xl\" style=\"background:rgba(19,78,74,0.5);border-color:#14b8a6\">\
         <h3 class=\"text-teal-400 text-lg font-bold mb-2\">要点（{w}）</h3>\
         <ul class=\"text-slate-200 text-sm leading-relaxed list-disc pl-4 space-y-2\">\
         <li>{state}<span class=\"text-slate-400 text-xs\">{hedge}</span></li>\
         {harder}{crowded}{when}</ul>\
         <p class=\"text-slate-400 text-xs mt-3 leading-relaxed max-w-lg\">\
         ここに書いたのは、下の図と表から出せることだけです。根拠は各図の下にあります。</p></div>",
        w = esc(where_),
        state = state,
        hedge = hedge,
        harder = if harder.is_empty() {
            String::new()
        } else {
            format!(
                "<li><strong>去年と同じやり方が通じにくくなっている職種</strong>は {}。\
                 1 求人あたりに見た人数がいちばん減った順です（求人数が多いほうの職種に絞っています）。\
                 求人票の書き方か、出す媒体を見直す候補になります。</li>",
                harder.join("、")
            )
        },
        crowded = if crowded.is_empty() {
            String::new()
        } else {
            format!(
                "<li><strong>募集は多いのに人が集まっていない職種</strong>は {}。\
                 かっこ内は 1 求人あたりに見た人数で、求人数が多いほうの職種のうち少ない順です。\
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
            Some((i, x)) => (
                format!("{} 月", i + 1),
                format!("{:+.0}%", (x - 1.0) * 100.0),
            ),
            None => ("—".to_string(), "—".to_string()),
        }
    };
    let (hm, hv) = pick(true);
    let (lm, lv) = pick(false);
    let years = seasons.first().map(|s| s.years).unwrap_or(0);
    let solid = seasons.iter().filter(|s| s.avg_monthly >= 1000.0).count();

    format!(
        "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
         <h3 class=\"text-slate-100 text-lg font-bold mb-1\">1 年のうち、いつ動くか（過去 {y} 年）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed max-w-lg\">\
         検索エンジンで職種名がどれだけ検索されたかを、{n} 職種ぶんならして\
         暦月ごとに平均したものです。<strong>0 が年間の平均</strong>で、\
         +10% なら平均より 1 割多い月という意味です。\
         上の求人数とは<strong>別の出どころ</strong>で、求人の数ではなく\
         「求職者の動き」を表します。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed max-w-xl\">\
         いちばん多いのは{hm}（{hv}）、少ないのは{lm}（{lv}）です。\
         差は 2 割ほどで、大きくはありません。</p>\
         <p class=\"text-slate-400 text-xs mt-2 leading-relaxed max-w-lg\">\
         職種ごとには出していません。検索数が少ない職種ほど月ごとのきざみが粗く、\
         波が大きく見えるためです（月 50 回未満の 16 職種では山が +28%、\
         月 1000 回以上の {solid} 職種では +10%）。\
         ならすとぶれが打ち消し合います。</p></div>",
        y = years,
        n = seasons.len(),
        chart = vbar_chart(
            &labels,
            // 「平均の何倍か」ではなく「平均から何 % ずれているか」で渡す。
            // 棒は 0 を基線にしないと、高さの比が実際の差と合わない。
            &idx.iter()
                .map(|v| v.map(|x| (x - 1.0) * 100.0))
                .collect::<Vec<_>>(),
            "年間平均からの差（%）",
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
fn mobile_section(snap: &Snapshot, pref: Option<&str>) -> String {
    // 県を選んでいればその県のスマホ率。以前は全国の値をそのまま出しており、
    // 県を選んでも数字が動かなかった
    let mut rows: Vec<(&str, f64)> = match pref {
        None => snap
            .titles
            .iter()
            .filter_map(|t| t.mobile_pct.map(|v| (t.name.as_str(), v)))
            .collect(),
        Some(p) => crate::indeed::aggregate::pref_rows(snap, p)
            .iter()
            .filter_map(|r| r.mobile_pct.map(|v| (r.title.as_str(), v)))
            .collect(),
    };
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
        labels.push(format!(
            "\u{2500}\u{2500} ほか {hidden} 職種 \u{2500}\u{2500}"
        ));
        values.push(None);
    }
    labels.extend(bottom.iter().map(|r| r.0.to_string()));
    values.extend(bottom.iter().map(|r| Some(r.1)));

    format!(
        "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
         <h3 class=\"text-slate-100 text-lg font-bold mb-1\">スマホで探されている職種・PC で探されている職種（{w}）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed max-w-lg\">\
         求人ページと応募フォームをどちらに合わせるかの手がかりです。\
         {n} 職種のうち、<strong>高いほう 10 と低いほう 10</strong>だけを出しています{mid}\
         。破線は全体の真ん中（{md}%）です。</p>{chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed max-w-xl\">\
         いちばん高いのは{t}（{tv}%）、いちばん低いのは{b}（{bv}%）で、その差は {gap} ポイントです。\
         「スマホで探す人は条件で絞り込む」という見方は、この数字と条件検索率の間には\
         ほとんど関係が無く（相関 0.04）、裏づけられません。</p></div>",
        n = n,
        w = esc(pref.unwrap_or("全国")),
        mid = if hidden > 0 {
            format!("（間の {hidden} 職種は「職種」の面の一覧で見られます）")
        } else {
            String::new()
        },
        md = dec1_opt(Some(med)),
        chart = hbar_chart(
            &labels,
            &values,
            "スマホからの検索の割合（%）",
            Some((med, "全体の真ん中")),
            Some(100.0),
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

/// 軸の上限を、切りのいい数に切り上げる。
///
/// 1 / 1.5 / 2 / 2.5 / 3 / 4 / 5 / 6 / 8 / 10 に 10 の累乗を掛けた並びから、
/// 値以上でいちばん小さいものを返す。83 → 100、38 → 40、45 → 50。
///
/// # 切り下げない
/// 切り下げると、いま図に入っている点が軸の外にはみ出す。
/// 上限は必ず元の値以上にする。
fn nice_ceil(v: f64) -> f64 {
    if !v.is_finite() || v <= 0.0 {
        return v;
    }
    let e = 10f64.powf(v.log10().floor());
    for m in [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0] {
        let c = m * e;
        // 桁落ちで 100.0 が 100.00000000000001 になっても 100 に丸める
        if v <= c * (1.0 + 1e-12) {
            return c;
        }
    }
    10.0 * e
}

fn scatter_section(snap: &Snapshot, pref: Option<&str>) -> String {
    use crate::indeed::industry;

    let months = &snap.meta.months;
    let mut points: Vec<(String, f64, f64, String)> = Vec::new();
    // 県を選んでいればその県の職種で描く。以前は全国のときしか出さず、
    // 県を選ぶと図が黙って消えていた
    let rows: Vec<(String, String, Overview)> = match pref {
        None => snap
            .titles
            .iter()
            .filter(|t| t.complete)
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
        Some(p) => crate::indeed::aggregate::pref_title_overviews(snap, p),
    };
    for (name, cat, ov) in &rows {
        let (Some(j), Some(spp)) = (ov.job.latest, ov.spp.latest) else {
            continue;
        };
        if j <= 0.0 {
            continue;
        }
        points.push((
            name.clone(),
            j,
            spp,
            industry::of_category(cat)
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

    // 縦軸の上限を 98 パーセンタイルで決め、そこを超える職種は図から外す。
    //
    // 東京都では 1 職種（1 求人あたり 207.5、求人 28 件）が上限を決めてしまい、
    // 残り 121 点が下 1/3 に押し込まれて重なっていた。外した職種は下の文に名前で
    // 出すので、情報は落ちない。1 求人あたりが極端に多い＝集めやすい職種であって、
    // この図が探している「右下」ではない。
    let cap = {
        let mut v: Vec<f64> = points.iter().map(|p| p.2).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        let i = ((v.len() as f64) * 0.98).ceil() as usize;
        v[i.saturating_sub(1).min(v.len() - 1)]
    };
    // 目盛りは 0,20,40,60,80 と刻まれるのに、いちばん上のラベルだけ 98 パーセンタイルの
    // 生値（東京 83 / 鳥取 38 / 全国 45）になり、最上段だけ間隔が不揃いに見えていた。
    // 切りのいい数に**切り上げる**。切り下げると、いま図に入っている点が軸の外に出る。
    //
    // 外す・外さないの境目も同じ値にそろえる。境目を生値のまま残すと、
    // 「縦軸の外なので図から外しています」と書いた職種が軸の内側に収まってしまう。
    // 切り上げなので、これまで出ていた点が新しく消えることはない。
    let cap = nice_ceil(cap);
    let mut over: Vec<(String, f64)> = points
        .iter()
        .filter(|p| p.2 > cap)
        .map(|p| (p.0.clone(), p.2))
        .collect();
    over.sort_by(|a, b| b.1.total_cmp(&a.1));
    let shown: Vec<(String, f64, f64, String)> =
        points.iter().filter(|p| p.2 <= cap).cloned().collect();

    // 「求人数が多いほう」を、その県の中で決める。
    //
    // 以前は「20,000 件以上」という全国基準の絶対値だった。実測すると
    // この条件を満たす職種は **47 県中 46 県でゼロ**（東京都だけ 3 職種）で、
    // 鳥取県では「1 求人あたりが少ないのは — の順でした。」という
    // 中身の無い文だけが残っていた。県の規模に合わせて上位 3 分の 1 で切る。
    let big_cut = |src: &[(String, f64, f64, String)]| -> Vec<(String, f64, f64, String)> {
        let mut v: Vec<(String, f64, f64, String)> = src.to_vec();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        let n = (v.len() / 3).max(5).min(v.len());
        v.truncate(n);
        v
    };
    let hard: Vec<String> = {
        let mut v = big_cut(&points);
        v.sort_by(|a, b| a.2.total_cmp(&b.2));
        v.iter().take(5).map(|p| p.0.clone()).collect()
    };
    // 図に名前を出す職種。**散らばるように選ぶ。**
    //
    // # なぜ「求人数の上位」で選ばないのか
    // 上位で選ぶと、定義上すべて X 軸の右端に集まる。しかも実データでは
    // 事務・一般事務・販売スタッフのように Y（1 求人あたり）も近いので、
    // **点そのものが団子になる**。この状態はラベルの置き方では解けない。
    //   - 重なりを隠させる → 愛知県で 8 個中 3 個しか残らず、本文が名指しした職種が消える
    //   - 縦にずらさせる → 全部出るかわりに 9 組が重なる
    //   - 右端だけ左に倒す → 位置関係が平行移動するだけ。6 ケース中 3 ケースで悪化
    // （いずれも ux-charts の実測。3 つとも「位置を動かす」操作なので効かなかった）
    //
    // 効くのは団子の中のラベル数を減らすことだけなので、**選ぶ時点で散らす**。
    // 求人数が最大 / 1 求人あたりが最大 / 同じく最小 の 3 つは、図の別々の場所に来る。
    // そこに本文で名指しした先頭 1 つを足す。
    const MAX_LABELS: usize = 5;
    let labeled: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        let mut push = |n: &str, v: &mut Vec<String>| {
            if !v.iter().any(|x| x == n) {
                v.push(n.to_string());
            }
        };
        // 本文で「1 求人あたりが少ないのは…」と名指しした先頭
        if let Some(n) = hard.first() {
            push(n, &mut v);
        }
        let pick = |key: fn(&(String, f64, f64, String)) -> f64, max: bool| -> Option<String> {
            let mut it: Vec<&(String, f64, f64, String)> = shown.iter().collect();
            it.sort_by(|a, b| {
                if max {
                    key(b).total_cmp(&key(a))
                } else {
                    key(a).total_cmp(&key(b))
                }
            });
            it.first().map(|p| p.0.clone())
        };
        for n in [
            pick(|p| p.1, true),  // 求人数が最大（右端）
            pick(|p| p.2, true),  // 1 求人あたりが最大（上端）
            pick(|p| p.2, false), // 1 求人あたりが最小（下端）
        ]
        .into_iter()
        .flatten()
        {
            push(&n, &mut v);
        }
        v.retain(|n| shown.iter().any(|p| &p.0 == n));
        v.truncate(MAX_LABELS);
        v
    };

    format!(
        "<div class=\"bg-navy-700 border border-slate-700 rounded-xl p-5\">\
         <h3 class=\"text-slate-100 text-lg font-bold mb-1\">{w}の職種の位置取り（{m}）</h3>\
         <p class=\"text-slate-400 text-xs mb-2 leading-relaxed max-w-lg\">\
         横は求人数（対数）、縦は 1 求人あたりに見た人数です。\
         <strong>右下ほど「募集は多いのに人が集まっていない」</strong>職種になります。\
         色と形は業界です。<strong>白抜きの点は求人が {few} 件に満たない職種</strong>で、\
         1 件の増減で位置が大きく動きます。数字が取れる {n} 職種のうち {sn} 職種を出しています。\
         <strong>この図は下の一覧の絞り込み（検索）には連動しません。</strong>いつも同じ {sn} 職種を出しています。</p>\
         {chart}\
         <p class=\"text-slate-300 text-sm mt-2 leading-relaxed max-w-xl\">\
         1 求人あたりの真ん中は {med} です。\
         求人数が多いほうの職種のうち、1 求人あたりが少ないのは {hard} の順でした。{ov}</p></div>",
        w = esc(pref.unwrap_or("全国")),
        m = esc(&snap.meta.latest),
        few = crate::handlers::indeed::render::FEW_JOBS as u32,
        n = points.len(),
        sn = shown.len(),
        chart = scatter_chart(&shown, &groups, &labeled, Some(cap), true, 380),
        med = dec1_opt(Some(med)),
        hard = if hard.is_empty() {
            "—".to_string()
        } else {
            hard.iter().map(|s| esc(s)).collect::<Vec<_>>().join("、")
        },
        ov = if over.is_empty() {
            String::new()
        } else {
            format!(
                "<br>{} は 1 求人あたりが {} を超えていて、縦軸の外なので図から外しています。",
                over.iter()
                    .map(|(n, v)| format!("{}（{}）", esc(n), dec1_opt(Some(*v))))
                    .collect::<Vec<_>>()
                    .join("、"),
                dec1_opt(Some(cap))
            )
        }
    )
}

#[cfg(test)]
mod axis_tests {
    use super::nice_ceil;

    /// 散布図の縦軸の上限が、目盛りと同じ刻みで終わること。
    ///
    /// 98 パーセンタイルの生値をそのまま上限にしていたため、
    /// 目盛りが 0,20,40,60,80 と並んでいるのに最上段だけ 83 のような端数になり、
    /// いちばん上だけ間隔が違って見えていた（東京 83 / 鳥取 38 / 全国 45 が実測値）。
    #[test]
    fn 軸の上限は切りのいい数になる() {
        assert_eq!(nice_ceil(83.0), 100.0, "東京の実測値");
        assert_eq!(nice_ceil(38.0), 40.0, "鳥取の実測値");
        assert_eq!(nice_ceil(45.0), 50.0, "全国の実測値");
        assert_eq!(nice_ceil(207.5), 250.0);
        assert_eq!(nice_ceil(4.2), 5.0);
        // 1 を下回る値。3.0 * 0.1 は 2 進では割り切れないので幅で見る
        assert!((nice_ceil(0.3) - 0.3).abs() < 1e-9, "{}", nice_ceil(0.3));
    }

    /// 切り下げない。切り下げると、いま図に出ている点が軸の外にはみ出す。
    #[test]
    fn 上限は必ず元の値以上になる() {
        for v in [
            1.0, 9.9, 10.0, 10.1, 38.0, 45.0, 83.0, 99.9, 100.0, 100.1, 1234.0,
        ] {
            assert!(nice_ceil(v) >= v, "{v} を下回った: {}", nice_ceil(v));
        }
    }

    /// 目盛りにならない値を渡されても落ちない。
    #[test]
    fn ゼロ以下や無限大はそのまま返す() {
        assert_eq!(nice_ceil(0.0), 0.0);
        assert_eq!(nice_ceil(-5.0), -5.0);
        assert!(nice_ceil(f64::NAN).is_nan());
    }
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
    ///
    /// 表記は「年間平均の何倍か」から「年間平均からの差（%）」に変えた。
    /// 棒の高さは 0 を基線にしないと実際の差と合わない（0.85 始まりの軸では
    /// 12 月 0.862 と 3 月 1.092 の実差 21% が、見かけ 1 対 8 になっていた）。
    /// 本文と図で単位がずれると、同じものに 2 つの数字が並ぶので両方を見る。
    #[test]
    fn 山と谷が本文と図で一致する() {
        let h = season_section(&many(84));
        assert!(h.contains("3 月（+9%）"), "山が本文に出ていない");
        assert!(h.contains("12 月（-14%）"), "谷が本文に出ていない");
        // 図には差（%）が渡る。1.090 のような倍率が残っていたら本文とずれている
        assert!(h.contains("9.00"), "図に山の値が渡っていない");
        assert!(
            !h.contains("1.090"),
            "図に倍率が残っている（本文は % なのでずれる）"
        );
        assert!(
            h.contains("年間平均からの差"),
            "軸名が差の表記になっていない"
        );
        // 検索数が多い職種の数を数えて書く
        assert!(h.contains("月 1000 回以上の 20 職種"));
    }
}
