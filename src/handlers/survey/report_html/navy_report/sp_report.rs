//! SP版 (仮) 専用ブロック (2026-07-11 追加、試作)
//!
//! レビューで挙がった改善を「SP版 (仮)」variant のみに全部入れした試作モジュール。
//! 既存 variant (Full / Public / MarketIntelligence / Extended) の出力には一切
//! 影響しない (呼出側 mod.rs が `variant == Sp` のときだけ本モジュールの関数を呼ぶ)。
//!
//! # 構成
//! - `render_sp_exec_onepager`  : 持ち歩ける経営サマリー1ページ (表紙・目次の直後)
//!     - 「結論の1文」3〜5箇条 (各箇条は完全な文 + 対応セクション番号 + SEVERITY チップ)
//!     - 「まず取り組む3つ」
//! - `render_sp_conclusion_band`: 各セクション冒頭の「このページの結論」1文バンド
//! - `render_sp_salary_quartiles`: §03 の給与 25/50/75 パーセンタイル四分位表示
//! - `render_sp_priority_actions`: §09/§10 の So What を集約した優先アクション表
//!     - 各行に「すぐ効く / 仕込みが要る」2 分類 + 担当/期限/確認指標 の記入欄 (contenteditable)
//!
//! # 文言規律
//! すべて普通の言葉・断定禁止。可能性表現 (「〜可能性があります」「〜とみられます」等) で
//! 統一し、`scripts/lint_statistical_claims.py` 0 件を維持する。因果断定はしない。
//!
//! # データソース
//! 集計 (`SurveyAggregation`) + 公的統計クロス集計 (`InsightContext` の ext_*/cross_*) のみ。
//! 介護データ・HW 求人の生データには依存しない (Extended と同じ入力範囲)。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::sp_report):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number, get_f64};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::common::{
    compute_distribution_stats, format_mm, push_page_head, safe_pct, severity_label,
};

// ============================================================
// 内部: 結論の1文 (Conclusion) データ構築
// ============================================================

/// 経営サマリー・結論バンド共通の「結論の1文」。
///
/// - `topic`: 結論の主題 (アクション組み立て時に section+sev だけでは区別できない
///   複数の §03 結論 (給与 / 新着) を弁別するための discriminant)。
/// - `sev`: SEVERITY タグ (pos / warn / neg / neu)。既存 `.tag-*` チップで色分け。
/// - `section`: 対応セクション番号 (例: "03")。
/// - `sentence`: トピック名でなく判定を含む完全な文 (可能性表現)。
/// - `outlook`: WARN/NEG のみ設定される「このままの場合に想定されること」1 行 (可能性表現)。
struct Conclusion {
    topic: ConclusionTopic,
    sev: &'static str,
    section: &'static str,
    sentence: String,
    outlook: Option<String>,
}

/// 結論の主題 (アクション集約時の弁別に使用)。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConclusionTopic {
    Sample,
    Salary,
    NewRatio,
    Tightness,
    Switcher,
    Region,
    Other,
}

/// 月給/時給に応じた金額表示 (万円 or 円/時)。
fn fmt_money(yen: i64, is_hourly: bool) -> String {
    if is_hourly {
        format!("{}円/時", format_number(yen))
    } else {
        format!("{}万円", format_mm(yen))
    }
}

/// 給与分布から「相場の中での位置づけ」の結論を組み立てる (§03)。
///
/// 賃金センサス等の外部相場は本 fixture 経路では未接続のため、ここでは
/// **サンプル内の四分位** (P25/P50/P75) の広がりから「レンジの広さ」を可能性表現で述べる。
fn conclusion_salary(agg: &SurveyAggregation) -> Conclusion {
    let is_hourly = agg.is_hourly;
    if let Some(s) =
        compute_distribution_stats(&agg.salary_values, if is_hourly { 50 } else { 10_000 })
    {
        // レンジ幅 (P75 - P25) を中央値で正規化して「広い/狭い」を判定。
        let spread = if s.median > 0 {
            (s.p75 - s.p25) as f64 / s.median as f64
        } else {
            0.0
        };
        if spread >= 0.35 {
            Conclusion {
                topic: ConclusionTopic::Salary,
                sev: "warn",
                section: "03",
                sentence: format!(
                    "給与は中央値 {} を中心に P25 {} 〜 P75 {} と幅が広めで、\
                     求人ごとの条件差が大きい可能性があります。",
                    fmt_money(s.median, is_hourly),
                    fmt_money(s.p25, is_hourly),
                    fmt_money(s.p75, is_hourly),
                ),
                outlook: Some(
                    "このままレンジ提示の幅が広いままだと、求職者が自分の該当水準を判断しにくく、\
                     応募前の離脱につながる可能性があります。"
                        .to_string(),
                ),
            }
        } else {
            Conclusion {
                topic: ConclusionTopic::Salary,
                sev: "pos",
                section: "03",
                sentence: format!(
                    "給与は中央値 {} を中心に P25 {} 〜 P75 {} とまとまっており、\
                     条件面を提示しやすい状態とみられます。",
                    fmt_money(s.median, is_hourly),
                    fmt_money(s.p25, is_hourly),
                    fmt_money(s.p75, is_hourly),
                ),
                outlook: None,
            }
        }
    } else {
        Conclusion {
            topic: ConclusionTopic::Salary,
            sev: "neu",
            section: "03",
            sentence: "給与の有効値が不足しており、相場の中での位置づけは判断を保留します。"
                .to_string(),
            outlook: None,
        }
    }
}

/// サンプル件数の信頼性から結論を組み立てる (§01/全体)。
fn conclusion_sample(agg: &SurveyAggregation) -> Conclusion {
    let total = agg.total_count;
    if total == 0 {
        Conclusion {
            topic: ConclusionTopic::Sample,
            sev: "neg",
            section: "01",
            sentence: "サンプルが 0 件のため、統計的な判断はできない状態です。取得範囲の見直しが必要とみられます。"
                .to_string(),
            outlook: Some(
                "このままサンプルが集まらない場合、後続の分析はいずれも参考にとどまる可能性があります。"
                    .to_string(),
            ),
        }
    } else if total < 30 {
        Conclusion {
            topic: ConclusionTopic::Sample,
            sev: "warn",
            section: "01",
            sentence: format!(
                "サンプルは n={} と少なめで、数値は傾向の参考にとどめるのが安全とみられます。",
                total
            ),
            outlook: Some(
                "このまま少数のまま判断を進めると、外れ値に引きずられた結論になる可能性があります。"
                    .to_string(),
            ),
        }
    } else {
        Conclusion {
            topic: ConclusionTopic::Sample,
            sev: "pos",
            section: "01",
            sentence: format!(
                "サンプルは n={} と実務判断に足る水準で、後続セクションの数値はそのまま参照できるとみられます。",
                total
            ),
            outlook: None,
        }
    }
}

/// 新着比率から採用活動の見え方の結論を組み立てる (§03 求人動向)。
fn conclusion_new_ratio(agg: &SurveyAggregation) -> Conclusion {
    let total = agg.total_count;
    let new_pct = if total > 0 {
        safe_pct(agg.new_count as f64 / total as f64 * 100.0).round() as i64
    } else {
        0
    };
    if total == 0 {
        Conclusion {
            topic: ConclusionTopic::NewRatio,
            sev: "neu",
            section: "03",
            sentence: "新着比率はサンプル不足のため評価を保留します。".to_string(),
            outlook: None,
        }
    } else if new_pct >= 15 {
        Conclusion {
            topic: ConclusionTopic::NewRatio,
            sev: "pos",
            section: "03",
            sentence: format!(
                "直近30日の新着比率は {}% と高めで、求人の更新・追加が相対的に活発とみられます。",
                new_pct
            ),
            outlook: None,
        }
    } else if new_pct < 5 {
        Conclusion {
            topic: ConclusionTopic::NewRatio,
            sev: "warn",
            section: "03",
            sentence: format!(
                "新着比率は {}% と低めで、掲載が固定化している可能性があります。",
                new_pct
            ),
            outlook: Some(
                "このまま新着が少ない状態が続くと、市場で情報が更新されず露出が伸びにくくなる可能性があります。"
                    .to_string(),
            ),
        }
    } else {
        Conclusion {
            topic: ConclusionTopic::NewRatio,
            sev: "neu",
            section: "03",
            sentence: format!(
                "新着比率は {}% で、おおむね平均的な水準とみられます。",
                new_pct
            ),
            outlook: None,
        }
    }
}

/// 採用市場の逼迫度 (有効求人倍率) から結論を組み立てる (§04)。
fn conclusion_tightness(ctx: Option<&InsightContext>) -> Conclusion {
    let ratio = ctx
        .and_then(|c| c.ext_job_ratio.last())
        .map(|r| get_f64(r, "ratio_total"))
        .filter(|v| v.is_finite() && *v > 0.0);
    match ratio {
        Some(r) if r >= 1.5 => Conclusion {
            topic: ConclusionTopic::Tightness,
            sev: "warn",
            section: "04",
            sentence: format!(
                "直近の有効求人倍率は {:.2} 倍と高く、採用の競争は激しめとみられます。",
                r
            ),
            outlook: Some(
                "このまま需給が逼迫したままだと、条件を据え置いた求人は埋まりにくくなる可能性があります。"
                    .to_string(),
            ),
        },
        Some(r) if r >= 1.0 => Conclusion {
            topic: ConclusionTopic::Tightness,
            sev: "neu",
            section: "04",
            sentence: format!(
                "直近の有効求人倍率は {:.2} 倍で、採用側と求職側がおおむね拮抗しているとみられます。",
                r
            ),
            outlook: None,
        },
        Some(r) => Conclusion {
            topic: ConclusionTopic::Tightness,
            sev: "pos",
            section: "04",
            sentence: format!(
                "直近の有効求人倍率は {:.2} 倍と落ち着いており、採用は比較的進めやすいとみられます。",
                r
            ),
            outlook: None,
        },
        None => Conclusion {
            topic: ConclusionTopic::Tightness,
            sev: "neu",
            section: "04",
            sentence: "採用市場の逼迫度は参照データが不足しているため判断を保留します。".to_string(),
            outlook: None,
        },
    }
}

/// 転職を考えている人の規模から結論を組み立てる (§10 追加図)。
fn conclusion_switcher(ctx: Option<&InsightContext>) -> Conclusion {
    // region_name が対象県 (region_code 末尾が "000" でない、全国=00000 以外) の行を優先。
    let rate = ctx.and_then(|c| {
        c.cross_switcher_supply
            .iter()
            .find(|r| super::super::super::super::helpers::get_str(r, "region_code") != "00000")
            .or_else(|| c.cross_switcher_supply.first())
            .map(|r| get_f64(r, "job_change_desire_rate"))
            .filter(|v| v.is_finite() && *v > 0.0)
    });
    match rate {
        Some(r) => Conclusion {
            topic: ConclusionTopic::Switcher,
            sev: "pos",
            section: "10",
            sentence: format!(
                "転職を考えている人は対象地域で概ね {:.1}% とみられ、潜在的な採用対象は一定数存在する可能性があります。",
                r
            ),
            outlook: None,
        },
        None => Conclusion {
            topic: ConclusionTopic::Switcher,
            sev: "neu",
            section: "10",
            sentence: "転職意向の規模は参照データが不足しているため判断を保留します。".to_string(),
            outlook: None,
        },
    }
}

/// 経営サマリー / 各所で共有する結論一覧 (掲載順)。
///
/// 3〜5 箇条に収まるよう、サンプル → 給与 → 新着 → 逼迫度 → 転職意向 の順で
/// 判定を含む完全な文を並べる。
fn build_conclusions(agg: &SurveyAggregation, ctx: Option<&InsightContext>) -> Vec<Conclusion> {
    vec![
        conclusion_sample(agg),
        conclusion_salary(agg),
        conclusion_new_ratio(agg),
        conclusion_tightness(ctx),
        conclusion_switcher(ctx),
    ]
}

// ============================================================
// (a) 持ち歩ける経営サマリー1ページ
// ============================================================

/// SP版 (仮) 専用: 表紙・目次の直後に置く 1 ページの経営サマリー。
///
/// - 「結論の1文」を SEVERITY チップ + 対応セクション番号付きで箇条書き。
/// - WARN/NEG の所見には「このままの場合に想定されること」を併記 (可能性表現)。
/// - 末尾に「まず取り組む3つ」を置く。
pub(crate) fn render_sp_exec_onepager(
    html: &mut String,
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
    target_region: &str,
    // 2026-07-13: Ver10 は経営サマリーを「超簡単・解説なし」に簡素化する。
    //   結論の1文リスト + まず取り組む3つ + 出典注記1行だけにし、前置き文・注釈文を削る。
    ver10: bool,
) {
    let conclusions = build_conclusions(agg, ctx);

    // Ver10 はタイトルも平易にする。
    let (deck, sub) = if ver10 {
        ("要点まとめ", "まず読むのはこの1ページだけで大丈夫です")
    } else {
        (
            "経営サマリー (仮)",
            "この1ページだけ持ち歩けば要点が伝わる構成です",
        )
    };
    html.push_str(
        "<section class=\"page-navy sp-onepager\" role=\"region\" aria-label=\"経営サマリー\">\n",
    );
    let eyebrow = if ver10 { "SUMMARY" } else { "SP SUMMARY" };
    push_page_head(html, eyebrow, deck, sub);

    // Ver10 は前置きの説明文を削る (超簡単・解説なし)。
    if !ver10 {
        html.push_str(&format!(
            "<p class=\"caption\">対象地域: <strong>{}</strong> — 以下は本レポートの結論を短い文にまとめたものです。\
             各行の右側の番号は詳しく載せているページです。</p>\n",
            escape_html(target_region)
        ));
    }

    // -- 結論の1文 (箇条書き)
    html.push_str("<div class=\"block-title block-title-spaced\">結論のまとめ</div>\n");
    html.push_str("<ol class=\"sp-conclusion-list\">\n");
    for c in &conclusions {
        html.push_str("<li>\n");
        html.push_str(&format!(
            "<div class=\"sp-c-head\">\
             <span class=\"tag tag-{sev}\">{sev_label}</span>\
             <span class=\"sp-c-ref\">§{section}</span>\
             </div>\n",
            sev = c.sev,
            sev_label = severity_label(c.sev),
            section = escape_html(c.section),
        ));
        html.push_str(&format!(
            "<p class=\"sp-c-sentence\">{}</p>\n",
            escape_html(&c.sentence)
        ));
        if let Some(outlook) = &c.outlook {
            html.push_str(&format!(
                "<p class=\"sp-c-outlook\"><span class=\"sp-c-outlook-label\">このままの場合</span> {}</p>\n",
                escape_html(outlook)
            ));
        }
        html.push_str("</li>\n");
    }
    html.push_str("</ol>\n");

    // -- まず取り組む3つ (WARN/NEG を優先して 3 つ選ぶ。足りなければ一般アクションで補う)
    let mut first_three: Vec<String> = Vec::new();
    for c in &conclusions {
        if first_three.len() >= 3 {
            break;
        }
        if c.sev == "warn" || c.sev == "neg" {
            first_three.push(match c.section {
                "01" => "サンプルの取得範囲を広げ、判断に足る件数を確保する".to_string(),
                "03" => {
                    "給与レンジの提示幅を見直し、求職者が水準を判断しやすい表記に整える".to_string()
                }
                "04" => "競合より条件が見劣りしないか、給与・待遇の訴求点を点検する".to_string(),
                _ => "気になった所見について、社内で担当と期限を決めて確認する".to_string(),
            });
        }
    }
    // 常設の一般アクションで 3 つに満たす (重複しないものを補う)。
    for fallback in [
        "求人票の訴求軸 (給与・休日・未経験可など) を1つに絞って打ち出す",
        "反応が弱いページから順に、写真・仕事内容の説明を1箇所ずつ改善する",
        "掲載の更新頻度を上げ、新着として表示される機会を増やす",
    ] {
        if first_three.len() >= 3 {
            break;
        }
        if !first_three.iter().any(|a| a == fallback) {
            first_three.push(fallback.to_string());
        }
    }

    html.push_str("<div class=\"block-title block-title-spaced\">まず取り組む3つ</div>\n");
    html.push_str("<ol class=\"sp-first-three\">\n");
    for a in first_three.iter().take(3) {
        html.push_str(&format!("<li>{}</li>\n", escape_html(a)));
    }
    html.push_str("</ol>\n");

    html.push_str(
        "<p class=\"caption\">※ 数値は今回アップロードされた求人データと公的統計にもとづく参考値です。\
         断定ではなく、傾向・可能性としてお読みください。</p>\n",
    );
    html.push_str("</section>\n");
}

// ============================================================
// (b) 各セクション冒頭の「このページの結論」バンド
// ============================================================

/// SP版 (仮) 専用: 各セクションの直前に置く「このページの結論」1 文バンド。
///
/// `section_code` に対応する結論文を選んで表示する。既存セクションの見出し文字列は
/// 一切変えず、独立したバンド要素として前置する。
pub(crate) fn render_sp_conclusion_band(
    html: &mut String,
    section_code: &str,
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
) {
    let c: Conclusion = match section_code {
        "02" => Conclusion {
            topic: ConclusionTopic::Region,
            sev: "neu",
            section: "02",
            sentence: {
                let n = agg.by_prefecture.len();
                if n <= 1 {
                    "対象地域は単一エリアに絞られており、この地域を深掘りする前提で読める内容です。".to_string()
                } else {
                    format!(
                        "求人は {} 都道府県にまたがっており、隣接地域との比較でも読める内容です。",
                        n
                    )
                }
            },
            outlook: None,
        },
        "03" => conclusion_salary(agg),
        "04" => conclusion_tightness(ctx),
        "05" => Conclusion {
            topic: ConclusionTopic::Other,
            sev: "neu",
            section: "05",
            sentence: "地域の企業構成から、競合となりうる採用主体の顔ぶれが読み取れる可能性があります。"
                .to_string(),
            outlook: None,
        },
        "06" => Conclusion {
            topic: ConclusionTopic::Other,
            sev: "neu",
            section: "06",
            sentence: "働き手の年齢構成から、狙える年齢層と手薄になりやすい層の見当がつく可能性があります。"
                .to_string(),
            outlook: None,
        },
        "07" => Conclusion {
            topic: ConclusionTopic::Other,
            sev: "neu",
            section: "07",
            sentence: "最低賃金や暮らしのデータから、提示すべき給与の下限感がつかめる可能性があります。"
                .to_string(),
            outlook: None,
        },
        "09" => Conclusion {
            topic: ConclusionTopic::Other,
            sev: "neu",
            section: "09",
            sentence: "地域ごとの相対的な採用しやすさから、媒体投下の優先順位を検討できる可能性があります。"
                .to_string(),
            outlook: None,
        },
        "10" => conclusion_switcher(ctx),
        _ => Conclusion {
            topic: ConclusionTopic::Other,
            sev: "neu",
            section: "—",
            sentence: "このページのデータから、次の打ち手のヒントが読み取れる可能性があります。".to_string(),
            outlook: None,
        },
    };

    html.push_str("<div class=\"sp-conclusion-band\">\n");
    html.push_str(&format!(
        "<span class=\"tag tag-{sev}\">{sev_label}</span>\
         <span class=\"sp-band-label\">このページの結論</span>\
         <span class=\"sp-band-text\">{text}</span>\n",
        sev = c.sev,
        sev_label = severity_label(c.sev),
        text = escape_html(&c.sentence),
    ));
    if let Some(outlook) = &c.outlook {
        html.push_str(&format!(
            "<span class=\"sp-band-outlook\">このままの場合: {}</span>\n",
            escape_html(outlook)
        ));
    }
    html.push_str("</div>\n");
}

// ============================================================
// (e) 給与 25/50/75 パーセンタイル 四分位表示 (§03)
// ============================================================

/// SP版 (仮) 専用: §03 の直後に置く給与四分位表示 (代表給与系列 salary_values ベース)。
///
/// aggregator の分布データ (compute_distribution_stats) から P25/P50/P75 を算出する。
/// salary_values が空 (有効値なし) の場合は何も出さない (graceful skip)。
pub(crate) fn render_sp_salary_quartiles(html: &mut String, agg: &SurveyAggregation) {
    let is_hourly = agg.is_hourly;
    let step = if is_hourly { 50 } else { 10_000 };
    let s = match compute_distribution_stats(&agg.salary_values, step) {
        Some(s) => s,
        None => return, // 有効値なし → skip
    };
    let unit = if is_hourly { "円/時" } else { "万円" };
    let disp = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };

    html.push_str("<div class=\"block-title block-title-spaced\">表 3-SP &nbsp;代表給与の四分位 (25/50/75 パーセンタイル)</div>\n");
    html.push_str(&format!(
        "<p class=\"caption\">代表給与 n={} の分布を四分位で示します。P25 は下位 4 分の 1、P50 は中央値、\
         P75 は上位 4 分の 1 の境目です。</p>\n",
        format_number(s.n as i64)
    ));
    html.push_str("<table class=\"table-navy sp-quartile-table\">\n<thead><tr>");
    html.push_str("<th>区分</th><th class=\"num\">給与</th><th>読み方</th>");
    html.push_str("</tr></thead>\n<tbody>\n");
    let rows: [(&str, i64, &str); 3] = [
        ("P25 (下位25%)", s.p25, "この額を下回る求人が全体の約4分の1"),
        ("P50 (中央値)", s.median, "ちょうど真ん中の水準"),
        ("P75 (上位25%)", s.p75, "この額を上回る求人が全体の約4分の1"),
    ];
    for (i, (label, yen, note)) in rows.iter().enumerate() {
        let hl = if i == 1 { " class=\"hl\"" } else { "" };
        html.push_str(&format!(
            "<tr{hl}><td>{label}</td><td class=\"num bold\">{val} {unit}</td><td>{note}</td></tr>\n",
            hl = hl,
            label = escape_html(label),
            val = disp(*yen),
            unit = unit,
            note = escape_html(note),
        ));
    }
    html.push_str("</tbody></table>\n");
    // 四分位範囲 (IQR) の一言。
    let iqr = s.p75 - s.p25;
    html.push_str(&format!(
        "<p class=\"caption\">P25〜P75 の幅 (四分位範囲) は {} {} です。幅が広いほど求人ごとの条件差が大きい傾向があります。</p>\n",
        disp(iqr),
        unit
    ));
}

// ============================================================
// (c) 優先アクション表 (§09/§10 の So What を集約)
// ============================================================

/// 優先アクション表 1 行分。
struct SpAction {
    /// インパクト×手間の平易表現: "すぐ効く" or "仕込みが要る"。
    kind: &'static str,
    action: String,
    /// 根拠となるセクション番号 (例: "03")。
    section: &'static str,
}

/// SP版 (仮) 専用: §09/§10 の So What 群から主要アクションを集約した表 (終盤・§08 の直前)。
///
/// 各行に「すぐ効く / 仕込みが要る」の 2 分類 (インパクト×手間の平易表現) と、
/// 担当 / 期限 / 確認指標 の記入欄 (contenteditable) を設ける。
pub(crate) fn render_sp_priority_actions(
    html: &mut String,
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
) {
    let conclusions = build_conclusions(agg, ctx);

    // 結論の severity をもとに、実行アクションを組み立てる。
    let mut actions: Vec<SpAction> = Vec::new();

    // topic + severity で弁別する (section だけだと §03 が給与/新着で重複するため)。
    for c in &conclusions {
        match c.topic {
            ConclusionTopic::Salary if c.sev == "warn" => actions.push(SpAction {
                kind: "すぐ効く",
                action: "給与レンジの提示幅を見直し、求職者が自分の水準を判断しやすい表記にする"
                    .to_string(),
                section: "03",
            }),
            ConclusionTopic::Salary if c.sev == "pos" => actions.push(SpAction {
                kind: "すぐ効く",
                action: "まとまった給与水準を強みとして、求人票の見出しで明確に打ち出す"
                    .to_string(),
                section: "03",
            }),
            ConclusionTopic::NewRatio if c.sev == "warn" => actions.push(SpAction {
                kind: "すぐ効く",
                action: "掲載の更新頻度を上げ、新着として表示される機会を増やす".to_string(),
                section: "03",
            }),
            ConclusionTopic::Tightness if c.sev == "warn" => actions.push(SpAction {
                kind: "仕込みが要る",
                action: "競合より条件が見劣りしないか、給与・休日・待遇の訴求点を点検する"
                    .to_string(),
                section: "04",
            }),
            ConclusionTopic::Switcher => actions.push(SpAction {
                kind: "仕込みが要る",
                action: "転職を考えている層に届くよう、媒体・配信地域の優先順位を検討する"
                    .to_string(),
                section: "10",
            }),
            ConclusionTopic::Sample if c.sev == "warn" || c.sev == "neg" => {
                actions.push(SpAction {
                    kind: "すぐ効く",
                    action: "サンプルの取得範囲を広げ、判断に足る件数を確保する".to_string(),
                    section: "01",
                })
            }
            _ => {}
        }
    }

    // 常設の底上げアクション (重複しない範囲で補う。最低 3 行を確保)。
    for (kind, action, section) in [
        (
            "すぐ効く",
            "反応が弱い求人から順に、写真と仕事内容の説明を1箇所ずつ改善する",
            "05",
        ),
        (
            "仕込みが要る",
            "手薄な年齢層に向けた訴求 (働き方・研修など) を1つ用意する",
            "06",
        ),
        (
            "すぐ効く",
            "掲載の更新頻度を上げ、新着として表示される機会を増やす",
            "03",
        ),
    ] {
        if actions.len() >= 6 {
            break;
        }
        if !actions.iter().any(|a| a.action == action) {
            actions.push(SpAction {
                kind,
                action: action.to_string(),
                section,
            });
        }
    }

    html.push_str(
        "<section class=\"page-navy sp-actions\" role=\"region\" aria-label=\"優先アクション\">\n",
    );
    push_page_head(
        html,
        "SP ACTIONS",
        "優先アクション表 (仮)",
        "何から手をつけるか。効き方と手間で分けています",
    );
    html.push_str(
        "<p class=\"caption\">「すぐ効く」は今日から着手でき効果が出やすいもの、\
         「仕込みが要る」は準備に時間はかかるが効きが大きいものです。\
         担当・期限・確認する指標の欄は、この画面上で直接入力・編集できます。</p>\n",
    );

    html.push_str("<table class=\"table-navy sp-action-table\">\n<thead><tr>");
    html.push_str(
        "<th>効き方</th><th>やること</th><th>根拠</th>\
         <th>担当</th><th>期限</th><th>確認する指標</th>",
    );
    html.push_str("</tr></thead>\n<tbody>\n");
    for a in &actions {
        let kind_tag = if a.kind == "すぐ効く" {
            "pos"
        } else {
            "warn"
        };
        html.push_str(&format!(
            "<tr>\
             <td><span class=\"tag tag-{kind_tag}\">{kind}</span></td>\
             <td>{action}</td>\
             <td class=\"dim\">§{section}</td>\
             <td contenteditable=\"true\" aria-label=\"担当を入力\">&nbsp;</td>\
             <td contenteditable=\"true\" aria-label=\"期限を入力\">&nbsp;</td>\
             <td contenteditable=\"true\" aria-label=\"確認する指標を入力\">&nbsp;</td>\
             </tr>\n",
            kind_tag = kind_tag,
            kind = escape_html(a.kind),
            action = escape_html(&a.action),
            section = escape_html(a.section),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p class=\"caption\">※ ここに挙げたのは今回のデータから読み取れる打ち手の候補です。\
         効果を保証するものではなく、現場の状況に合わせて取捨選択してください。</p>\n",
    );
    html.push_str("</section>\n");
}

// ============================================================
// Tests (SP版 (仮) 専用ブロックのデータ妥当性)
//   MEMORY: feedback_test_data_validation / feedback_reverse_proof_tests 準拠。
//   検証: パーセンタイル (P25<=P50<=P75) / 結論バンドの可能性表現 (断定語なし) /
//         経営サマリーの箇条数 (3〜5) / 優先アクション表の contenteditable 記入欄。
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn agg_with_salary(values: Vec<i64>) -> SurveyAggregation {
        SurveyAggregation {
            total_count: values.len().max(1),
            salary_values: values,
            ..Default::default()
        }
    }

    // ---- (e) 給与四分位: P25 <= P50 <= P75 の順序不変条件 ----

    #[test]
    fn quartiles_are_monotonic_and_render() {
        let agg = agg_with_salary(vec![
            200_000, 220_000, 240_000, 260_000, 280_000, 300_000, 320_000, 340_000,
        ]);
        let stats =
            compute_distribution_stats(&agg.salary_values, 10_000).expect("値があれば Some");
        // ドメイン不変条件: 四分位は単調非減少
        assert!(
            stats.p25 <= stats.median,
            "P25 <= P50: {:?}",
            (stats.p25, stats.median)
        );
        assert!(
            stats.median <= stats.p75,
            "P50 <= P75: {:?}",
            (stats.median, stats.p75)
        );

        let mut html = String::new();
        render_sp_salary_quartiles(&mut html, &agg);
        assert!(html.contains("表 3-SP"), "四分位表タイトル: {}", html);
        assert!(html.contains("P25 (下位25%)"), "P25 行: {}", html);
        assert!(html.contains("P50 (中央値)"), "P50 行: {}", html);
        assert!(html.contains("P75 (上位25%)"), "P75 行: {}", html);
    }

    #[test]
    fn quartiles_skip_when_no_salary_values() {
        // salary_values 空 → 何も出さない (graceful skip、panic しない)。
        let agg = agg_with_salary(vec![]);
        let mut html = String::new();
        render_sp_salary_quartiles(&mut html, &agg);
        assert!(html.is_empty(), "有効値なしは空出力: {}", html);
    }

    // ---- (b) 結論バンド: 可能性表現であり断定語を含まない ----

    /// 断定・約束表現の禁止語リスト (lint_statistical_claims.py と整合)。
    const FORBIDDEN_WORDS: [&str; 6] = [
        "必ず",
        "確実に",
        "断言",
        "証明されました",
        "間違いなく",
        "絶対に",
    ];

    #[test]
    fn conclusion_band_uses_possibility_language_not_assertions() {
        let agg = agg_with_salary(vec![200_000, 240_000, 280_000, 320_000, 360_000, 400_000]);
        // 主要セクションすべてのバンドで断定語が出ないことを確認。
        for code in ["02", "03", "04", "05", "06", "07", "09", "10"] {
            let mut html = String::new();
            render_sp_conclusion_band(&mut html, code, &agg, None);
            assert!(
                html.contains("このページの結論"),
                "band ラベル ({}): {}",
                code,
                html
            );
            for w in FORBIDDEN_WORDS {
                assert!(
                    !html.contains(w),
                    "セクション {} の結論バンドに断定語「{}」が混入: {}",
                    code,
                    w,
                    html
                );
            }
        }
    }

    #[test]
    fn conclusion_band_wide_salary_marks_warn_with_possibility() {
        // レンジが広い給与分布 → warn チップ + 可能性表現 + 「このままの場合」1 行。
        let agg = agg_with_salary(vec![150_000, 160_000, 170_000, 300_000, 500_000, 700_000]);
        let mut html = String::new();
        render_sp_conclusion_band(&mut html, "03", &agg, None);
        assert!(html.contains("tag-warn"), "広いレンジは warn: {}", html);
        assert!(html.contains("可能性があります"), "可能性表現: {}", html);
        assert!(html.contains("このままの場合"), "放置見通しの1行: {}", html);
    }

    // ---- (a) 経営サマリー: 結論 3〜5 箇条 + まず取り組む3つ ----

    #[test]
    fn exec_onepager_has_3_to_5_conclusions_and_first_three() {
        let agg = agg_with_salary(vec![200_000, 240_000, 280_000, 320_000, 360_000]);
        let conclusions = build_conclusions(&agg, None);
        assert!(
            (3..=5).contains(&conclusions.len()),
            "結論は 3〜5 箇条: {} 件",
            conclusions.len()
        );

        let mut html = String::new();
        render_sp_exec_onepager(&mut html, &agg, None, "群馬県 高崎市", false);
        assert!(
            html.contains("経営サマリー (仮)"),
            "サマリータイトル: {}",
            html
        );
        assert!(html.contains("結論のまとめ"), "結論見出し: {}", html);
        assert!(
            html.contains("まず取り組む3つ"),
            "まず取り組む3つ: {}",
            html
        );
        // まず取り組む3つ は必ず 3 項目 (<li> が 3 つ以上、うち first-three リスト内に 3)
        let ft = html.split("sp-first-three").nth(1).unwrap_or("");
        assert_eq!(
            ft.matches("<li>").count(),
            3,
            "まず取り組む3つは 3 項目: {}",
            ft
        );
    }

    #[test]
    fn exec_onepager_zero_sample_no_panic() {
        // total=0 相当 (salary 空 + total 0) でも panic せず neg 判定。
        let agg = SurveyAggregation {
            total_count: 0,
            ..Default::default()
        };
        let mut html = String::new();
        render_sp_exec_onepager(&mut html, &agg, None, "全国", false);
        assert!(html.contains("経営サマリー (仮)"));
        assert!(!html.contains("NaN"), "0 件で NaN 混入なし");
    }

    // ---- (c) 優先アクション表: contenteditable 記入欄 + 効き方 2 分類 ----

    #[test]
    fn priority_actions_has_editable_fields_and_two_kinds() {
        let agg = agg_with_salary(vec![
            150_000, 160_000, 170_000, 300_000, 500_000,
            700_000, // 広いレンジ → warn 誘発
        ]);
        let mut html = String::new();
        render_sp_priority_actions(&mut html, &agg, None);
        assert!(
            html.contains("優先アクション表 (仮)"),
            "アクション表タイトル: {}",
            html
        );
        assert!(html.contains("すぐ効く"), "すぐ効く分類: {}", html);
        assert!(html.contains("仕込みが要る"), "仕込みが要る分類: {}", html);
        // 担当/期限/確認指標の記入欄 (contenteditable) が存在する
        assert!(
            html.contains("contenteditable=\"true\""),
            "記入欄 (contenteditable): {}",
            html
        );
        assert!(html.contains("担当を入力"), "担当欄: {}", html);
        assert!(html.contains("期限を入力"), "期限欄: {}", html);
        assert!(html.contains("確認する指標を入力"), "確認指標欄: {}", html);
    }
}
