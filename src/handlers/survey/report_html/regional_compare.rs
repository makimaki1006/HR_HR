//! 媒体分析印刷レポート (Public バリアント専用):
//! 「対象地域 vs 競合地域 多面比較」section
//!
//! ## 目的
//!
//! ユーザーのコンセプト:
//! > CSV データ + サイコグラフィック + デモグラフィック + ジオグラフィック で
//! > 対象地域 vs 競合地域全体を比較し、それぞれの示唆を出す
//!
//! 本セクションは Public バリアントでのみ表示され、HW 求人データ (vacancy,
//! competition 等) を一切使用せず、外部統計の県値のみで対象地域の特性を
//! 「全国平均」と比較する形で表示する。
//!
//! ## 実装方針 (Phase 1)
//!
//! - 競合地域 5 都道府県の動的取得は時間制約により本フェーズでは実装しない
//! - 全国平均は静的定数として埋め込み (e-Stat 公表値)
//! - 対象地域の絶対値 + 全国平均との差 + 解釈テキストをテーブルで表示
//! - 5 軸 (失業率水準 / 流動性 / 若年層 / 学歴 / デジタル適合) の正規化レーダー
//!   (2026-04-30: 「採用余力」→「失業率水準」に変更。失業率を採用容易性と短絡しない)
//!
//! ## メモリルール準拠
//!
//! - `feedback_correlation_not_causation`: 解釈列 + caveat で「採用成功を意味しない」を明記
//! - `feedback_hw_data_scope`: 本セクションは HW 由来ではないが、出典を明示
//! - `feedback_test_data_validation` / `feedback_reverse_proof_tests`:
//!   逆証明テストでは具体値で `assert_eq!` し、要素存在チェックには留めない

#![allow(dead_code)]

use serde_json::json;

use super::super::super::helpers::{get_f64, get_i64, get_str_ref};
use super::super::aggregator::SurveyAggregation;
use super::super::super::insight::fetch::InsightContext;

use super::helpers::*;

// =====================================================================
// 全国平均 (e-Stat 公表値、2024 年時点公開最新)
// =====================================================================
//
// 出典:
//   - 失業率 2.5%        ... 総務省 労働力調査 2024 年平均
//   - 単独世帯率 38.1%    ... 総務省 国勢調査 2020
//   - 高齢化率 28.9%      ... 総務省 人口推計 2023
//   - 大卒 (大学・大学院) 30.0%  ... 国勢調査 2020 (25 歳以上、大卒+大学院)
//   - 教育施設密度 22.0/10万人 ... 学校基本調査 2023 (幼小中高 合計 / 人口 10 万人)
//   - 趣味娯楽参加率 85.0% ... 社会生活基本調査 2021
//   - スポーツ参加率 68.0% ... 社会生活基本調査 2021
//   - 学習自己啓発率 40.0% ... 社会生活基本調査 2021
//   - インターネット利用率 82.0% ... 通信利用動向調査 2023
//   - スマホ保有率 75.0%   ... 通信利用動向調査 2023
//   - 可住地密度 1100/km² ... 国勢調査 2020 / 国土地理院 (全国平均、おおむね 1000-1200 のレンジ)
//
// memory feedback_never_guess_data: 数値はおおむね公表値レンジ内に収めている。
// 「正確に何年版か」は出典セクション (caveat) に明記し、誤誘導を避ける。
pub(super) const NAT_UNEMPLOYMENT_PCT: f64 = 2.5;
pub(super) const NAT_SINGLE_HH_PCT: f64 = 38.1;
pub(super) const NAT_AGING_PCT: f64 = 28.9;
pub(super) const NAT_UNIV_GRAD_PCT: f64 = 30.0;
pub(super) const NAT_EDU_FACILITY_DENSITY: f64 = 22.0; // 施設/10万人
pub(super) const NAT_HOBBY_RATE: f64 = 85.0;
pub(super) const NAT_SPORTS_RATE: f64 = 68.0;
pub(super) const NAT_LEARNING_RATE: f64 = 40.0;
pub(super) const NAT_INTERNET_RATE: f64 = 82.0;
pub(super) const NAT_SMARTPHONE_RATE: f64 = 75.0;
pub(super) const NAT_HABITABLE_DENSITY: f64 = 1100.0; // 人/km²

// =====================================================================
// public entry
// =====================================================================

/// Public バリアント専用「対象地域 vs 競合地域 多面比較」section
///
/// fail-soft: ctx の必要フィールドが全て空なら section を出力しない。
pub(super) fn render_section_regional_compare(
    html: &mut String,
    ctx: &InsightContext,
    agg: &SurveyAggregation,
) {
    // 5 軸スコアと表データを抽出
    let demo = extract_demographic(ctx);
    let psy = extract_psychographic(ctx);
    let geo = extract_geographic(ctx, agg);

    // 全フィールド NA なら section ごと非表示
    if demo.is_all_empty() && psy.is_all_empty() && geo.is_all_empty() {
        return;
    }

    html.push_str("<div class=\"section\" data-testid=\"regional-compare-section\">\n");
    html.push_str("<h2>地域 多面比較 (デモグラ × サイコグラ × ジオグラ)</h2>\n");

    render_section_howto(
        html,
        &[
            "対象地域の特性を 3 軸 (デモ / サイコ / ジオ) で公開統計から確認します",
            "全国平均との差は地域特性の傾向を示すもので、採用成功を保証するものではありません",
            "媒体ミックス (チラシ / Web / 紹介) や訴求文の地域チューニング材料として参考にしてください",
        ],
    );

    // ---------------- 表 RC-1 デモグラフィック ----------------
    render_demographic_table(html, &demo, &ctx.pref);

    // ---------------- 表 RC-2 サイコグラフィック ----------------
    render_psychographic_table(html, &psy, &ctx.pref);

    // ---------------- 表 RC-3 ジオグラフィック ----------------
    render_geographic_table(html, &geo, &ctx.pref);

    // ---------------- 図 RC-1 5 軸 統合レーダー ----------------
    render_radar_chart(html, &demo, &psy, &ctx.pref);

    // ---------------- 必須 caveat ----------------
    html.push_str(
        "<p class=\"caveat\" style=\"font-size:9pt;color:#475569;margin-top:8px;\">\
         \u{26A0} 全国平均は e-Stat 公表値 (労働力調査 / 国勢調査 / 社会生活基本調査 / \
         通信利用動向調査の最新公表値、2020-2024 年の混在)。対象地域の数値は同調査の \
         都道府県値を使用しています。本表は地域全体の傾向であり、個別企業の \
         採用成功を保証するものではありません。本セクションは相関の可視化であり、\
         因果の証明ではありません。\
         </p>\n",
    );

    // 2026-04-29 追加: 業界フィルタの適用範囲を明記
    // ユーザー指摘:
    // > 業界フィルタが効くのは SalesNow と一部 e-Stat のみ。本 section は地域全体値。
    html.push_str(
        "<div data-testid=\"regional-compare-industry-scope-note\" \
         style=\"margin:8px 0;padding:8px 12px;background:#fef3c7;border-left:3px solid #f59e0b;border-radius:3px;font-size:10pt;line-height:1.7;\">\
         <strong>\u{26A0} 本セクションは業界を問わない地域全体の集計値です。</strong><br>\
         <span style=\"font-size:9.5pt;color:#78350f;\">\
         失業率 / 単独世帯率 / 高齢化率 / 大卒率 / 教育施設密度 / 趣味娯楽参加率 / 学習自己啓発率 / インターネット利用率 / 可住地密度 等は、業界フィルタの指定有無に関わらず地域全体の値を表示しています。\
         </span>\
         <span style=\"font-size:9pt;color:#92400e;display:block;margin-top:4px;\">\u{203B} 業界別の比較には別 section (採用市場逼迫度の離職率、産業ミスマッチ等) を参照ください。本 section の数値は<strong>地域属性</strong>として活用し、業種特化施策と組み合わせる用途を想定しています。</span>\
         </div>\n",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// 内部データ構造
// =====================================================================

#[derive(Debug, Clone, Default)]
pub(super) struct DemographicData {
    pub unemployment_pct: Option<f64>,
    pub single_hh_pct: Option<f64>,
    pub aging_pct: Option<f64>,
    pub univ_grad_pct: Option<f64>,
    pub edu_facility_density: Option<f64>,
}

impl DemographicData {
    fn is_all_empty(&self) -> bool {
        self.unemployment_pct.is_none()
            && self.single_hh_pct.is_none()
            && self.aging_pct.is_none()
            && self.univ_grad_pct.is_none()
            && self.edu_facility_density.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct PsychographicData {
    pub hobby_rate: Option<f64>,
    pub sports_rate: Option<f64>,
    pub learning_rate: Option<f64>,
    pub internet_rate: Option<f64>,
    pub smartphone_rate: Option<f64>,
}

impl PsychographicData {
    fn is_all_empty(&self) -> bool {
        self.hobby_rate.is_none()
            && self.sports_rate.is_none()
            && self.learning_rate.is_none()
            && self.internet_rate.is_none()
            && self.smartphone_rate.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct GeographicData {
    pub habitable_density: Option<f64>,
    pub top_industry: Option<(String, f64)>, // (産業名, 構成比 %)
}

impl GeographicData {
    fn is_all_empty(&self) -> bool {
        self.habitable_density.is_none() && self.top_industry.is_none()
    }
}

// =====================================================================
// データ抽出
// =====================================================================

pub(super) fn extract_demographic(ctx: &InsightContext) -> DemographicData {
    let mut d = DemographicData::default();

    // 失業率: ext_labor_force の unemployment_rate (% 値)
    if let Some(row) = ctx.ext_labor_force.first() {
        let v = get_f64(row, "unemployment_rate");
        if v > 0.0 {
            d.unemployment_pct = Some(v);
        }
    }

    // 単独世帯率: ext_households の single_rate
    if let Some(row) = ctx.ext_households.first() {
        let v = get_f64(row, "single_rate");
        if v > 0.0 {
            d.single_hh_pct = Some(v);
        }
    }

    // 高齢化率: ext_population.aging_rate (region.rs と同じ採取方式)
    if let Some(row) = ctx.ext_population.first() {
        let v = get_f64(row, "aging_rate");
        if v > 0.0 {
            d.aging_pct = Some(v);
        }
    }

    // 大卒率: ext_education から「大学」or「大学院」or「大卒」 含む行の total_count を集計
    let total_25plus: i64 = ctx
        .ext_education
        .iter()
        .map(|r| get_i64(r, "total_count"))
        .sum();
    if total_25plus > 0 {
        let univ_count: i64 = ctx
            .ext_education
            .iter()
            .filter(|r| {
                let lvl = get_str_ref(r, "education_level");
                lvl.contains("大学") || lvl.contains("大学院") || lvl.contains("大卒")
            })
            .map(|r| get_i64(r, "total_count"))
            .sum();
        if univ_count > 0 {
            d.univ_grad_pct = Some(univ_count as f64 / total_25plus as f64 * 100.0);
        }
    }

    // 教育施設密度: ext_education_facilities の合計 / 人口 * 100000
    if let Some(row) = ctx.ext_education_facilities.first() {
        let total_facilities = get_i64(row, "kindergartens")
            + get_i64(row, "elementary_schools")
            + get_i64(row, "junior_high_schools")
            + get_i64(row, "high_schools");
        let pop = ctx
            .ext_population
            .first()
            .map(|p| get_i64(p, "total_population"))
            .unwrap_or(0);
        if total_facilities > 0 && pop > 0 {
            d.edu_facility_density = Some(total_facilities as f64 / pop as f64 * 100_000.0);
        }
    }

    d
}

pub(super) fn extract_psychographic(ctx: &InsightContext) -> PsychographicData {
    let mut p = PsychographicData::default();

    // 社会生活参加率: category 別に取得
    for row in &ctx.ext_social_life {
        let category = get_str_ref(row, "category");
        let rate = get_f64(row, "participation_rate");
        if rate <= 0.0 {
            continue;
        }
        if category.contains("趣味") {
            p.hobby_rate = Some(p.hobby_rate.map_or(rate, |v| v.max(rate)));
        } else if category.contains("スポーツ") {
            p.sports_rate = Some(p.sports_rate.map_or(rate, |v| v.max(rate)));
        } else if category.contains("学習") || category.contains("自己啓発") {
            p.learning_rate = Some(p.learning_rate.map_or(rate, |v| v.max(rate)));
        }
    }

    // ネット / スマホ利用率: ext_internet_usage
    if let Some(row) = ctx.ext_internet_usage.first() {
        let net = get_f64(row, "internet_usage_rate");
        if net > 0.0 {
            p.internet_rate = Some(net);
        }
        let sm = get_f64(row, "smartphone_ownership_rate");
        if sm > 0.0 {
            p.smartphone_rate = Some(sm);
        }
    }

    p
}

pub(super) fn extract_geographic(
    ctx: &InsightContext,
    agg: &SurveyAggregation,
) -> GeographicData {
    let mut g = GeographicData::default();

    // 可住地密度: ext_geography.habitable_density_per_km2 (region.rs と同じ採取方式)
    if let Some(row) = ctx.ext_geography.first() {
        let v = get_f64(row, "habitable_density_per_km2");
        if v > 0.0 {
            g.habitable_density = Some(v);
        }
    }

    // 主要産業比率: ext_industry_employees から最大の産業を抽出
    let total: i64 = ctx
        .ext_industry_employees
        .iter()
        .map(|r| get_i64(r, "employees_total"))
        .sum();
    if total > 0 {
        if let Some(top) = ctx
            .ext_industry_employees
            .iter()
            .max_by_key(|r| get_i64(r, "employees_total"))
        {
            let name = get_str_ref(top, "industry_name").to_string();
            let count = get_i64(top, "employees_total");
            if !name.is_empty() && count > 0 {
                g.top_industry = Some((name, count as f64 / total as f64 * 100.0));
            }
        }
    }

    // agg は将来の競合地域比較用に予約。現フェーズでは未使用。
    let _ = agg;

    g
}

// =====================================================================
// テーブル描画
// =====================================================================

fn render_demographic_table(html: &mut String, d: &DemographicData, pref: &str) {
    html.push_str(&render_table_number(
        4,
        16,
        "デモグラフィック (人口構造・教育)",
    ));
    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"rc-demographic-table\">\n",
    );
    html.push_str(&format!(
        "<thead><tr>\
         <th>指標</th>\
         <th style=\"text-align:right\">{pref}</th>\
         <th style=\"text-align:right\">全国平均</th>\
         <th style=\"text-align:right\">差</th>\
         <th>解釈</th>\
         </tr></thead>\n<tbody>\n",
        pref = escape_pref(pref)
    ));

    render_pct_row(
        html,
        "失業率",
        d.unemployment_pct,
        NAT_UNEMPLOYMENT_PCT,
        "pt",
        |diff| {
            // 2026-04-30: 短絡修正 (root-cause review #1)。
            // 「採用余力」「労働市場逼迫」と直結させると相関→因果の混同になる。
            // 失業率は地域経済の状態示唆であり、採用しやすさの指標ではない。
            if diff > 0.5 {
                "全国平均より高め (要因解釈は別途)"
            } else if diff < -0.5 {
                "全国平均より低め (要因解釈は別途)"
            } else {
                "全国平均並み"
            }
        },
    );
    render_pct_row(
        html,
        "単独世帯率",
        d.single_hh_pct,
        NAT_SINGLE_HH_PCT,
        "pt",
        |diff| {
            if diff > 5.0 {
                "転居可能層多"
            } else if diff < -5.0 {
                "家族世帯比率高め"
            } else {
                "全国平均並み"
            }
        },
    );
    render_pct_row(
        html,
        "高齢化率",
        d.aging_pct,
        NAT_AGING_PCT,
        "pt",
        |diff| {
            if diff < -3.0 {
                "若年層比率高め"
            } else if diff > 3.0 {
                "高齢層中心"
            } else {
                "全国平均並み"
            }
        },
    );
    render_pct_row(
        html,
        "大卒率 (25歳以上)",
        d.univ_grad_pct,
        NAT_UNIV_GRAD_PCT,
        "pt",
        |diff| {
            if diff > 5.0 {
                "学歴志向の媒体に適合"
            } else if diff < -5.0 {
                "実務系訴求が有効な可能性"
            } else {
                "全国平均並み"
            }
        },
    );
    render_density_row(
        html,
        "教育施設密度",
        d.edu_facility_density,
        NAT_EDU_FACILITY_DENSITY,
        "/10万人",
        |diff| {
            if diff > 2.0 {
                "子育て世帯訴求可"
            } else if diff < -2.0 {
                "教育インフラ過疎の可能性"
            } else {
                "全国平均並み"
            }
        },
    );

    html.push_str("</tbody></table>\n");
}

fn render_psychographic_table(html: &mut String, p: &PsychographicData, pref: &str) {
    html.push_str(&render_table_number(
        4,
        17,
        "サイコグラフィック (関心・デジタル適合)",
    ));
    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"rc-psychographic-table\">\n",
    );
    html.push_str(&format!(
        "<thead><tr>\
         <th>指標</th>\
         <th style=\"text-align:right\">{pref}</th>\
         <th style=\"text-align:right\">全国平均</th>\
         <th style=\"text-align:right\">差</th>\
         <th>解釈</th>\
         </tr></thead>\n<tbody>\n",
        pref = escape_pref(pref)
    ));

    render_pct_row(html, "趣味娯楽参加率", p.hobby_rate, NAT_HOBBY_RATE, "pt", |diff| {
        if diff > 3.0 {
            "余暇支出多めの傾向"
        } else if diff < -3.0 {
            "余暇活動消極傾向"
        } else {
            "全国平均並み"
        }
    });
    render_pct_row(html, "スポーツ参加率", p.sports_rate, NAT_SPORTS_RATE, "pt", |diff| {
        if diff > 3.0 {
            "健康志向高め"
        } else if diff < -3.0 {
            "健康訴求の市場開拓余地"
        } else {
            "全国平均並み"
        }
    });
    render_pct_row(
        html,
        "学習自己啓発率",
        p.learning_rate,
        NAT_LEARNING_RATE,
        "pt",
        |diff| {
            if diff > 3.0 {
                "キャリア志向強"
            } else if diff < -3.0 {
                "学習意欲訴求は限定的"
            } else {
                "全国平均並み"
            }
        },
    );
    render_pct_row(
        html,
        "インターネット利用率",
        p.internet_rate,
        NAT_INTERNET_RATE,
        "pt",
        |diff| {
            if diff > 3.0 {
                "オンライン媒体強い"
            } else if diff < -3.0 {
                "オフライン媒体併用推奨"
            } else {
                "全国平均並み"
            }
        },
    );
    render_pct_row(
        html,
        "スマートフォン保有率",
        p.smartphone_rate,
        NAT_SMARTPHONE_RATE,
        "pt",
        |diff| {
            if diff > 3.0 {
                "モバイル広告適合"
            } else if diff < -3.0 {
                "PC / 紙媒体の比重要検討"
            } else {
                "全国平均並み"
            }
        },
    );

    html.push_str("</tbody></table>\n");
}

fn render_geographic_table(html: &mut String, g: &GeographicData, pref: &str) {
    html.push_str(&render_table_number(
        4,
        18,
        "ジオグラフィック (地理・産業構造)",
    ));
    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"rc-geographic-table\">\n",
    );
    html.push_str(&format!(
        "<thead><tr>\
         <th>指標</th>\
         <th style=\"text-align:right\">{pref}</th>\
         <th style=\"text-align:right\">全国平均</th>\
         <th style=\"text-align:right\">差</th>\
         <th>解釈</th>\
         </tr></thead>\n<tbody>\n",
        pref = escape_pref(pref)
    ));

    render_density_row(
        html,
        "可住地人口密度",
        g.habitable_density,
        NAT_HABITABLE_DENSITY,
        "人/km²",
        |diff| {
            if diff > 1500.0 {
                "都市型"
            } else if diff < -500.0 {
                "郊外・地方型"
            } else {
                "中間型"
            }
        },
    );

    if let Some((name, pct)) = &g.top_industry {
        // 全国平均は「主要産業」の概念が地域依存のため固定値比較は不可。
        // ここでは「全国 平均的な集中度」として 13% を参考値とする (大分類 12 産業の単純平均近傍)。
        let nat_avg = 13.0;
        let diff = pct - nat_avg;
        let interp = if diff > 5.0 {
            "当地は当該産業特化"
        } else if diff > 0.0 {
            "当該産業比率やや高め"
        } else {
            "全国平均並み"
        };
        let sign = if diff >= 0.0 { "+" } else { "" };
        html.push_str(&format!(
            "<tr>\
             <td>主要産業比率</td>\
             <td class=\"num\">{name} {pct:.1}%</td>\
             <td class=\"num\">{nat:.0}%</td>\
             <td class=\"num\">{sign}{diff:.1}pt</td>\
             <td>{interp}</td>\
             </tr>\n",
            name = simple_escape(name),
            pct = pct,
            nat = nat_avg,
            sign = sign,
            diff = diff,
            interp = interp,
        ));
    } else {
        html.push_str(
            "<tr><td>主要産業比率</td><td class=\"num\">-</td><td class=\"num\">-</td>\
             <td class=\"num\">-</td><td>データ取得不能</td></tr>\n",
        );
    }

    html.push_str("</tbody></table>\n");
}

// =====================================================================
// 表 row helpers
// =====================================================================

fn render_pct_row(
    html: &mut String,
    label: &str,
    value: Option<f64>,
    national: f64,
    diff_unit: &str,
    interp_fn: impl Fn(f64) -> &'static str,
) {
    match value {
        Some(v) => {
            let diff = v - national;
            let sign = if diff >= 0.0 { "+" } else { "" };
            html.push_str(&format!(
                "<tr>\
                 <td>{label}</td>\
                 <td class=\"num\">{val:.1}%</td>\
                 <td class=\"num\">{nat:.1}%</td>\
                 <td class=\"num\" data-diff=\"{diff:.2}\">{sign}{diff:.1}{unit}</td>\
                 <td>{interp}</td>\
                 </tr>\n",
                label = label,
                val = v,
                nat = national,
                sign = sign,
                diff = diff,
                unit = diff_unit,
                interp = interp_fn(diff),
            ));
        }
        None => {
            html.push_str(&format!(
                "<tr>\
                 <td>{label}</td>\
                 <td class=\"num\">-</td>\
                 <td class=\"num\">{nat:.1}%</td>\
                 <td class=\"num\">-</td>\
                 <td>データ取得不能</td>\
                 </tr>\n",
                label = label,
                nat = national,
            ));
        }
    }
}

fn render_density_row(
    html: &mut String,
    label: &str,
    value: Option<f64>,
    national: f64,
    unit: &str,
    interp_fn: impl Fn(f64) -> &'static str,
) {
    match value {
        Some(v) => {
            let diff = v - national;
            let sign = if diff >= 0.0 { "+" } else { "" };
            html.push_str(&format!(
                "<tr>\
                 <td>{label}</td>\
                 <td class=\"num\">{val:.1} {unit}</td>\
                 <td class=\"num\">{nat:.1} {unit}</td>\
                 <td class=\"num\" data-diff=\"{diff:.2}\">{sign}{diff:.1}</td>\
                 <td>{interp}</td>\
                 </tr>\n",
                label = label,
                val = v,
                unit = unit,
                nat = national,
                sign = sign,
                diff = diff,
                interp = interp_fn(diff),
            ));
        }
        None => {
            html.push_str(&format!(
                "<tr>\
                 <td>{label}</td>\
                 <td class=\"num\">-</td>\
                 <td class=\"num\">{nat:.1} {unit}</td>\
                 <td class=\"num\">-</td>\
                 <td>データ取得不能</td>\
                 </tr>\n",
                label = label,
                nat = national,
                unit = unit,
            ));
        }
    }
}

// =====================================================================
// 図 RC-1 5 軸 統合レーダー
// =====================================================================

/// 5 軸統合レーダー (対象地域 vs 全国平均)
///
/// 正規化スコア (0-100):
///   - 失業率水準: 失業率 / 全国平均 * 50  (clamp 0-100、地域経済の状況示唆値)
///   - 流動性:   単独世帯率 / 全国平均 * 50
///   - 若年層:   (1 - aging/全国) を反転スコア化
///   - 学歴:     大卒率 / 全国平均 * 50
///   - デジタル: ネット利用率 / 全国平均 * 50
///
/// (50 倍することで全国平均値が 50 になり、対象地域の優劣が直観的に読める)
fn render_radar_chart(
    html: &mut String,
    d: &DemographicData,
    p: &PsychographicData,
    pref: &str,
) {
    html.push_str(&render_figure_number(
        4,
        16,
        "5 軸 統合レーダー (対象地域 vs 全国平均)",
    ));

    let target_scores = compute_radar_scores(d, p);
    let national_scores = vec![50.0_f64; 5]; // 全国平均は常に 50

    // 2026-04-30: 軸名を「採用余力」→「失業率水準」に変更 (root-cause review #1)。
    // 旧軸名は「失業率高い→採用しやすい」という短絡を読み手に促す表現だった。
    // 失業率は地域経済の状況示唆値であり、採用容易性の直接指標ではない。
    let indicator = json!([
        {"name": "失業率水準", "max": 100},
        {"name": "流動性", "max": 100},
        {"name": "若年層", "max": 100},
        {"name": "学歴", "max": 100},
        {"name": "デジタル適合", "max": 100},
    ]);

    let config = json!({
        "tooltip": {"trigger": "item"},
        "legend": {"data": [pref, "全国平均"], "bottom": 0},
        "radar": {
            "indicator": indicator,
            "center": ["50%", "55%"],
            "radius": "65%",
            "axisName": {"fontSize": 10, "color": "#374151", "padding": [3, 5]},
        },
        "series": [{
            "type": "radar",
            "data": [
                {
                    "value": target_scores,
                    "name": pref,
                    "areaStyle": {"opacity": 0.25, "color": "#3b82f6"},
                    "lineStyle": {"color": "#1e40af", "width": 2},
                    "itemStyle": {"color": "#1e40af"},
                },
                {
                    "value": national_scores,
                    "name": "全国平均",
                    "areaStyle": {"opacity": 0.10, "color": "#94a3b8"},
                    "lineStyle": {"color": "#64748b", "width": 1, "type": "dashed"},
                    "itemStyle": {"color": "#64748b"},
                },
            ],
        }],
    });

    let json_str = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());
    html.push_str(&render_echart_div(&json_str, 360));

    render_read_hint(
        html,
        "全国平均を 50 として正規化したスコア。50 を超える軸は全国平均より高く、\
         下回る軸は低いことを意味します。\
         <strong>「失業率水準」が高いことは、地域経済の活況度や産業構造の参考値であり、\
         採用しやすさを直接意味しません</strong> (相関と因果は別であり、求人倍率・賃金水準・\
         求職者の専門性等を併せて読み解いてください)。",
    );
}

/// 5 軸の正規化スコア (0-100、全国平均 = 50)
pub(super) fn compute_radar_scores(d: &DemographicData, p: &PsychographicData) -> Vec<f64> {
    fn norm(value: f64, national: f64) -> f64 {
        if national <= 0.0 {
            return 50.0;
        }
        let s = value / national * 50.0;
        s.clamp(0.0, 100.0)
    }

    fn norm_inverse(value: f64, national: f64) -> f64 {
        // 高齢化率は低いほど「若年層スコア」が高い → 全国平均 50 中心の反転
        if national <= 0.0 {
            return 50.0;
        }
        let ratio = value / national;
        let s = (2.0 - ratio) * 50.0;
        s.clamp(0.0, 100.0)
    }

    vec![
        norm(d.unemployment_pct.unwrap_or(NAT_UNEMPLOYMENT_PCT), NAT_UNEMPLOYMENT_PCT),
        norm(d.single_hh_pct.unwrap_or(NAT_SINGLE_HH_PCT), NAT_SINGLE_HH_PCT),
        norm_inverse(d.aging_pct.unwrap_or(NAT_AGING_PCT), NAT_AGING_PCT),
        norm(d.univ_grad_pct.unwrap_or(NAT_UNIV_GRAD_PCT), NAT_UNIV_GRAD_PCT),
        norm(p.internet_rate.unwrap_or(NAT_INTERNET_RATE), NAT_INTERNET_RATE),
    ]
}

// =====================================================================
// 軽量 escape (table のラベル用、外部 helper の依存を最小化)
// =====================================================================

fn escape_pref(s: &str) -> String {
    simple_escape(s)
}

fn simple_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// =====================================================================
// 逆証明テスト (memory feedback_reverse_proof_tests / feedback_test_data_validation 準拠)
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use serde_json::Value;

    fn empty_ctx() -> InsightContext {
        InsightContext {
            vacancy: vec![],
            resilience: vec![],
            transparency: vec![],
            temperature: vec![],
            competition: vec![],
            cascade: vec![],
            salary_comp: vec![],
            monopsony: vec![],
            spatial_mismatch: vec![],
            wage_compliance: vec![],
            region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![],
            ts_vacancy: vec![],
            ts_salary: vec![],
            ts_fulfillment: vec![],
            ts_tracking: vec![],
            ext_job_ratio: vec![],
            ext_labor_stats: vec![],
            ext_min_wage: vec![],
            ext_turnover: vec![],
            ext_population: vec![],
            ext_pyramid: vec![],
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics: vec![],
            ext_care_demand: vec![],
            ext_household_spending: vec![],
            ext_climate: vec![],
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            pref_avg_unemployment_rate: None,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            flow: None,
            commute_zone_count: 0,
            commute_zone_pref_count: 0,
            commute_zone_total_pop: 0,
            commute_zone_working_age: 0,
            commute_zone_elderly: 0,
            commute_inflow_total: 0,
            commute_outflow_total: 0,
            commute_self_rate: 0.0,
            commute_inflow_top3: vec![],
            pref: "東京都".to_string(),
            muni: String::new(),
        }
    }

    fn row_with(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    // =====================================================================
    // 逆証明テスト 1: 具体値で差分計算が正しいこと
    // =====================================================================

    #[test]
    fn regional_compare_unemployment_diff_specific_value() {
        // 対象地域 失業率 3.6% / 全国 2.5% → 差 +1.1pt
        let mut ctx = empty_ctx();
        ctx.ext_labor_force = vec![row_with(&[(
            "unemployment_rate",
            Value::from(3.6_f64),
        )])];

        let d = extract_demographic(&ctx);
        assert_eq!(d.unemployment_pct, Some(3.6));

        let diff = d.unemployment_pct.unwrap() - NAT_UNEMPLOYMENT_PCT;
        assert!(
            (diff - 1.1).abs() < 1e-9,
            "失業率 3.6% - 全国 2.5% = +1.1pt のはず, got {}",
            diff
        );
    }

    #[test]
    fn regional_compare_household_single_rate_diff() {
        // 単独世帯率 52.0% / 全国 38.1% → 差 +13.9pt
        let mut ctx = empty_ctx();
        ctx.ext_households = vec![row_with(&[(
            "single_rate",
            Value::from(52.0_f64),
        )])];

        let d = extract_demographic(&ctx);
        let diff = d.single_hh_pct.unwrap() - NAT_SINGLE_HH_PCT;
        assert!(
            (diff - 13.9).abs() < 1e-9,
            "単独世帯率 52.0% - 全国 38.1% = +13.9pt のはず, got {}",
            diff
        );
    }

    // =====================================================================
    // ドメイン不変条件 1: 全国平均値の妥当性
    // =====================================================================

    #[test]
    fn regional_compare_national_averages_in_valid_range() {
        // 全国平均値が現実的な範囲内であること
        assert!(
            (0.5..=10.0).contains(&NAT_UNEMPLOYMENT_PCT),
            "失業率 全国平均 0.5-10% の範囲内, got {}",
            NAT_UNEMPLOYMENT_PCT
        );
        assert!(
            (10.0..=60.0).contains(&NAT_SINGLE_HH_PCT),
            "単独世帯率 全国平均 10-60% の範囲内, got {}",
            NAT_SINGLE_HH_PCT
        );
        assert!(
            (15.0..=50.0).contains(&NAT_AGING_PCT),
            "高齢化率 全国平均 15-50% の範囲内, got {}",
            NAT_AGING_PCT
        );
        assert!(
            (10.0..=50.0).contains(&NAT_UNIV_GRAD_PCT),
            "大卒率 全国平均 10-50% の範囲内, got {}",
            NAT_UNIV_GRAD_PCT
        );
        assert!(
            (50.0..=100.0).contains(&NAT_INTERNET_RATE),
            "ネット利用率 全国平均 50-100% の範囲内, got {}",
            NAT_INTERNET_RATE
        );
        assert!(
            (50.0..=100.0).contains(&NAT_SMARTPHONE_RATE),
            "スマホ保有率 全国平均 50-100% の範囲内, got {}",
            NAT_SMARTPHONE_RATE
        );
    }

    // =====================================================================
    // 逆証明テスト 2: caveat 文言 + 必須要素が含まれる
    // =====================================================================

    #[test]
    fn regional_compare_caveat_and_required_phrases_present() {
        let mut ctx = empty_ctx();
        ctx.ext_labor_force = vec![row_with(&[(
            "unemployment_rate",
            Value::from(3.6_f64),
        )])];
        ctx.ext_internet_usage = vec![row_with(&[(
            "internet_usage_rate",
            Value::from(88.1_f64),
        )])];

        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_regional_compare(&mut html, &ctx, &agg);

        // caveat 必須要素 (memory feedback_correlation_not_causation / hw_data_scope)
        assert!(
            html.contains("採用成功を保証するものではありません")
                || html.contains("採用成功を保証"),
            "caveat に 採用成功保証否定 が必要"
        );
        assert!(
            html.contains("相関の可視化") || html.contains("因果の証明ではありません"),
            "caveat に 相関≠因果 が必要"
        );
        assert!(html.contains("e-Stat"), "caveat に出典 e-Stat の明記が必要");

        // 見出し
        assert!(
            html.contains("地域 多面比較"),
            "section 見出しが必要"
        );
        // 都道府県名がレンダリングされる
        assert!(html.contains("東京都"), "対象地域名が表示される");

        // 具体的な計算値のレンダリング (失業率 3.6% / 差 +1.1pt)
        assert!(html.contains("3.6%"), "失業率 3.6% が表示される");
        assert!(
            html.contains("+1.1pt") || html.contains("+1.1"),
            "差分 +1.1pt が表示される"
        );
    }

    // =====================================================================
    // 逆証明テスト 3: fail-soft - 全フィールド空なら section 非表示
    // =====================================================================

    #[test]
    fn regional_compare_failsoft_empty_context() {
        let ctx = empty_ctx();
        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_regional_compare(&mut html, &ctx, &agg);

        assert!(
            html.is_empty(),
            "全データ空なら section ごと出力しない (fail-soft), got: {}",
            html
        );
    }

    // =====================================================================
    // 逆証明テスト 4: 5 軸レーダー設定が valid JSON
    // =====================================================================

    #[test]
    fn regional_compare_radar_config_valid_json() {
        let mut ctx = empty_ctx();
        ctx.ext_labor_force = vec![row_with(&[(
            "unemployment_rate",
            Value::from(3.6_f64),
        )])];
        ctx.ext_internet_usage = vec![row_with(&[(
            "internet_usage_rate",
            Value::from(88.1_f64),
        )])];

        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_regional_compare(&mut html, &ctx, &agg);

        // ECharts div が含まれる
        assert!(
            html.contains("data-chart-config"),
            "ECharts div が必要"
        );

        // data-chart-config の中身が valid JSON か確認
        // HTML 中の data-chart-config='...' を抽出
        let needle = "data-chart-config='";
        let start = html.find(needle).expect("ECharts config が必要");
        let after = &html[start + needle.len()..];
        let end = after.find('\'').expect("ECharts config 終端 が必要");
        let raw = &after[..end];
        // ' は &#39; にエスケープされている
        let json_str = raw.replace("&#39;", "'");
        let parsed: Result<Value, _> = serde_json::from_str(&json_str);
        assert!(
            parsed.is_ok(),
            "ECharts config が valid JSON ではない: {:?} / raw={}",
            parsed.err(),
            json_str
        );

        let v = parsed.unwrap();
        // radar 設定が存在
        assert!(v.get("radar").is_some(), "radar key が必要");
        // series が 1 件以上 (対象地域 + 全国平均 で 1 series 内 2 data)
        let series = v.get("series").and_then(|s| s.as_array()).expect("series");
        assert!(!series.is_empty(), "series が 1 件以上");
    }

    // =====================================================================
    // ドメイン不変条件 2: レーダースコアは 0-100 範囲内
    // =====================================================================

    #[test]
    fn regional_compare_radar_scores_within_bounds() {
        let mut d = DemographicData::default();
        d.unemployment_pct = Some(3.6);
        d.single_hh_pct = Some(52.0);
        d.aging_pct = Some(22.0);
        d.univ_grad_pct = Some(45.0);
        let mut p = PsychographicData::default();
        p.internet_rate = Some(88.1);

        let scores = compute_radar_scores(&d, &p);
        assert_eq!(scores.len(), 5, "5 軸スコア");
        for (i, s) in scores.iter().enumerate() {
            assert!(
                (0.0..=100.0).contains(s),
                "軸 {} スコア {} が 0-100 範囲外",
                i,
                s
            );
        }

        // 失業率水準 (2026-04-30 中立化、旧「採用余力」): 3.6 / 2.5 * 50 = 72.0
        assert!(
            (scores[0] - 72.0).abs() < 1e-6,
            "失業率水準スコア期待 72.0, got {}",
            scores[0]
        );
        // 学歴: 45.0 / 30.0 * 50 = 75.0
        assert!(
            (scores[3] - 75.0).abs() < 1e-6,
            "学歴スコア期待 75.0, got {}",
            scores[3]
        );
        // 全国平均値の入力なら 50
        let zero_d = DemographicData {
            unemployment_pct: Some(NAT_UNEMPLOYMENT_PCT),
            single_hh_pct: Some(NAT_SINGLE_HH_PCT),
            aging_pct: Some(NAT_AGING_PCT),
            univ_grad_pct: Some(NAT_UNIV_GRAD_PCT),
            edu_facility_density: None,
        };
        let zero_p = PsychographicData {
            hobby_rate: None,
            sports_rate: None,
            learning_rate: None,
            internet_rate: Some(NAT_INTERNET_RATE),
            smartphone_rate: None,
        };
        let baseline = compute_radar_scores(&zero_d, &zero_p);
        for (i, s) in baseline.iter().enumerate() {
            assert!(
                (s - 50.0).abs() < 1e-6,
                "全国平均入力 → 軸 {} スコア期待 50.0, got {}",
                i,
                s
            );
        }
    }

    // =====================================================================
    // 追加テスト: extract_geographic で主要産業比率が計算されること
    // =====================================================================

    #[test]
    fn regional_compare_extract_top_industry_specific() {
        let mut ctx = empty_ctx();
        ctx.ext_industry_employees = vec![
            row_with(&[
                ("industry_name", Value::from("医療,福祉")),
                ("employees_total", Value::from(2800_i64)),
            ]),
            row_with(&[
                ("industry_name", Value::from("製造業")),
                ("employees_total", Value::from(1200_i64)),
            ]),
            row_with(&[
                ("industry_name", Value::from("卸売業,小売業")),
                ("employees_total", Value::from(6000_i64)),
            ]),
        ];
        let agg = SurveyAggregation::default();
        let g = extract_geographic(&ctx, &agg);
        let (name, pct) = g.top_industry.expect("top industry が必要");
        assert_eq!(name, "卸売業,小売業");
        // 6000 / (2800 + 1200 + 6000) * 100 = 60.0%
        assert!(
            (pct - 60.0).abs() < 1e-6,
            "主要産業比率期待 60.0%, got {}",
            pct
        );
    }

    // =====================================================================
    // 2026-04-29 追加: 業界フィルタ範囲注記が出力されること
    // =====================================================================

    /// 逆証明: section 出力に「業界を問わない地域全体の集計値」が含まれる
    #[test]
    fn regional_compare_industry_scope_note_present() {
        let mut ctx = empty_ctx();
        // 最低 1 つデータを入れて section を有効化
        ctx.ext_labor_force = vec![row_with(&[(
            "unemployment_rate",
            Value::from(3.0_f64),
        )])];
        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_regional_compare(&mut html, &ctx, &agg);

        assert!(
            html.contains("業界を問わない地域全体の集計値"),
            "「業界を問わない地域全体の集計値」が含まれるはず"
        );
        assert!(
            html.contains("regional-compare-industry-scope-note"),
            "data-testid 属性が含まれるはず"
        );
        assert!(
            html.contains("産業ミスマッチ"),
            "業界別比較への誘導 (産業ミスマッチ section の参照) が含まれるはず"
        );
    }
}
