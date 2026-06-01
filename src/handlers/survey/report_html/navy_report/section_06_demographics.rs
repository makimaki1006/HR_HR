//! Section 06 - 人材デモグラフィック (Phase 3 navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 7 / β Section Team / 2026-05-30) で抽出。
//!
//! 元 `navy_report/mod.rs` L173-L1176 の以下を物理コピー:
//! - `render_navy_section_06_demographics`        (公開 API: pub(crate))
//! - `age_lo`                                     (private helper)
//! - `age_sort_key`                               (private helper)
//! - `build_navy_pyramid_svg`                     (private helper)
//! - `build_navy_pyramid_svg_mini`                (private helper)
//! - `build_demographics_so_what`                 (private helper)
//!
//! 2026-06-01: 図 6-3 / 表 6-G/H/I/J の HW postings 求人側集計ブロックを
//! 削除 (HW postings が最新版でないという業務判断)。以下の旧 helper /
//! 定数も同時削除:
//! - `render_navy_section_06_posting_target`      (図 6-3 描画)
//! - `build_distribution_table`                   (表 6-G/H/I/J 共通ビルダ)
//! - `build_hourly_band_distribution`             (表 6-J: H4 時給帯 bucket)
//! - 定数 `HOURLY_BAND_BOUNDARIES`                (表 6-J 用 13 段境界)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_06_demographics`
//!   (Commit 2/3/4/5/6 パターン踏襲: `pub(super)` は階層不足で E0364 になるため `pub(crate)`)
//!
//! 残りの helper は本ファイル内のみで使用。`navy_report` モジュール外への露出はない。
//!
//! `build_navy_auto_table` は mod.rs に残置 (Section 03/05/06/07 で共有)。
//! `super::build_navy_auto_table` で参照する。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_06_demographics):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::build_navy_auto_table;
use super::common::{push_kpi, push_page_head, push_region_scope_banner};

// ============================================================
// Section 06: 人材デモグラフィック (Phase 3 navy 本実装)
// ============================================================

/// Phase 2-A (2026-05-29): `agg` 引数追加。
///   `agg.is_hourly` を `build_demographics_so_what` 内の採用候補層 / 訴求軸の切替に
///   利用する。デモグラフィック (人口ピラミッド等) 自体には is_hourly 依存はない。
///   旧 `render_navy_section_06_posting_target` への伝播は 2026-06-01 削除済み。
pub(crate) fn render_navy_section_06_demographics(
    html: &mut String,
    agg: &SurveyAggregation,
    hw_context: Option<&InsightContext>,
    target_region: &str,
) {
    let is_hourly = agg.is_hourly;
    html.push_str("<section class=\"page-navy navy-demographics\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 06",
        "人材デモグラフィック",
        "人口ピラミッド / 労働力 / 教育施設密度",
    );
    push_region_scope_banner(html, target_region);

    let ctx = match hw_context {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption\">外部統計データが取得できなかったため、本セクションは省略表示となります。</p>\n");
            html.push_str("</section>\n");
            return;
        }
    };

    // -- ピラミッドデータ抽出
    use super::super::super::super::helpers::{get_f64, get_i64, get_str_ref};
    let mut bands: Vec<(String, i64, i64)> = ctx
        .ext_pyramid
        .iter()
        .map(|r| {
            (
                get_str_ref(r, "age_group").to_string(),
                get_i64(r, "male_count"),
                get_i64(r, "female_count"),
            )
        })
        .filter(|(l, _, _)| !l.is_empty())
        .collect();
    bands.sort_by_key(|(l, _, _)| age_sort_key(l));

    // -- 集計
    let total_pop: i64 = bands.iter().map(|(_, m, f)| m + f).sum();
    let working_age: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 15 && age_lo(l) < 65)
        .map(|(_, m, f)| m + f)
        .sum();
    let target_age: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 25 && age_lo(l) < 45)
        .map(|(_, m, f)| m + f)
        .sum();
    let senior: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 65)
        .map(|(_, m, f)| m + f)
        .sum();

    let working_pct = if total_pop > 0 {
        working_age as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };
    let target_pct = if total_pop > 0 {
        target_age as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };
    let senior_pct = if total_pop > 0 {
        senior as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };

    // -- 労働力率 / 失業率
    let labor_force_rate = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "labor_force_participation_rate"))
        .filter(|v| *v > 0.0);
    let unemployment_rate = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "unemployment_rate"))
        .filter(|v| *v > 0.0);

    // -- 教育施設密度
    let school_count: i64 = ctx
        .ext_education_facilities
        .iter()
        .map(|r| {
            get_i64(r, "elementary_schools")
                + get_i64(r, "junior_high_schools")
                + get_i64(r, "high_schools")
        })
        .sum();

    // -- exec-headline
    let lede = format!(
        "対象地域の生産年齢層厚みを把握します。総人口 <strong>{}</strong> 名 / \
         生産年齢 (15-64) <strong>{:.1}%</strong> / 採用ターゲット (25-44) <strong>{:.1}%</strong> / \
         高齢 (65+) <strong>{:.1}%</strong>。",
        format_number(total_pop),
        working_pct,
        target_pct,
        senior_pct,
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- KPI 5 cell
    let working_dot = if working_pct >= 60.0 {
        "pos"
    } else if working_pct >= 50.0 {
        "neu"
    } else {
        "warn"
    };
    let target_dot = if target_pct >= 22.0 {
        "pos"
    } else if target_pct >= 17.0 {
        "neu"
    } else {
        "warn"
    };
    let senior_dot = if senior_pct >= 35.0 {
        "warn"
    } else if senior_pct >= 25.0 {
        "neu"
    } else {
        "pos"
    };

    html.push_str("<div class=\"block-title\">図 6-1 &nbsp;人口構造 主要 KPI</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");
    push_kpi(
        html,
        "総人口",
        &format_number(total_pop),
        "名",
        "neu",
        "国勢調査 5 歳階級集計",
        false,
    );
    push_kpi(
        html,
        "生産年齢 (15-64)",
        &format!("{:.1}", working_pct),
        "%",
        working_dot,
        &format!("実数 {} 名", format_number(working_age)),
        true,
    );
    push_kpi(
        html,
        "ターゲット (25-44)",
        &format!("{:.1}", target_pct),
        "%",
        target_dot,
        &format!("実数 {} 名", format_number(target_age)),
        false,
    );
    push_kpi(
        html,
        "高齢 (65+)",
        &format!("{:.1}", senior_pct),
        "%",
        senior_dot,
        &format!("実数 {} 名", format_number(senior)),
        false,
    );
    let lfr_val = labor_force_rate
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "—".into());
    let lfr_dot = match labor_force_rate {
        Some(v) if v >= 62.0 => "pos",
        Some(v) if v >= 55.0 => "neu",
        Some(_) => "warn",
        None => "neu",
    };
    let lfr_foot = match unemployment_rate {
        Some(u) => format!("失業率 {:.1}%", u),
        None => "失業率データなし".to_string(),
    };
    push_kpi(html, "労働力率", &lfr_val, "%", lfr_dot, &lfr_foot, false);
    html.push_str("</div>\n");

    // -- 人口ピラミッド SVG
    if !bands.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 6-2 &nbsp;年齢階級別 人口ピラミッド</div>\n");
        html.push_str(&build_navy_pyramid_svg(&bands));
        html.push_str("<p class=\"caption\">左 (紺) = 男性 / 右 (金) = 女性。各バーは 5 歳階級別の人口を表示。出典: 国勢調査 v2_external_population_pyramid。</p>\n");
    }

    // -- 図 6-2b 市区町村別 人口ピラミッド (上位 3) [P1-5 (2026-05-25) 追加]
    //    対象都道府県内で postings (HW 掲載求人) 件数上位 3 市区町村のピラミッドを並列表示。
    //    ctx.muni_pyramids が空 (pref 未指定 / データ不足) のときは何も出力しない。
    if !ctx.muni_pyramids.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 6-2b &nbsp;市区町村別 人口ピラミッド (上位 3)</div>\n");
        html.push_str(
            "<div class=\"muni-pyramid-grid\" \
             style=\"display:grid;grid-template-columns:1fr 1fr 1fr;gap:6mm;margin-top:2mm;\">\n",
        );
        for mp in &ctx.muni_pyramids {
            let mut sub_bands: Vec<(String, i64, i64)> = mp
                .bands
                .iter()
                .map(|r| {
                    (
                        get_str_ref(r, "age_group").to_string(),
                        get_i64(r, "male_count"),
                        get_i64(r, "female_count"),
                    )
                })
                .filter(|(l, _, _)| !l.is_empty())
                .collect();
            sub_bands.sort_by_key(|(l, _, _)| age_sort_key(l));

            html.push_str(
                "<div class=\"muni-pyramid-card\" \
                 style=\"border:1px solid var(--rule-soft);padding:3mm;background:var(--paper-pure);\">\n",
            );
            html.push_str(&format!(
                "<div style=\"text-align:center;font-weight:700;font-size:10pt;color:#0B1E3F;margin-bottom:2mm;\">{}</div>\n",
                escape_html(&mp.muni_name)
            ));
            if sub_bands.is_empty() {
                html.push_str(
                    "<div class=\"dim\" style=\"text-align:center;font-size:9pt;\">データ取得不可</div>\n",
                );
            } else {
                html.push_str(&build_navy_pyramid_svg_mini(&sub_bands));
            }
            html.push_str("</div>\n");
        }
        html.push_str("</div>\n");
        html.push_str(
            "<p class=\"caption\">対象都道府県の CSV 件数上位 3 市区町村のピラミッドを並列表示。\
             出典: 国勢調査 v2_external_population_pyramid (市区町村粒度)。</p>\n",
        );
    }

    // -- 表 6-B 人口統計詳細 (ext_population) ピラミッド補強  [旧 7.5-D 統合 2026-05-15]
    if !ctx.ext_population.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-B &nbsp;人口統計詳細 (総人口・男女別 年次推移)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_population, 5));
        html.push_str("<p class=\"caption\">出典: 国勢調査 v2_external_population。ピラミッドの 5 歳階級集計に対し、本表は総人口・男女別の年次推移を示す。先頭 5 行表示。</p>\n");
    }

    // -- 表 6-C 人口移動 (ext_migration) ⭐ 採用流入/定着指標  [旧 7.5-E 統合 2026-05-15]
    if !ctx.ext_migration.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-C &nbsp;人口移動 (転入・転出・純増減)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_migration, 5));
        let latest_net: i64 = ctx
            .ext_migration
            .first()
            .map(|r| get_i64(r, "net_migration"))
            .unwrap_or(0);
        let migration_insight = if latest_net > 0 {
            format!("最新値で <strong>転入超過 +{} 名</strong>。社外からの流入が継続しており、<strong>採用候補プール 拡大局面</strong>。広域採用・移住セット訴求 (住宅手当 / 引越補助) との相性 良。",
                format_number(latest_net))
        } else if latest_net < 0 {
            format!("最新値で <strong>転出超過 {} 名</strong>。人口流出が継続しており、<strong>採用難 + 離職リスクの両面</strong>に注意。定着策 (キャリアパス明示 / 地元志向人材の囲い込み) を優先推奨。",
                format_number(latest_net))
        } else {
            "転入・転出が均衡。人材の純流入による母集団拡大は期待しにくく、<strong>定着重視</strong>の採用方針が有効。".to_string()
        };
        html.push_str(&format!(
            "<p class=\"caption\">出典: 住民基本台帳 人口移動報告 v2_external_migration。先頭 5 行表示。<br/><strong>示唆:</strong> {}</p>\n",
            migration_insight
        ));
    }

    // -- 表 6-D 自然増減 (出生・死亡) 中長期人口動態  [旧 7.5-M 統合 2026-05-15]
    if !ctx.ext_vital.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-D &nbsp;自然増減 (出生・死亡)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_vital, 5));
        let latest_natural: i64 = ctx
            .ext_vital
            .first()
            .map(|r| get_i64(r, "natural_change"))
            .unwrap_or(0);
        let vital_insight = if latest_natural < 0 {
            format!("最新値で <strong>自然減 {} 名</strong> (死亡 > 出生)。中長期 (5-10 年) で<strong>労働力供給の減少局面</strong>が継続する見通し、自動化投資・省人化施策の並走を推奨。",
                format_number(latest_natural))
        } else {
            format!("自然増 +{} 名で人口再生産は継続。短期の採用環境は本指標より表 6-C (社会移動) の影響が支配的。",
                format_number(latest_natural))
        };
        html.push_str(&format!(
            "<p class=\"caption\">出典: 人口動態統計 v2_external_vital。先頭 5 行表示。<br/><strong>示唆:</strong> {}</p>\n",
            vital_insight
        ));
    }

    // -- 教育施設密度 (block-title + 1 段落)
    if school_count > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-A &nbsp;教育施設 (小・中・高 合計)</div>\n");
        html.push_str(&format!(
            "<table class=\"table-navy\">\n<thead><tr>\
             <th>区分</th><th class=\"num\">学校数</th><th>備考</th>\
             </tr></thead>\n<tbody>\n"
        ));
        let mut sum_elem = 0i64;
        let mut sum_jh = 0i64;
        let mut sum_high = 0i64;
        for r in &ctx.ext_education_facilities {
            sum_elem += get_i64(r, "elementary_schools");
            sum_jh += get_i64(r, "junior_high_schools");
            sum_high += get_i64(r, "high_schools");
        }
        html.push_str(&format!(
            "<tr><td><strong>小学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 1-3 km 想定</span></td></tr>\n",
            format_number(sum_elem)
        ));
        html.push_str(&format!(
            "<tr><td><strong>中学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 3-5 km 想定</span></td></tr>\n",
            format_number(sum_jh)
        ));
        html.push_str(&format!(
            "<tr class=\"hl\"><td><strong>高等学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 10 km 級。新卒採用接点として活用可</span></td></tr>\n",
            format_number(sum_high)
        ));
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: 文部科学省 学校基本調査 v2_external_education_facilities。家族層 (子育て世帯) 採用時の生活インフラ指標として併記。</p>\n");
    }

    // -- 表 6-E 労働力統計 詳細 (ext_labor_stats)  KPI 労働力率の明細  [旧 7.5-C 統合 2026-05-15]
    if !ctx.ext_labor_stats.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-E &nbsp;労働力統計 詳細 (就業者・産業構成)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_labor_stats, 5));
        html.push_str("<p class=\"caption\">出典: e-Stat 社会人口統計体系 v2_external_labor_stats。図 6-1 KPI「労働力率」の内訳として、男女別就業者・第1-3 次産業就業者の構成比を示す。先頭 5 行表示。</p>\n");
    }

    // -- 表 6-F 学歴構成 (ext_education) [P1-5 (2026-05-25): 手書き化 + 構成比列追加]
    //    旧実装: build_navy_auto_table(&ctx.ext_education, 5)
    //    変更点: education_level / 男性人数 / 女性人数 / 合計 / 構成比 (%) の 5 列固定。
    //    構成比 = total_count / SUM(total_count) * 100 (小数 1 桁、右寄せ + bold)。
    if !ctx.ext_education.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-F &nbsp;進学率・学歴 (新卒採用接点)</div>\n");

        // 合計算出 (構成比の分母)
        let total_sum: i64 = ctx
            .ext_education
            .iter()
            .map(|r| get_i64(r, "total_count"))
            .sum();

        html.push_str("<table class=\"table-navy\">\n");
        html.push_str(
            "<thead><tr>\
             <th>学歴レベル</th>\
             <th class=\"num\">男性人数</th>\
             <th class=\"num\">女性人数</th>\
             <th class=\"num\">合計</th>\
             <th class=\"num\">構成比 (%)</th>\
             </tr></thead>\n<tbody>\n",
        );

        for r in ctx.ext_education.iter().take(5) {
            let level = get_str_ref(r, "education_level");
            let male = get_i64(r, "male_count");
            let female = get_i64(r, "female_count");
            let total = get_i64(r, "total_count");
            let pct = if total_sum > 0 {
                total as f64 / total_sum as f64 * 100.0
            } else {
                0.0
            };
            html.push_str(&format!(
                "<tr>\
                 <td>{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num bold\">{:.1}</td>\
                 </tr>\n",
                escape_html(level),
                format_number(male),
                format_number(female),
                format_number(total),
                pct,
            ));
        }
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: 学校基本調査 v2_external_education。表 6-A の学校数 (施設密度) に対し、本表は進学率・学歴構成を示す。高校進学率は新卒採用の母集団品質、大学進学率は U ターン採用の射程に直結。先頭 5 行表示。</p>\n");
    }

    // 図 6-3 (求人ターゲット プロファイル) / 表 6-G/H/I/J は 2026-06-01 削除。
    //   理由: HW postings は最新版ではないため求人側集計の信頼性が低い、というユーザー判断。
    //   `ctx.posting_target` field 自体は他経路 (analysis/fetch 経由) で参照されているため
    //   InsightContext からの削除は行わず、本セクションでの利用のみ停止する。

    // -- so-what
    let so_what = build_demographics_so_what(
        working_pct,
        target_pct,
        senior_pct,
        labor_force_rate,
        is_hourly,
    );
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

// 2026-06-01: 図 6-3 / 表 6-G/H/I/J の HW postings 求人側集計ブロック (旧
// `render_navy_section_06_posting_target` + 関連 helper) を削除。
// HW postings が最新版でないという業務判断によりレンダリング側から完全除去。
// `ctx.posting_target` field は他経路 (analysis/fetch) で生存しているため残置。
// 旧 helper `build_distribution_table` / `build_hourly_band_distribution` /
// 定数 `HOURLY_BAND_BOUNDARIES` も本ブロック専用だったため同時削除。

// 「20-24」「25-29」「85+」等のラベルから下端年齢を取得
fn age_lo(label: &str) -> i32 {
    let mut s = String::new();
    for c in label.chars() {
        if c.is_ascii_digit() {
            s.push(c);
        } else {
            break;
        }
    }
    s.parse::<i32>().unwrap_or(-1)
}

fn age_sort_key(label: &str) -> i32 {
    let v = age_lo(label);
    if v >= 0 {
        v
    } else {
        i32::MAX
    }
}

/// navy 人口ピラミッド SVG (左=男性 ink-soft / 右=女性 accent)
pub(crate) fn build_navy_pyramid_svg(bands: &[(String, i64, i64)]) -> String {
    if bands.is_empty() {
        return String::new();
    }
    let n = bands.len();
    let row_h: f64 = 18.0;
    let h: f64 = 40.0 + n as f64 * row_h + 24.0;
    let w: f64 = 720.0;
    // 2026-05-14: 年齢ラベルがバーの中央 (men/women 境界) に乗り、紺/金バーと潰れて
    //             判読困難だった問題を解消。ラベルを左外側の専用カラムに移動し、
    //             バー描画領域を左にオフセットして重なりを除去する。
    let label_col_w: f64 = 56.0; // 左端のラベル列幅
    let center_gap: f64 = 8.0; // 男女バー間のセンター隙間
    let bar_max_w: f64 = (w - label_col_w) / 2.0 - center_gap;
    let center: f64 = label_col_w + bar_max_w + center_gap; // 男女境界 (シフトした中心)

    let max_count: f64 = bands
        .iter()
        .flat_map(|(_, m, f)| [*m as f64, *f as f64])
        .fold(0.0, f64::max)
        .max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"人口ピラミッド\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n\
         <title>年齢階級別 人口ピラミッド</title>\n",
        w = w as i64,
        h = h as i64
    );
    // R2-P1-3 (ultrathink Round 2, 2026-05-28): a11y のため SVG 直後に <title> を挿入。
    // スクリーンリーダーは aria-label と <title> の双方を読み上げ得るため、両立させる。
    // タイトルラベル (左カラム = 年齢, 男性 = 中央左, 女性 = 中央右)
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"18\" font-size=\"10\" fill=\"#6A6E7A\" font-weight=\"700\">年齢</text>\
         <text x=\"{:.1}\" y=\"18\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\" text-anchor=\"end\">男性</text>\
         <text x=\"{:.1}\" y=\"18\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\">女性</text>\n",
        4.0, center - 8.0, center + 8.0
    ));
    // 中央軸
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"30\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
        center, center, h - 24.0
    ));

    for (i, (label, male, female)) in bands.iter().rev().enumerate() {
        let cy = 36.0 + i as f64 * row_h;
        let mw = (*male as f64 / max_count) * bar_max_w;
        let fw = (*female as f64 / max_count) * bar_max_w;
        // 男性 (左)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#1F2D4D\"/>\n",
            center - mw,
            cy,
            mw.max(0.5)
        ));
        // 女性 (右)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#C9A24B\"/>\n",
            center,
            cy,
            fw.max(0.5)
        ));
        // 年齢ラベル (左カラム、独立した白背景領域)
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#0B1E3F\" font-weight=\"600\" text-anchor=\"start\">{}</text>\n",
            4.0,
            cy + 10.0,
            escape_html(label)
        ));
    }

    // 軸スケール
    svg.push_str(&format!(
        "<text x=\"4\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\">{} 名</text>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"end\">{} 名</text>\n",
        h - 8.0,
        format_number(max_count as i64),
        w - 4.0,
        h - 8.0,
        format_number(max_count as i64)
    ));
    svg.push_str("</svg>\n");
    svg
}

/// 図 6-2b 用ミニピラミッド SVG (3 列横並びレイアウト想定、幅 220px)。
///
/// `build_navy_pyramid_svg` の構造をベースに、グリッドカード内に収まるようサイズと
/// フォントを縮小: 幅 220px / 行高 14px / フォント 7-8pt / ラベル列幅 32px。
/// 色 (#1F2D4D / #C9A24B) は本体ピラミッドと一貫させる。
pub(crate) fn build_navy_pyramid_svg_mini(bands: &[(String, i64, i64)]) -> String {
    if bands.is_empty() {
        return String::new();
    }
    let n = bands.len();
    let row_h: f64 = 14.0;
    let h: f64 = 30.0 + n as f64 * row_h + 18.0;
    let w: f64 = 220.0;
    let label_col_w: f64 = 32.0;
    let center_gap: f64 = 4.0;
    let bar_max_w: f64 = (w - label_col_w) / 2.0 - center_gap;
    let center: f64 = label_col_w + bar_max_w + center_gap;

    let max_count: f64 = bands
        .iter()
        .flat_map(|(_, m, f)| [*m as f64, *f as f64])
        .fold(0.0, f64::max)
        .max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"市区町村別 人口ピラミッド\" \
         style=\"display:block;background:var(--paper-pure);\">\n\
         <title>市区町村別 人口ピラミッド (年齢階級別 男女別 人口)</title>\n",
        w = w as i64,
        h = h as i64
    );
    // R2-P1-3 (ultrathink Round 2, 2026-05-28): a11y のため SVG 直後に <title> を挿入。
    // タイトル行 (男性 / 女性)
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"14\" font-size=\"7\" fill=\"#6A6E7A\" font-weight=\"700\">年齢</text>\
         <text x=\"{:.1}\" y=\"14\" font-size=\"8\" fill=\"#0B1E3F\" font-weight=\"700\" text-anchor=\"end\">男</text>\
         <text x=\"{:.1}\" y=\"14\" font-size=\"8\" fill=\"#0B1E3F\" font-weight=\"700\">女</text>\n",
        2.0, center - 4.0, center + 4.0
    ));
    // 中央軸
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"22\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
        center, center, h - 18.0
    ));

    for (i, (label, male, female)) in bands.iter().rev().enumerate() {
        let cy = 28.0 + i as f64 * row_h;
        let mw = (*male as f64 / max_count) * bar_max_w;
        let fw = (*female as f64 / max_count) * bar_max_w;
        // 男性 (左)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"10\" fill=\"#1F2D4D\"/>\n",
            center - mw,
            cy,
            mw.max(0.5)
        ));
        // 女性 (右)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"10\" fill=\"#C9A24B\"/>\n",
            center,
            cy,
            fw.max(0.5)
        ));
        // 年齢ラベル
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"7\" fill=\"#0B1E3F\" font-weight=\"600\" text-anchor=\"start\">{}</text>\n",
            2.0,
            cy + 8.0,
            escape_html(label)
        ));
    }

    // 軸スケール (最大値)
    svg.push_str(&format!(
        "<text x=\"2\" y=\"{:.1}\" font-size=\"6\" fill=\"#6A6E7A\">{} 名</text>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"6\" fill=\"#6A6E7A\" text-anchor=\"end\">{} 名</text>\n",
        h - 4.0,
        format_number(max_count as i64),
        w - 2.0,
        h - 4.0,
        format_number(max_count as i64)
    ));
    svg.push_str("</svg>\n");
    svg
}

/// Phase 2-A (2026-05-29): `is_hourly` 引数追加。
///   時給モードでは採用候補層を 25-49 (主婦層含めて広め) に変更し、
///   訴求軸も「給与訴求 + 福利厚生」→「扶養範囲明示 + シフト柔軟性 + 交通費」に切替える。
fn build_demographics_so_what(
    working_pct: f64,
    target_pct: f64,
    senior_pct: f64,
    labor_force_rate: Option<f64>,
    is_hourly: bool,
) -> String {
    let target_label = if is_hourly {
        "採用候補層 (25-49)"
    } else {
        "採用ターゲット層 (25-44)"
    };
    let appeal_text = if is_hourly {
        "扶養範囲明示 + シフト柔軟性 + 交通費"
    } else {
        "給与訴求 + 福利厚生"
    };
    let pool_judge = if target_pct >= 22.0 {
        format!(
            "{} が <strong>{:.0}%</strong> を占め、<strong>採用候補プール 厚</strong>。{}の充実度で勝負できる地域です。",
            target_label, target_pct, appeal_text
        )
    } else if target_pct >= 17.0 {
        format!(
            "{} は <strong>{:.0}%</strong>。<strong>採用候補プール 中</strong>。エントリー要件の柔軟化 (経験不問 / 異業種歓迎) で母集団拡大を検討してください。",
            target_label, target_pct
        )
    } else {
        format!(
            "{} が <strong>{:.0}%</strong> と薄く、<strong>採用候補プール 細</strong>。\
             年齢帯拡張 ({}) や近隣広域への採用範囲拡大が必要です。",
            target_label,
            target_pct,
            if is_hourly {
                "55-69 ベテラン層を含める / 学生層 18-24 を含める"
            } else {
                "45-54 層への展開"
            },
        )
    };

    let age_balance = if senior_pct >= 35.0 {
        " 高齢層 35%+ で <strong>人口構造は超高齢化</strong>。退職タイミングを見据えた中期的な人員計画 (3-5 年) が必要です。"
    } else if senior_pct >= 25.0 {
        " 高齢層 25%+ で全国平均並み。生産年齢層の絶対数を維持する施策 (定着 / 中途採用) を継続的に。"
    } else {
        " 高齢層比率が低く、生産年齢層が厚い <strong>採用に有利な構造</strong> です。"
    };

    let labor_note = match labor_force_rate {
        Some(v) if v >= 62.0 => format!(
            " 労働力率 {:.1}% は高水準で、既就業者の引き抜き競争が激しい可能性があります。",
            v
        ),
        Some(v) if v >= 55.0 => format!(" 労働力率 {:.1}% は標準的水準です。", v),
        Some(v) => format!(
            " 労働力率 {:.1}% は低めで、潜在労働力 (非労働力人口) のリーチ施策に余地があります。",
            v
        ),
        None => String::new(),
    };

    let _ = working_pct;
    format!("{}{}{}", pool_judge, age_balance, labor_note)
}
