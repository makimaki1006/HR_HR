//! Indeed の社内タブと顧客レポートが共通で使う描画部品。
//!
//! 数字の丸め方と言い回しをここに集める。画面ごとに書くと、
//! 同じ指標が画面によって違う値に見える。

use crate::indeed::aggregate::{CategoryRow, Metric, Overview};

/// HTML に入れてよい形に直す。職種名は DB 由来なので必ず通す。
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// JSON の文字列に入れてよい形に直してから、HTML 属性用にも退避する。
///
/// 図の設定は `data-chart-config='{...}'` という 1 つの属性に JSON を入れている。
/// 職種名は DB 由来なので、二重引用符や逆スラッシュが入ると JSON が壊れ、
/// その図だけが黙って消える。今のデータには無いが、増えた職種で起きうる。
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    esc(&out)
}

/// URL のクエリに入れてよい形にしてから、HTML 属性用にも退避する。
///
/// 職種名は日本語で、`&` や `#` を含む可能性もある。素のまま
/// `?name=...` に入れると、そこでクエリが切れたり別の引数に化けたりする。
/// 英数字と `-_.~` 以外はすべて %XX にする（RFC 3986 の unreserved）。
pub fn url_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    // ここまでで `&"'<>` は残らないが、属性に入れる以上は同じ道を通す
    esc(&out)
}

/// 3 桁区切り。
pub fn num(v: f64) -> String {
    let neg = v < 0.0;
    let n = v.abs().round() as i64;
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// 欠測は「—」。0 と欠測を同じ見た目にしない。
pub fn num_opt(v: Option<f64>) -> String {
    match v {
        Some(x) => num(x),
        None => "—".to_string(),
    }
}

/// 見出しに出す数。
///
/// 「1 求人あたり 10.2 人」を整数に丸めると 10 になり、
/// 前月からの動きが見えなくなる。桁が小さいものは小数第 1 位まで出す。
pub fn headline_opt(v: Option<f64>) -> String {
    match v {
        None => "—".to_string(),
        Some(x) if x.abs() < 1000.0 => format!("{x:.1}"),
        Some(x) => num(x),
    }
}

/// 符号つきのパーセント。
pub fn pct_opt(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:+.1}%"),
        None => "—".to_string(),
    }
}

/// 小数第 1 位まで。
pub fn dec1_opt(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.1}"),
        None => "—".to_string(),
    }
}

/// 増減の向きを、色以外でも分かる形で出す。
///
/// 赤と緑だけで増減を伝えると、色の見え方が違う人に伝わらない。
pub fn arrow(v: Option<f64>) -> &'static str {
    match v {
        Some(x) if x > 0.0 => "▲",
        Some(x) if x < 0.0 => "▼",
        Some(_) => "→",
        None => "",
    }
}

/// 増減の向きに応じた色クラス。良し悪しの判断は入れない。
pub fn dir_class(v: Option<f64>, dark: bool) -> &'static str {
    match v {
        Some(x) if x > 0.0 => {
            if dark {
                "text-emerald-400"
            } else {
                "ind-up"
            }
        }
        Some(x) if x < 0.0 => {
            if dark {
                "text-rose-400"
            } else {
                "ind-down"
            }
        }
        _ => {
            if dark {
                "text-slate-400"
            } else {
                "ind-flat"
            }
        }
    }
}

/// 数の並びを ECharts が読める配列文字列にする。欠測は null。
fn series_json(v: &[Option<f64>]) -> String {
    v.iter()
        .map(|x| match x {
            Some(n) => format!("{:.3}", n),
            None => "null".to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn labels_json(months: &[String]) -> String {
    months
        .iter()
        .map(|m| format!("\"{}\"", json_str(m)))
        .collect::<Vec<_>>()
        .join(",")
}

/// 折れ線 1 本。
///
/// `data-chart-config` はシングルクォートで囲まれるので、
/// 中の JSON にシングルクォートを入れない（[`esc`] が `&#39;` に変える）。
pub fn line_chart(months: &[String], series: &[(String, Vec<Option<f64>>)], dark: bool, height: u32) -> String {
    let axis = if dark { "#94a3b8" } else { "#5b6472" };
    let palette = [
        "#38bdf8", "#f59e0b", "#34d399", "#f472b6", "#a78bfa", "#fb7185", "#4ade80",
    ];
    let items: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(i, (name, v))| {
            format!(
                "{{\"name\":\"{n}\",\"type\":\"line\",\"smooth\":false,\"showSymbol\":false,\"connectNulls\":false,\"lineStyle\":{{\"width\":2}},\"itemStyle\":{{\"color\":\"{c}\"}},\"data\":[{d}]}}",
                n = json_str(name),
                c = palette[i % palette.len()],
                d = series_json(v)
            )
        })
        .collect();
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\"tooltip\":{{\"trigger\":\"axis\"}},\"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\"grid\":{{\"left\":\"12%\",\"right\":\"4%\",\"top\":\"10%\",\"bottom\":\"22%\"}},\"xAxis\":{{\"type\":\"category\",\"boundaryGap\":false,\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\"yAxis\":{{\"type\":\"value\",\"scale\":true,\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.15}}}}}},\"series\":[{sr}]}}'></div>",
        h = height,
        ax = axis,
        lb = labels_json(months),
        sr = items.join(",")
    )
}

/// 指標 1 つ分の見出し。
///
/// # 長い文を入れない
/// 以前はここに「期間全体では 26% 増えていますが、月ごとの上下（±5%）が
/// 毎月の動き（約 2.0%）より大きく、一本調子ではありません」という 60 字の文を
/// 入れていた。5 枚並べると 1 枚の幅が画面の 1/5 しかなく、3 行以上に折り返す。
/// しかも同じ内容が「なぜそうなったか」と図の下にも出て、3 回読ませていた。
/// ここは数字を一目で拾う場所なので、動き方は短い札だけにする。
pub fn metric_card(m: &Metric, dark: bool) -> String {
    let (card, label, value) = if dark {
        (
            "bg-navy-800/60 border border-slate-700 rounded-lg p-4",
            "text-slate-400 text-xs",
            "text-slate-100 text-2xl font-bold tabular-nums",
        )
    } else {
        ("kpi-card", "kpi-label", "kpi-value")
    };
    format!(
        "<div class=\"{card}\"><div class=\"{label}\">{name}</div>\
         <div class=\"{value}\">{v}</div>\
         <div class=\"{dc} text-sm mt-1 tabular-nums\">{ar} {p}</div>\
         <div class=\"{label} mt-1\">{t}</div></div>",
        name = esc(&m.label),
        v = headline_opt(m.latest),
        dc = dir_class(m.change_pct, dark),
        ar = arrow(m.change_pct),
        p = pct_opt(m.change_pct),
        t = esc(m.label_trend)
    )
}

/// 20 分類の定点表。
pub fn category_table_html(rows: &[CategoryRow], dark: bool) -> String {
    let (wrap, th, td) = if dark {
        (
            "w-full text-sm border-collapse",
            "text-left text-slate-400 font-medium px-3 py-2 border-b border-slate-700",
            "px-3 py-2 border-b border-slate-800 text-slate-200",
        )
    } else {
        ("tbl", "", "")
    };
    let mut h = String::new();
    // 顧客レポート側は印刷指定の入った .tbl-wrap を使う。インラインの
    // overflow-x:auto のままだと印刷で overflow:visible に切り替わらず、
    // 8 列あるこの表だけが用紙からはみ出す。
    h.push_str(if dark {
        "<div style=\"overflow-x:auto\">"
    } else {
        "<div class=\"tbl-wrap\">"
    });
    h.push_str("<table class=\"");
    h.push_str(wrap);
    // 狭い幅で縮ませない。縮むと分類名が 1 文字ずつ縦に折り返る
    h.push_str("\" style=\"min-width:720px\"><thead><tr>");
    for (name, align) in [
        ("分類", "left"),
        ("職種数", "right"),
        ("求人数（最新月）", "right"),
        ("求人数の変化", "right"),
        ("見た人数の変化", "right"),
        ("1求人あたり", "right"),
        ("その変化", "right"),
        ("動き方", "left"),
    ] {
        h.push_str(&format!(
            "<th scope=\"col\" class=\"{th}\" style=\"text-align:{align}\">{name}</th>"
        ));
    }
    h.push_str("</tr></thead><tbody>");
    for r in rows {
        h.push_str(&format!(
            "<tr>\
             <td class=\"{td}\">{name}</td>\
             <td class=\"{td}\" style=\"text-align:right\">{t}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{j}</td>\
             <td class=\"{td} tabular-nums {jc}\" style=\"text-align:right\">{jp}</td>\
             <td class=\"{td} tabular-nums {cc}\" style=\"text-align:right\">{cp}</td>\
             <td class=\"{td} tabular-nums\" style=\"text-align:right\">{sl}</td>\
             <td class=\"{td} tabular-nums {sc}\" style=\"text-align:right\">{sp}</td>\
             <td class=\"{td}\">{tr}</td>\
             </tr>",
            name = esc(&r.name),
            t = r.titles,
            j = num_opt(r.job_latest),
            jc = dir_class(r.job_change_pct, dark),
            jp = pct_opt(r.job_change_pct),
            cc = dir_class(r.ctk_change_pct, dark),
            cp = pct_opt(r.ctk_change_pct),
            sl = dec1_opt(r.spp_latest),
            sc = dir_class(r.spp_change_pct, dark),
            sp = pct_opt(r.spp_change_pct),
            tr = esc(r.trend)
        ));
    }
    h.push_str("</tbody></table></div>");
    h
}

/// 分類どうしを同じ物差しで並べた図。
///
/// 業界ごとに縦軸を自動調整すると、伸びの違う業界が全部同じ形に見える。
/// 期間の最初の月を 100 に揃える。
pub fn indexed_chart(
    months: &[String],
    named: &[(String, &Overview)],
    dark: bool,
    height: u32,
) -> String {
    let series: Vec<(String, Vec<Option<f64>>)> = named
        .iter()
        .map(|(n, o)| (n.clone(), o.job.indexed()))
        .collect();
    line_chart(months, &series, dark, height)
}
