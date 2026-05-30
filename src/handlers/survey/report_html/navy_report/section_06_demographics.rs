//! Section 06 - 人材デモグラフィック (Phase 3 navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 7 / β Section Team / 2026-05-30) で抽出。
//!
//! 元 `navy_report/mod.rs` L173-L1176 の以下を物理コピー:
//! - `render_navy_section_06_demographics`        (公開 API: pub(crate))
//! - `render_navy_section_06_posting_target`      (private helper、本ファイル内のみで使用)
//! - `build_distribution_table`                   (private helper)
//! - `build_hourly_band_distribution`             (pub(crate) — report_html 外
//!                                                  `hourly_report_qa_test.rs` から
//!                                                  `super::navy_report::build_hourly_band_distribution`
//!                                                  で参照されているため再エクスポート必須)
//! - `age_lo`                                     (private helper)
//! - `age_sort_key`                               (private helper)
//! - `build_navy_pyramid_svg`                     (private helper)
//! - `build_navy_pyramid_svg_mini`                (private helper)
//! - `build_demographics_so_what`                 (private helper)
//! - 定数 `HOURLY_BAND_BOUNDARIES` (build_hourly_band_distribution 用、module-private)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_06_demographics`
//!   (Commit 2/3/4/5/6 パターン踏襲: `pub(super)` は階層不足で E0364 になるため `pub(crate)`)
//! - `pub(crate) fn build_hourly_band_distribution`
//!   (`hourly_report_qa_test.rs` が `super::navy_report::build_hourly_band_distribution`
//!   path で参照しており、`navy_report/mod.rs` 側で `pub(super) use` 再エクスポート
//!   できる必要があるため `pub(crate)` に昇格)
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
use super::common::{push_kpi, push_page_head, push_region_scope_banner, safe_pct};

// ============================================================
// Section 06: 人材デモグラフィック (Phase 3 navy 本実装)
// ============================================================

/// Phase 2-A (2026-05-29): `agg` 引数追加。
///   `agg.is_hourly` を Section 06 内の `render_navy_section_06_posting_target` 呼出に
///   伝播するためだけに使用。デモグラフィック自体には is_hourly 依存はない。
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
            format!("最新値で <strong>自然減 {} 名</strong> (死亡 > 出生)。中長期 (5-10 年) で<strong>労働力供給の構造的縮小</strong>が見込まれ、自動化投資・省人化施策の並走を推奨。",
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

    // -- 図 6-3 求人ターゲット プロファイル (求人側集計) [P2-3 (2026-05-28) 追加]
    //
    //   背景: hellowork.db に求職者個人テーブルが存在しないため、postings (HW 求人) 側の
    //   募集対象条件 (年齢制限 / 給与レンジ / 経験 / 雇用形態) を集計して
    //   「求人側から見たターゲット プロファイル」として提示する。
    //
    //   出典明記: 「HW 求人 (postings) の募集条件集計」
    //   人数推定は行わず、求人件数のみを集計 (DISPLAY_SPEC v1.0 §2 / Hard NG 用語不使用)。
    //   ctx.posting_target == None または total_postings == 0 の場合は本ブロックを skip。
    if let Some(pt) = ctx.posting_target.as_ref().filter(|p| p.total_postings > 0) {
        // Phase 2-B (2026-05-29): agg を追加 — H4 表 6-J で salary_min_values_native を使うため。
        render_navy_section_06_posting_target(html, pt, is_hourly, agg);
    }

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

/// P2-3 (2026-05-28) 図 6-3: 求人ターゲット プロファイル (求人側集計) の描画。
///
/// 注意:
/// - 本関数が扱うのは **求人件数** のみ。「求職者人数」「ターゲット人数」「想定人数」
///   「推定人数」「母集団人数」等の禁止語句 (DISPLAY_SPEC v1.0 §2 / Hard NG) を使わない。
/// - 各分布の caption に「出典: HW 求人 (postings) の募集条件集計」を明記する。
/// - 構成比は分布内の sum を分母にして算出 (0 件分布が混在しても合計 100%)。
///
/// Phase 2-A (2026-05-29): `is_hourly` 引数追加。給与レンジの bucket 表記と
/// salary_type フィルタの注記を時給/月給で切替える。
///
/// Phase 2-B (2026-05-29): `agg` 引数追加。表 6-J (H4: 時給帯別 求人件数) で
/// `agg.salary_min_values_native` を 100 円刻みで集計するため使用。
/// 時給モード (is_hourly == true) でのみ表 6-J を出力する。
pub(crate) fn render_navy_section_06_posting_target(
    html: &mut String,
    pt: &super::super::super::super::analysis::fetch::PostingTargetProfile,
    is_hourly: bool,
    agg: &SurveyAggregation,
) {
    html.push_str(
        "<div class=\"block-title block-title-spaced\">\
         図 6-3 &nbsp;求人ターゲット プロファイル (求人側集計)\
         </div>\n",
    );
    html.push_str(
        "<p class=\"caption\">本ブロックは <strong>HW 求人 (postings) の募集条件</strong> を集計した\
         <strong>求人件数</strong> ベースの分布です。求職者個人データではなく、\
         募集側がどの層を想定しているかの傾向を示します。</p>\n",
    );

    // ---- KPI: 総求人件数 / 年齢制限主要層 / 給与中央レンジ / 雇用形態主流
    //
    // R2-P1-6 (ultrathink Round 2, 2026-05-28): `max_by_key` は同値ペアで
    // last-wins の挙動を取る。distribution の **全カウントが 0** の場合
    // (例: salary_type が「月給」の求人が 1 件もない地域) 、最後のラベル
    // (例: 「〜20万」) を選んでしまい KPI に誤表示される。
    // → max_by_key の戻り値が count == 0 の場合は「—」に明示的に置換する。
    let take_top_or_dash = |pair_opt: Option<(String, i64)>| -> (String, i64) {
        match pair_opt {
            Some((l, c)) if c > 0 => (l, c),
            _ => ("—".to_string(), 0),
        }
    };
    // 主要年齢層 = age_range_distribution の最多バケット (件数降順 1 位)
    let top_age = take_top_or_dash(
        pt.age_range_distribution
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(l, c)| (l.clone(), *c)),
    );
    // 主要給与レンジ = salary_target_distribution の最多バケット
    let top_salary = take_top_or_dash(
        pt.salary_target_distribution
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(l, c)| (l.clone(), *c)),
    );
    // 主流雇用形態 = employment_type_distribution の最多バケット (既に降順 sort 済)
    // R2-P1-6: first() でも count==0 ガードを適用 (employment_type も全 0 の可能性あり)
    let top_emp = take_top_or_dash(
        pt.employment_type_distribution
            .first()
            .map(|(l, c)| (l.clone(), *c)),
    );
    // 経験不問 (実質) の比率
    let total_exp: i64 = pt
        .experience_required_distribution
        .iter()
        .map(|(_, c)| *c)
        .sum();
    let unspec_count: i64 = pt
        .experience_required_distribution
        .iter()
        .find(|(l, _)| l == "経験不問 (実質)")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    // R2-P1-1 (ultrathink Round 2, 2026-05-28): total_exp > 0 ガード後でも
    // 浮動小数誤差で 100% 超えになる可能性をクランプで防御。
    let unspec_pct = if total_exp > 0 {
        safe_pct(unspec_count as f64 / total_exp as f64 * 100.0)
    } else {
        0.0
    };

    html.push_str("<div class=\"kpi-row\">\n");
    push_kpi(
        html,
        "集計求人件数",
        &format_number(pt.total_postings),
        "件",
        "neu",
        "HW postings (pref/muni 一致)",
        true,
    );
    push_kpi(
        html,
        "年齢制限 主要層",
        &top_age.0,
        "",
        "neu",
        &format!("{} 件", format_number(top_age.1)),
        false,
    );
    push_kpi(
        html,
        "給与 主要レンジ",
        &top_salary.0,
        "",
        "neu",
        // Phase 2-A: 給与記載 (salary_type) を is_hourly で切替
        &format!(
            "{} 件 ({}記載のみ)",
            format_number(top_salary.1),
            if is_hourly { "時給" } else { "月給" }
        ),
        false,
    );
    push_kpi(
        html,
        "経験不問 比率",
        &format!("{:.1}", unspec_pct),
        "%",
        if unspec_pct >= 70.0 {
            "pos"
        } else if unspec_pct >= 40.0 {
            "neu"
        } else {
            "warn"
        },
        "experience_required 未記載求人",
        false,
    );
    push_kpi(
        html,
        "雇用形態 主流",
        &top_emp.0,
        "",
        "neu",
        &format!("{} 件", format_number(top_emp.1)),
        false,
    );
    html.push_str("</div>\n");

    // ---- 表 6-G: 年齢制限 × 求人件数
    html.push_str(
        "<div class=\"block-title block-title-spaced\">\
         表 6-G &nbsp;年齢制限別 求人件数 (求人側集計)\
         </div>\n",
    );
    html.push_str(&build_distribution_table(
        &pt.age_range_distribution,
        "年齢制限ラベル",
    ));
    html.push_str(
        "<p class=\"caption\">出典: HW 求人 (postings) の age_min / age_max 列を集計。\
         「制限なし」は両方 NULL の求人。年齢制限は雇用対策法上の例外 \
         (試用期間/技能継承/特定職種) を含む可能性があります。</p>\n",
    );

    // ---- 表 6-H: 給与レンジ × 求人件数 (Phase 2-A: is_hourly でラベル/注記切替)
    let salary_table_title = if is_hourly {
        "表 6-H &nbsp;給与レンジ別 求人件数 (時給記載のみ)"
    } else {
        "表 6-H &nbsp;給与レンジ別 求人件数 (月給記載のみ)"
    };
    let salary_label_header = if is_hourly {
        "時給レンジ"
    } else {
        "月給レンジ"
    };
    let salary_caption = if is_hourly {
        "<p class=\"caption\">出典: HW 求人 (postings) の salary_min 列を集計 (時給帯)。\
         salary_type が「時給」かつ salary_min &gt; 0 の求人のみが母集団 \
         (月給・年俸はここでは除外)。本表の件数合計は KPI「集計求人件数」より少なくなります。</p>\n"
    } else {
        "<p class=\"caption\">出典: HW 求人 (postings) の salary_min 列を月給換算なしで集計。\
         salary_type が「月給」かつ salary_min &gt; 0 の求人のみが母集団 \
         (時給・年俸はここでは除外)。本表の件数合計は KPI「集計求人件数」より少なくなります。</p>\n"
    };
    html.push_str(&format!(
        "<div class=\"block-title block-title-spaced\">{}</div>\n",
        salary_table_title
    ));
    html.push_str(&build_distribution_table(
        &pt.salary_target_distribution,
        salary_label_header,
    ));
    html.push_str(salary_caption);

    // ---- 表 6-J: 時給帯別 求人件数 (Phase 2-B H4, 2026-05-29)
    //   時給モードのみ表示。agg.salary_min_values_native を 100 円刻みで bucket 化。
    //   表 6-H (salary_target_distribution: HW postings 月給 salary_min の bucket) との違い:
    //     - 表 6-H は HW postings の salary_min を単一値で月給 bucket 化
    //     - 表 6-J は CSV (媒体分析側) の時給ネイティブ値で 100 円刻みの価格弾力性を見る
    //   silent fallback 防止: is_hourly == false の月給モードでは完全に省略。
    if is_hourly {
        let distribution = build_hourly_band_distribution(&agg.salary_min_values_native);
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 6-J &nbsp;時給帯別 求人件数 (100円刻み)</div>\n",
        );
        html.push_str(&build_distribution_table(&distribution, "時給帯"));
        html.push_str(
            "<p class=\"caption\">出典: CSV 集計 (下限給与ネイティブ円/時)。\
             100 円刻みの求人件数分布。\
             <strong>表 6-H との違い:</strong> 表 6-H は salary_min 単一値の bucket、\
             本表は時給市場の価格弾力性を見る (100円帯ごとの厚みで競合密度を把握)。</p>\n",
        );
    }

    // ---- 表 6-I: 雇用形態 × 求人件数
    html.push_str(
        "<div class=\"block-title block-title-spaced\">\
         表 6-I &nbsp;雇用形態別 求人件数\
         </div>\n",
    );
    html.push_str(&build_distribution_table(
        &pt.employment_type_distribution,
        "雇用形態",
    ));
    html.push_str(
        "<p class=\"caption\">出典: HW 求人 (postings) の employment_type 列を集計 (件数降順)。\
         「未記載」は元データが空文字または NULL の求人。</p>\n",
    );
}

/// 分布 `(label, count)` のリストから 3 列表 (ラベル / 件数 / 構成比 %) を生成する共通ビルダ。
///
/// # 引数
/// - `distribution`: `(ラベル, 件数)` のリスト。
///   - **順序は呼出側の責任**。本関数では並べ替えない (年齢/給与は表示順固定、雇用形態は降順、
///     経験 2 値は固定順を維持するため)。
///   - ラベルは生 String を受け、`escape_html` で安全化される (`<script>` 等の混入を防ぐ)。
///   - 件数は i64。負値は理論上発生しないが、合計計算では負値も含めて算術する
///     (異常データ検出を呼出側に委ねる設計)。
/// - `label_header`: 1 列目の `<th scope="col">` 内容。例: "年齢制限ラベル" / "月給レンジ" / "雇用形態"。
///
/// # 戻り値
/// HTML 表全体 (`<table class="table-navy">...</table>`)。
/// 空 `distribution` または件数合計 `total == 0` のときは「該当データなし」を 1 行表示。
///
/// # 不変条件 (テストで検証)
/// - 構成比合計 ≈ 100% (各行は `count / total * 100`、浮動誤差は `safe_pct` で [0, 100] にクランプ)
/// - 各 `<th>` に `scope="col"` 付与 (a11y / Round 2 P1-4 で導入)
/// - `<th>` / `<td>` 内のラベルは必ず `escape_html` を通す (XSS 防御)
/// - 空入力時の "該当データなし" 行も `<tbody>` 内 (構造保証)
///
/// # silent fallback 監査
/// - 件数合計 0 は明示的に `<td colspan="3">該当データなし</td>` で表示 (空文字列を返さない)
/// - `total > 0` ガード後に除算するため zero-div 不可
pub(crate) fn build_distribution_table(
    distribution: &[(String, i64)],
    label_header: &str,
) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    // R2-P1-4 (ultrathink Round 2, 2026-05-28): a11y のため列ヘッダに scope="col" を付与。
    s.push_str(&format!(
        "<th scope=\"col\">{}</th><th scope=\"col\" class=\"num\">求人件数</th><th scope=\"col\" class=\"num\">構成比 (%)</th>",
        escape_html(label_header)
    ));
    s.push_str("</tr></thead>\n<tbody>\n");

    let total: i64 = distribution.iter().map(|(_, c)| *c).sum();
    if distribution.is_empty() || total == 0 {
        s.push_str(
            "<tr><td colspan=\"3\" class=\"dim\">該当データなし</td></tr>\n\
             </tbody></table>\n",
        );
        return s;
    }

    for (label, count) in distribution {
        // R2-P1-1 (ultrathink Round 2, 2026-05-28): total > 0 ガード済だが
        // 浮動小数誤差を safe_pct で [0, 100] にクランプ。
        let pct = safe_pct(*count as f64 / total as f64 * 100.0);
        s.push_str(&format!(
            "<tr><td>{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{:.1}</td></tr>\n",
            escape_html(label),
            format_number(*count),
            pct
        ));
    }
    s.push_str("</tbody></table>\n");
    s
}

// ============================================================
// Phase 2-B (2026-05-29): 時給モード H4 — 時給帯別 求人件数分布
// ============================================================
//
// 仕様:
//   - 100 円刻みで bucket 化: <900 / 900-1000 / 1000-1100 / 1100-1200 / 1200-1300 /
//                              1300-1400 / 1400-1500 / 1500-1600 / 1600-1700 /
//                              1700-1800 / 1800-1900 / 1900-2000 / 2000+
//     合計 13 段
//   - 各 bucket: (ラベル, 件数) のペアを順序保持で返す
//
// 不変条件 (テストで検証):
//   - bucket 合計 == values.iter().filter(>0).count()
//   - 単一値 [1200, 1200, 1200] → "1200-1300円" bucket に 3 件
//   - 境界値 1000 → "1000-1100円" (lo 包含、hi 排他)
//   - empty → 全 bucket 0 件のリスト (build_distribution_table 側で total==0 のとき「該当データなし」)
const HOURLY_BAND_BOUNDARIES: [(i64, i64, &str); 13] = [
    (0, 900, "<900円"),
    (900, 1000, "900-1000円"),
    (1000, 1100, "1000-1100円"),
    (1100, 1200, "1100-1200円"),
    (1200, 1300, "1200-1300円"),
    (1300, 1400, "1300-1400円"),
    (1400, 1500, "1400-1500円"),
    (1500, 1600, "1500-1600円"),
    (1600, 1700, "1600-1700円"),
    (1700, 1800, "1700-1800円"),
    (1800, 1900, "1800-1900円"),
    (1900, 2000, "1900-2000円"),
    (2000, i64::MAX, "2000円+"),
];

/// 時給値リストを 100 円刻みの bucket 分布 `(ラベル, 件数)` に変換。
///
/// # 引数
/// - `values`: 時給ネイティブ値 (円/時)。<= 0 は除外。
///
/// # 戻り値
/// `(ラベル, 件数)` のリスト。順序は HOURLY_BAND_BOUNDARIES の宣言順 (昇順)。
/// 全 bucket を返す (count==0 のものも含む) → build_distribution_table 側で
/// total==0 のときのみ「該当データなし」を表示するため、空 Vec は返さない。
///
/// # 不変条件
/// - 戻り値 .len() == HOURLY_BAND_BOUNDARIES.len() (= 13)
/// - sum(counts) == values.iter().filter(|v| **v > 0).count()
pub(crate) fn build_hourly_band_distribution(values: &[i64]) -> Vec<(String, i64)> {
    let mut counts: Vec<i64> = vec![0; HOURLY_BAND_BOUNDARIES.len()];
    for v in values.iter().copied().filter(|x| *x > 0) {
        for (i, (lo, hi, _)) in HOURLY_BAND_BOUNDARIES.iter().enumerate() {
            // [lo, hi) 判定。最後の "2000円+" は hi = i64::MAX のため上限なし。
            if v >= *lo && v < *hi {
                counts[i] += 1;
                break;
            }
        }
    }
    HOURLY_BAND_BOUNDARIES
        .iter()
        .zip(counts.iter())
        .map(|((_, _, label), c)| (label.to_string(), *c))
        .collect()
}

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
