//! 解説資料 (Reader's Guide) — `?variant=guide` (2026-07-17 追加)
//!
//! レポート本体 (SP版等) に添える顧客向けの読み解きガイドを、同じセッション集計
//! (`SurveyAggregation`) + 公的統計 (`InsightContext`) から決定的に生成する。
//! 富田林商談 (2026-07) で手作業生成した解説資料 v4 の構成を標準化したもの。
//!
//! # 構成
//! - §1 貴社の現在地 (`?company=社名` 指定時のみ): CSV 内の該当企業求人を市場分布に重ねる
//! - §2 市場の実像: 各観測数字 → 「だから」 (データがある項目のみ描画、silent fallback しない)
//! - §3 次の一手: sp_report の結論エンジン (`build_conclusions`) を流用
//! - §4 よくある質問
//!
//! # 文言規律 (sp_report と同一)
//! 断定禁止・可能性表現で統一 (「〜可能性があります」「〜とみられます」)。
//! 因果断定はしない。公的統計には粒度注記 (県・産業計の参考値) を必ず付す。
//! `scripts/lint_statistical_claims.py` 0 件を維持する。
//!
//! # データソース制約
//! 集計 (`SurveyAggregation`) + 公的統計 (`InsightContext` の ext_*/commute_*) のみ。
//! 自前 HW DB 由来フィールド (hw_industry_counts / salary_scatter_pairs 等) は
//! 一切参照しない (顧客向け文書のため。テストで番兵保証)。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::guide):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number, get_f64};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::sp_report::build_conclusions;

// ============================================================
// 小さな数値ヘルパー (このファイル内のみ)
// ============================================================

/// 中央値 (ソートコピー方式)。空なら None。
fn median_of(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut v: Vec<i64> = values.to_vec();
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// 値が分布の下位から何 % の位置か (0-100)。分布が空なら None。
fn percentile_from_below(values: &[i64], target: i64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let below = values.iter().filter(|v| **v <= target).count();
    Some(below as f64 / values.len() as f64 * 100.0)
}

/// 万円表示 (中間値・中央値系は小数 1 桁)。
fn man_yen(v: i64) -> String {
    format!("{:.1}万円", v as f64 / 10_000.0)
}

// ============================================================
// 本体
// ============================================================

/// 解説資料ページ全体の HTML を返す。
///
/// - `company`: `?company=社名` (部分一致で `agg.by_company` から検索)。
///   None / 空文字列なら §1 を描画しない。ヒット 0 件なら「見つからなかった」旨を
///   明示する (silent skip しない)。
pub(crate) fn render_survey_guide_page(
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
    pref: &str,
    muni: &str,
    company: Option<&str>,
) -> String {
    let region = if muni.is_empty() {
        pref.to_string()
    } else {
        format!("{} {}", pref, muni)
    };
    let mut html = String::with_capacity(32 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str(&format!(
        "<title>求人市場レポート 解説資料【{}】</title>\n",
        escape_html(&region)
    ));
    html.push_str(GUIDE_CSS);
    html.push_str("</head>\n<body>\n<div class=\"page\">\n");

    // ヘッダー
    html.push_str(&format!(
        "<header class=\"doc\">\
         <div class=\"eyebrow\">READER'S GUIDE</div>\
         <h1>求人市場 総合診断レポート 解説資料</h1>\
         <div class=\"lede\">対象: 求人市場 総合診断レポート【{}】。本資料は、レポートの実測値から\
         「何が言えるか」を整理した読み解きガイドです。記載はデータから言える範囲の傾向・可能性であり、\
         断定ではありません。</div></header>\n",
        escape_html(&region)
    ));

    // §1 貴社の現在地 (company 指定時のみ)
    if let Some(name) = company.filter(|s| !s.trim().is_empty()) {
        push_section_position(&mut html, agg, name.trim());
    }

    // §2 市場の実像
    html.push_str("<h2>市場の実像 — レポートの数字から言えること</h2>\n");
    push_block_market_size(&mut html, agg);
    push_block_salary(&mut html, agg);
    push_block_holidays(&mut html, agg);
    push_block_tags(&mut html, agg);
    push_block_popularity(&mut html, agg);
    push_block_tightness(&mut html, ctx);
    push_block_commute(&mut html, ctx);

    // §3 次の一手 (結論エンジン流用)
    push_section_next_steps(&mut html, agg, ctx);

    // §4 よくある質問
    push_section_faq(&mut html, agg);

    // フッター
    html.push_str(
        "<footer class=\"doc\">出典: 今回アップロードされた求人検索データ (重複整理後) / \
         国勢調査 (通勤 OD 含む) / e-Stat 各種統計 (詳細はレポート本体の「注記・出典・免責」)。<br>\
         本資料の数値はレポート本体と同じ集計から生成しています。記載はデータから言える範囲の\
         傾向・可能性であり、応募数・採用可否を保証するものではありません。</footer>\n",
    );
    html.push_str("</div>\n</body>\n</html>\n");
    html
}

// ============================================================
// §1 貴社の現在地
// ============================================================

fn push_section_position(html: &mut String, agg: &SurveyAggregation, company: &str) {
    html.push_str("<h2>貴社の現在地</h2>\n");

    // 部分一致 (双方向) で企業を検索。複数ヒット時は掲載件数最多を採用。
    let hit = agg
        .by_company
        .iter()
        .filter(|c| c.name.contains(company) || company.contains(c.name.as_str()))
        .max_by_key(|c| c.count);

    let Some(c) = hit else {
        html.push_str(&format!(
            "<p>収集データ内に「{}」に該当する企業名の求人は見つかりませんでした。\
             企業名の表記ゆれ (法人格の有無・カナ表記等) をご確認のうえ、\
             `company` パラメータを変えて再表示してください。</p>\n",
            escape_html(company)
        ));
        return;
    };

    let market_median = median_of(&agg.salary_values);
    let pos = percentile_from_below(&agg.salary_values, c.median_salary);

    html.push_str("<table><tr><th>観測 (収集データ内の貴社求人)</th><th>市場との重ね合わせ</th></tr>\n");
    html.push_str(&format!(
        "<tr><td>掲載 {} 件 (企業名: {})</td><td>同じ検索画面で比較される求人 {} 件の中での掲載数です</td></tr>\n",
        format_number(c.count as i64),
        escape_html(&c.name),
        format_number(agg.total_count as i64),
    ));
    if c.median_salary > 0 {
        let market_str = market_median
            .map(man_yen)
            .unwrap_or_else(|| "—".to_string());
        html.push_str(&format!(
            "<tr><td>提示給与の中央値 {}</td><td>市場の中央値は {}",
            man_yen(c.median_salary),
            market_str,
        ));
        if let Some(p) = pos {
            html.push_str(&format!(
                "。貴社は分布の下位からおよそ {:.0}% の位置にあります (下限と上限の中間値ベースの参考値)",
                p
            ));
        }
        html.push_str("</td></tr>\n");
    }
    html.push_str("</table>\n");

    if let (Some(p), true) = (pos, c.median_salary > 0) {
        let landing = if p >= 60.0 {
            "提示額は市場の上位側にあり、金額そのものよりも「何がその金額に含まれるか」の見せ方が比較材料になるとみられます。"
        } else if p >= 40.0 {
            "提示額は市場の中心帯にあり、金額だけでは差がつきにくい位置とみられます。休日・働き方など金額以外の定量項目の見せ方が比較材料になる可能性があります。"
        } else {
            "提示額は市場の中心より低い側にあります。金額を動かせない場合は、上限の見せ方や金額以外の訴求で補う構成が考えられます。"
        };
        html.push_str(&format!("<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n", landing));
    }
    html.push_str(
        "<p class=\"note\">※ 給与の位置づけは、貴社求人の給与欄をレポートと同じ基準 \
         (下限と上限の中間値・月給換算) で市場分布に重ねた参考値です。手当・賞与等の\
         条件は反映していません。</p>\n",
    );
}

// ============================================================
// §2 市場の実像 — 各ブロック (データがある項目のみ描画)
// ============================================================

fn push_block_market_size(html: &mut String, agg: &SurveyAggregation) {
    if agg.total_count == 0 {
        return;
    }
    let new_pct = agg.new_count as f64 / agg.total_count as f64 * 100.0;
    // 勤務地上位 3 市区町村 (件数降順)
    let mut munis: Vec<(&str, usize)> = agg
        .by_municipality_salary
        .iter()
        .map(|m| (m.name.as_str(), m.count))
        .collect();
    munis.sort_by(|a, b| b.1.cmp(&a.1));
    let top3 = munis
        .iter()
        .take(3)
        .map(|(n, c)| format!("{} {}件", escape_html(n), format_number(*c as i64)))
        .collect::<Vec<_>>()
        .join(" / ");

    html.push_str("<h3>土俵の広さと動き</h3>\n");
    html.push_str(&format!(
        "<p>重複整理後 {} 件。掲載から間もない求人の割合 (目安) は {:.0}%。勤務地の上位: {}。</p>\n",
        format_number(agg.total_count as i64),
        new_pct,
        if top3.is_empty() { "—".to_string() } else { top3 },
    ));
    html.push_str(&format!(
        "<div class=\"dakara\">→ <strong>だから:</strong> この件数が、検索地で仕事を探す人に\
         実際に見えている比較対象の規模です。勤務地は検索地の市内に限らず周辺に広がっており、\
         貴社の求人は<strong>通勤圏の求人と同じ画面で比較されている</strong>可能性が高い、\
         というのがこのレポートの前提です。新着の割合が高い ({:.0}%) ほど入れ替わりが多い市場\
         (活発とも、消化されず再投入が多いとも読めます) とみられます。</div>\n",
        new_pct,
    ));
}

fn push_block_salary(html: &mut String, agg: &SurveyAggregation) {
    let lo = median_of(&agg.salary_min_values);
    let hi = median_of(&agg.salary_max_values);
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return;
    };
    // 幅あり比率: 分子 = scatter_min_max の (下限 < 上限)、分母 = 下限給与が取れた求人数。
    // 2026-07-17 逆証明で修正: scatter_min_max は上下限が両方ある求人しか含まないため、
    // これを分母にすると常に 100% 近くなる (富田林データで実害確認、実際は 77%)。
    let range_info = if !agg.salary_min_values.is_empty() {
        let with_range = agg.scatter_min_max.iter().filter(|p| p.y > p.x).count();
        let pct = with_range as f64 / agg.salary_min_values.len() as f64 * 100.0;
        Some(pct.min(100.0))
    } else {
        None
    };

    html.push_str("<h3>給与 — 下限と上限のどちらで差がつくか</h3>\n");
    html.push_str(&format!(
        "<p>下限給与の中央値 {} / 上限給与の中央値 {}。",
        man_yen(lo),
        man_yen(hi)
    ));
    if let Some(r) = range_info {
        html.push_str(&format!(
            "給与欄に幅 (下限〜上限) を示す求人はおよそ {:.0}% (下限給与が取れた求人比)。",
            r
        ));
    }
    html.push_str("</p>\n");

    let spread_man = (hi - lo) as f64 / 10_000.0;
    let landing = if spread_man >= 3.0 {
        format!(
            "下限は中央値付近に集まりやすく、差が出ているのは上限側 (中央値ベースで約 {:.0}万円の開き) \
             とみられます。昇給・資格取得後の到達額を給与欄の上限として見せることが、\
             この市場の標準的な見せ方に並ぶ最小工数の一手になる可能性があります。",
            spread_man
        )
    } else {
        "下限と上限の開きが小さく、金額の幅では差がつきにくい市場とみられます。\
         休日・働き方など金額以外の定量項目の見せ方が比較材料になる可能性があります。"
            .to_string()
    };
    html.push_str(&format!("<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n", landing));
}

fn push_block_holidays(html: &mut String, agg: &SurveyAggregation) {
    let jb = &agg.jobbox;
    // 抽出数が少ないと比率が不安定なため 20 件未満は描画しない (明示ゲート)
    if jb.annual_holidays_values.len() < 20 {
        return;
    }
    let n = jb.annual_holidays_values.len();
    let med = median_of(&jb.annual_holidays_values).unwrap_or(0);
    let ge120 = jb.holiday_pct_ge_120 * 100.0;
    let ge125 = jb.holiday_pct_ge_125 * 100.0;

    html.push_str("<h3>年間休日 — どの日数から差別化になるか</h3>\n");
    html.push_str(&format!(
        "<p>年間休日の記載を抽出できた {} 件のうち、120日以上は {:.0}%、125日以上は {:.0}% \
         (中央値 {} 日)。記載がない求人は含まれない点にご注意ください。</p>\n",
        format_number(n as i64),
        ge120,
        ge125,
        med
    ));
    let landing = if ge120 >= 50.0 {
        format!(
            "「年間休日120日以上」という定型句は市場の過半 ({:.0}%) と同じ表現で、目立ちにくいと\
             みられます。125日以上は {:.0}% にとどまるため、これを上回る条件をお持ちであれば\
             具体的な日数で書くことに意味がある市場です。",
            ge120, ge125
        )
    } else {
        format!(
            "120日以上を明示できている求人は {:.0}% にとどまります。休日条件が市場水準を上回る場合、\
             本文・条件欄の見やすい位置で日数を明示するだけで比較の土俵で目立てる可能性があります。",
            ge120
        )
    };
    html.push_str(&format!("<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n", landing));

    if let Some(r) = jb.salary_holidays_correlation {
        if r.abs() < 0.3 {
            html.push_str(&format!(
                "<p class=\"note\">※ 休日と給与の相関はほぼありません (r={:.2})。\
                 「休日が多い求人は給与が低い」という関係はこの市場では確認できず、\
                 休日と給与の同時訴求はデータ上矛盾しません。</p>\n",
                r
            ));
        }
    }
}

fn push_block_tags(html: &mut String, agg: &SurveyAggregation) {
    // 件数 10 件以上・プラス側のタグ上位 3 件
    let mut tags: Vec<_> = agg
        .by_tag_salary
        .iter()
        .filter(|t| t.count >= 10 && t.diff_percent > 0.0)
        .collect();
    tags.sort_by(|a, b| b.diff_percent.partial_cmp(&a.diff_percent).unwrap_or(std::cmp::Ordering::Equal));
    if tags.is_empty() {
        return;
    }
    let list = tags
        .iter()
        .take(3)
        .map(|t| {
            format!(
                "「{}」({} 件、市場平均比 +{:.0}%)",
                escape_html(&t.tag),
                format_number(t.count as i64),
                t.diff_percent
            )
        })
        .collect::<Vec<_>>()
        .join("、");

    html.push_str("<h3>訴求ワード — 何を書いた求人が高給与側にいるか</h3>\n");
    html.push_str(&format!("<p>給与が市場平均より高い側に分布するタグ: {}。</p>\n", list));
    html.push_str(
        "<div class=\"dakara\">→ <strong>だから:</strong> これらの語を掲げる求人ほど高給与側に\
         分布しています (相関であり、書けば給与が上がるという因果ではありません)。給与とセットで\
         「何ができるようになるか」を語るのが、この市場の高給与側の書き方の傾向とみられます。</div>\n",
    );
}

fn push_block_popularity(html: &mut String, agg: &SurveyAggregation) {
    let p = &agg.popularity;
    if p.indeed_sp_total == 0 || (p.popular_count + p.super_popular_count) == 0 {
        return;
    }
    html.push_str("<h3>人気表示 — 選ばれているカードの傾向</h3>\n");
    html.push_str(&format!(
        "<p>Indeed (SP) の表示で「人気」「超人気」が付いた求人は {} 件 (対象 {} 件中 {:.0}%)。</p>\n",
        format_number((p.popular_count + p.super_popular_count) as i64),
        format_number(p.indeed_sp_total as i64),
        p.popular_ratio * 100.0,
    ));
    if let (Some(pm), Some(nm)) = (p.popular_salary_median, p.non_popular_salary_median) {
        if p.popular_n_salary >= 5 && p.non_popular_n_salary >= 5 {
            // 2026-07-17 逆証明で分岐追加: 差が僅少 (1万円未満) のとき
            // 「物差しに使える」と書くと空回りする。差の有無で文言を切り替える。
            let landing = if (pm - nm).abs() < 10_000 {
                format!(
                    "人気表示の有無で月給中央値に明確な差は見られません ({} vs {})。この市場では\
                     人気が給与以外の要素 (仕事内容の伝え方・写真・条件の見せ方など) で決まっている\
                     可能性があり、給与を上げる前に見せ方を検証する余地があるとみられます。",
                    man_yen(pm),
                    man_yen(nm),
                )
            } else {
                format!(
                    "人気表示つき求人の月給中央値は {}、なしは {}。人気表示は応募・閲覧の実績を\
                     反映するとみられ、選ばれているカードの給与・条件の水準を測る物差しとして\
                     使えます (人気表示の基準は媒体側の非公開ロジックです)。",
                    man_yen(pm),
                    man_yen(nm),
                )
            };
            html.push_str(&format!(
                "<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n",
                landing
            ));
        }
    }
}

fn push_block_tightness(html: &mut String, ctx: Option<&InsightContext>) {
    let Some(c) = ctx else { return };
    let ratio = c
        .ext_job_ratio
        .last()
        .map(|r| get_f64(r, "ratio_total"))
        .filter(|v| v.is_finite() && *v > 0.0);
    let Some(r) = ratio else { return };

    html.push_str("<h3>需給 — 市場の混み具合</h3>\n");
    html.push_str(&format!(
        "<p>直近の有効求人倍率は {:.2} 倍 (県・産業計の参考値であり、対象職種の実勢とは差がある\
         可能性があります)。</p>\n",
        r
    ));
    let landing = if r >= 1.5 {
        "1 人の求職者を複数の求人が取り合う水準で、採用の競争は激しめとみられます。\
         条件の見せ方と応募後の初動 (連絡の速さ) が結果を左右しやすい環境です。"
    } else if r >= 1.0 {
        "採用側と求職側がおおむね拮抗している水準です。「市場が厳しいから採れない」とは\
         言い切れない数字であり、露出と見せ方の改善が結果に反映されやすい環境と考えられます。"
    } else {
        "求職側が相対的に多い水準で、採用は比較的進めやすいとみられます。\
         応募の質を上げる絞り込み型の訴求も選択肢になります。"
    };
    html.push_str(&format!("<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n", landing));
}

fn push_block_commute(html: &mut String, ctx: Option<&InsightContext>) {
    let Some(c) = ctx else { return };
    if c.commute_inflow_total == 0 && c.commute_outflow_total == 0 {
        return;
    }
    html.push_str("<h3>通勤の構造 — 働き手はどこから来て、どこへ行くか</h3>\n");
    html.push_str(&format!(
        "<p>市外へ通勤する人 {} 人 / 市外から来る人 {} 人 / 市内で完結する通勤 {:.1}% (国勢調査 OD)。</p>\n",
        format_number(c.commute_outflow_total),
        format_number(c.commute_inflow_total),
        c.commute_self_rate * 100.0,
    ));
    let landing = if c.commute_outflow_total > c.commute_inflow_total {
        "働き手が市外へ出ていく構造です。市外へ通っている層にとって「地元で同水準の条件で働ける」\
         ことは通勤時間の短縮という具体的な便益になり得ます。"
    } else {
        "市外から働き手が入ってくる構造です。周辺自治体への露出 (配信対象地域の設定) が\
         母集団の確保につながる可能性があります。"
    };
    html.push_str(&format!("<div class=\"dakara\">→ <strong>だから:</strong> {}", landing));
    if !c.commute_inflow_top3.is_empty() {
        let top = c
            .commute_inflow_top3
            .iter()
            .take(3)
            .map(|(_, m, cnt)| format!("{} {}人", escape_html(m), format_number(*cnt)))
            .collect::<Vec<_>>()
            .join(" / ");
        html.push_str(&format!(
            " 実際の流入元の上位は {} で、配信を広げる際の最初の候補になります。",
            top
        ));
    }
    html.push_str("</div>\n");
}

// ============================================================
// §3 次の一手 (sp_report の結論エンジンを流用)
// ============================================================

fn push_section_next_steps(html: &mut String, agg: &SurveyAggregation, ctx: Option<&InsightContext>) {
    let conclusions = build_conclusions(agg, ctx);
    if conclusions.is_empty() {
        return;
    }
    html.push_str("<h2>まとめ — このレポートから導ける判断材料</h2>\n<ol>\n");
    for c in &conclusions {
        html.push_str(&format!("<li>{}", c.sentence));
        if let Some(o) = &c.outlook {
            html.push_str(&format!("<br><span class=\"note\">{}</span>", o));
        }
        html.push_str("</li>\n");
    }
    html.push_str("</ol>\n");
    html.push_str(
        "<p class=\"note\">いずれも応募数・採用を保証するものではなく、市場データから見た\
         判断材料の提示です。仕事内容固有の条件 (出張の有無・試用期間の設計等) は市場データでは\
         測れないため、応募実績と面談で検証する領域です。</p>\n",
    );
}

// ============================================================
// §4 よくある質問
// ============================================================

fn push_section_faq(html: &mut String, agg: &SurveyAggregation) {
    // 勤務地最多の市区町村 (説明用)
    let top_muni = agg
        .by_municipality_salary
        .iter()
        .max_by_key(|m| m.count)
        .map(|m| (m.name.clone(), m.count));

    html.push_str("<h2>よくある質問</h2>\n<table>\n<tr><th>質問</th><th>回答</th></tr>\n");
    let mut q1 = format!(
        "いいえ。検索地で表示される市場全体 (周辺市を含む) の件数です。求人サイトは検索地の\
         周辺求人もあわせて表示するため、「検索地で仕事を探す人に見えている市場」を写しています。"
    );
    if let Some((name, cnt)) = top_muni {
        q1.push_str(&format!(
            " 今回の勤務地の最多は {} ({} 件) でした。",
            escape_html(&name),
            format_number(cnt as i64)
        ));
    }
    html.push_str(&format!(
        "<tr><td>「{} 件」は検索地の市内だけの求人数?</td><td>{}</td></tr>\n",
        format_number(agg.total_count as i64),
        q1
    ));
    html.push_str(
        "<tr><td>中央値と平均値はどちらを見ればよい?</td>\
         <td>まず中央値をおすすめします。平均値は一部の高額求人に引っ張られて高く出ることが\
         あります。両者が大きく離れている場合は分布に偏りがあるサインです。</td></tr>\n",
    );
    html.push_str(
        "<tr><td>年間休日などの集計はすべての求人が対象?</td>\
         <td>いいえ、求人票に記載があった分だけの集計です。記載がないことは\
         「条件がない」ことを意味しません。</td></tr>\n",
    );
    html.push_str(
        "<tr><td>求人倍率などの公的統計はこの職種の値?</td>\
         <td>県単位・産業計の参考値です。対象職種の実勢とは差がある可能性があるため、\
         「市場の背景」としてお読みください。</td></tr>\n",
    );
    html.push_str("</table>\n");
}

// ============================================================
// CSS (self-contained、印刷対応)
// ============================================================

pub(super) const GUIDE_CSS: &str = r#"<style>
  :root { --ink:#1e2a4a; --muted:#5a6478; --rule:#d8dce6; --accent:#8a6d3b; --tint:#f7f5f0; }
  * { box-sizing:border-box; margin:0; padding:0; }
  body { font-family:"Noto Sans JP","Hiragino Sans","Yu Gothic",sans-serif; color:var(--ink); line-height:1.85; background:#fff; }
  .page { max-width:840px; margin:0 auto; padding:40px 48px; }
  header.doc { border-bottom:2px solid var(--ink); padding-bottom:14px; margin-bottom:22px; }
  .eyebrow { font-size:11px; letter-spacing:.18em; color:var(--accent); font-weight:700; }
  h1 { font-size:23px; font-weight:900; margin-top:4px; }
  .lede { color:var(--muted); font-size:13px; margin-top:6px; }
  h2 { font-size:16px; font-weight:800; border-left:5px solid var(--ink); padding-left:10px; margin:30px 0 10px; }
  h3 { font-size:13.5px; font-weight:700; margin:18px 0 6px; }
  p, li { font-size:13px; }
  ul, ol { padding-left:1.4em; }
  table { width:100%; border-collapse:collapse; margin:10px 0 14px; font-size:12.5px; }
  th { background:var(--tint); text-align:left; padding:7px 9px; border-bottom:1.5px solid var(--ink); font-size:12px; }
  td { padding:7px 9px; border-bottom:1px solid var(--rule); vertical-align:top; }
  .dakara { background:#eef1f7; border-left:4px solid var(--ink); padding:8px 14px; margin:8px 0 16px; font-size:13px; }
  .note { color:var(--muted); font-size:11.5px; }
  footer.doc { border-top:1px solid var(--rule); margin-top:34px; padding-top:12px; color:var(--muted); font-size:11px; }
  @media print { .page { padding:10mm 12mm; max-width:none; } h2,h3 { break-after:avoid; } table, .dakara { break-inside:avoid; } }
</style>
"#;

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::super::super::super::aggregator::{CompanyAgg, TagSalaryAgg};
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    /// 解説資料は顧客向け文書のため、断定語・過剰表現を一切含まないこと。
    const BANNED: &[&str] = &[
        "問題ない",
        "問題ありません",
        "完璧",
        "絶対に",
        "劇的",
        "必ず採用",
        "間違いなく",
        "100%",
        "(仮)",
    ];

    fn rich_agg() -> SurveyAggregation {
        let mut agg = SurveyAggregation {
            total_count: 600,
            new_count: 120,
            salary_values: (0..100).map(|i| 250_000 + i * 1_000).collect(),
            salary_min_values: (0..100).map(|i| 220_000 + i * 800).collect(),
            salary_max_values: (0..100).map(|i| 300_000 + i * 1_200).collect(),
            ..Default::default()
        };
        agg.by_company.push(CompanyAgg {
            name: "テスト工業株式会社".to_string(),
            count: 2,
            avg_salary: 260_000,
            median_salary: 255_000,
        });
        agg.by_tag_salary.push(TagSalaryAgg {
            tag: "経験者歓迎".to_string(),
            count: 50,
            avg_salary: 330_000,
            diff_from_avg: 30_000,
            diff_percent: 10.5,
        });
        agg.jobbox.annual_holidays_values = (0..40).map(|i| 105 + (i % 30)).collect();
        agg.jobbox.holiday_pct_ge_120 = 0.53;
        agg.jobbox.holiday_pct_ge_125 = 0.18;
        agg.jobbox.salary_holidays_correlation = Some(0.08);
        agg
    }

    fn rich_ctx() -> InsightContext {
        let mut row: HashMap<String, serde_json::Value> = HashMap::new();
        row.insert("ratio_total".to_string(), json!(1.21));
        InsightContext {
            ext_job_ratio: vec![row],
            commute_inflow_total: 19_545,
            commute_outflow_total: 36_956,
            commute_self_rate: 0.315,
            commute_inflow_top3: vec![
                ("大阪府".to_string(), "河内長野市".to_string(), 3_207),
                ("大阪府".to_string(), "堺市".to_string(), 3_117),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn guide_without_company_has_no_position_section() {
        let html = render_survey_guide_page(&rich_agg(), None, "大阪府", "富田林市", None);
        assert!(!html.contains("貴社の現在地"), "company 未指定で §1 が出ている");
        assert!(html.contains("市場の実像"), "§2 は常に出る");
    }

    #[test]
    fn guide_with_company_hit_renders_position() {
        let html = render_survey_guide_page(
            &rich_agg(),
            None,
            "大阪府",
            "富田林市",
            Some("テスト工業"),
        );
        assert!(html.contains("貴社の現在地"));
        assert!(html.contains("テスト工業株式会社"));
        assert!(html.contains("下位からおよそ"), "分布内の位置づけが出る");
    }

    #[test]
    fn guide_with_company_miss_renders_explicit_not_found() {
        let html = render_survey_guide_page(
            &rich_agg(),
            None,
            "大阪府",
            "富田林市",
            Some("存在しない会社XYZQ"),
        );
        assert!(
            html.contains("見つかりませんでした"),
            "ヒット 0 件は silent skip せず明示する"
        );
    }

    #[test]
    fn guide_banned_words_absent() {
        // company ヒットあり + ctx ありの最大描画状態で禁止語ゼロを保証。
        // CSS 内の `width:100%` 等を誤検出しないよう、検査対象は </style> 以降の本文のみ。
        let html = render_survey_guide_page(
            &rich_agg(),
            Some(&rich_ctx()),
            "大阪府",
            "富田林市",
            Some("テスト工業"),
        );
        let body = html
            .split("</style>")
            .nth(1)
            .expect("解説資料には <style> ブロックが 1 つある前提");
        for w in BANNED {
            assert!(!body.contains(w), "禁止語 '{}' が解説資料に混入", w);
        }
    }

    #[test]
    fn guide_tightness_has_granularity_note() {
        let html = render_survey_guide_page(&rich_agg(), Some(&rich_ctx()), "大阪府", "富田林市", None);
        assert!(html.contains("1.21"), "求人倍率の値が出る");
        assert!(
            html.contains("県・産業計の参考値"),
            "公的統計の粒度注記が必須"
        );
    }

    #[test]
    fn guide_commute_direction_logic() {
        // 流出超過 → 「働き手が市外へ出ていく構造」+ 流入元の配信候補
        let html = render_survey_guide_page(&rich_agg(), Some(&rich_ctx()), "大阪府", "富田林市", None);
        assert!(html.contains("市外へ出ていく構造"));
        assert!(html.contains("河内長野市"));
    }

    #[test]
    fn guide_does_not_leak_hw_fields() {
        // 自前 HW DB 由来フィールドに番兵を仕込んでも解説資料には一切出ない
        let mut ctx = rich_ctx();
        ctx.hw_industry_counts = vec![("HW番兵産業GUIDEZZQ".to_string(), 999_999)];
        ctx.hw_job_type_counts = vec![("HW番兵職種GUIDEZZQ".to_string(), 999_999)];
        ctx.salary_scatter_pairs = vec![(123_457.0, 234_561.0)];
        let html = render_survey_guide_page(&rich_agg(), Some(&ctx), "大阪府", "富田林市", None);
        assert!(!html.contains("HW番兵産業GUIDEZZQ"));
        assert!(!html.contains("HW番兵職種GUIDEZZQ"));
        assert!(!html.contains("対象地域から最大 1000 件抽出"));
    }

    #[test]
    fn guide_zero_rows_no_panic() {
        let agg = SurveyAggregation::default();
        let html = render_survey_guide_page(&agg, None, "大阪府", "富田林市", Some("どこか"));
        assert!(html.contains("解説資料"), "0 件でも文書自体は生成される");
        assert!(!html.contains("NaN"), "0 件で NaN 混入なし");
    }

    #[test]
    fn median_and_percentile_helpers() {
        assert_eq!(median_of(&[]), None);
        assert_eq!(median_of(&[3, 1, 2]), Some(2));
        assert_eq!(percentile_from_below(&[], 5), None);
        let p = percentile_from_below(&[1, 2, 3, 4], 2).unwrap();
        assert!((p - 50.0).abs() < f64::EPSILON);
    }
}
