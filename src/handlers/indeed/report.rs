//! 顧客レポート `/report/indeed`。
//!
//! # 目的
//! 商談の入口を作ること。読んだ人が自社の状況を確かめたくなる形にする。
//! 診断も指示もしない。数字と、その読み方だけを置く。
//!
//! # 社内タブとの違い
//! 出す範囲だけが違い、数字は [`crate::indeed::aggregate`] の同じ値を使う。
//! 内輪向けの但し書き・仮説・手法の説明はここには出さない。
//!
//! # 既定で閉じている
//! 社外配布の可否が未確認のため、`INDEED_PUBLIC=on` が立つまで 404 を返す。

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;

use super::render::{
    arrow, category_table_html, dec1_opt, dir_class, esc, num, num_opt, pct_opt, raw_line_chart,
};
use crate::indeed::aggregate::{
    category_table, extreme, nation_overview, pref_overview, pref_title_overviews, CategoryRow,
    Metric, Overview,
};
use crate::indeed::data::{snapshot, Snapshot};
use crate::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ReportQuery {
    /// 都道府県で絞る。空なら全国
    pub pref: Option<String>,
}

pub async fn report_indeed(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReportQuery>,
) -> Response {
    // 配布可否が未確認のあいだは存在ごと伏せる
    if !crate::indeed::public_report_enabled() {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    let Some(db) = state.indeed_db.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "レポートの元データが読み込まれていません。",
        )
            .into_response();
    };
    let snap = match snapshot(db) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("indeed snapshot failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "レポートの元データを読めませんでした。",
            )
                .into_response();
        }
    };
    Html(render_report(snap, q.pref.as_deref())).into_response()
}

/// 印刷しても崩れないことを前提にした最小限の見た目。
///
/// `@page` を二重に定義しない。`body` の余白と `@page` の余白を
/// 両方かけると本文の幅が縮む。
const CSS: &str = r#"
:root{
  --ink:#1c2430; --ink-soft:#4a5568; --line:#dfe3ea; --bg:#ffffff;
  --panel:#f7f8fa; --accent:#1f6feb; --up:#0f7b4f; --down:#b3261e;
}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);
  font-family:"Hiragino Kaku Gothic ProN","Yu Gothic",Meiryo,system-ui,sans-serif;
  font-size:15px;line-height:1.9;-webkit-font-smoothing:antialiased}
.wrap{max-width:960px;margin:0 auto;padding:48px 28px 80px}
.cover{border-bottom:3px solid var(--ink);padding-bottom:22px;margin-bottom:34px}
.cover h1{font-size:30px;line-height:1.35;margin:0 0 10px;letter-spacing:.01em;text-wrap:balance}
.cover .sub{color:var(--ink-soft);font-size:14px;margin:0}
h2{font-size:20px;margin:44px 0 8px;padding-bottom:7px;border-bottom:1px solid var(--line);text-wrap:balance}
h3{font-size:16px;margin:26px 0 6px}
/* 日本語は 1 行 38〜42 字あたりが目で追いやすい。960px 幅のまま流すと
   1 行 60 字になり、承認済みの見本（40em 指定）から大きく外れる。
   表と図は 960px のままでよく、本文だけを絞る */
p{margin:.5em 0;max-width:40em}
.lead{font-size:16px;line-height:2.0;max-width:38em}
.note{background:var(--panel);border-left:3px solid var(--accent);padding:12px 16px;
  color:var(--ink-soft);font-size:13.5px;line-height:1.85;margin:16px 0;max-width:42em}
/* auto-fit だと幅 700px 前後で 3 列になり、4 枚のうち 1 枚だけが
   次の行に取り残される。列数を明示して常に揃える */
.kpis{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin:18px 0}
@media (max-width:700px){.kpis{grid-template-columns:repeat(2,1fr)}}
.kpi-card{border:1px solid var(--line);border-radius:8px;padding:14px 16px;background:var(--bg)}
.kpi-label{color:var(--ink-soft);font-size:12px;line-height:1.7}
.kpi-value{font-size:25px;font-weight:700;font-variant-numeric:tabular-nums;margin:2px 0}
/* 単位は値より小さく、色も落とす。「162.4」と「万件」が同じ強さだと
   数と単位の切れ目が見えず、桁を読み違える */
.kpi-unit{font-size:14px;font-weight:600;color:var(--ink-soft);margin-left:2px}
.kpi-chg{font-size:13px;font-variant-numeric:tabular-nums}
.ind-up{color:var(--up)} .ind-down{color:var(--down)} .ind-flat{color:var(--ink-soft)}
table.tbl{width:100%;border-collapse:collapse;font-size:13.5px;margin:14px 0;
  /* 狭い幅で縮ませない。縮むと職種名が 1 文字ずつ縦に折り返って読めなくなる。
     受け皿(.tbl-wrap)の中で横に送る方が読める */
  min-width:660px}
table.tbl td:first-child,table.tbl th:first-child{min-width:9em}
table.tbl th{background:var(--panel);border-bottom:2px solid var(--line);
  padding:9px 11px;font-weight:600;white-space:nowrap;color:var(--ink)}
table.tbl td{border-bottom:1px solid var(--line);padding:9px 11px;
  font-variant-numeric:tabular-nums;white-space:nowrap}
table.tbl td:first-child,table.tbl th:first-child{white-space:normal}
table.tbl tr:nth-child(even) td{background:#fbfcfd}
.tbl-wrap{overflow-x:auto;-webkit-overflow-scrolling:touch;margin:14px 0}
.chart{margin:14px 0}
/* 3 枚を必ず横に並べる。auto-fit(minmax) だと紙の幅で 2 枚 + 1 枚になり、
   3 枚目だけが次の行に取り残される（.kpis で直したのと同じ形） */
.panels{display:grid;grid-template-columns:repeat(3,1fr);gap:16px;margin:14px 0}
/* 狭い画面で 3 列のままだと 1 枚 180px を切り、月のラベルが読めなくなる。
   縦に積む。紙は常に 3 列（@media print は max-width を見ない） */
@media (max-width:640px){.panels{grid-template-columns:1fr}}
.panel-name{font-size:13.5px;font-weight:700;margin:0;line-height:1.6}
.panel-sub{color:var(--ink-soft);font-size:11.5px;line-height:1.7;margin:0 0 2px;
  font-variant-numeric:tabular-nums}
@media (max-width:640px){
  .wrap{padding:28px 16px 56px}
  body{font-size:14px}
  .cover h1{font-size:24px}
  h2{font-size:18px}
  table.tbl{font-size:12.5px;min-width:600px}
}
.next{border:1px solid var(--line);border-left:4px solid var(--accent);border-radius:6px;
  padding:16px 18px;margin:26px 0 0;background:var(--panel)}
.next h3{margin:0 0 6px;font-size:15px}
.pick{margin:18px 0 0;padding:14px 16px;border:1px solid var(--line);border-radius:6px}
.pick form{display:flex;gap:8px;flex-wrap:wrap;align-items:center;margin-top:6px}
.pick select{padding:6px 8px;font-size:14px;border:1px solid var(--line);border-radius:4px;
  background:var(--bg);color:var(--ink)}
.pick button{padding:6px 14px;font-size:14px;border:1px solid var(--accent);border-radius:4px;
  background:var(--bg);color:var(--accent);cursor:pointer}
@media print{.pick{display:none}}
.next p{margin:.35em 0;font-size:14px;color:var(--ink-soft)}
.foot{margin-top:56px;padding-top:18px;border-top:1px solid var(--line);
  color:var(--ink-soft);font-size:12.5px;line-height:1.9}
@media print{
  /* 余白は @page 側だけで持つ。body と二重にかけると本文の幅が縮む */
  .wrap{max-width:none;padding:0}
  h2{break-after:avoid} h3{break-after:avoid}
  table{break-inside:auto} tr{break-inside:avoid}
  .tbl-wrap{overflow:visible}
  .kpi-card,.chart,.note,.panels>div{break-inside:avoid}
  /* 図は画面幅のまま描かれた SVG を持っている。紙の本文幅はそれより狭いので、
     そのままでは右にはみ出す。実測（A4・余白 16/14mm）では、分類別の図が
     用紙 596pt に対し 654pt まで出て 2026-06 以降が消え、全体の動きの 3 枚は
     隣どうし重なっていた。CHART_INIT が viewBox を付けてあるので、
     幅を紙に合わせれば中身ごと縮む。高さは SVG の比率に任せる。

     ECharts は器の中にもう 1 枚 div を作り、そこに px 直書きの幅と
     overflow:hidden を置く。SVG は position:absolute でその中に浮いている。
     SVG だけ 100% にしても、基準になるのがこの固定幅の div なので何も変わらない
     （実測で幅 904px のまま動かなかった）。中の div ごと広げ、SVG を
     いったん流し込みに戻してから幅を与える。印刷指定はスタイルシート側なので、
     !important を付ければ ECharts のインライン指定より強い */
  .echart{height:auto!important}
  .echart>div{width:100%!important;height:auto!important;overflow:visible!important}
  .echart svg{position:static!important;width:100%!important;height:auto!important}
  /* 背景を落とすと表の縞と注記の枠が消えて読みにくくなる */
  *{-webkit-print-color-adjust:exact;print-color-adjust:exact}
}
@page{size:A4;margin:16mm 14mm}
"#;

/// 図を実際に描くための最小限の script。
///
/// アプリの中では `static/js/app.js` が `.echart[data-chart-config]` を拾って
/// 初期化しているが、このレポートは 1 枚で完結する別ページなので、その仕組みが無い。
/// ライブラリを読み込んだだけでは 1 枚も描かれない（実測で 2 枚とも空だった）。
const CHART_INIT: &str = r#"<script>
(function () {
  // ECharts の SVG には width/height だけが入り viewBox が無い。無いと
  // CSS で幅を詰めても中身が縮まず、右がそのまま切れる。紙の本文幅は
  // 画面より狭いので、印刷のたびに図がはみ出していた（実測で用紙 596pt に
  // 対し図が 654pt。全体の動きの 3 枚は隣どうし重なっていた）。
  // 描いた直後の実寸を viewBox に写しておけば、あとは幅を与えるだけで縮む。
  function fit(el) {
    var svg = el.querySelector('svg');
    if (!svg) return;
    var w = svg.getAttribute('width');
    var h = svg.getAttribute('height');
    if (!w || !h) return;
    svg.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    svg.setAttribute('preserveAspectRatio', 'xMidYMid meet');
  }
  function draw() {
    if (typeof echarts === 'undefined') return;
    document.querySelectorAll('.echart[data-chart-config]').forEach(function (el) {
      if (echarts.getInstanceByDom(el)) { fit(el); return; }
      try {
        var cfg = JSON.parse(el.getAttribute('data-chart-config'));
        cfg.backgroundColor = 'transparent';
        cfg.animation = false;              // 印刷で描き終わる前に出ないように
        cfg.aria = { enabled: true, decal: { show: true } };
        // いちばん右の月のラベルは、軸の端から半分はみ出す位置に置かれる。
        // 右の余白が足りないと外にはみ出した分が切れる。実測では
        // 分類の図の「2026-08」が「2026-0」になっていた。最新月は
        // このレポートでいちばん見たい月なので、足りなければ広げる
        if (cfg.grid && typeof cfg.grid.right === 'number' && cfg.grid.right < 28) {
          cfg.grid.right = 28;
        }
        // 分類 → 色 の表は 12 分類を 6 色に載せているので、同じ図に
        // 同じ色の線が 2 本入りうる（線種は必ず違う）。線そのものは紙でも
        // 実線・破線・点線を見分けられたが、凡例の印は既定 25px だと
        // 破線が「ほぼ実線の棒 + 点 1 つ」にしか見えず、名前と線を
        // 結びつけられなかった（A4 で刷って確認）。印を広げると 3 種類が
        // はっきり分かれる。画面でも損はしない
        if (cfg.legend && cfg.legend.show !== false) {
          cfg.legend.itemWidth = 42;
          cfg.legend.itemGap = 18;
        }
        // 1 枚 1 系列の図（2 節）。色や線種で区別する相手がいないのに
        // 分類と同じパレットから採ると、隣の 3 節で同じ色が別の分類を指す。
        // 本文と同じ墨色の実線にそろえ、色は 3 節だけの手がかりにする
        if (el.hasAttribute('data-solo')) {
          (cfg.series || []).forEach(function (s) {
            s.itemStyle = { color: '#334155' };
            s.lineStyle = { width: 2, type: 'solid' };
          });
          cfg.legend = { show: false };     // 系列名は図の見出しに出ている
          if (cfg.grid) cfg.grid.bottom = 6; // 凡例のために空けていた分を線に回す
        }
        // canvas は印刷エンジンによって出たり出なかったりする。SVG なら確実
        echarts.init(el, null, { renderer: 'svg' }).setOption(cfg);
        fit(el);
      } catch (e) {
        // 図が出せなくても本文と表で意味が通るようにしてある。ここでは黙って諦める
        el.style.display = 'none';
      }
    });
  }
  // 描き直すと width/height が書き換わる。viewBox が古いままだと像が歪むので
  // resize と fit は必ず対で呼ぶ
  function redraw() {
    document.querySelectorAll('.echart').forEach(function (el) {
      var c = echarts.getInstanceByDom(el);
      if (c) { c.resize(); fit(el); }
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', draw);
  } else {
    draw();
  }
  window.addEventListener('load', draw);
  window.addEventListener('resize', redraw);
  // 印刷の直前にもう一度描く。幅が変わると中身がずれるため。
  // この時点ではまだ画面の幅なので、縮めるのは viewBox と印刷 CSS の仕事
  window.addEventListener('beforeprint', redraw);
})();
</script>"#;

fn render_report(snap: &Snapshot, pref: Option<&str>) -> String {
    let months = &snap.meta.months;
    let prefs = snap.prefectures();
    let pref = pref.filter(|p| !p.is_empty() && prefs.iter().any(|x| x == p));

    // 社内タブと同じ関数を通す。ここで合算をやり直さない
    let overview = match pref {
        None => nation_overview(snap),
        Some(p) => pref_overview(snap, p),
    };

    let scope = match pref {
        None => "全国".to_string(),
        Some(p) => p.to_string(),
    };
    let period = format!(
        "{} 〜 {}",
        months.first().map(String::as_str).unwrap_or("—"),
        snap.meta.latest
    );

    let mut h = String::with_capacity(120_000);
    h.push_str(&format!(
        "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>採用市場レポート {scope}</title>\
         <script src=\"https://cdn.jsdelivr.net/npm/echarts@5.5.1/dist/echarts.min.js\" \
         integrity=\"sha384-Mx5lkUEQPM1pOJCwFtUICyX45KNojXbkWdYhkKUKsbv391mavbfoAmONbzkgYPzR\" \
         crossorigin=\"anonymous\"></script>\
         <style>{CSS}</style></head><body><div class=\"wrap\">",
        scope = esc(&scope)
    ));

    // 表紙
    h.push_str(&format!(
        "<div class=\"cover\"><h1>採用市場レポート<br>{scope}・{period}</h1>\
         <p class=\"sub\">対象 {n} 職種／{np} 都道府県　作成 {built}<br>\n         数字の増減は、この {mn} か月全体の変化です（直近 1 か月の変化ではありません）。\n         月ごとの上下をならした線に沿って出しています。</p></div>",
        scope = esc(&scope),
        period = esc(&period),
        n = snap.titles.len(),
        np = prefs.len(),
        built = esc(snap.meta.built_at.get(0..10).unwrap_or("")),
        mn = months.len()
    ));

    // 要約
    h.push_str("<h2>1. この期間に起きたこと</h2>");
    h.push_str(&format!(
        "<p class=\"lead\">{}</p><p class=\"lead\">{}</p>",
        esc(&overview.job.sentence),
        esc(&overview.spp.sentence)
    ));

    h.push_str("<div class=\"kpis\">");
    // 単位はここで持つ。「求人の数」という名前だけでは、件なのか人なのか
    // 社なのかが読み手に分からない
    for (m, unit) in [
        (&overview.job, "件"),
        (&overview.ctk, "人"),
        (&overview.emp, "社"),
        (&overview.spp, "人"),
    ] {
        h.push_str(&format!(
            "<div class=\"kpi-card\"><div class=\"kpi-label\">{n}</div>\
             <div class=\"kpi-value\">{v}</div>\
             <div class=\"kpi-chg {dc}\">{a} {p}</div></div>",
            n = esc(&m.label),
            v = headline_jp(m.latest, unit),
            dc = dir_class(m.change_pct, false),
            // 増減を色だけで伝えない。赤と緑の見え方が違う人には符号しか
            // 手がかりが残らない（`render::arrow` と同じ理由）
            a = arrow(m.change_pct),
            p = pct_opt(m.change_pct)
        ));
    }
    h.push_str("</div>");

    // 全体の動き
    h.push_str("<h2>2. 全体の動き</h2>");
    let panels: [(&Metric, &str); 3] = [
        (&overview.job, "件"),
        (&overview.ctk, "人"),
        (&overview.emp, "社"),
    ];
    // 以前は「求人数と見た人数では桁が 40 倍ほど違う」という固定の文だった。
    // 40 倍なのは見た人数と企業数の比で、文が名指ししている求人数と見た人数は
    // 実データでは 11 倍。しかも固定文字なので、データが変われば黙って嘘になる。
    // 出すたびに数えて書く
    h.push_str(&format!(
        "<p>実数です。{ratio}1 つの目盛りに重ねず 3 つ並べています。\
         号をまたいで同じ数字を比べられます。</p>",
        ratio = match spread(&panels) {
            Some(r) => format!("いちばん多い数といちばん少ない数で {} 倍ちがうため、", num(r)),
            None => String::new(),
        }
    ));
    h.push_str(&overview_panels(months, &panels));

    // 分類別（全国のみ）
    if pref.is_none() {
        let cats = category_table(snap);
        h.push_str("<h2>3. 分類ごとの動き</h2>");

        // 変化率だけで代表を選ぶと、件数の少ない分類が上に来る。
        // 実際、最新月で全体の 0.2% しかない分類が +171% と出た。
        // 算数としては正しくても、レポートの冒頭に置くと読み手を誤らせる。
        // 母数が小さいものは代表から外し、件数も併記する。
        const MIN_SHARE: f64 = 0.02;
        let total: f64 = cats.iter().filter_map(|c| c.job_latest).sum();
        let major: Vec<CategoryRow> = cats
            .iter()
            .filter(|c| c.job_latest.unwrap_or(0.0) >= total * MIN_SHARE)
            .cloned()
            .collect();
        if let Some((row, positive)) = extreme(&major, true) {
            let verb = if positive {
                "増え方が大きい"
            } else {
                "減り方が小さい"
            };
            // 「件数の多い分類のなかで」とだけ書くと、実際には 10 番目の分類でも
            // 上位のように読める。どこで線を引いたのかを数字で示す。
            h.push_str(&format!(
                "<p>最新月に {min} 件以上ある分類のなかで、この期間の求人数の{verb}のは\
                 {name}です（{p}、最新月 {j} 件）。分類によって動きの向きも幅も異なります。</p>",
                min = num_opt(Some(total * MIN_SHARE)),
                verb = verb,
                name = esc(&row.name),
                p = pct_opt(row.job_change_pct),
                j = num_opt(row.job_latest)
            ));
        }
        h.push_str(
            "<p>件数の少ない分類は、少しの増減でも変化率が大きく出ます。\
             下の表では変化率と件数を並べていますので、両方を見てください。</p>",
        );
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
            "<div class=\"chart\">{}</div>",
            raw_line_chart(
                months,
                &refs
                    .iter()
                    .map(|(n, o)| (n.clone(), o.job.series.clone()))
                    .collect::<Vec<_>>(),
                false,
                340,
                "件"
            )
        ));

        h.push_str("<h3>分類別の定点表</h3>");
        h.push_str(
            "<p>毎号この同じ表を載せます。号をまたいで同じ場所を見れば、変化がそのまま追えます。</p>",
        );
        h.push_str(&category_table_html(&cats, false));
    }

    // 職種の上位
    h.push_str(if pref.is_none() {
        "<h2>4. 職種ごとの動き（求人数の多い順・上位 30）</h2>"
    } else {
        "<h2>3. 職種ごとの動き（求人数の多い順・上位 30）</h2>"
    });
    h.push_str(&title_table_html(snap, pref, 30));

    // 読むときの注意
    h.push_str(if pref.is_none() {
        "<h2>5. このレポートについて</h2>"
    } else {
        "<h2>4. このレポートについて</h2>"
    });
    h.push_str(
        "<p>このレポートの数字は、求人媒体に掲載された求人と、そこでの閲覧の記録にもとづいています。\n         次の点にご注意のうえお読みください。</p>",
    );
    h.push_str(&format!(
        "<div class=\"note\">{}</div>",
        esc(&snap.meta.caveat)
    ));
    h.push_str(
        "<p>「1 求人あたりに見た人数」は、求人を見た人数を求人の数で割ったものです。\
         この数が下がるとき、見た人が減った場合と、求人が増えた場合の両方があります。\
         どちらが効いているかは、「求人を見た人数」と「求人の数」の 2 つを見比べると分かります。</p>",
    );

    // 地域で絞れることは、読んだ人がいちばん確かめたいところ。
    // 機能はあるのに選ぶ場所が無いと、無いのと同じになる。
    // 印刷には出さない（紙の上では押せないため）。
    h.push_str("<div class=\"pick\">");
    h.push_str("<strong>地域を選んで見る</strong>");
    h.push_str("<form method=\"get\" action=\"/report/indeed\">");
    h.push_str("<select name=\"pref\" aria-label=\"都道府県\">");
    h.push_str(&format!(
        "<option value=\"\"{}>全国</option>",
        if pref.is_none() { " selected" } else { "" }
    ));
    for p in &prefs {
        h.push_str(&format!(
            "<option value=\"{v}\"{sel}>{v}</option>",
            v = esc(p),
            sel = if pref == Some(p.as_str()) {
                " selected"
            } else {
                ""
            }
        ));
    }
    h.push_str("</select><button type=\"submit\">この地域で見る</button></form></div>");

    // このレポートは商談の入口として配る。診断も指示もしないが、
    // 読んだ人が次に何をすればよいか分からないままでは入口にならない。
    // 押し付けずに、同じ数字を自社の範囲で出せることだけを置く。
    h.push_str(&format!(
        "<div class=\"next\"><h3>この先の見方</h3>\
         <p>ここに載せているのは{scope}の全体像です。同じ数字は、職種や地域を\
         絞っても同じ作り方で出せます。自社が採用している職種と、実際に募集している\
         地域だけに絞ると、全体の平均とは違う動きが見えることがあります。</p>\
         <p>この {n} 職種・{np} 都道府県のうち、気になる組み合わせがあれば\
         同じ期間・同じ指標でお出しします。{contact}</p></div>",
        scope = esc(&scope),
        n = snap.titles.len(),
        np = prefs.len(),
        // 連絡先はここでは決めない。運用（営業がメールに添付するのか、
        // URL 単体で配るのか）が分からないまま宛先を書くと嘘になる。
        // INDEED_CONTACT に URL か mailto: を入れたときだけ 1 本出す。
        contact = match std::env::var("INDEED_CONTACT") {
            Ok(v) if !v.trim().is_empty() => format!(
                "<br><a href=\"{u}\">お問い合わせはこちら</a>",
                u = esc(v.trim())
            ),
            _ => String::new(),
        }
    ));

    h.push_str(&format!(
        "<div class=\"foot\">出どころ：{src}<br>対象 {period}／作成 {built}</div>",
        src = esc(&snap.meta.source),
        period = esc(&period),
        built = esc(snap.meta.built_at.get(0..10).unwrap_or(""))
    ));

    h.push_str(CHART_INIT);
    h.push_str("</div></body></html>");
    h
}

/// 見出しに出す大きな数。単位まで含めて返す。
///
/// # なぜ万で出すのか
/// 指数をやめて実数に戻したので、見た人数は 18,240,805 のような 8 桁で出る。
/// 桁を数えないと大きさが分からず、号をまたいで見比べるときに読み違える。
/// 1 万を超えるものは「1,824.1 万人」と万で出す。小数第 1 位まで残すので
/// 1,000 人単位の動きは見える。実数のほうは 2 節の図の見出しと表に出ている。
fn headline_jp(v: Option<f64>, unit: &str) -> String {
    let Some(x) = v else {
        return "—".to_string();
    };
    let (n, u) = if x.abs() >= 10_000.0 {
        // 先に 1,000 で割って四捨五入してから 10 で割る。
        // 小数の引き算で 0.1 が 0.0999… になっても桁が崩れない
        let y = (x / 1_000.0).round() / 10.0;
        let i = y.trunc();
        let f = ((y - i).abs() * 10.0).round() as i64;
        (format!("{}.{}", num(i), f), format!("万{unit}"))
    } else if x.abs() < 1_000.0 {
        // 「1 求人あたり 11.2 人」を整数に丸めると、月ごとの動きが見えなくなる
        (format!("{x:.1}"), unit.to_string())
    } else {
        (num(x), unit.to_string())
    };
    format!("{n}<span class=\"kpi-unit\">{u}</span>", u = esc(&u))
}

/// 並べる指標のあいだで、いちばん多い数といちばん少ない数が何倍離れているか。
///
/// 2 倍を切るなら「桁が違うので重ねなかった」という断りがそもそも要らないので、
/// そのときは何も返さない。
fn spread(panels: &[(&Metric, &str)]) -> Option<f64> {
    let v: Vec<f64> = panels.iter().filter_map(|(m, _)| m.latest).collect();
    if v.len() < 2 {
        return None;
    }
    let max = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min = v.iter().copied().fold(f64::INFINITY, f64::min);
    (min > 0.0 && max / min >= 2.0).then_some(max / min)
}

/// 規模の違う指標を、重ねずに 1 枚ずつ並べる。
///
/// # なぜ [`super::render::small_multiples`] を使わないのか
/// あちらは図の見出しの増減を「最初の月と最後の月の素の比」で出す。
/// このレポートの増減はどこも [`Metric::change_pct`]（月ごとの上下をならした
/// 線に沿った変化）で、表紙にもそう書いてある。実データでは同じ
/// 「募集している企業の数」が、すぐ上のカードで +11.9%、図の見出しで +3.4% と
/// 並んでいた。同じページの同じ指標に 2 つの値が出ている状態は直さないといけない。
/// ここは Metric の値だけを使い、カード・図・表をすべて同じ数にそろえる。
///
/// 枠は [`CSS`] の `.panels` で持つ。あちらの grid はインライン style で
/// `auto-fit` なので、紙の幅で 2 枚 + 1 枚に割れて 3 枚目が取り残される。
fn overview_panels(months: &[String], panels: &[(&Metric, &str)]) -> String {
    let mut h = String::from("<div class=\"panels\">");
    for (m, unit) in panels {
        let one = vec![(m.label.clone(), m.series.clone())];
        h.push_str(&format!(
            "<div><p class=\"panel-name\">{n}</p>\
             <p class=\"panel-sub\">{last} は {v} {u}\
             （この期間 <span class=\"{dc}\">{a} {p}</span>）</p>{chart}</div>",
            n = esc(&m.label),
            last = esc(months.last().map(String::as_str).unwrap_or("")),
            // ここは概観ではなく突き合わせ用なので実数のまま出す
            v = num_opt(m.latest),
            u = esc(unit),
            dc = dir_class(m.change_pct, false),
            a = arrow(m.change_pct),
            p = pct_opt(m.change_pct),
            chart = solo_chart(&raw_line_chart(months, &one, false, 190, unit))
        ));
    }
    h.push_str("</div>");
    h
}

/// 2 節の図に「1 枚 1 系列である」という印を付ける。
///
/// [`raw_line_chart`] は系列名から色と線種を決める。3 節ではそれが
/// 「分類 → 見た目」の約束になっていて意味があるが、2 節は 1 枚 1 系列で、
/// 色に区別の役目がない。それでも同じパレットから採るので、実データでは
/// 2 節の「求人の数」と 3 節の「事務・オフィスワーク」がどちらも薔薇色の破線、
/// 2 節の「求人を見た人数」と 3 節の「製造・生産」がどちらも同じ青になっていた。
/// 隣り合う節で同じ見た目が別のものを指すと、読み手の対応づけが壊れる。
///
/// # なぜ属性を足すだけなのか
/// 軸・単位・万への丸め・欠測の扱いは [`raw_line_chart`] のものを使いたい。
/// ここで別に組むと、2 節と 3 節で数字の作り方が分かれる。
/// 最初は出来上がった JSON の色や凡例を文字列で置き換えていたが、
/// [`raw_line_chart`] が `grid` の書き方を変えた時点で置換が空振りした
/// （色は変わったのに余白だけ元のまま、という気づきにくい壊れ方をした）。
/// 印だけ付けて、実際の差し替えは [`CHART_INIT`] が `JSON.parse` した後の
/// オブジェクトに対して行う。あちらは形で触るので、書き方が変わっても効く。
fn solo_chart(chart: &str) -> String {
    // `raw_line_chart` の出だしはこの形。変わったら印が付かず、
    // 色と凡例が既定のまま出る（tests で押さえてある）
    chart.replacen(
        "<div class=\"echart\"",
        "<div class=\"echart\" data-solo=\"1\"",
        1,
    )
}

fn title_table_html(snap: &Snapshot, pref: Option<&str>, limit: usize) -> String {
    let months = &snap.meta.months;
    let mut h = String::from(
        "<div class=\"tbl-wrap\"><table class=\"tbl\"><thead><tr>\
         <th scope=\"col\" style=\"text-align:left\">職種</th>\
         <th scope=\"col\" style=\"text-align:left\">分類</th>\
         <th scope=\"col\" style=\"text-align:right\">求人数（最新月）</th>\
         <th scope=\"col\" style=\"text-align:right\">求人数の変化</th>\
         <th scope=\"col\" style=\"text-align:right\">1求人あたり</th>\
         <th scope=\"col\" style=\"text-align:right\">その変化</th></tr></thead><tbody>",
    );
    let rows: Vec<(String, String, Overview)> = match pref {
        None => crate::indeed::aggregate::title_table(snap)
            .into_iter()
            .take(limit)
            .filter_map(|r| {
                snap.by_title.get(&r.name).map(|s| {
                    (
                        r.name.clone(),
                        r.category.clone(),
                        Overview::from_series(&r.name, s, months),
                    )
                })
            })
            .collect(),
        // 社内タブと同じ関数を通す
        Some(p) => {
            let mut v = pref_title_overviews(snap, p);
            // 欠測は 0 ではなく最下位に置く。0 扱いだと「多い順」の途中に紛れる
            v.sort_by(|a, b| {
                b.2.job
                    .latest
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&a.2.job.latest.unwrap_or(f64::NEG_INFINITY))
            });
            v.truncate(limit);
            v
        }
    };
    for (name, cat, o) in rows {
        h.push_str(&format!(
            "<tr><td>{n}</td><td>{c}</td>\
             <td style=\"text-align:right\">{j}</td>\
             <td style=\"text-align:right\" class=\"{jc}\">{jp}</td>\
             <td style=\"text-align:right\">{s}</td>\
             <td style=\"text-align:right\" class=\"{sc}\">{sp}</td></tr>",
            n = esc(&name),
            c = esc(&cat),
            j = num_opt(o.job.latest),
            jc = dir_class(o.job.change_pct, false),
            jp = pct_opt(o.job.change_pct),
            s = dec1_opt(o.spp.latest),
            sc = dir_class(o.spp.change_pct, false),
            sp = pct_opt(o.spp.change_pct)
        ));
    }
    h.push_str("</tbody></table></div>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::local_sqlite::LocalDb;
    use crate::indeed::data::load;

    /// 実データを読む。テスト用のダミーは作らない。
    ///
    /// 作りかけのレポートを見るのに架空の数字を使うと、桁数も欠測も本物と違い、
    /// 「実データでだけ崩れる」ものを見逃す。同梱の gz から本番と同じ手順で開く。
    fn real_snapshot() -> Snapshot {
        crate::ensure_db_from_gz("data/indeed_insights.db");
        let db = LocalDb::new("data/indeed_insights.db").expect("Indeed 分析 DB を開けない");
        load(&db).expect("Indeed 分析 DB を読めない")
    }

    /// 節を切り出す。節ごとに見たいので、全文に対する contains では足りない。
    fn section<'a>(html: &'a str, from: &str, to: &str) -> &'a str {
        let a = html.find(from).unwrap_or_else(|| panic!("{from} が無い"));
        let b = html[a..].find(to).map(|i| a + i).unwrap_or(html.len());
        &html[a..b]
    }

    /// カードと図の見出しで、同じ指標に同じ増減が出ていること。
    ///
    /// # 逆向きにも見る
    /// 「期待した数字が入っている」だけだと、素の比がたまたま同じ値のときにも通る。
    /// 素の比（最初の月と最後の月だけを見た値）が違う値になる指標については、
    /// **その素の比が 2 節に出ていないこと**まで見る。直す前はここが
    /// カード +11.9% ／ 図の見出し +3.4% に割れていた。
    #[test]
    fn カードと図で同じ指標の増減がそろっている() {
        let snap = real_snapshot();
        let html = render_report(&snap, None);
        let sec2 = section(&html, "<h2>2. ", "<h2>3. ");
        let ov = crate::indeed::aggregate::nation_overview(&snap);
        let wants: Vec<String> = [&ov.job, &ov.ctk, &ov.emp]
            .iter()
            .map(|m| pct_opt(m.change_pct))
            .collect();

        for m in [&ov.job, &ov.ctk, &ov.emp] {
            let want = pct_opt(m.change_pct);
            assert!(
                sec2.contains(&want),
                "{} の図の見出しに、カードと同じ {want} が無い",
                m.label
            );
            let first = m.series.iter().flatten().next().copied();
            let last = m.series.iter().flatten().next_back().copied();
            let (Some(a), Some(b)) = (first, last) else { continue };
            if a <= 0.0 {
                continue;
            }
            let raw = pct_opt(Some((b - a) / a * 100.0));
            // 素の比が、たまたま別の指標の期間変化と同じ文字列になることはある。
            // そのときは見分けられないので判定しない
            if raw != want && !wants.iter().any(|w| *w == raw) {
                assert!(
                    !sec2.contains(&raw),
                    "{} の 2 節に素の比 {raw} が出ている（期間の増減は {want}）",
                    m.label
                );
            }
        }
    }

    /// 色に意味を持たせるのは分類の図だけ。
    ///
    /// 1 枚 1 系列の図が分類と同じパレットから色を採ると、隣の節で
    /// 同じ色が別の分類を指す。印が付かなくなったら（[`solo_chart`] が
    /// 当てにしている出だしが変わったら）ここで落ちる。
    #[test]
    fn 一枚一系列の図には色の意味を持たせない() {
        let snap = real_snapshot();
        let html = render_report(&snap, None);
        let sec2 = section(&html, "<h2>2. ", "<h2>3. ");
        assert_eq!(
            sec2.matches("data-solo=\"1\"").count(),
            3,
            "2 節の 3 枚に印が付いていない"
        );
        let sec3 = section(&html, "<h2>3. ", "<h2>4. ");
        assert!(
            !sec3.contains("data-solo"),
            "分類の図に印が付いている。色と線種の約束が消える"
        );
    }

    /// 指数をやめたのに、指数を前提にした説明が残っていないこと。
    #[test]
    fn 指数だったころの言い回しが残っていない() {
        let snap = real_snapshot();
        for pref in [None, Some("三重県")] {
            let html = render_report(&snap, pref);
            for w in ["指数", "を 100", "100 として", "基準の月"] {
                assert!(!html.contains(w), "「{w}」が残っている（pref={pref:?}）");
            }
        }
    }

    /// 大きな桁は万で出す。桁を数えないと大きさが分からないため。
    #[test]
    fn 見出しの数は万で出す() {
        assert!(
            headline_jp(Some(18_240_805.0), "人").starts_with("1,824.1<span"),
            "{}",
            headline_jp(Some(18_240_805.0), "人")
        );
        assert!(headline_jp(Some(1_624_095.0), "件").starts_with("162.4<span"));
        assert!(headline_jp(Some(450_838.0), "社").starts_with("45.1<span"));
        // 1 求人あたりは小数第 1 位まで残す。整数に丸めると動きが消える
        assert!(headline_jp(Some(11.23), "人").starts_with("11.2<span"));
        assert_eq!(headline_jp(None, "件"), "—");
        // 単位は必ず添える。「求人の数 162.4」だけでは件か人か分からない
        assert!(headline_jp(Some(1_624_095.0), "件").contains("万件"));
    }

    /// 目で見るための HTML を書き出す。
    ///
    /// # なぜテストの中から出すのか
    /// `/report/indeed` は `INDEED_PUBLIC` が立つまで 404 で、動いているサーバを
    /// 開け直さないと現物が見られない。図の重なりや印刷の切れ方は読むだけでは
    /// 分からないので、実データを通した HTML をそのままファイルに落として開く。
    ///
    /// 既定では何も書かない。`INDEED_REPORT_DUMP=<出力先ディレクトリ>` を
    /// 指定したときだけ、全国版と県版を書き出す。
    #[test]
    fn レポートを目視用に書き出す() {
        let Ok(dir) = std::env::var("INDEED_REPORT_DUMP") else {
            return;
        };
        let snap = real_snapshot();
        std::fs::create_dir_all(&dir).expect("出力先を作れない");
        std::fs::write(
            format!("{dir}/report_nation.html"),
            render_report(&snap, None),
        )
        .expect("全国版を書けない");
        let pref = snap.prefectures().first().cloned().unwrap_or_default();
        std::fs::write(
            format!("{dir}/report_pref.html"),
            render_report(&snap, Some(&pref)),
        )
        .expect("県版を書けない");
        eprintln!("dumped to {dir} (pref={pref})");
    }
}
