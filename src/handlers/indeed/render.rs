//! Indeed の社内タブと顧客レポートが共通で使う描画部品。
//!
//! 数字の丸め方と言い回しをここに集める。画面ごとに書くと、
//! 同じ指標が画面によって違う値に見える。

use crate::indeed::aggregate::{CategoryRow, Metric};

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
                "text-red-400"
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
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\"tooltip\":{{\"trigger\":\"axis\"}},\"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\"grid\":{{\"left\":\"12%\",\"right\":\"4%\",\"top\":\"10%\",\"bottom\":\"22%\"}},\"xAxis\":{{\"type\":\"category\",\"boundaryGap\":false,\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\"yAxis\":{{\"type\":\"value\",\"scale\":true,\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\"series\":[{sr}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
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
            "bg-navy-800/60 border border-slate-500 rounded-lg p-4",
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
            "w-full text-sm",
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

/// 実数の目盛りをどう出すかを決める。
///
/// # なぜ Rust 側で割るのか
/// 軸ラベルの整形に ECharts の `formatter` を使うと JS の関数文字列になる。
/// この画面の図は `data-chart-config` の JSON から作っており、関数文字列は
/// `JSON.parse` を通っても関数に戻らないので、その図だけ黙って壊れる
/// （`tests/no_function_string_in_charts.rs` で禁止している）。
/// 値を先に割っておき、単位は軸名で示す。
///
/// # 「万」に切り替える境目を 100 万にしてある
///
/// ここに来る値の幅は実データで **5 〜 2,054 万**（40 万倍）ある。
/// 職種×県の求人数は中央値 87 件・最小 5 件、全国ぜんぶの見た人数は 2,054 万人。
/// どちらか一方に単位を固定することはできない。全部「万」にすると
/// 職種×県の求人数 4,956 通りのうち **99.7% が「0.00xx 万件」**になり、
/// 全部「実数」にすると全国の目盛りが「20,000,000」になる。
///
/// 境目をどこに置くかだけが選べる。以前は 10 万だったが、そこは県の分布の
/// ど真ん中で、**同じ指標なのに県を替えると単位が変わる**場所だった。
/// 実データで数えると、県・職種・職種×県の全 15,390 通りのうち
/// 「同じ指標・同じ画面の種類の中で単位が多数派と違う」ものが 80 通りあった。
/// 境目を 100 万にすると 15 通りに減り、とくに
/// - 求人数は 47 県すべてが「件」（以前は東京・大阪・愛知・神奈川・埼玉だけ「万件」）
/// - 企業数は 47 県も全国も「社」（以前は全国だけ「万社」。全国 571,336 社と
///   東京都 62,419 社を、桁を読み替えずに並べられる）
/// になる。
///
/// 一方で **見た人数だけは、全国から県へ移るときの単位が変わりやすくなる**。
/// 以前は全国も 40 県も「万人」だったが、いまは全国と上位 6 県だけが「万人」で
/// 残り 41 県は「人」になる。ただし県どうしの食い違い（以前は 40 対 7）は
/// 6 対 41 とほぼ同じ大きさで、順序対で数えると差し引き 0 だった。
/// 求人数と企業数の分だけが正味の改善になっている。
///
/// 100 万にしたのは、実数の目盛りが 7 桁になるのがここだからでもある。
/// `small_multiples` の 1 枚は最小 210px しかなく、ECharts に実際に描かせて
/// 目盛りの文字幅を測ると 「600,000」が 38.7px（枠の 18%）に対し
/// 「2,100,000」は 46px、「25,000,000」は 51.9px で、月のラベルが間引かれ
/// 折れ線の幅がはっきり削られた。**6 桁までは実数、7 桁になるなら万**が境目。
///
/// この境目なら小数の目盛りも出ない。100 万以上を 1 万で割れば必ず 100 以上、
/// 100 万未満はそのままなので、どちらの側でも目盛りは整数になる。
fn scale_of(series: &[(String, Vec<Option<f64>>)]) -> (f64, &'static str) {
    let max = series
        .iter()
        .flat_map(|(_, v)| v.iter().flatten())
        .fold(0.0f64, |a, b| a.max(*b));
    if max >= 1_000_000.0 {
        (10_000.0, "万")
    } else {
        (1.0, "")
    }
}

fn scaled(v: &[Option<f64>], by: f64) -> Vec<Option<f64>> {
    v.iter().map(|x| x.map(|n| n / by)).collect()
}

// 分類 → 色と線種。県が変わっても同じ分類は同じ見た目にする。
//
// 以前は「その県での上位」を順位順にパレットへ割り当てていたため、
// 全国では緑・実線だった「接客・販売」が鳥取県では橙になっていた。
//
// # 近さは色差と色相角の両方で見る
//
// 最初は Machado 2009 + CIE Lab の色差だけで判定していた。だが実物を見ると、
// 色差 26.9 の 桃 × 薔薇 が **1 本の太い破線に見えて区別できなかった**
// （ux-charts の目視。東京都で 警備・誘導 と 飲食・フードが 0.7 万件差で並走）。
// 一方、色差 20.1 の 軽作業 × 製造・生産 は問題なく見分けられた。
//
// 原因は、Lab のユークリッド距離が明度差を含むのに対し、折れ線で「どの線か」を
// 見分けるのは主に**色相**だから。桃と薔薇は明度差 0.5・色相差 23 度しかない。
// そこで **色差 20 未満 または 色相差 40 度未満**を保護対象にした。該当は 4 組:
//   緑 × 薔薇 9.2 / 橙 × 黄 15.7（色相 13 度）/ 青 × 藍 16.8 / 桃 × 薔薇（色相 23 度）
//
// # 薔薇はこの表では使わない
//
// 薔薇は 4 組のうち 2 組の当事者で、これを含めると線種 3 種類では
// 12 分類をさばけない（容量 11）。分類の表からは外し、6 色で回している。
// 薔薇はパレットには残っていて、散布図の業界などでは使う。
//
// 残る制約は 青 × 藍 と 橙 × 黄 の 2 組だけになり、
//   緑 = 実線・破線・点線 / 桃 = 実線・破線・点線
//   青 = 実線、藍 = 破線・点線（線種を共有しない）
//   橙 = 実線、黄 = 破線・点線（同上）
// で容量ちょうど 12。実データで上位 6 に一度でも入る分類も 12 個。
//
// 検査は (色, 線種) の完全一致だけでは足りない。最初はそれだけを見ていて、
// 知覚的に近い色が同じ線種で並ぶのを見逃していた。共起で判定するのも駄目で、
// 「たまたま同時に上位へ入らない」というデータ依存の保証にしかならない。
// いまは表の形だけで成立させ、`category_style_tests` で固定している。
pub const CATEGORY_STYLE: [(&str, usize, &str); 12] = [
    ("製造・生産", 0, "solid"),
    ("事務・オフィスワーク", 1, "solid"),
    ("接客・販売", 2, "solid"),
    ("軽作業", 3, "solid"),
    ("物流・配送", 4, "dashed"),
    ("清掃", 6, "dashed"),
    ("警備・誘導", 2, "dashed"),
    ("飲食・フード", 3, "dashed"),
    ("カスタマーサービス", 4, "dotted"),
    ("営業 無形商材", 6, "dotted"),
    ("製造・開発 (電気・機械・金属・化学)", 2, "dotted"),
    ("保全・管理（設備・建物）", 3, "dotted"),
];

/// 系列名から色と線種を決める。
///
/// 表に載っている分類は、県が変わっても同じ見た目になる。
///
/// 載っていない名前は**名前だけから決める**。並び順で決めると、
/// 「その県での上位」が変わるたびに同じものの色が変わる。それを直したのが
/// [`CATEGORY_STYLE`] なので、既定側で並び順に戻しては意味がない。
/// 一度これを書き忘れて並び順を混ぜ、`表に無い名前も県で変わらない` が落ちた。
fn style_of(name: &str, dark: bool) -> (&'static str, &'static str) {
    let pal = palette(dark);
    match CATEGORY_STYLE.iter().find(|(n, _, _)| *n == name) {
        Some((_, ci, d)) => (pal[*ci % pal.len()], *d),
        None => {
            // 名前から決める素朴なハッシュ。県をまたいでも同じ名前は同じ見た目になる。
            // 表に無い名前どうしでは色が近くなることがある（枠が 12 でちょうどのため）。
            let h = name
                .bytes()
                .fold(2_166_136_261u32, |a, b| (a ^ b as u32).wrapping_mul(16_777_619));
            (
                pal[(h % pal.len() as u32) as usize],
                dash((h / pal.len() as u32) as usize),
            )
        }
    }
}

/// 実数のまま重ねた折れ線。
///
/// # なぜ指数をやめたのか
/// 以前は「期間の最初の月を 100」とした指数だった。しかし基準は固定の月ではなく
/// 「その系列にデータがある最初の月」（`Metric::indexed`）で、しかも分析に使う月は
/// 取得が 98% 以上そろった月から毎回決め直している（`indeed_build_insights.js`）。
/// 古い月を 1 か月ぶん取り込むだけで基準が前にずれ、**過去に出した数字が全部
/// 書き換わる**。実測では基準を 2025-07 から 2025-08 に動かすと軽作業が
/// 105.9 → 98.5 になり、実数が 1 件も動いていないのに「増えた」が「減った」に
/// 反転した。先月の画面と今月の画面を並べられないので、実数に戻す。
///
/// 重ねてよいのは系列間のレンジが 5 倍以内のときだけ。それを超えるものは
/// [`small_multiples`] で 1 枚ずつ描く。
pub fn raw_line_chart(
    months: &[String],
    series: &[(String, Vec<Option<f64>>)],
    dark: bool,
    height: u32,
    unit: &str,
) -> String {
    raw_line_chart_colored(months, series, dark, height, unit, None)
}

/// 色を呼び出し側から決められる版。
///
/// 名前だけで色を決めると、「製造・生産」のように**分類にも業界にもある名前**を
/// 区別できない。散布図では業界として橙、小さい図では分類として青、という
/// 食い違いが実際に起きていた。どちらとして描くかは呼び出し側しか知らない。
pub fn raw_line_chart_colored(
    months: &[String],
    series: &[(String, Vec<Option<f64>>)],
    dark: bool,
    height: u32,
    unit: &str,
    force: Option<usize>,
) -> String {
    raw_line_chart_scaled(months, series, dark, height, unit, force, None)
}

/// 単位の倍率も外から決められる版。
///
/// 図を並べるときは、1 枚ずつ倍率を決めると単位が混ざる。
pub fn raw_line_chart_scaled(
    months: &[String],
    series: &[(String, Vec<Option<f64>>)],
    dark: bool,
    height: u32,
    unit: &str,
    force: Option<usize>,
    scale: Option<f64>,
) -> String {
    let axis = axis_color(dark);
    let (by, prefix) = match scale {
        Some(b) if b >= 10_000.0 => (b, "万"),
        Some(b) => (b, ""),
        None => scale_of(series),
    };
    let items: Vec<String> = series
        .iter()
        .map(|(name, v)| {
            // 分類名が表にあれば県によらず同じ色と線種にする。
            // 呼び出し側が色を決めているならそちらを優先する
            let st = match force {
                Some(ci) => (palette(dark)[ci % 7], "solid"),
                None => style_of(name, dark),
            };
            format!(
                "{{\"name\":\"{n}\",\"type\":\"line\",\"smooth\":false,\"showSymbol\":false,\"connectNulls\":false,\"lineStyle\":{{\"width\":2,\"type\":\"{t}\"}},\"itemStyle\":{{\"color\":\"{c}\"}},\"data\":[{d}]}}",
                n = json_str(name),
                t = st.1,
                c = st.0,
                d = series_json(&scaled(v, by))
            )
        })
        .collect();
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\"tooltip\":{{\"trigger\":\"axis\"}},\"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\"grid\":{{\"left\":8,\"right\":14,\"top\":\"16%\",\"bottom\":\"22%\",\"containLabel\":true}},\"xAxis\":{{\"type\":\"category\",\"boundaryGap\":false,\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\"yAxis\":{{\"type\":\"value\",\"min\":0,\"name\":\"{yn}\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10,\"align\":\"left\"}},\"nameGap\":8,\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\"series\":[{sr}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        ax = axis,
        yn = json_str(&format!("{prefix}{unit}")),
        lb = labels_json(months),
        sr = items.join(",")
    )
}

/// 規模の違う系列を、重ねずに 1 枚ずつ並べる。
///
/// # なぜ重ねないのか
/// 全国では求人数 175 万・見た人数 2,032 万・企業数 49 万で **41.6 倍**離れている。
/// 同じ目盛りに置くと、いちばん小さい系列が最大の 2.4% の高さになって形が読めない。
/// 指数にすれば重ねられるが、それだと基準月が動いたときに過去の数字が変わる
/// （[`raw_line_chart`] のコメント参照）。1 枚ずつ自分の目盛りで描けば、
/// 実数のまま形が読める。傾きを目で比べるかわりに、増減は各図の見出しに数字で出す。
pub fn small_multiples(
    months: &[String],
    series: &[(String, Vec<Option<f64>>, &str, usize)],
    dark: bool,
    each_height: u32,
) -> String {
    // 社内タブは Tailwind、顧客レポートは自前の CSS を使う。
    // クラス名に頼ると片方で無地になるので、ここは style 属性で書く。
    let (head, sub) = if dark {
        ("#e2e8f0", "#94a3b8")
    } else {
        ("#1e293b", "#64748b")
    };
    // 並べた図どうしで単位をそろえる。1 枚ずつ決めると、東京都の業界 5 枚が
    // 件 / 件 / 件 / 万件 / 件 になる（サービス・販売だけ 10 万件を超えるため）。
    // 同じものを並べて比べる図で単位が混ざると、桁の読み替えが要る。
    //
    // 単位が違うもの（件・人・社）が混ざる図もあるので、単位ごとにまとめて決める。
    let scale_for_unit = |unit: &str| -> f64 {
        let all: Vec<(String, Vec<Option<f64>>)> = series
            .iter()
            .filter(|(_, _, u, _)| *u == unit)
            .map(|(n, v, _, _)| (n.clone(), v.clone()))
            .collect();
        scale_of(&all).0
    };
    let cells: Vec<String> = series
        .iter()
        .map(|(name, v, unit, ci)| {
            let one = vec![(name.clone(), v.clone())];
            let first = v.iter().flatten().next().copied();
            let last = v.iter().flatten().next_back().copied();
            let chg = match (first, last) {
                (Some(a), Some(b)) if a > 0.0 => Some((b - a) / a * 100.0),
                _ => None,
            };
            format!(
                "<div><div style=\"color:{head};font-size:13px;font-weight:700\">{n}</div>\
                 <div style=\"color:{sub};font-size:11px;margin-bottom:2px;font-variant-numeric:tabular-nums\">\
                 {m} は {v} {u}（{fm} から {ar} {p}）</div>{chart}</div>",
                head = head,
                sub = sub,
                n = esc(name),
                m = esc(months.last().map(|s| s.as_str()).unwrap_or("")),
                v = num_opt(last),
                u = esc(unit),
                fm = esc(months.first().map(|s| s.as_str()).unwrap_or("")),
                ar = arrow(chg),
                p = pct_opt(chg),
                chart = raw_line_chart_scaled(
                    months,
                    &one,
                    dark,
                    each_height,
                    unit,
                    Some(*ci),
                    Some(scale_for_unit(unit)),
                )
            )
        })
        .collect();
    format!(
        "<div style=\"display:grid;grid-template-columns:repeat(auto-fit,minmax(210px,1fr));gap:16px\">{}</div>",
        cells.join("")
    )
}

/// 求人の数の伸びが、会社が増えたぶんか 1 社あたりが増えたぶんかを示す箱。
///
/// # なぜ図ではなく数字なのか
/// 求人の数と企業の数は、すぐ上の小さい図で既に並べて出している。
/// 同じものをもう一度線で出しても、読み手が頭の中で割り算することになる。
/// 割り算の答えだけを言葉と数字にする。
///
/// # 出さない場合がある
/// 求人の数がほとんど動いていない職種は、割合の分母が小さくて跳ねる。
/// 実測で清掃スタッフは求人 -0.1% / 企業 -4.2% で割合が 4000% を超えた。
/// そういう職種では箱ごと出さない（[`crate::indeed::aggregate::growth_breakdown`] が `None` を返す）。
pub fn breakdown_html(b: Option<crate::indeed::aggregate::Breakdown>) -> String {
    let Some(b) = b else {
        return String::new();
    };
    // 2 つの寄与を、向きも含めて帯にする。
    //
    // # 前の作りが壊れていた理由
    // 割合を 0〜100 に丸めて 1 本の帯を描いていた。会社と 1 社あたりが
    // 逆を向くと割合が範囲の外に出る（実測でフォークリフトが -99%）。
    // 丸めた結果 0% になり、**帯に何も描かれないのに文章だけ「-99%」と出た**。
    // 割合が範囲の外に出るのは異常値ではなく、押し合っている正常な状態なので、
    // 帯のほうをその状態を描ける形にする。
    //
    // # どう描くか
    // 掛け算を対数にすると足し算になる（求人 = 会社 × 1 社あたり）。
    // 2 つの寄与の**絶対値**で帯を割り、どちらがどれだけ効いたかを長さで出す。
    // 向きが伸びと逆のものには斜線を敷いて「押し下げた側」だと分かるようにする。
    let le = (1.0 + b.emp_pct / 100.0).ln();
    let lp = (1.0 + b.per_pct / 100.0).ln();
    let lj = le + lp;
    let total = le.abs() + lp.abs();
    let (we, wp) = if total > 0.0 {
        (le.abs() / total * 100.0, lp.abs() / total * 100.0)
    } else {
        (50.0, 50.0)
    };
    // 伸びと逆を向いている側は、打ち消しているぶん
    let e_gyaku = lj != 0.0 && le != 0.0 && le.signum() != lj.signum();
    let p_gyaku = lj != 0.0 && lp != 0.0 && lp.signum() != lj.signum();
    let shima = "background-image:repeating-linear-gradient(45deg,rgba(0,0,0,.45) 0 3px,transparent 3px 6px);";
    let note = if e_gyaku {
        "会社の数は逆を向いていて、1 社あたりの増加を打ち消しています。"
    } else if p_gyaku {
        "1 社あたりの本数は逆を向いていて、会社の増加を打ち消しています。"
    } else {
        ""
    };
    format!(
        "<div class=\"mt-3 border border-slate-600 rounded-lg p-3\">         <p class=\"text-slate-100 text-sm font-bold mb-1\">         求人が {j:+.1}% 動いたのは、会社が増えたからか、1 社あたりが増えたからか</p>         <div class=\"flex flex-wrap gap-4 items-baseline mb-2\">         <span class=\"text-slate-300 text-sm\">募集した会社の数          <strong class=\"{ce}\">{ae} {e:+.1}%</strong></span>         <span class=\"text-slate-300 text-sm\">1 社あたりの本数          <strong class=\"{cp}\">{ap} {p:+.1}%</strong></span></div>         <div class=\"flex h-3 rounded bg-slate-700 overflow-hidden mb-1\">         <div class=\"h-3 bg-blue-500\" style=\"width:{we:.1}%;{se}\" title=\"会社の数\"></div>         <div class=\"h-3 bg-teal-500\" style=\"width:{wp:.1}%;{sp}\" title=\"1 社あたりの本数\"></div></div>         <p class=\"text-slate-400 text-xs leading-relaxed\">         青が<strong>会社の数</strong>、緑が<strong>1 社あたりの本数</strong>の効き具合です。         斜線は伸びと逆を向いている側です。{note}{r}。</p></div>",
        j = b.job_pct,
        e = b.emp_pct,
        p = b.per_pct,
        ae = arrow(Some(b.emp_pct)),
        ap = arrow(Some(b.per_pct)),
        ce = dir_class(Some(b.emp_pct), true),
        cp = dir_class(Some(b.per_pct), true),
        we = we,
        wp = wp,
        se = if e_gyaku { shima } else { "" },
        sp = if p_gyaku { shima } else { "" },
        note = note,
        r = b.reading(),
    )
}

/// 目盛りの刻みを 1 / 2 / 5 × 10^n のはしごから選び、データを包む最小の窓を返す。
///
/// 返すのは `(下端, 上端, 刻み)`。決められないときは `None`（呼び出し側で 0 起点に戻す）。
///
/// # なぜ 0 から始めないのか
/// この図は「2 本の**形**を見比べる」ためのもので、交点にも上下関係にも意味がない
/// （左右で目盛りが違う）。それなのに 0 起点だと、値が高いところで小さく動く系列が
/// 図の上のほうに貼り付いて平らに見える。実測では **図の高さの平均 39% しか
/// 使えていなかった**（Indeed 側は 17〜28%）。このはしごなら平均 80% まで上がる。
///
/// # なぜ刻みを「はしご」に縛るのか
/// 窓をデータにぴったり合わせると、刻みが 37 や 4863 のような数字になり、
/// 1 本あたりいくつ動いたのかが読めない。刻みを 1/2/5 × 10^n に限れば、
/// どの図でも「1 本 = 20」「1 本 = 5 万」のようにキリのいい値になる。
/// **刻みの決め方そのものはデータを見ない**ので、月が増えても規則は変わらない。
/// 指数（基準が「データのある最初の月」）をやめた理由と同じ考え方。
///
/// # 本数を 3〜6 にしている理由
/// 本数を固定して刻みだけ動かすと、窓が外側に飛んで**かえって潰れる**ことがある。
/// 実測で交通誘導の検索が 60% → 45% に悪化した（刻み 50 が選ばれ、窓が 50〜250 に
/// 広がったため）。刻みをはしごに固定して本数のほうを 3〜6 で動かすと、
/// 同じデータで 90% まで上がる。少なすぎると形が読めず、多すぎると線で埋まる。
fn ladder_axis(values: &[Option<f64>]) -> Option<(f64, f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in values.iter().flatten().filter(|v| v.is_finite()) {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    // 値が 1 つだけ・全部同じ・そもそも無い場合は窓を作れない
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return None;
    }
    for e in -6..=12 {
        for m in [1.0_f64, 2.0, 5.0] {
            let s = m * 10.0_f64.powi(e);
            let min = (lo / s).floor() * s;
            let max = (hi / s).ceil() * s;
            let n = ((max - min) / s).round() as i64;
            if (3..=6).contains(&n) {
                return Some((min, max, s));
            }
        }
    }
    None
}

/// 軸の値を JSON に出す。指数表記（1e-7）になると ECharts が読めないので避ける。
fn axis_num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".to_string() } else { s.to_string() }
}

/// 単位の違う 2 本を、左右の軸に分けて実数のまま重ねる。
///
/// # なぜ指数にしないのか
/// 「検索エンジンでの検索回数」と「Indeed で求人を見た人数」は数えているものが違い、
/// 1 つの軸には置けない。以前は両方を指数にして形だけ並べていたが、その基準は
/// 「Indeed 側にデータがある最初の月」だった。Indeed の取得月が 1 つ増減するだけで
/// 両系列の値が全部ずれ、先月出した図と比べられなくなる。
/// 軸を 2 本にすれば、実数のまま形を並べられる。
///
/// 左右の軸で目盛りが違うので、交点や上下関係には意味がない。読むのは形だけ。
pub fn dual_line_chart(
    months: &[String],
    left: (&str, &[Option<f64>], &str),
    right: (&str, &[Option<f64>], &str),
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let pal = palette(dark);
    let (lby, lpre) = scale_of(&[(left.0.to_string(), left.1.to_vec())]);
    let (rby, rpre) = scale_of(&[(right.0.to_string(), right.1.to_vec())]);
    // 軸は「万」に直したあとの値で決める。生の値で決めると桁がずれる
    let ld = scaled(left.1, lby);
    let rd = scaled(right.1, rby);
    let lax = ladder_axis(&ld).map_or_else(
        || "\"min\":0".to_string(),
        |(a, b, st)| format!("\"min\":{},\"max\":{},\"interval\":{}", axis_num(a), axis_num(b), axis_num(st)),
    );
    let rax = ladder_axis(&rd).map_or_else(
        || "\"min\":0".to_string(),
        |(a, b, st)| format!("\"min\":{},\"max\":{},\"interval\":{}", axis_num(a), axis_num(b), axis_num(st)),
    );
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\"tooltip\":{{\"trigger\":\"axis\"}},\"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\"grid\":{{\"left\":\"13%\",\"right\":\"13%\",\"top\":\"16%\",\"bottom\":\"24%\"}},\"xAxis\":{{\"type\":\"category\",\"boundaryGap\":false,\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\"yAxis\":[{{\"type\":\"value\",{lax},\"name\":\"{ln}\",\"nameTextStyle\":{{\"color\":\"{c1}\",\"fontSize\":10,\"align\":\"left\"}},\"nameGap\":8,\"axisLabel\":{{\"color\":\"{c1}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},{{\"type\":\"value\",{rax},\"name\":\"{rn}\",\"nameTextStyle\":{{\"color\":\"{c2}\",\"fontSize\":10,\"align\":\"right\"}},\"nameGap\":8,\"axisLabel\":{{\"color\":\"{c2}\",\"fontSize\":10}},\"splitLine\":{{\"show\":false}}}}],\"series\":[{{\"name\":\"{n1}\",\"type\":\"line\",\"yAxisIndex\":0,\"smooth\":false,\"showSymbol\":false,\"connectNulls\":false,\"lineStyle\":{{\"width\":2,\"type\":\"solid\"}},\"itemStyle\":{{\"color\":\"{c1}\"}},\"data\":[{d1}]}},{{\"name\":\"{n2}\",\"type\":\"line\",\"yAxisIndex\":1,\"smooth\":false,\"showSymbol\":false,\"connectNulls\":false,\"lineStyle\":{{\"width\":2,\"type\":\"dashed\"}},\"itemStyle\":{{\"color\":\"{c2}\"}},\"data\":[{d2}]}}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        ax = ax,
        c1 = pal[0],
        c2 = pal[1],
        lb = labels_json(months),
        ln = json_str(&format!("{lpre}{}", left.2)),
        rn = json_str(&format!("{rpre}{}", right.2)),
        n1 = json_str(left.0),
        n2 = json_str(right.0),
        lax = lax,
        rax = rax,
        d1 = series_json(&ld),
        d2 = series_json(&rd)
    )
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
            "#38bdf8", "#f59e0b", "#34d399", "#f472b6", "#fde047", "#fb7185", "#818cf8",
        ]
    } else {
        [
            "#0369a1", "#b45309", "#3730a3", "#166534", "#be185d", "#701a75", "#6d28d9",
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
/// 「この職種はこの県では標本が薄い」と見なす求人数。
///
/// 鳥取県の求人数は職種によって 5 件から 425 件まで開きがあり、
/// 5 件の職種の「1 求人あたりに見た人数」と 425 件の職種のそれが、
/// 散布図で同じ見た目の点として並んでいた。少ないほうは 1 件の増減で
/// 大きく動くので、白抜きにして見分けられるようにする。
pub const FEW_JOBS: f64 = 30.0;

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

/// 目盛り線（`splitLine`）の色。
///
/// # なぜ色で書くのか
/// 以前は `opacity` だけを指定していた。線そのものの色は ECharts のテーマ任せで
/// （実測で暗い画面 `#484753`・紙 `#E0E6F1`）、そこに 0.12〜0.15 を掛けていたので、
/// **地の明るさが変わると線の見え方が一緒に動いてしまう**。
/// カードの地が `#0e1628` から `bg-navy-700`（`#1e293b`）に明るくなったとき、
/// 目盛り線の実効色は `#232d3e`、地との明暗差は **1.06:1** になっていた。
/// 紙側はもっと悪く、`#fafbfd` の **1.04:1**。どちらも線を引いていないのと同じ。
/// 地に左右されないよう、色を書き切る。
///
/// # 3:1 を目指さない
/// WCAG 1.4.11 の 3:1 は「情報を伝えるのに要るもの」に対する下限で、
/// 目盛り線は補助。3:1 まで上げると目盛り線がデータより目立つ。狙いは 1.7〜2.0:1。
///
/// # 暗い画面に地が 2 つある
/// いま図が載るカードは 1 種類ではない。`tab.rs` は `bg-navy-700`（`#1e293b`）、
/// `title.rs` は `bg-navy-800/60` が `bg-navy-900` に重なった `#0e1628` で、
/// ここからはどちらに載るか分からない。実測した明暗差は
///
/// | 色 | 対 `#1e293b` | 対 `#0e1628` |
/// |---|---|---|
/// | `#334155` | 1.41 | 1.74 |
/// | **`#3f4d63`** | **1.71** | **2.11** |
/// | `#475569` | 1.93 | 2.38 |
/// | `#64748b` | 3.07 | 3.79 |
///
/// で、`#475569` は明るいカードには合うが暗いカードでは 2.38 まで出て、
/// 目盛り線がデータと競り始める。両方を 1.7〜2.0 の近くに収められるのは
/// `#3f4d63` なのでこちらを採る。カードが 1 種類にそろったら見直すこと。
///
/// 紙は地が白 1 種類なので素直に真ん中を採り、`#b8c0cd` で 1.83:1。
fn grid_line_color(dark: bool) -> &'static str {
    if dark {
        "#3f4d63"
    } else {
        "#b8c0cd"
    }
}

/// 棒の**内側**に置く文字の色。
///
/// 軸の色（[`axis_color`]）は地の上に置く前提で選んである。同じ色を棒の上に
/// 載せると、暗い画面の #94a3b8 と棒の #38bdf8 で明暗差が 1.4:1 しかなく読めない。
/// 棒の色は暗い画面では明るい（#38bdf8 / #f59e0b）、紙では暗い（#0369a1 / #b45309）
/// ので、載せる文字はその逆にする。いずれも 6:1 以上あり、
/// `棒の内側の文字は棒の色に対して四対五以上ある` で固定している。
fn on_bar_color(dark: bool) -> &'static str {
    if dark {
        "#0f172a"
    } else {
        "#ffffff"
    }
}

/// 職種を「求人数 × 1 求人あたりに見た人数」で置いた散布図。
///
/// # 点の大きさで求人数を表すのをやめた
/// 以前は `symbolSize` を求人数の平方根にしていたが、実測すると鳥取県では
/// 86 点すべてが下限 5px に張り付き（東京都でも 122 点中 108 点）、
/// 大きさは何も伝えていなかった。しかも求人数は横軸そのものなので二重表現だった。
/// いまは大きさを一定にし、そのぶんを**標本の薄さ**に使う。求人が
/// [`FEW_JOBS`] 件に満たない職種は白抜きにする。鳥取県の求人数は 5〜425 件で、
/// 5 件の職種と 425 件の職種が同じ点として並んでいた。
///
/// # 縦軸の上限を呼び出し側から渡す
/// 東京都では 1 点（207.5）が上限を決めてしまい、残り 121 点が下 1/3 に
/// 潰れて重なっていた。呼び出し側で上位を外し、その上限をここに渡す。
///
/// # ラベル
/// 目立たせたい職種名だけを `labeled` で渡す。全点に出すと重なって読めない。
pub fn scatter_chart(
    points: &[(String, f64, f64, String)],
    groups: &[String],
    labeled: &[String],
    y_max: Option<f64>,
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let palette = palette(dark);
    // ラベルの重なりは、位置をずらして解こうとしても解けない。
    //
    // 一度「右寄りの点だけラベルを左に出す」を入れたが、実測で 6 ケース中 3 ケースが
    // 悪化した。狙った組は動かず（点そのものが団子なので、同じ側に倒しても位置関係が
    // 平行移動するだけ）、左に倒したラベルが中盤のラベルの領域に入り込んだ。
    // 効くのは**団子の中のラベル数を減らす**ことだけ。選ぶ側（`tab.rs`）で、
    // 図の別々の場所に来る職種を選んでいる。
    let series: Vec<String> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let c = palette[i % palette.len()];
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
                    let style = if *x < FEW_JOBS {
                        format!(
                            "\"itemStyle\":{{\"color\":\"transparent\",\"borderColor\":\"{c}\",\"borderWidth\":1.4}}"
                        )
                    } else {
                        format!("\"itemStyle\":{{\"color\":\"{c}\",\"opacity\":0.85}}")
                    };
                    let label = if labeled.iter().any(|l| l == name) {
                        format!(
                            ",\"label\":{{\"show\":true,\"position\":\"right\",\"formatter\":\"{{b}}\",\
                             \"color\":\"{ax}\",\"fontSize\":10}}"
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "{{\"name\":\"{n}\",\"value\":[{x:.0},{y:.2}],{st}{lb}}}",
                        n = json_str(name),
                        x = x,
                        y = y,
                        st = style,
                        lb = label
                    )
                })
                .collect();
            format!(
                "{{\"name\":\"{n}\",\"type\":\"scatter\",\"symbol\":\"{sy}\",\"symbolSize\":9,\"itemStyle\":{{\"color\":\"{lc}\"}},\
                 \"data\":[{d}]}}",
                n = json_str(g),
                sy = symbol(i),
                // 凡例はここの色を使う。点ごとの itemStyle は凡例に届かないので、
                // 系列側にも同じ色を入れないと凡例と実物が食い違う
                lc = c,
                d = pts.join(",")
            )
        })
        .collect();
    let ymx = match y_max {
        Some(m) => format!(",\"max\":{m:.0}"),
        None => String::new(),
    };
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"labelLayout\":{{\"moveOverlap\":\"shiftY\",\"hideOverlap\":true}},\"tooltip\":{{\"trigger\":\"item\",\"formatter\":\"{{b}}<br/>求人数 {{c0}} 件／1求人あたり {{c1}}\"}},\
         \"legend\":{{\"bottom\":0,\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\
         \"grid\":{{\"left\":\"12%\",\"right\":\"9%\",\"top\":\"8%\",\"bottom\":\"30%\"}},\
         \"xAxis\":{{\"type\":\"log\",\"name\":\"求人数（最新月・対数）\",\"nameLocation\":\"middle\",\"nameGap\":26,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         \"yAxis\":{{\"type\":\"value\",\"min\":0{ym},\"name\":\"1求人あたりに見た人数\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         \"series\":[{sr}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        ax = ax,
        ym = ymx,
        sr = series.join(",")
    )
}

/// 横棒 1 本の高さ（px）の見積り。
///
/// # なぜ見積りが要るのか
/// 棒が細いと [`hbar_chart`] の decal（塗り分けの模様）が縞に見えず斑点になる。
/// 外すかどうかを決めるには棒の太さが要るが、ECharts が決めるので設定には出てこない。
/// 一方、材料はこちらが渡している（図の高さと行数）ので、そこから逆算できる。
///
/// # 数字の出どころ
/// テンプレートが読み込むのと同じ ECharts 5.5.1 に [`hbar_chart`] の設定を
/// そのまま描かせ、SVG の矩形を測った（ヘッドレス Chrome）。
///
/// | 行数 | 図の高さ | 1 行の帯 | 棒 |
/// |---|---|---|---|
/// | 47 | 845px | 16.00px | 11.04px |
/// | 30 | 590px | 17.50px | 12.08px |
/// | 21 | 602px | 25.51px | 14px（上限） |
/// | 47 | 1200px | 22.72px | 14px（上限） |
///
/// 帯は「指定した高さ × 0.89」（grid の上 3% と下 8% を除いた分）を行数で割った値、
/// 棒はその 69% で、4 例とも小数第 2 位まで一致した。`barMaxWidth` の 14px で頭打ち。
fn hbar_bar_height(height: u32, rows: usize) -> f64 {
    if rows == 0 {
        return 0.0;
    }
    (height as f64 * 0.89 / rows as f64 * 0.69).min(14.0)
}

/// 横棒。並べ替え済みの値をそのまま出す。基準線を 1 本引ける。
///
/// 47 都道府県のように「順位を見たい」ものは、折れ線でも表でもなく横棒が早い。
pub fn hbar_chart(
    labels: &[String],
    values: &[Option<f64>],
    unit: &str,
    baseline: Option<(f64, &str)>,
    axis_max: Option<f64>,
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

    // 横軸の右端の見当。`axis_max` を渡されていればそれ、渡されていなければ
    // データの最大値を使う（ECharts はそれ以上のきりのいい値に丸めるので、
    // 実際の軸幅はこれと同じか少し広い）。下のラベルの寄せ幅はこれを基準にする。
    let span = axis_max.unwrap_or_else(|| {
        vals.iter()
            .flatten()
            .filter(|n| n.is_finite())
            .fold(0.0f64, |a, b| a.max(*b))
    });

    // 基準線の上と下で色を変える。線だけ引いて同じ色にしていたので、
    // どちらが上回っているのかを長さから読み直す必要があった。
    //
    // 値は棒の脇に出す。47 県だと目盛りが 5 刻みで、76 と 78 の差が読めなかった。
    // ラベルの整形に関数文字列は使えない（`no_function_string_in_charts`）ので、
    // 小数 1 桁に丸めた値をそのまま渡す。表示と並び順が同じ値になる。
    //
    // # 基準線に近い行だけ、値を棒の内側に入れる
    // 脇に出したままだと、基準線の破線が値ラベルを貫いて桁が読めない。
    // ECharts 5.5.1 に実データ（職種「事務」47 県・全国 16.6・高さ 845px）を
    // 描かせて測ると、図の幅 1200px で 15.9 / 16.1 / 16.2 の 3 本、
    // 740px で 5 本、560px で 9 本が貫かれていた。
    //
    // ラベルは棒の右端から 5px 離れた位置に出て、「NN.N」の実測幅は 18.7px。
    // つまり基準線が棒の右端から 23.7px 以内にあると必ず重なる。図の幅は CSS で
    // 決まるので Rust 側では分からないが、狭いほど 1px あたりの値が大きくなるので
    // いちばん狭い側で押さえればよい。560px 幅なら軸 1 単位が 12.3px で、
    // 23.7px は軸幅の 6.4% にあたる。余裕を見て **軸幅の 8%** を「近い」とする。
    //
    // 内側は右端（`insideRight`）に置く。基準線は必ず棒の右端より右にあり、
    // 内側のラベルは右端より左に入るので、どれだけ近くても重ならない。
    // 左端（`insideLeft`）でも重ならないが、47 本が図の左に一列に並んで
    // 目盛りに見えるので、読む位置は棒の端のままにする。
    //
    // 棒が短いと文字が収まらず、左へはみ出して地の上に暗い文字が出る。
    // ラベル 18.7px ＋ 内外の余白で 29px ほど要り、560px 幅では軸幅の 7.8%。
    // **軸幅の 10% に満たない棒は外側のまま**にする。全国値が最大値の 10% を
    // 下回る職種は実データに無い（最小は「測量」の 14.1%、中央値 38.3%）ので、
    // いまのデータでは内側に入れられずに貫かれる行は出ない。
    let near = span * 0.08;
    let too_short = span * 0.10;
    let inside = on_bar_color(dark);
    let data: Vec<String> = vals
        .iter()
        .map(|v| match v {
            Some(n) if n.is_finite() => {
                let c = match baseline {
                    Some((b, _)) if *n < b => pal[1],
                    _ => pal[0],
                };
                let lb = match baseline {
                    Some((b, _)) if *n <= b && b - *n <= near && *n >= too_short => {
                        format!(",\"label\":{{\"position\":\"insideRight\",\"color\":\"{inside}\"}}")
                    }
                    _ => String::new(),
                };
                format!(
                    "{{\"value\":{:.1},\"itemStyle\":{{\"color\":\"{}\"}}{}}}",
                    n, c, lb
                )
            }
            _ => "null".to_string(),
        })
        .collect();

    // decal（塗りの模様）を棒が細いときだけ外す。
    //
    // decal は app.js とレポート側の初期化が `aria.decal.show` で一律に付けており、
    // どちらも `JSON.parse` の**あとで** `cfg.aria` を丸ごと上書きするので、
    // ここから `aria` では消せない。系列の `itemStyle.decal` は aria より優先される
    // ので、そこに透明な模様を入れて打ち消す（ECharts 5.5.1 で実測。
    // なお `"decal":"none"` は文字列のまま扱われて例外になり、図が出なくなる）。
    //
    // 外す理由は 2 つある。
    // 1. 既定の decal の縦の周期は実測 7px。[`hbar_bar_height`] の測定では
    //    47 県の図の棒は 11.04px しかなく、縞が 1 周期半しか入らないので
    //    斑点に見え、棒の端がどこか読みにくい。
    // 2. この図は系列が 1 本で、47 本すべてに**同じ**模様が付く（実測でも
    //    pattern は 1 つ）。基準線の上下は棒ごとの色で分けているが、decal は
    //    それに追従しないので、消しても伝わる情報は減らない。
    let decal = if hbar_bar_height(height, labels.len()) < 12.0 {
        ",\"itemStyle\":{\"decal\":{\"color\":\"transparent\"}}"
    } else {
        ""
    };

    let max = match axis_max {
        Some(m) => format!(",\"max\":{m:.0}"),
        None => String::new(),
    };
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"grid\":{{\"left\":\"22%\",\"right\":\"12%\",\"top\":\"3%\",\"bottom\":\"8%\"}},\
         \"xAxis\":{{\"type\":\"value\",\"min\":0{mx},\"name\":\"{u}\",\"nameLocation\":\"middle\",\"nameGap\":26,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         \"yAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"series\":[{{\"type\":\"bar\",\"data\":[{d}],\"barMaxWidth\":14{dc},\
         \"label\":{{\"show\":true,\"position\":\"right\",\"color\":\"{ax}\",\"fontSize\":9}}{mk}}}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        u = json_str(unit),
        ax = ax,
        mx = max,
        lb = labels_json,
        d = data.join(","),
        dc = decal,
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
    // 軸の左端。dataMin をそのまま使うと「903.358」のような半端な目盛りが出て、
    // 隣の 1,000 / 1,100 と並んだときに読みにくい。100 円単位に切り下げる
    let axis_min = base
        .iter()
        .flatten()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let axis_min = if axis_min.is_finite() {
        format!("{:.0}", (axis_min / 100.0).floor() * 100.0)
    } else {
        String::from("\"dataMin\"")
    };

    // 帯の両端に点を打つ。
    //
    // 帯だけだと、青は「左端＝最低賃金・右端＝掲示時給」、橙は「左端＝掲示時給・
    // 右端＝最低賃金」で、見た目が同じ左→右のバーなのに意味が反転する。
    // しかも土台の透明な系列は凡例から外してあるので、**左端が何かを説明するものが
    // 図の中に無かった**（ux-charts の初回指摘。2 ビルドのあいだ手つかずだった）。
    // 最低賃金＝白丸、掲示時給＝塗りつぶしの丸にすれば、
    // どちらの向きでも「白丸が下限・塗り丸が相場」で読める。
    let min_pt: Vec<Option<f64>> = lo.clone();
    let wage_pt: Vec<Option<f64>> = hi.clone();
    let dot_bg = if dark { "#0d1525" } else { "#ffffff" };
    let (c_up, c_down) = if dark {
        ("#38bdf8", "#f59e0b")
    } else {
        ("#0369a1", "#b45309")
    };
    // 青と橙にする。緑と橙は色の見え方が違う人にはいちばん見分けにくい組
    let legend = if has_down {
        format!(
            "\"{}\",\"{}\",\"{}\",\"掲示時給の中央値\"",
            json_str(high_name),
            json_str(below_name),
            json_str(low_name)
        )
    } else {
        format!(
            "\"{}\",\"{}\",\"掲示時給の中央値\"",
            json_str(high_name),
            json_str(low_name)
        )
    };
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"legend\":{{\"bottom\":0,\"data\":[{lg}],\"textStyle\":{{\"color\":\"{ax}\",\"fontSize\":11}}}},\
         \"grid\":{{\"left\":\"22%\",\"right\":\"8%\",\"top\":\"3%\",\"bottom\":\"14%\"}},\
         \"xAxis\":{{\"type\":\"value\",\"name\":\"{u}\",\"nameLocation\":\"middle\",\"nameGap\":26,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"min\":{amin},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         \"yAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"series\":[\
         {{\"name\":\"帯の土台\",\"type\":\"bar\",\"stack\":\"w\",\"data\":[{d0}],\
         \"tooltip\":{{\"show\":false}},\"itemStyle\":{{\"color\":\"transparent\"}},\"barMaxWidth\":12}},\
         {{\"name\":\"{hn}\",\"type\":\"bar\",\"stack\":\"w\",\"data\":[{d1}],\
         \"itemStyle\":{{\"color\":\"{cu}\"}},\"barMaxWidth\":12}},\
         {{\"name\":\"{bn}\",\"type\":\"bar\",\"stack\":\"w\",\"data\":[{d2}],\
         \"itemStyle\":{{\"color\":\"{cd}\"}},\"barMaxWidth\":12}},\
         {{\"name\":\"{ln}\",\"type\":\"scatter\",\"symbolSize\":7,\
         \"itemStyle\":{{\"color\":\"{db}\",\"borderColor\":\"{ax}\",\"borderWidth\":1.4}},\
         \"data\":[{dp0}]}},\
         {{\"name\":\"掲示時給の中央値\",\"type\":\"scatter\",\"symbolSize\":7,\
         \"itemStyle\":{{\"color\":\"{ax}\"}},\"data\":[{dp1}]}}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        ax = ax,
        lg = legend,
        db = dot_bg,
        dp0 = series_json(&min_pt),
        dp1 = series_json(&wage_pt),
        amin = axis_min,
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

/// 中央 0 の横棒。増えたものを右、減ったものを左に出す。
///
/// # なぜ順位の横棒と分けるのか
/// [`hbar_chart`] は 0 から伸びる量の比較で、長さがそのまま大小になる。
/// こちらは**向きに意味がある**。同じ長さでも右と左では逆のことを言うので、
/// 色を分け、0 に線を引いて、どちら側かが先に目に入るようにする。
///
/// `rows` は (ラベル, 値)。値の符号がそのまま向きになる。
/// 並べ替えは呼び出し側の責任（降順で渡せば増えたものが上に来る）。
pub fn tornado_chart(
    rows: &[(String, Option<f64>)],
    unit: &str,
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let pal = palette(dark);
    // 増えた側と減った側。青と橙にする（緑と橙は色の見え方が違う人に見分けにくい）
    let (c_up, c_down) = (pal[0], pal[1]);
    // ECharts の縦軸は下から積むので、上を 1 位にするため逆順に入れる
    let labels = rows
        .iter()
        .rev()
        .map(|r| format!("\"{}\"", json_str(&r.0)))
        .collect::<Vec<_>>()
        .join(",");
    let data = rows
        .iter()
        .rev()
        .map(|(_, v)| match v {
            Some(x) if x.is_finite() => format!(
                "{{\"value\":{:.3},\"itemStyle\":{{\"color\":\"{}\"}}}}",
                x,
                if *x >= 0.0 { c_up } else { c_down }
            ),
            _ => "null".to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"grid\":{{\"left\":\"30%\",\"right\":\"8%\",\"top\":\"3%\",\"bottom\":\"14%\"}},\
         \"xAxis\":{{\"type\":\"value\",\"name\":\"{u}\",\"nameLocation\":\"middle\",\"nameGap\":26,\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         \"yAxis\":{{\"type\":\"category\",\"data\":[{lb}],\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLine\":{{\"show\":false}},\"axisTick\":{{\"show\":false}}}},\
         \"series\":[{{\"type\":\"bar\",\"data\":[{d}],\"barMaxWidth\":14,\
         \"markLine\":{{\"silent\":true,\"symbol\":\"none\",\
         \"lineStyle\":{{\"color\":\"{ax}\",\"width\":1}},\"label\":{{\"show\":false}},\
         \"data\":[{{\"xAxis\":0}}]}}}}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        ax = ax,
        u = json_str(unit),
        lb = labels,
        d = data
    )
}

/// 暦月などを縦棒で並べる。
///
/// # 0 を切らない
/// 以前は `"scale":true` で軸を自動調整していた。季節の図では値が 0.86〜1.09 の
/// 範囲なので軸が 0.85 から始まり、12 月（0.862）と 3 月（1.092）の棒の高さが
/// **約 1 対 8** に見えていた。実際の差は 21% しかない。棒は面で量を表すので、
/// 切った基線は誤読を招く。いまは呼び出し側で「平均からの差」に直して渡し、
/// 0 を基線にしている。
pub fn vbar_chart(
    labels: &[String],
    values: &[Option<f64>],
    unit: &str,
    dark: bool,
    height: u32,
) -> String {
    let ax = axis_color(dark);
    let pal = palette(dark);
    // 上振れと下振れで色を分ける。同じ色だと、基線のどちら側かを
    // 棒の向きだけで読み直すことになる。
    let data: Vec<String> = values
        .iter()
        .map(|v| match v {
            Some(n) if n.is_finite() => format!(
                "{{\"value\":{:.2},\"itemStyle\":{{\"color\":\"{}\"}}}}",
                n,
                if *n < 0.0 { pal[1] } else { pal[0] }
            ),
            _ => "null".to_string(),
        })
        .collect();
    format!(
        "<div class=\"echart\" style=\"height:{h}px;\" data-chart-config='{{\
         \"tooltip\":{{\"trigger\":\"axis\",\"axisPointer\":{{\"type\":\"shadow\"}}}},\
         \"grid\":{{\"left\":\"14%\",\"right\":\"6%\",\"top\":\"14%\",\"bottom\":\"14%\"}},\
         \"xAxis\":{{\"type\":\"category\",\"data\":[{lb}],\"axisLine\":{{\"onZero\":true}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}}}},\
         \"yAxis\":{{\"type\":\"value\",\"name\":\"{u}\",\
         \"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10,\"align\":\"left\"}},\"nameGap\":8,\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         \"series\":[{{\"type\":\"bar\",\"data\":[{d}],\"barMaxWidth\":34,\
         \"label\":{{\"show\":true,\"position\":\"outside\",\"color\":\"{ax}\",\"fontSize\":9}}}}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
        ax = ax,
        u = json_str(unit),
        lb = labels
            .iter()
            .map(|l| format!("\"{}\"", json_str(l)))
            .collect::<Vec<_>>()
            .join(","),
        d = data.join(",")
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
         {{\"type\":\"value\",\"min\":0,\"name\":\"{bn}\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"lineStyle\":{{\"color\":\"{gl}\"}}}}}},\
         {{\"type\":\"value\",\"name\":\"{lnn}\",\"nameTextStyle\":{{\"color\":\"{ax}\",\"fontSize\":10}},\
         \"axisLabel\":{{\"color\":\"{ax}\",\"fontSize\":10}},\"splitLine\":{{\"show\":false}},\"min\":0}}],\
         \"series\":[\
         {{\"name\":\"{bn}\",\"type\":\"bar\",\"data\":[{d1}],\"itemStyle\":{{\"color\":\"{c0}\",\"opacity\":0.75}},\"barMaxWidth\":26}},\
         {{\"name\":\"{lnn}\",\"type\":\"line\",\"yAxisIndex\":1,\"data\":[{d2}],\"showSymbol\":false,\
         \"lineStyle\":{{\"width\":2,\"color\":\"{c1}\"}},\"itemStyle\":{{\"color\":\"{c1}\"}}}}]}}'></div>",
        h = height,
        gl = grid_line_color(dark),
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
        let v = parsed(&scatter_chart(&pts, &groups, &[], None, true, 300));
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
        let v = parsed(&scatter_chart(&pts, &groups, &[], None, true, 300));
        assert_eq!(v["series"][0]["data"][0]["name"], "変な\"名前\"の職種");
    }

    #[test]
    fn 横棒は一位が上に来る() {
        let labels = vec!["東京都".into(), "大阪府".into(), "福岡県".into()];
        let values = vec![Some(12.0), Some(8.0), Some(5.0)];
        let v = parsed(&hbar_chart(&labels, &values, "人", Some((9.0, "全国")), None, true, 300));
        // ECharts の縦軸は下から積むので、渡す配列は逆順になっているのが正しい
        let cats = v["yAxis"]["data"].as_array().unwrap();
        assert_eq!(cats[cats.len() - 1], "東京都", "1 位が上に来ていない");
        let data = v["series"][0]["data"].as_array().unwrap();
        // 棒ごとに色を持たせたので、値は {value, itemStyle} の中にある
        assert_eq!(data[data.len() - 1]["value"], 12.0);
        assert!(v["series"][0]["markLine"].is_object(), "基準線が無い");
        // 基準線(9.0)の上下で色を変える。線だけ引いて同じ色にすると、
        // どちらが上回っているかを長さから読み直すことになる
        assert_ne!(
            data[data.len() - 1]["itemStyle"]["color"],
            data[0]["itemStyle"]["color"],
            "基準線の上下で色が変わっていない"
        );
        // 値は棒の脇に出す。47 県だと目盛りが粗く、76 と 78 の差が読めない
        assert_eq!(v["series"][0]["label"]["show"], true, "値ラベルが出ていない");
    }

    /// 基準線に近い行の値ラベルは棒の内側に入れる。
    ///
    /// # なぜ見張るのか
    /// 脇に出したままだと、基準線の破線が数字を貫いて桁が読めない。
    /// ECharts 5.5.1 に職種「事務」の 47 県（全国 16.6）を描かせて測ると、
    /// 図の幅 1200px で 15.9 / 16.1 / 16.2 の 3 本、740px で 5 本、
    /// 560px で 9 本のラベルが貫かれていた。ここで使う値はその実データ。
    ///
    /// 内側は右端に置く。基準線は必ず棒の右端より右にあるので、
    /// 右端より左に入れれば、どれだけ近くても重ならない。
    #[test]
    fn 基準線に近い値のラベルは棒の内側に入れる() {
        let labels: Vec<String> = ["鳥取県", "茨城県", "岡山県", "大阪府", "静岡県", "福井県"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let values = vec![
            Some(28.93),
            Some(16.96),
            Some(16.18),
            Some(16.07),
            Some(15.92),
            Some(6.71),
        ];
        let v = parsed(&hbar_chart(
            &labels,
            &values,
            "1 求人あたりに見た人数",
            Some((16.635, "全国")),
            None,
            true,
            300,
        ));
        let d = v["series"][0]["data"].as_array().unwrap();
        // 逆順に入るので [福井, 静岡, 大阪, 岡山, 茨城, 鳥取]
        for (i, name) in [(1usize, "静岡県"), (2, "大阪府"), (3, "岡山県")] {
            assert_eq!(
                d[i]["label"]["position"], "insideRight",
                "{name} のラベルが棒の外に出たままで、基準線に貫かれる"
            );
            assert!(
                d[i]["label"]["color"].is_string(),
                "{name} のラベルが棒の上に載るのに色を変えていない"
            );
        }
        // 基準線より上の行は、ラベルが線より右に出るので動かさない
        assert!(d[4]["label"].is_null(), "茨城県は基準線より上なのに内側へ寄せている");
        assert!(d[5]["label"].is_null(), "1 位まで内側へ寄せている");
        // 遠い行も動かさない。全部内側にすると読む位置が揃わない
        assert!(d[0]["label"].is_null(), "基準線から遠い福井県まで内側へ寄せている");
    }

    /// 文字が収まらない短い棒は、内側に入れない。
    ///
    /// 棒より文字が長いと左へはみ出し、暗い文字が地の上に出て読めなくなる。
    /// 全国値が最大値のすぐ上にある図では、基準線に近い棒はどれも短い。
    #[test]
    fn 短すぎる棒には値を入れない() {
        let labels = vec!["A".to_string(), "B".to_string()];
        let values = vec![Some(100.0), Some(5.0)];
        let v = parsed(&hbar_chart(&labels, &values, "件", Some((6.0, "全国")), None, true, 300));
        let d = v["series"][0]["data"].as_array().unwrap();
        // 逆順なので添字 0 が B（軸の 5% しかない）
        assert!(d[0]["label"].is_null(), "軸の 5% しかない棒に文字を入れている");
    }

    /// 棒の高さの見積りが、ECharts の実寸と合っていること。
    ///
    /// この見積りだけで模様を外すかどうかが決まるので、ずれると
    /// 外す/外さないが逆になる。数字はヘッドレス Chrome で
    /// ECharts 5.5.1 に描かせて測った実寸。
    #[test]
    fn 棒の高さの見積りは実測と合う() {
        for (rows, h, want) in [
            (47usize, 845u32, 11.04),
            (30, 590, 12.08),
            (21, 602, 14.0),
            (47, 1200, 14.0),
        ] {
            let got = hbar_bar_height(h, rows);
            assert!(
                (got - want).abs() < 0.05,
                "{rows} 行・{h}px の棒を {got:.2}px と見積もったが、実測は {want}px"
            );
        }
        assert_eq!(hbar_bar_height(300, 0), 0.0, "0 行で 0 除算している");
    }

    /// 棒が細いときだけ模様（decal）を外す。
    ///
    /// # なぜ見張るのか
    /// 模様は app.js とレポート側が `aria.decal.show` で一律に付けている。
    /// 既定の模様は縦の周期が実測 7px で、47 県の図の棒 11.04px には
    /// 1 周期半しか入らず、縞ではなく斑点に見えて棒の端が読みにくい。
    /// 打ち消せるのは系列の `itemStyle.decal` だけ（`aria` は初期化側が
    /// `JSON.parse` のあとで丸ごと上書きするので、ここからは触れない）。
    #[test]
    fn 細い横棒では模様を外す() {
        let chart = |n: usize, h: u32| {
            let labels: Vec<String> = (0..n).map(|i| i.to_string()).collect();
            let values: Vec<Option<f64>> = (0..n).map(|i| Some(i as f64 + 1.0)).collect();
            parsed(&hbar_chart(&labels, &values, "件", None, None, true, h))
        };
        // 47 県・845px は棒 11.04px
        let thin = chart(47, 845);
        assert_eq!(
            thin["series"][0]["itemStyle"]["decal"]["color"], "transparent",
            "11px の棒に模様が残っている"
        );
        // 20 職種＋区切り 1 行・602px は上限いっぱいの 14px。ここは触らない
        let thick = chart(21, 602);
        assert!(
            thick["series"][0]["itemStyle"].is_null(),
            "14px ある棒の模様まで外している"
        );
    }

    /// どの図でも目盛り線を**色**で指定していること。
    ///
    /// # なぜ 1 枚ずつ見張るのか
    /// 目盛り線の指定は 9 つの関数に散っている。`opacity` 任せに戻すと、
    /// 地の明るさが変わった日にその図だけ線が消える。実際にそれが起きて、
    /// カードを `bg-navy-700` に明るくしただけで全図の目盛り線が
    /// 1.06:1（＝引いていないのと同じ）になった。
    /// 「1 枚直して終わり」にしないよう、全種類を作って確かめる。
    #[test]
    fn どの図も目盛り線を色で指定している() {
        /// 設定の中の `splitLine` を全部集める。
        fn collect(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, x) in m {
                        if k == "splitLine" {
                            out.push(x.clone());
                        }
                        collect(x, out);
                    }
                }
                serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
                _ => {}
            }
        }

        let months = vec!["2026-07".to_string(), "2026-08".to_string()];
        let two = vec![Some(1.0), Some(2.0)];
        let labels = vec!["東京都".to_string(), "大阪府".to_string()];
        let pts = vec![("A".to_string(), 100.0, 5.0, "G".to_string())];
        let groups = vec!["G".to_string()];
        for dark in [true, false] {
            let want = grid_line_color(dark);
            let charts = [
                ("line_chart", line_chart(&months, &[("全国".to_string(), two.clone())], dark, 300)),
                ("raw_line_chart", raw_line_chart(&months, &[("全国".to_string(), two.clone())], dark, 300, "件")),
                ("dual_line_chart", dual_line_chart(&months, ("検索", &two, "回"), ("見た人", &two, "人"), dark, 300)),
                ("scatter_chart", scatter_chart(&pts, &groups, &[], None, dark, 300)),
                ("hbar_chart", hbar_chart(&labels, &two, "件", Some((1.5, "全国")), None, dark, 300)),
                ("dumbbell_chart", dumbbell_chart(&labels, &[Some(1000.0), Some(1100.0)], &[Some(1200.0), Some(1300.0)], "最低賃金", "上乗せ", "下回る分", "円", dark, 300)),
                ("tornado_chart", tornado_chart(&[("A".to_string(), Some(1.0)), ("B".to_string(), Some(-1.0))], "ポイント", dark, 300)),
                ("vbar_chart", vbar_chart(&labels, &two, "件", dark, 300)),
                ("bar_line_chart", bar_line_chart(&months, "棒", &two, "線", &two, dark, 300)),
            ];
            for (name, html) in &charts {
                let mut found = Vec::new();
                collect(&parsed(html), &mut found);
                assert!(!found.is_empty(), "{name} に splitLine の指定が無い");
                for sl in &found {
                    // 出さない軸（2 軸の図の右側）はそのままでよい
                    if sl["show"] == serde_json::Value::Bool(false) {
                        continue;
                    }
                    assert!(
                        sl["lineStyle"]["opacity"].is_null(),
                        "{name}（dark={dark}）の目盛り線が opacity 指定に戻っている。\
                         地の明るさが変わると消える"
                    );
                    assert_eq!(
                        sl["lineStyle"]["color"].as_str(),
                        Some(want),
                        "{name}（dark={dark}）の目盛り線の色がほかの図とそろっていない"
                    );
                }
            }
        }
    }

    /// 単位を「万」にするのは、実数のままだと目盛りが 7 桁になるときだけ。
    ///
    /// # なぜ見張るのか
    /// 境目が 10 万だった頃は、そこが県の分布のど真ん中にあり、
    /// **同じ指標なのに県を替えると単位が変わっていた**（求人数は東京・大阪・
    /// 愛知・神奈川・埼玉だけ「万件」、企業数は全国だけ「万社」）。
    /// 100 万にすると、県ごとの求人数も企業数も単位が動かない。
    #[test]
    fn 単位を万にするのは目盛りが七桁になるときだけ() {
        let unit_name = |max: f64| {
            let v = parsed(&raw_line_chart(
                &["2026-08".to_string()],
                &[("A".to_string(), vec![Some(max)])],
                true,
                200,
                "件",
            ));
            v["yAxis"]["name"].as_str().unwrap().to_string()
        };
        // 6 桁までは実数のまま。東京都の求人数 277,464 件、
        // 全国の企業数 571,336 社はどちらもこちら側に入る
        assert_eq!(unit_name(277_464.0), "件");
        assert_eq!(unit_name(571_336.0), "件");
        assert_eq!(unit_name(999_999.0), "件");
        // 7 桁になるところから「万」。全国ぜんぶの求人数と見た人数だけ
        assert_eq!(unit_name(1_000_000.0), "万件");
        assert_eq!(unit_name(1_949_238.0), "万件");
        assert_eq!(unit_name(20_546_069.0), "万件");
    }

    /// 「万」に割ったあとの目盛りに小数が出ないこと。
    ///
    /// 「0.06 万件」は読むのに掛け算が要る。境目が 100 万なら、
    /// 割ったあとは必ず 100 以上になるので整数の目盛りで収まる。
    #[test]
    fn 万に割ったあとの値は必ず百以上になる() {
        for max in [1_000_000.0, 1_949_238.0, 20_546_069.0_f64] {
            let (by, prefix) = scale_of(&[("A".to_string(), vec![Some(max)])]);
            assert_eq!(prefix, "万");
            assert!(
                max / by >= 100.0,
                "{max} を {by} で割ると {:.2} になり、目盛りに小数が出る",
                max / by
            );
        }
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
        // 帯 3 本（透明の土台・上乗せ・下回る分）＋ 両端の点 2 本
        assert_eq!(series.len(), 5);
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

        // 帯の両端に点を打つ。帯だけだと、上乗せは「左端＝最低賃金」、
        // 下回る分は「左端＝掲示時給」で、同じ向きのバーなのに意味が反転する。
        // しかも土台は透明で凡例からも外れているので、左端が何かを説明するものが
        // 図中に無かった。白丸＝最低賃金、塗り丸＝掲示時給で、どちらの向きでも読める。
        assert_eq!(series[3]["type"], "scatter", "最低賃金の点が無い");
        assert_eq!(series[4]["type"], "scatter", "掲示時給の点が無い");
        let lo_pt = series[3]["data"].as_array().unwrap();
        let hi_pt = series[4]["data"].as_array().unwrap();
        assert_eq!(lo_pt[lo_pt.len() - 1], 1226.0, "最低賃金の点が下限に無い");
        assert_eq!(hi_pt[hi_pt.len() - 1], 1520.0, "掲示時給の点が相場に無い");
        // 白丸と塗り丸で描き分ける
        assert_ne!(
            series[3]["itemStyle"]["color"],
            series[4]["itemStyle"]["color"],
            "両端の点が同じ見た目になっている"
        );
    }

    /// 増えた側と減った側で色が変わり、0 に線が入ること。
    ///
    /// 順位の横棒と違い、こちらは長さだけでなく**向き**に意味がある。
    /// 同じ長さでも右と左では逆のことを言うので、色で先に分かるようにする。
    #[test]
    fn 増減の横棒は向きで色が変わる() {
        let rows = vec![
            ("一般事務".to_string(), Some(13.3)),
            ("事務".to_string(), Some(-16.3)),
            ("欠測".to_string(), None),
        ];
        let v = parsed(&tornado_chart(&rows, "ポイント", true, 300));
        let d = v["series"][0]["data"].as_array().unwrap();
        // 逆順に入るので、末尾が 1 行目（一般事務）
        assert_eq!(d[2]["value"], 13.3);
        assert_eq!(d[1]["value"], -16.3);
        let up = d[2]["itemStyle"]["color"].as_str().unwrap();
        let down = d[1]["itemStyle"]["color"].as_str().unwrap();
        assert_ne!(up, down, "増えた側と減った側が同じ色");
        assert!(d[0].is_null(), "欠測が 0 として入っている");
        // 0 の線が引かれていること
        assert_eq!(v["series"][0]["markLine"]["data"][0]["xAxis"], 0);
    }

    /// 横軸の左端を丸める。
    ///
    /// `min: dataMin` をそのまま使うと、いちばん低い値がそのまま目盛りになる。
    /// 実データでは「903.358」と出て、隣の 1,000 / 1,100 と並ぶと読みにくかった。
    #[test]
    fn 横軸の左端は百円単位に切り下げる() {
        let v = parsed(&dumbbell_chart(
            &["徳島県".into(), "大分県".into()],
            &[Some(1046.0), Some(1024.0)],
            &[Some(903.358), Some(1497.0)],
            "最低賃金",
            "上乗せ",
            "下回る分",
            "円",
            true,
            300,
        ));
        assert_eq!(v["xAxis"]["min"], 900.0, "左端が丸められていない");
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

        // 凡例に出す項目は、色見本が見えるものだけ。
        //
        // 以前は「最低賃金 が凡例に無いこと」で確かめていたが、それは
        // 「最低賃金＝透明な土台」という当時の実装に縛られた書き方だった。
        // 帯の両端に点を足したとき、最低賃金は白丸の系列として凡例に出したいのに
        // このテストが落ち、名前だけを見ていたことが分かった。
        // 見たいのは「凡例のどの項目も透明ではない」という不変条件のほう。
        for name in &lg {
            let hit = series
                .iter()
                .find(|s| s["name"].as_str() == Some(name.as_str()))
                .unwrap_or_else(|| panic!("凡例の {name} に対応する系列が無い"));
            assert_ne!(
                hit["itemStyle"]["color"], "transparent",
                "凡例の {name} の色見本が透明"
            );
        }
        // 透明な土台そのものは凡例に出さない
        assert!(!lg.contains(&"帯の土台".to_string()));
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
mod category_style_tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// 色が近い 3 組が、同じ線種を一切共有しないこと。
    ///
    /// # なぜ共起で確かめないのか
    /// 最初は「同時に上位へ入る分類どうし」だけを検査していた。それだと
    /// 緑 dotted × 薔薇 dotted のような組が「たまたま同時に上位へ入らない」という
    /// データ依存の事実だけで守られる。古い月を取り込めば顔ぶれは変わるので、
    /// 指数を廃止したのと同じ穴になる。ここでは表の形だけを見る。
    ///
    /// 近さは色差（Machado 2009 + CIE Lab）と色相角の両方で見る。
    ///
    /// 色差だけで判定していたときは、色差 26.9 の 桃 × 薔薇 が実物では
    /// 1 本の太い破線に見えて区別できなかった（ux-charts の目視）。
    /// 折れ線で「どの線か」を見分けるのは主に色相なので、色相差 40 度未満も
    /// 保護対象に入れる。該当は 4 組。
    ///   緑×薔薇 9.2 / 橙×黄 15.7（色相 13 度）/ 青×藍 16.8 / 桃×薔薇（色相 23 度）
    const CLOSE: [(usize, usize); 4] = [(2, 5), (1, 4), (0, 6), (3, 5)];

    #[test]
    fn 色が近い組は線種を共有しない() {
        let mut used: HashMap<usize, HashSet<&str>> = HashMap::new();
        for (_, ci, d) in CATEGORY_STYLE.iter() {
            used.entry(*ci).or_default().insert(d);
        }
        for (a, b) in CLOSE {
            let ea = used.get(&a).cloned().unwrap_or_default();
            let eb = used.get(&b).cloned().unwrap_or_default();
            let both: Vec<&&str> = ea.intersection(&eb).collect();
            assert!(
                both.is_empty(),
                "色 {a} と色 {b} は見分けにくいのに、線種 {both:?} を共有している。\
                 同じ図に並ぶと色も線種も同じになる"
            );
        }
    }

    #[test]
    fn 同じ見た目の分類が二つ無い() {
        let mut seen: HashSet<(usize, &str)> = HashSet::new();
        for (n, ci, d) in CATEGORY_STYLE.iter() {
            assert!(seen.insert((*ci, d)), "{n} が他の分類と同じ色・同じ線種になっている");
        }
    }

    /// 線種が片寄っていないこと。
    ///
    /// 点線は暗い背景でいちばん追いにくい。12 分類の半分が点線だと、
    /// 1 枚の図に点線が 4〜5 本並びうる。
    /// 薔薇はこの表で使わないこと。
    ///
    /// 薔薇は保護対象 4 組のうち 2 組（緑×薔薇 9.2、桃×薔薇 色相 23 度）の
    /// 当事者で、表に入れると線種 3 種類では 12 分類をさばけない（容量 11）。
    /// パレットには残してあり、散布図の業界などでは使う。
    #[test]
    fn 分類の表に薔薇を使わない() {
        for (n, ci, _) in CATEGORY_STYLE.iter() {
            assert_ne!(*ci, 5, "{n} が薔薇を使っている。容量が足りなくなる");
        }
    }

    #[test]
    fn 線種が片寄っていない() {
        let mut c: HashMap<&str, usize> = HashMap::new();
        for (_, _, d) in CATEGORY_STYLE.iter() {
            *c.entry(d).or_default() += 1;
        }
        let dotted = c.get("dotted").copied().unwrap_or(0);
        assert!(
            dotted * 3 <= CATEGORY_STYLE.len() + 1,
            "点線が {dotted} 本。全 {} 分類に対して多すぎる",
            CATEGORY_STYLE.len()
        );
    }

    /// 表に無い名前でも、県をまたいで同じ見た目になること。
    ///
    /// 以前は並び順で色を決めていたので、「その県での上位」が変わるたびに
    /// 同じ分類の色が入れ替わっていた。既定側で並び順に戻すと元に戻ってしまう。
    #[test]
    fn 表に無い名前も県で変わらない() {
        // 並び順を渡す口自体を無くしてある。名前が同じなら必ず同じ見た目
        let a = style_of("表に無い分類", true);
        let b = style_of("表に無い分類", true);
        assert_eq!(a, b);
        assert_ne!(
            style_of("表に無い分類", true),
            style_of("別の分類", true),
            "違う名前が同じ見た目になっている"
        );
    }
}

#[cfg(test)]
mod scatter_size_tests {
    use super::chart_tests::parsed;
    use super::*;

    /// 標本の薄い職種が白抜きになること。
    ///
    /// 以前はここで「点の大きさが求人数で決まる」ことを確かめていた。
    /// だが実測すると鳥取県では 86 点すべてが下限 5px、東京都でも 122 点中
    /// 108 点が下限で、大きさは何も伝えていなかった。求人数は横軸そのものなので
    /// 二重表現でもあった。いまは大きさを一定にし、標本の薄さを塗りで見せる。
    #[test]
    fn 求人の少ない職種は白抜きになる() {
        let pts = vec![
            ("薄い".to_string(), 5.0, 90.0, "G".to_string()),
            ("厚い".to_string(), 200_000.0, 3.0, "G".to_string()),
        ];
        let groups = vec!["G".to_string()];
        let v = parsed(&scatter_chart(&pts, &groups, &[], None, true, 300));
        let d = v["series"][0]["data"].as_array().unwrap();
        assert_eq!(
            d[0]["itemStyle"]["color"], "transparent",
            "求人 {FEW_JOBS} 件未満は白抜きにする"
        );
        assert_ne!(d[1]["itemStyle"]["color"], "transparent");
        // 大きさは系列側で一定。点ごとに持たせない
        assert!(d[0]["symbolSize"].is_null());
        assert_eq!(v["series"][0]["symbolSize"], 9);
        // 値は [求人数, 1 求人あたり] の 2 つ。tooltip の {c0}/{c1} がこれを指す
        assert_eq!(d[0]["value"].as_array().unwrap().len(), 2);
    }

    /// 縦軸の上限と、名前を出す点を呼び出し側から決められること。
    ///
    /// 東京都では 1 点（207.5）が上限を決めてしまい、残り 121 点が
    /// 下 1/3 に潰れて重なっていた。
    #[test]
    fn 縦軸の上限とラベルを外から渡せる() {
        let pts = vec![
            ("名前を出す".to_string(), 100.0, 5.0, "G".to_string()),
            ("出さない".to_string(), 200.0, 6.0, "G".to_string()),
        ];
        let groups = vec!["G".to_string()];
        let names = vec!["名前を出す".to_string()];
        let v = parsed(&scatter_chart(&pts, &groups, &names, Some(80.0), true, 300));
        assert_eq!(v["yAxis"]["max"], 80.0);
        assert_eq!(v["yAxis"]["min"], 0);
        let d = v["series"][0]["data"].as_array().unwrap();
        assert_eq!(d[0]["label"]["show"], true);
        assert!(d[1]["label"].is_null(), "渡していない点に名前を出さない");
    }

    /// 本文で名前を挙げた職種は、図にもラベルとして渡っていること。
    ///
    /// # なぜこれを見張るのか
    /// 散布図のラベルは重なると読めないので ECharts 側で間引かせている。
    /// はじめ `hideOverlap` だけを指定したところ、実測で最大 62% のラベルが
    /// 消えた（愛知県で 8 個中 3 個）。`hideOverlap` は描画順で機械的に落とすので、
    /// **本文が「1 求人あたりが少ないのは A、B、C の順でした」と名指しした職種の
    /// ラベルが消える**ことがある。そうなると本文と図の対応が切れる。
    /// いまは先に縦へずらし（`moveOverlap`）、それでも駄目なものだけ隠している。
    ///
    /// ここで確かめるのは「渡す側が落としていないこと」まで。実際に何本描かれるかは
    /// 画面の幅と点の込み具合で決まるので、そこは目視に任せる。
    #[test]
    fn 名前を挙げた職種はラベルに渡す() {
        let pts = vec![
            ("多い職種".to_string(), 90_000.0, 3.0, "G".to_string()),
            ("普通の職種".to_string(), 500.0, 9.0, "G".to_string()),
        ];
        let groups = vec!["G".to_string()];
        let names = vec!["多い職種".to_string()];
        let v = parsed(&scatter_chart(&pts, &groups, &names, None, true, 300));
        let d = v["series"][0]["data"].as_array().unwrap();
        assert_eq!(d[0]["label"]["show"], true, "名指しした職種にラベルが無い");
        assert!(d[1]["label"].is_null(), "渡していない職種にラベルが付いている");
        // 重なりは隠す前にずらす。隠すだけだと名指しした職種が消えうる。
        //
        // **トップレベルに置くこと。** series に置くと ECharts は series 単位で
        // 評価するので、series をまたいだ重なりが解決されない。ラベルを出す点は
        // 6 業界中 5 業界に散っているため、ほぼ全部が series 間の衝突になる
        // （ux-visual が zrender の実測で特定）。
        let ll = &v["labelLayout"];
        assert_eq!(ll["moveOverlap"], "shiftY", "重なりをずらす指定が無い");
        assert_eq!(ll["hideOverlap"], true);
        assert!(
            v["series"][0]["labelLayout"].is_null(),
            "labelLayout が series 側にある。series をまたいだ重なりが解決されない"
        );
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
            scatter_chart(&pts, &vec!["G".to_string()], &[], None, true, 300),
            hbar_chart(
                &["東京都".to_string(), "大阪府".to_string()],
                &[Some(1.0), Some(2.0)],
                "件",
                Some((1.5, "全国")),
                None,
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

    /// 目盛りの刻みは 1 / 2 / 5 × 10^n のはしごからしか出ない。
    ///
    /// # なぜデータを使わずに検査するのか
    /// 「いまのデータでは刻みがキリのいい数字になっている」は保証ではない。
    /// 桁を広く振って試し、**どんな入力でもはしごから外れない**ことを見る。
    /// 指数の基準がデータ依存だった件と同じ型の欠陥を防ぐ。
    #[test]
    fn 目盛りの刻みははしごから選ばれる() {
        let mut 試した = 0;
        for e in -3..=8 {
            for lo_m in [1.0_f64, 1.7, 3.0, 4.9, 6.3, 8.8] {
                for 幅倍 in [0.03_f64, 0.1, 0.35, 0.9, 2.5, 7.0] {
                    let lo = lo_m * 10.0_f64.powi(e);
                    let hi = lo + lo * 幅倍;
                    let v = vec![Some(lo), Some((lo + hi) / 2.0), Some(hi)];
                    let Some((min, max, step)) = ladder_axis(&v) else {
                        continue;
                    };
                    試した += 1;
                    // 刻みが 1/2/5 × 10^n であること
                    let k = step / 10.0_f64.powi(step.log10().floor() as i32);
                    assert!(
                        [1.0, 2.0, 5.0].iter().any(|m| (k - m).abs() < 1e-6),
                        "刻み {step} は 1/2/5 × 10^n ではない（{lo}〜{hi}）"
                    );
                    // 窓がデータを包んでいること
                    assert!(min <= lo + 1e-9, "下端 {min} がデータ {lo} を切っている");
                    assert!(max >= hi - 1e-9, "上端 {max} がデータ {hi} を切っている");
                    // 本数が 3〜6 であること
                    let n = ((max - min) / step).round() as i64;
                    assert!((3..=6).contains(&n), "本数 {n} が 3〜6 から外れた（{lo}〜{hi}）");
                }
            }
        }
        assert!(試した > 300, "検査したケースが {試した} 件しかない");
    }

    /// 窓はデータに寄っている（0 起点より狭い）。
    ///
    /// 0 起点だと図の高さの平均 39% しか使えていなかった。この検査は
    /// **「0 から始めていない」ことを直接見る**。0 に戻す変更が入れば落ちる。
    #[test]
    fn 二軸の図は零から始めない() {
        // 値が高いところで小さく動く形。0 起点だといちばん潰れる
        let v: Vec<Option<f64>> = vec![Some(278.0), Some(301.0), Some(340.0), Some(370.0)];
        let (min, max, step) = ladder_axis(&v).expect("窓を決められなかった");
        assert!(min > 0.0, "下端が {min} で 0 起点のまま");
        let 使える高さ = (370.0 - 278.0) / (max - min);
        assert!(
            使える高さ > 0.5,
            "図の高さの {:.0}% しか使えていない（窓 {min}〜{max} 刻み {step}）",
            使える高さ * 100.0
        );
    }

    /// 値が全部同じ・空のときは窓を作らず 0 起点に戻す。
    ///
    /// 窓が潰れて刻みが 0 になると ECharts が描かなくなる。
    #[test]
    fn 窓を決められないときは零起点に戻す() {
        assert_eq!(ladder_axis(&[]), None, "空");
        assert_eq!(ladder_axis(&[None, None]), None, "全部欠測");
        assert_eq!(ladder_axis(&[Some(5.0)]), None, "1 点だけ");
        assert_eq!(ladder_axis(&[Some(5.0), Some(5.0)]), None, "全部同じ値");
        // 実際の図でも min:0 に戻っていること
        let flat = vec![Some(5.0), Some(5.0)];
        let h = dual_line_chart(
            &["2026-07".to_string(), "2026-08".to_string()],
            ("検索", &flat, "回"),
            ("見た人", &flat, "人"),
            true,
            300,
        );
        assert!(h.contains("\"min\":0"), "窓を作れないのに 0 起点に戻っていない");
    }

    /// 伸びの内訳の帯は、どんな向きの組み合わせでも幅を持つ。
    ///
    /// # 何を防いでいるか
    /// 最初の実装は割合を 0〜100 に丸めて 1 本の帯を描いていた。
    /// 会社と 1 社あたりが逆を向くと割合が範囲の外に出る（実測でフォークリフトが
    /// -99%）。丸めた結果 0% になり、**帯に何も描かれないのに文章だけ「-99%」**
    /// と出た。ユーザーから「描写が機能してない」と指摘されて分かった。
    /// 幅を数えるこの検査があれば、同じ壊し方はできない。
    #[test]
    fn 内訳の帯はどの向きでも幅を持つ() {
        use crate::indeed::aggregate::growth_breakdown;
        fn 並び(a: f64, b: f64) -> Vec<Option<f64>> {
            vec![Some(a), Some(a), Some(a), Some(b), Some(b), Some(b)]
        }
        // (求人, 会社) の組み合わせ。逆を向くものを必ず含める
        let 組 = [
            (104.5, 95.8),  // フォークリフト: 会社が減って 1 社あたりが押し上げた
            (111.5, 111.9), // 調理: 会社が増えて 1 社あたりが下がった
            (138.3, 102.6), // 一般事務: ほぼ 1 社あたり
            (119.9, 119.1), // 営業: ほぼ会社
            (92.0, 95.0),   // 両方とも減った
            (90.0, 104.0),  // 求人が減ったのに会社は増えた
        ];
        for (j, e) in 組 {
            let b = growth_breakdown(&並び(100.0, j), &並び(100.0, e))
                .unwrap_or_else(|| panic!("求人 {j} / 会社 {e} が分解できない"));
            let h = breakdown_html(Some(b));
            // style="width:○○%" を全部拾って合計する
            let mut 幅: Vec<f64> = Vec::new();
            for part in h.split("width:").skip(1) {
                let num: String = part.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                if let Ok(v) = num.parse::<f64>() {
                    幅.push(v);
                }
            }
            assert_eq!(幅.len(), 2, "帯が 2 本になっていない（求人 {j} / 会社 {e}）");
            let 合計: f64 = 幅.iter().sum();
            assert!(
                (合計 - 100.0).abs() < 0.5,
                "帯の合計が {合計:.1}% （求人 {j} / 会社 {e}）"
            );
            for w in &幅 {
                assert!(
                    *w > 0.0,
                    "幅 0 の帯がある。これが 2026-09-15 に報告された不具合そのもの（求人 {j} / 会社 {e}）"
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

    /// 棒の**内側**に置く文字は、棒の色に対して 4.5:1 以上（WCAG 1.4.3）。
    ///
    /// # なぜ地とは別に見るのか
    /// 上の検査が見ているのは「線と地」の 3:1 で、文字を載せる相手が違う。
    /// 軸と同じ #94a3b8 を棒の上に置くと、暗い画面の #38bdf8 に対して
    /// 1.4:1 しかなく、数字が読めない。載せる色は棒の明暗の逆を採る。
    #[test]
    fn 棒の内側の文字は棒の色に対して四対五以上ある() {
        for (dark, name) in [(true, "暗い画面"), (false, "紙")] {
            let pal = palette(dark);
            // `hbar_chart` が棒に使うのは 0 番（基準線より上）と 1 番（下）
            for i in [0usize, 1] {
                let r = contrast(on_bar_color(dark), pal[i]);
                assert!(
                    r >= 4.5,
                    "{name} の {i} 番の棒 {} に載せた {} は {r:.2}:1 しかない",
                    pal[i],
                    on_bar_color(dark)
                );
            }
        }
    }

    /// 目盛り線は「見えるが、データより目立たない」範囲に収める。
    ///
    /// # なぜ数字で止めるのか
    /// 以前は `opacity` だけで指定していた。線の色はテーマ任せだったので、
    /// カードの地が `#0e1628` から `#1e293b` に明るくなった瞬間に
    /// 明暗差が **1.06:1**（紙は 1.04:1）に落ち、線を引いていないのと同じになった。
    /// 地を変えた人がこの図を見るとは限らないので、色の側で止める。
    ///
    /// 上限も要る。WCAG 1.4.11 の 3:1 は「情報を伝えるのに要るもの」に対する下限で、
    /// 目盛り線は補助。3:1 まで上げると目盛り線がデータより目立つ。
    #[test]
    fn 目盛り線は見えるがデータより目立たない() {
        // 暗い画面は地が 2 種類ある（`grid_line_color` のコメント参照）
        for (dark, bg, name) in [
            (true, "#1e293b", "tab.rs のカード"),
            (true, "#0e1628", "title.rs のカード"),
            (false, "#ffffff", "紙"),
        ] {
            let c = grid_line_color(dark);
            let r = contrast(c, bg);
            assert!(r >= 1.5, "{name}（{bg}）の目盛り線 {c} は {r:.2}:1 しかなく、地に沈む");
            assert!(r < 3.0, "{name}（{bg}）の目盛り線 {c} は {r:.2}:1 あり、データより目立つ");
        }
        // いちばん多く見られるのは tab.rs の明るいカード。ここは狙いの 1.7〜2.0 に入れる
        let r = contrast(grid_line_color(true), "#1e293b");
        assert!(
            (1.7..=2.0).contains(&r),
            "明るいカードの目盛り線が {r:.2}:1 で、狙いの 1.7〜2.0 から外れた"
        );

        // 目盛り線が軸の文字や系列の色より目立たないこと。
        // 3 者の強さの順が入れ替わると、図の中で先に目に入るものが変わる
        for (dark, bg) in [(true, "#1e293b"), (true, "#0e1628"), (false, "#ffffff")] {
            let g = contrast(grid_line_color(dark), bg);
            assert!(
                g < contrast(axis_color(dark), bg),
                "{bg} で目盛り線が軸の文字より目立つ"
            );
            for c in palette(dark) {
                assert!(g < contrast(c, bg), "{bg} で目盛り線が系列の色 {c} より目立つ");
            }
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
