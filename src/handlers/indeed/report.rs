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
    category_table_html, dec1_opt, dir_class, esc, headline_opt, indexed_chart, line_chart,
    num_opt, pct_opt,
};
use crate::indeed::aggregate::{
    category_table, extreme, nation_overview, pref_overview, pref_title_overviews, CategoryRow,
    Overview,
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
  .kpi-card,.chart,.note{break-inside:avoid}
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
  function draw() {
    if (typeof echarts === 'undefined') return;
    document.querySelectorAll('.echart[data-chart-config]').forEach(function (el) {
      if (echarts.getInstanceByDom(el)) return;
      try {
        var cfg = JSON.parse(el.getAttribute('data-chart-config'));
        cfg.backgroundColor = 'transparent';
        cfg.animation = false;              // 印刷で描き終わる前に出ないように
        cfg.aria = { enabled: true, decal: { show: true } };
        // canvas は印刷エンジンによって出たり出なかったりする。SVG なら確実
        echarts.init(el, null, { renderer: 'svg' }).setOption(cfg);
      } catch (e) {
        // 図が出せなくても本文と表で意味が通るようにしてある。ここでは黙って諦める
        el.style.display = 'none';
      }
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', draw);
  } else {
    draw();
  }
  window.addEventListener('load', draw);
  window.addEventListener('resize', function () {
    document.querySelectorAll('.echart').forEach(function (el) {
      var c = echarts.getInstanceByDom(el);
      if (c) c.resize();
    });
  });
  // 印刷の直前にもう一度描く。幅が変わると中身がずれるため
  window.addEventListener('beforeprint', function () {
    document.querySelectorAll('.echart').forEach(function (el) {
      var c = echarts.getInstanceByDom(el);
      if (c) c.resize();
    });
  });
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
    for m in [
        &overview.job,
        &overview.ctk,
        &overview.emp,
        &overview.spp,
    ] {
        h.push_str(&format!(
            "<div class=\"kpi-card\"><div class=\"kpi-label\">{n}</div>\
             <div class=\"kpi-value\">{v}</div>\
             <div class=\"{dc}\" style=\"font-size:13px;font-variant-numeric:tabular-nums\">{p}</div></div>",
            n = esc(&m.label),
            v = headline_opt(m.latest),
            dc = dir_class(m.change_pct, false),
            p = pct_opt(m.change_pct)
        ));
    }
    h.push_str("</div>");

    // 全体の動き
    h.push_str("<h2>2. 全体の動き</h2>");
    h.push_str(
        "<p>期間の最初の月を 100 として重ねています。件数の大小ではなく、\
         増え方・減り方の違いを見るための図です。</p>",
    );
    h.push_str(&format!(
        "<div class=\"chart\">{}</div>",
        line_chart(
            months,
            &[
                ("求人の数".to_string(), overview.job.indexed()),
                ("求人を見た人数".to_string(), overview.ctk.indexed()),
                ("募集している企業の数".to_string(), overview.emp.indexed()),
            ],
            false,
            320
        )
    ));

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
            .take(7)
            .filter_map(|c| {
                snap.by_category
                    .get(&c.name)
                    .map(|s| (c.name.clone(), Overview::from_series(&c.name, s, months)))
            })
            .collect();
        let refs: Vec<(String, &Overview)> = top.iter().map(|(n, o)| (n.clone(), o)).collect();
        h.push_str(&format!(
            "<div class=\"chart\">{}</div>",
            indexed_chart(months, &refs, false, 340)
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
