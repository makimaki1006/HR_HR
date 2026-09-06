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
///
/// # なぜ is_finite を見るのか
/// NaN や inf をそのまま書くと `[NaN,1,2]` という JSON でない文字列になり、
/// `JSON.parse` が落ちて**その図だけが黙って消える**。表は出るのに図が無い、
/// という気づきにくい壊れ方をする。今のデータでは 0 除算の手前で弾いているが、
/// 元データの型が変われば入りうるので、出口でも欠測として扱う。
fn series_json(v: &[Option<f64>]) -> String {
    v.iter()
        .map(|x| match x {
            Some(n) if n.is_finite() => format!("{:.3}", n),
            _ => "null".to_string(),
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
    let axis = axis_color(dark);
    let palette = palette(dark);
    let items: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(i, (name, v))| {
            format!(
                "{{\"name\":\"{n}\",\"type\":\"line\",\"smooth\":false,\"showSymbol\":false,\"connectNulls\":false,\"lineStyle\":{{\"width\":2,\"type\":\"{t}\"}},\"itemStyle\":{{\"color\":\"{c}\"}},\"data\":[{d}]}}",
                n = json_str(name),
                t = dash(i),
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

// ---------------------------------------------------------------------------
// 図の種類
//
// 折れ線ばかり並べると、どれも同じに見えて読み分けられない。
// 「量は多いが集まりにくいのはどれか」「どの県が集めやすいか」「相場が下限から
// どれだけ離れているか」は、それぞれ違う形の図でないと答えられない。
// ---------------------------------------------------------------------------

/// 系列の色。暗い画面と紙で分ける。
///
/// # なぜ紙で色を変えるのか
/// 暗い画面向けに選んだ明るい色（#4ade80 等）は、白い紙の上では
/// コントラスト比 1.7〜2.7 にしかならず、印刷すると薄く霞む。
/// WCAG 1.4.11 が図に求めるのは 3:1 以上。紙側は同じ色相のまま
/// 明度を落とし、いずれも 4:1 以上を確保している。
pub fn palette(dark: bool) -> [&'static str; 7] {
    if dark {
        [
            "#38bdf8", "#f59e0b", "#34d399", "#f472b6", "#a78bfa", "#fb7185", "#4ade80",
        ]
    } else {
        [
            "#0369a1", "#b45309", "#047857", "#be185d", "#6d28d9", "#b91c1c", "#4d7c0f",
        ]
    }
}

/// 折れ線の線種。色の見え方が違う人にも系列を区別してもらう。
///
/// 近い色相の組（#34d399 と #4ade80、#f472b6 と #fb7185）は、
/// 折れ線では凡例と往復しないと見分けられない。ECharts の decal は
/// 塗り面（棒・円）にしか効かず、線には乗らないので、線種で分ける。
fn dash(i: usize) -> &'static str {
    ["solid", "dashed", "dotted"][i % 3]
}

/// 散布図の点の大きさ。求人数の平方根に比例させる。
///
/// # なぜ大きさを変えるのか
/// 1 求人あたりが極端に大きい職種は、どれも求人が数十件しかない
/// （用務スタッフ 70 件で 63.0、漁師 82 件で 58.2）。同じ大きさで描くと
/// 図の上端で目立つが、商談では見ない職種。大きさで「どれだけ効く話か」を先に伝える。
///
/// # なぜ JavaScript の式で書かないのか
/// ECharts の `symbolSize` は関数を受け取れるが、図の設定は HTML 属性に入れた
/// JSON なので、関数は文字列として渡すしかない。それを関数に戻す処理は app.js に
/// しか無く、顧客レポートの初期化経路には無い。実際に文字列のまま渡って
/// **点が 1 つも描かれなかった**（軸だけが出る）。値はここで確定させる。
fn point_size(jobs: f64) -> f64 {
    if !jobs.is_finite() || jobs <= 0.0 {
        return 5.0;
    }
    (jobs.sqrt() / 14.0).clamp(5.0, 30.0)
}

/// 散布図の点の形。色だけに頼らないための区別。
fn symbol(i: usize) -> &'static str {
    ["circle", "triangle", "diamond", "rect", "pin", "arrow"][i % 6]
}

/// 軸まわりの色。暗い画面と紙で分ける。
fn axis_color(dark: bool) -> &'static str {
    if dark {
        "#94a3b8"
    } else {
        "#5b6472"
    }
}

/// 散布図。1 点 1 職種。
///
/// x に求人数（対数）、y に 1 求人あたりに見た人数を置くと、
/// 右下＝「募集は多いのに人が集まっていない」区画になる。
/// 表を上から読むより、position で先に目が行く。
///
/// `points` は (職種名, 求人数, 1 求人あたり, 業界名)。
pub fn scatter_chart(
    points: &[(String, f64, f64, String)],
    groups: &[String],
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let palette = palette(dark);
    let series: Vec<String> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let pts: Vec<String> = points
                .iter()
                .filter(|(_, _, _, grp)| grp == g)
                // 横軸は対数。0 以下は log が取れず、ECharts はその点を黙って
                // 描かない。呼び出し側でも弾いているが、外した呼び出しが増えても
                // 「点が減ったことに気づけない」状態にはしない。
                .filter(|(_, x, y, _)| *x > 0.0 && x.is_finite() && y.is_finite())
                .map(|(name, x, y, _)| {
                    // 点に name を持たせる。こうすると tooltip を "{b}" で書けて、
                    // JavaScript の関数文字列を属性に埋め込まずに済む
                    // （属性はシングルクォート囲みなので、関数の中の引用符で壊れやすい）
                    format!(
                        "{{\"name\":\"{n}\",\"value\":[{x:.0},{y:.2}],\"symbolSize\":{s:.1}}}",
                        n = json_str(name),
                        x = x,
                        y = y,
                        s = point_size(*x)
                    )
                })
                .collect();
            format!(
                "{{\"name\":\"{n}\",\"type\":\"scatter\",\"symbol\":\"{sy}\",\
                 \"itemStyle\":{{\"color\":\"{c}\",\"opacity\":0.8}},\"data\":[{d}]}}",
                n = json_str(g),
                sy = symbol(i),
                c = palette[i % palette.len()],
                d = pts.join(",")
            )
        })
        .collect();
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"item\",\"formatter\":\"{{b}}<br/>求人数 {{c0}} 件／1求人あたり {{c1}}\"}},\
         \"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\
         \"grid\":{{\"left\":\"12%\",\"right\":\"5%\",\"top\":\"8%\",\"bottom\":\"22%\"}},\
         \"xAxis\":{{\"type\":\"log\",\"name\":\"求人数（最新月・対数）\",\"nameLocation\":\"middle\",\"nameGap\":28,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.12}}}}}},\
         \"yAxis\":{{\"type\":\"value\",\"name\":\"1求人あたりに見た人数\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.12}}}}}},\
         \"series\":[{sr}]}}'></div>",
        h = height,
        ax = ax,
        sr = series.join(",")
    )
}

/// 横棒。並べ替え済みの値をそのまま出す。基準線を 1 本引ける。
///
/// 47 都道府県のように「順位を見たい」ものは、折れ線でも表でもなく横棒が早い。
pub fn hbar_chart(
    labels: &[String],
    values: &[Option<f64>],
    unit: &str,
    baseline: Option<(f64, &str)>,
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let pal = palette(dark);
    let mark = match baseline {
        Some((v, name)) => format!(
            ",\"markLine\":{{\"silent\":true,\"symbol\":\"none\",\
             \"lineStyle\":{{\"color\":\"{c1}\",\"type\":\"dashed\",\"width\":1}},\
             \"label\":{{\"formatter\":\"{n}\",\"color\":\"{c1}\",\"fontSize\":10,\"position\":\"end\"}},\
             \"data\":[{{\"xAxis\":{v:.3}}}]}}",
            n = json_str(name),
            c1 = pal[1],
            v = v
        ),
        None => String::new(),
    };
    // 上が 1 位に見えるよう、ECharts の縦軸は逆順にする
    let labels_json = labels
        .iter()
        .rev()
        .map(|l| format!("\"{}\"", json_str(l)))
        .collect::<Vec<_>>()
        .join(",");
    let vals: Vec<Option<f64>> = values.iter().rev().copied().collect();
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"grid\":{{\"left\":\"22%\",\"right\":\"8%\",\"top\":\"3%\",\"bottom\":\"8%\"}},\
         \"xAxis\":{{\"type\":\"value\",\"name\":\"{u}\",\"nameLocation\":\"middle\",\"nameGap\":26,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.12}}}}}},\
         \"yAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"series\":[{{\"type\":\"bar\",\"data\":[{d}],\"itemStyle\":{{\"color\":\"{c0}\"}},\"barMaxWidth\":14{mk}}}]}}'></div>",
        h = height,
        u = json_str(unit),
        ax = ax,
        lb = labels_json,
        d = series_json(&vals),
        c0 = pal[0],
        mk = mark
    )
}

/// 下限からの帯。左端が低いほうの値、右端が高いほうの値。
///
/// # 下回る県を 0 に丸めない
/// 掲示時給が最低賃金を下回る組み合わせは実データに 40 件ある
/// （タクシードライバー×和歌山県 −179 円、警備員×東京都 −80 円など）。
/// 以前はこの差を `.max(0.0)` で 0 に丸めていたので、その県だけ帯の長さが 0 になり
/// **「データが無い行」に見える**のに、図の下の文章には「−179 円」と出ていた。
/// 図と文章が食い違ううえ、いちばん目を引くべき行が消えていた。
/// 下回る分は色を変えた別の帯として、反対向きではなく同じ位置に描く。
///
/// # 土台を凡例に出さない
/// 帯の起点までを透明な棒で埋めている。これを凡例に出すと、色見本が
/// 透明のまま項目名だけ並び、壊れた表示に見える。凡例は色のついた帯だけにする。
pub fn dumbbell_chart(
    labels: &[String],
    low: &[Option<f64>],
    high: &[Option<f64>],
    low_name: &str,
    high_name: &str,
    below_name: &str,
    unit: &str,
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let labels_json = labels
        .iter()
        .rev()
        .map(|l| format!("\"{}\"", json_str(l)))
        .collect::<Vec<_>>()
        .join(",");
    let lo: Vec<Option<f64>> = low.iter().rev().copied().collect();
    let hi: Vec<Option<f64>> = high.iter().rev().copied().collect();
    let pair = |f: fn(f64, f64) -> Option<f64>| -> Vec<Option<f64>> {
        lo.iter()
            .zip(hi.iter())
            .map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => f(*a, *b),
                _ => None,
            })
            .collect()
    };
    // 土台は低いほうの値まで。ここが帯の左端になる
    let base = pair(|a, b| Some(a.min(b)));
    // 上乗せ（高いほうが上）と、下回る分。片方が値を持つときもう片方は欠測
    let up = pair(|a, b| if b >= a { Some(b - a) } else { None });
    let down = pair(|a, b| if b < a { Some(a - b) } else { None });
    let has_down = down.iter().any(|v| v.is_some());

    let (c_up, c_down) = if dark {
        ("#38bdf8", "#f59e0b")
    } else {
        ("#0369a1", "#b45309")
    };
    // 青と橙にする。緑と橙は色の見え方が違う人にはいちばん見分けにくい組
    let legend = if has_down {
        format!("\"{}\",\"{}\"", json_str(high_name), json_str(below_name))
    } else {
        format!("\"{}\"", json_str(high_name))
    };
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"legend\":{{\"bottom\":0,\"data\":[{lg}],\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\
         \"grid\":{{\"left\":\"22%\",\"right\":\"8%\",\"top\":\"3%\",\"bottom\":\"14%\"}},\
         \"xAxis\":{{\"type\":\"value\",\"name\":\"{u}\",\"nameLocation\":\"middle\",\"nameGap\":26,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"min\":\"dataMin\",\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.12}}}}}},\
         \"yAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"series\":[\
         {{\"name\":\"{ln}\",\"type\":\"bar\",\"stack\":\"w\",\"data\":[{d0}],\
         \"tooltip\":{{\"show\":false}},\"itemStyle\":{{\"color\":\"transparent\"}},\"barMaxWidth\":12}},\
         {{\"name\":\"{hn}\",\"type\":\"bar\",\"stack\":\"w\",\"data\":[{d1}],\
         \"itemStyle\":{{\"color\":\"{cu}\"}},\"barMaxWidth\":12}},\
         {{\"name\":\"{bn}\",\"type\":\"bar\",\"stack\":\"w\",\"data\":[{d2}],\
         \"itemStyle\":{{\"color\":\"{cd}\"}},\"barMaxWidth\":12}}]}}'></div>",
        h = height,
        ax = ax,
        lg = legend,
        ln = json_str(low_name),
        hn = json_str(high_name),
        bn = json_str(below_name),
        cu = c_up,
        cd = c_down,
        u = json_str(unit),
        lb = labels_json,
        d0 = series_json(&base),
        d1 = series_json(&up),
        d2 = series_json(&down)
    )
}

/// 縦棒。基準線を 1 本引ける。
///
/// # なぜ横棒と分けるのか
/// 横棒は「順位」を見る形で、47 県のように順序に意味がある並びに使う。
/// 暦月 1〜12 のように**並び順が決まっていて量を比べたい**ものは縦棒のほうが
/// 読みやすい。同じ形にすると、順位の図と季節の図が見分けられなくなる。
pub fn vbar_chart(
    labels: &[String],
    values: &[Option<f64>],
    unit: &str,
    baseline: Option<(f64, &str)>,
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let pal = palette(dark);
    let mark = match baseline {
        Some((v, name)) => format!(
            ",\"markLine\":{{\"silent\":true,\"symbol\":\"none\",\
             \"lineStyle\":{{\"color\":\"{c1}\",\"type\":\"dashed\",\"width\":1}},\
             \"label\":{{\"formatter\":\"{n}\",\"color\":\"{c1}\",\"fontSize\":10,\"position\":\"end\"}},\
             \"data\":[{{\"yAxis\":{v:.3}}}]}}",
            n = json_str(name),
            c1 = pal[1],
            v = v
        ),
        None => String::new(),
    };
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"grid\":{{\"left\":\"12%\",\"right\":\"6%\",\"top\":\"10%\",\"bottom\":\"14%\"}},\
         \"xAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"yAxis\":{{\"type\":\"value\",\"name\":\"{u}\",\"scale\":true,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.12}}}}}},\
         \"series\":[{{\"type\":\"bar\",\"data\":[{d}],\"itemStyle\":{{\"color\":\"{c0}\"}},\"barMaxWidth\":34{mk}}}]}}'></div>",
        h = height,
        ax = ax,
        u = json_str(unit),
        lb = labels
            .iter()
            .map(|l| format!("\"{}\"", json_str(l)))
            .collect::<Vec<_>>()
            .join(","),
        d = series_json(values),
        c0 = pal[0],
        mk = mark
    )
}

/// 棒と折れ線を 1 枚に重ねる。軸は 2 本。
///
/// 「求人が増えたから 1 求人あたりが薄まった」という関係は、
/// 別々の図に分けると読み手が頭の中で重ねなければならない。
pub fn bar_line_chart(
    months: &[String],
    bar_name: &str,
    bar: &[Option<f64>],
    line_name: &str,
    line: &[Option<f64>],
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let pal = palette(dark);
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\"}},\
         \"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\
         \"grid\":{{\"left\":\"13%\",\"right\":\"11%\",\"top\":\"8%\",\"bottom\":\"20%\"}},\
         \"xAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"yAxis\":[\
         {{\"type\":\"value\",\"name\":\"{bn}\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"opacity\":0.12}}}}}},\
         {{\"type\":\"value\",\"name\":\"{lnn}\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"show\":false}},\"scale\":true}}],\
         \"series\":[\
         {{\"name\":\"{bn}\",\"type\":\"bar\",\"data\":[{d1}],\"itemStyle\":{{\"color\":\"{c0}\",\"opacity\":0.75}},\"barMaxWidth\":26}},\
         {{\"name\":\"{lnn}\",\"type\":\"line\",\"yAxisIndex\":1,\"data\":[{d2}],\"showSymbol\":false,\
         \"lineStyle\":{{\"width\":2,\"color\":\"{c1}\"}},\"itemStyle\":{{\"color\":\"{c1}\"}}}}]}}'></div>",
        h = height,
        ax = ax,
        lb = months
            .iter()
            .map(|m| format!("\"{}\"", json_str(m)))
            .collect::<Vec<_>>()
            .join(","),
        bn = json_str(bar_name),
        lnn = json_str(line_name),
        d1 = series_json(bar),
        d2 = series_json(line),
        c0 = pal[0],
        c1 = pal[1]
    )
}

#[cfg(test)]
mod chart_tests {
    use super::*;

    /// 属性から図の設定を取り出し、ブラウザと同じ手順で JSON に戻す。
    ///
    /// 属性はシングルクォート囲みなので、ブラウザは先に HTML 実体参照を
    /// ほどいてから JSON.parse する。ここでも同じ順で確かめる。
    pub fn parsed(html: &str) -> serde_json::Value {
        let start = html
            .find("data-chart-config='")
            .expect("設定が見つからない")
            + "data-chart-config='".len();
        let rest = &html[start..];
        let end = rest.find('\'').expect("設定が閉じていない");
        let raw = &rest[..end];
        let decoded = raw
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        serde_json::from_str(&decoded).unwrap_or_else(|e| panic!("JSON が壊れている: {e}\n{decoded}"))
    }

    #[test]
    fn 散布図の設定がjsonとして正しく点が入っている() {
        let pts = vec![
            ("販売スタッフ".to_string(), 167_591.0, 11.1, "サービス・販売".to_string()),
            ("配送ドライバー".to_string(), 83_647.0, 8.1, "物流・運輸".to_string()),
        ];
        let groups = vec!["サービス・販売".to_string(), "物流・運輸".to_string()];
        let v = parsed(&scatter_chart(&pts, &groups, true, 300));
        let series = v["series"].as_array().expect("series が無い");
        assert_eq!(series.len(), 2, "業界の数だけ系列が要る");
        let total: usize = series
            .iter()
            .map(|s| s["data"].as_array().map(|d| d.len()).unwrap_or(0))
            .sum();
        assert_eq!(total, 2, "点が落ちている");
        // 点は name つきの物。tooltip の {b} で名前が出る
        assert_eq!(series[0]["data"][0]["name"], "販売スタッフ");
    }

    /// 職種名に二重引用符が入っても図が壊れないこと。
    #[test]
    fn 職種名に引用符が入っても図が壊れない() {
        let pts = vec![(
            "変な\"名前\"の職種".to_string(),
            100.0,
            5.0,
            "その他".to_string(),
        )];
        let groups = vec!["その他".to_string()];
        let v = parsed(&scatter_chart(&pts, &groups, true, 300));
        assert_eq!(v["series"][0]["data"][0]["name"], "変な\"名前\"の職種");
    }

    #[test]
    fn 横棒は一位が上に来る() {
        let labels = vec!["東京都".into(), "大阪府".into(), "福岡県".into()];
        let values = vec![Some(12.0), Some(8.0), Some(5.0)];
        let v = parsed(&hbar_chart(&labels, &values, "人", Some((9.0, "全国")), true, 300));
        // ECharts の縦軸は下から積むので、渡す配列は逆順になっているのが正しい
        let cats = v["yAxis"]["data"].as_array().unwrap();
        assert_eq!(cats[cats.len() - 1], "東京都", "1 位が上に来ていない");
        let data = v["series"][0]["data"].as_array().unwrap();
        assert_eq!(data[data.len() - 1], 12.0);
        assert!(v["series"][0]["markLine"].is_object(), "基準線が無い");
    }

    #[test]
    fn ダンベルは下限からの差を帯にする() {
        let labels = vec!["東京都".into(), "鳥取県".into()];
        let low = vec![Some(1226.0), Some(1030.0)];
        let high = vec![Some(1520.0), Some(1207.0)];
        let v = parsed(&dumbbell_chart(
            &labels, &low, &high, "最低賃金", "上乗せ", "下回る分", "円", true, 300,
        ));
        let series = v["series"].as_array().unwrap();
        assert_eq!(series.len(), 3);
        // 1 本目は透明の土台
        assert_eq!(series[0]["itemStyle"]["color"], "transparent");
        let gap = series[1]["data"].as_array().unwrap();
        // 逆順に入るので、最後が東京都の 1520-1226=294
        assert_eq!(gap[gap.len() - 1], 294.0);
        // 下回る県が無いので、その帯は全部欠測
        assert!(series[2]["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.is_null()));
    }

    /// 掲示時給が最低賃金を下回る県を 0 に丸めない。
    ///
    /// 実データに 40 件あり、警備員×東京都もその 1 つ。丸めると
    /// その行だけ帯が消え、図の下の「−80 円」という文章と食い違う。
    #[test]
    fn 最低賃金を下回る県は別の帯として残る() {
        let labels = vec!["神奈川県".into(), "東京都".into()];
        let low = vec![Some(1225.0), Some(1226.0)];
        let high = vec![Some(1400.0), Some(1146.0)]; // 東京都は 80 円下回る
        let out = dumbbell_chart(
            &labels,
            &low,
            &high,
            "最低賃金",
            "上乗せ",
            "下回る分",
            "円",
            true,
            300,
        );
        let v = parsed(&out);
        let series = v["series"].as_array().unwrap();
        // 逆順なので添字 0 が東京都
        assert_eq!(series[1]["data"][0], serde_json::Value::Null, "上乗せ側に出ている");
        assert_eq!(series[2]["data"][0], 80.0, "下回る分が 80 円で出ていない");
        // 土台は低いほうの値。東京都は掲示の 1146 が左端になる
        assert_eq!(series[0]["data"][0], 1146.0);
        // 凡例に下回る帯が出る。透明の土台は出さない
        let lg: Vec<String> = v["legend"]["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        assert!(lg.contains(&"下回る分".to_string()), "凡例に下回る帯が無い");
        assert!(
            !lg.contains(&"最低賃金".to_string()),
            "色見本が透明の項目を凡例に出している"
        );
    }

    #[test]
    fn 棒と折れ線は軸を分ける() {
        let months = vec!["2026-07".to_string(), "2026-08".to_string()];
        let v = parsed(&bar_line_chart(
            &months,
            "求人の数",
            &[Some(100.0), Some(120.0)],
            "1 求人あたり",
            &[Some(10.0), Some(8.0)],
            true,
            300,
        ));
        assert_eq!(v["yAxis"].as_array().unwrap().len(), 2, "軸が 2 本無い");
        assert_eq!(v["series"][0]["type"], "bar");
        assert_eq!(v["series"][1]["type"], "line");
        assert_eq!(v["series"][1]["yAxisIndex"], 1, "折れ線が右軸になっていない");
    }

    /// 欠測は 0 ではなく null で渡すこと。0 にすると図が谷に見える。
    #[test]
    fn 欠測はゼロではなくnullで渡す() {
        let months = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let v = parsed(&bar_line_chart(
            &months,
            "棒",
            &[Some(1.0), None, Some(3.0)],
            "線",
            &[Some(1.0), None, Some(3.0)],
            true,
            200,
        ));
        assert!(v["series"][0]["data"][1].is_null(), "欠測が 0 になっている");
    }
}

#[cfg(test)]
mod scatter_size_tests {
    use super::chart_tests::parsed;
    use super::*;

    /// 点の大きさが求人数に対応していること。
    ///
    /// 1 求人あたりが極端に大きい職種は求人数が少ない。同じ大きさで描くと、
    /// 図の上端で目立つのに商談では見ない職種、という見え方になる。
    #[test]
    fn 点の大きさが求人数で決まる() {
        // 求人が少ない職種は下限の 5、多い職種は上限の 30 に張り付く
        assert_eq!(point_size(70.0), 5.0);
        assert_eq!(point_size(176_400.0), 30.0);
        // 中間はなめらかに増える
        assert!(point_size(10_000.0) > point_size(1_000.0));
        // 0 や欠測相当でも落ちない
        assert_eq!(point_size(0.0), 5.0);
        assert_eq!(point_size(f64::NAN), 5.0);

        let pts = vec![
            ("小さい".to_string(), 100.0, 5.0, "G".to_string()),
            ("大きい".to_string(), 200_000.0, 3.0, "G".to_string()),
        ];
        let v = parsed(&scatter_chart(&pts, &vec!["G".to_string()], true, 300));
        let d = v["series"][0]["data"].as_array().unwrap();
        assert_eq!(d[0]["symbolSize"], 5.0);
        assert_eq!(d[1]["symbolSize"], 30.0);
        // 値は [求人数, 1 求人あたり] の 2 つ。tooltip の {c0}/{c1} がこれを指す
        assert_eq!(d[0]["value"].as_array().unwrap().len(), 2);
    }

    /// 図の設定に JavaScript の式を残さない。
    ///
    /// # なぜこれを見張るのか
    /// `symbolSize` を `"function (d) { ... }"` という文字列で渡していた。
    /// 社内タブは app.js が文字列を関数に戻すが、顧客レポートの初期化経路には
    /// その処理が無い。実際には社内タブでも戻らず、**点が 1 つも描かれないまま
    /// 軸と凡例だけが出ていた**。「図がある」ことを数える検査は素通りする
    /// （要素も実体も存在し、データも入っているため）。
    /// 式を文字列で渡すのをやめたので、二度と入り込まないようにここで止める。
    #[test]
    fn 図の設定に関数の文字列を入れない() {
        let months = vec!["2026-07".to_string(), "2026-08".to_string()];
        let pts = vec![("A".to_string(), 100.0, 5.0, "G".to_string())];
        let outs = vec![
            scatter_chart(&pts, &vec!["G".to_string()], true, 300),
            hbar_chart(
                &["東京都".to_string(), "大阪府".to_string()],
                &[Some(1.0), Some(2.0)],
                "件",
                Some((1.5, "全国")),
                true,
                300,
            ),
            dumbbell_chart(
                &["東京都".to_string()],
                &[Some(1000.0)],
                &[Some(1200.0)],
                "最低賃金",
                "上乗せ",
                "下回る分",
                "円",
                true,
                300,
            ),
            bar_line_chart(
                &months,
                "求人の数",
                &[Some(100.0), Some(120.0)],
                "1 求人あたり",
                &[Some(10.0), Some(8.0)],
                true,
                300,
            ),
            line_chart(
                &months,
                &[("全国".to_string(), vec![Some(1.0), Some(2.0)])],
                true,
                300,
            ),
        ];
        for (i, o) in outs.iter().enumerate() {
            assert!(
                !o.contains("function"),
                "{i} 番目の図に JavaScript の式が残っている"
            );
        }
    }
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    /// 色を WCAG の相対輝度に直す。
    fn lum(hex: &str) -> f64 {
        let v = |i: usize| {
            let c = u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * v(1) + 0.7152 * v(3) + 0.0722 * v(5)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (x, y) = (lum(a), lum(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// 図の線と地の明暗差は 3:1 以上（WCAG 1.4.11）。
    ///
    /// # なぜ計算で持つのか
    /// 暗い画面向けに選んだ色をそのまま顧客レポート（白地）に流用していた。
    /// 7 色すべてが 1.7〜2.7 しかなく、印刷すると薄く霞む。
    /// 目で見て決めると同じ穴に落ちるので、数字で止める。
    #[test]
    fn 系列の色は地に対して三対一以上ある() {
        for (dark, bg, name) in [(true, "#0e1628", "暗い画面"), (false, "#ffffff", "紙")] {
            for (i, c) in palette(dark).iter().enumerate() {
                let r = contrast(c, bg);
                assert!(
                    r >= 3.0,
                    "{name} の {i} 番目 {c} は {r:.2}:1 しかない（3.0 以上が必要）"
                );
            }
        }
    }

    /// 色が近い組は線種で分ける。
    ///
    /// 暗い画面の 2 番と 6 番（#34d399 と #4ade80）、3 番と 5 番
    /// （#f472b6 と #fb7185）は色相差 20 度前後で、折れ線では見分けにくい。
    /// ECharts の decal は塗り面にしか乗らないので線種で区別する。
    #[test]
    fn 色の近い系列は線種が違う() {
        for (a, b) in [(2usize, 6usize), (3, 5)] {
            assert_ne!(
                dash(a),
                dash(b),
                "{a} 番と {b} 番が同じ線種で、色も近い"
            );
        }
    }

    /// 散布図は色だけでなく形でも分かれる。
    #[test]
    fn 散布図の系列は形が重ならない() {
        let mut seen = std::collections::HashSet::new();
        // 5 業界＋「5 業界の外」で最大 6 系列
        for i in 0..6 {
            assert!(seen.insert(symbol(i)), "{i} 番目で形が重複した");
        }
    }
}
