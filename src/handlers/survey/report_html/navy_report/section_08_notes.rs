//! Section 08 - 注記・出典・免責 (Phase 4 navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 2 / 2026-05-29) で抽出。
//!
//! 元 `navy_report/mod.rs` L6010-L6122 の `render_navy_section_08_notes` を
//! 物理コピー。API 表面 (`pub(super) fn render_navy_section_08_notes`) は不変。
//!
//! `push_page_head` は `super::common::*;` 経由で参照 (mod.rs 側で
//! `pub(super) use common::*;` 再エクスポート済み)。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_08_notes):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::escape_html;
use super::super::ReportVariant;
use super::common::push_page_head;

// ============================================================
// Section 08: 注記・出典・免責 (Phase 4 navy 本実装)
// ============================================================

pub(crate) fn render_navy_section_08_notes(html: &mut String, variant: ReportVariant, now: &str) {
    let show_hw = matches!(variant, ReportVariant::Full);

    html.push_str("<section class=\"page-navy navy-notes\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 08",
        "注記・出典・免責",
        "データソース / 集計定義 / 免責事項",
    );

    // -- 冒頭の lede (堅実な 1 段落)
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>本レポートで使用したデータソース、集計定義、および解釈上の前提を以下に明示します。\
         数値は <strong>{}</strong> 時点で取得可能な最新値を採用しており、その後の更新により\
         実態と乖離する可能性があります。施策判断には現場文脈・最新の一次情報を併用してください。</p>\
         </div>\n",
        escape_html(now)
    ));

    // -- 表 8-A データソース一覧
    html.push_str("<div class=\"block-title\">表 8-A &nbsp;データソース一覧</div>\n");
    html.push_str(
        "<table class=\"table-navy\">\n<thead><tr>\
        <th>No.</th><th>名称</th><th>出典</th><th>用途</th><th>更新頻度</th>\
        </tr></thead>\n<tbody>\n",
    );
    let sources: Vec<(&str, &str, &str, &str)> = if show_hw {
        vec![
            (
                "アップロード CSV",
                "ユーザー提供",
                "全 Section の主集計対象",
                "都度",
            ),
            (
                "求人媒体ローカル DB",
                "求人媒体 (postings テーブル)",
                "Section 02 媒体掲載数 / 推移",
                "日次更新",
            ),
            (
                "求人媒体時系列",
                "Turso v2_ts_*",
                "Section 02 3 ヶ月 / 1 年推移",
                "週次",
            ),
            (
                "有効求人倍率",
                "e-Stat (v2_external_job_openings_ratio)",
                "Section 04 採用難度",
                "月次",
            ),
            (
                "労働力調査 (失業率)",
                "e-Stat (v2_external_labor_force)",
                "Section 04 / 06 失業率",
                "月次",
            ),
            (
                "雇用動向調査 (離職率)",
                "e-Stat (v2_external_turnover)",
                "Section 04 離職率・入職率",
                "年次",
            ),
            (
                "国勢調査 産業構造",
                "e-Stat (v2_external_industry_structure)",
                "Section 05 産業大分類",
                "5 年",
            ),
            (
                "国勢調査 人口ピラミッド",
                "e-Stat (v2_external_population_pyramid)",
                "Section 06 人口構造",
                "5 年",
            ),
            (
                "国勢調査 OD",
                "e-Stat (v2_external_commute)",
                "Section 07 通勤圏",
                "5 年",
            ),
            (
                "学校基本調査",
                "文部科学省 (v2_external_education_facilities)",
                "Section 06 教育施設密度",
                "年次",
            ),
            (
                "地域別最低賃金",
                "厚生労働省 (v2_external_minimum_wage)",
                "Section 07 最低賃金推移",
                "年次 (10 月)",
            ),
            (
                "家計調査",
                "総務省 (v2_external_household_spending)",
                "Section 07 家計支出構成",
                "月次 / 年平均",
            ),
            (
                "通信利用動向調査",
                "総務省 (v2_external_internet_usage)",
                "Section 07 ネット利用率",
                "年次",
            ),
            (
                "地域企業データ",
                "地域企業データベース",
                "Section 05 法人セグメント",
                "都度同期",
            ),
        ]
    } else {
        vec![
            (
                "アップロード CSV",
                "ユーザー提供",
                "全 Section の主集計対象",
                "都度",
            ),
            (
                "有効求人倍率",
                "e-Stat (v2_external_job_openings_ratio)",
                "Section 04 採用難度",
                "月次",
            ),
            (
                "労働力調査 (失業率)",
                "e-Stat (v2_external_labor_force)",
                "Section 04 / 06 失業率",
                "月次",
            ),
            (
                "雇用動向調査 (離職率)",
                "e-Stat (v2_external_turnover)",
                "Section 04 離職率・入職率",
                "年次",
            ),
            (
                "国勢調査 産業構造",
                "e-Stat (v2_external_industry_structure)",
                "Section 05 産業大分類",
                "5 年",
            ),
            (
                "国勢調査 人口ピラミッド",
                "e-Stat (v2_external_population_pyramid)",
                "Section 06 人口構造",
                "5 年",
            ),
            (
                "国勢調査 OD",
                "e-Stat (v2_external_commute)",
                "Section 07 通勤圏",
                "5 年",
            ),
            (
                "学校基本調査",
                "文部科学省 (v2_external_education_facilities)",
                "Section 06 教育施設密度",
                "年次",
            ),
            (
                "地域別最低賃金",
                "厚生労働省 (v2_external_minimum_wage)",
                "Section 07 最低賃金推移",
                "年次 (10 月)",
            ),
            (
                "家計調査",
                "総務省 (v2_external_household_spending)",
                "Section 07 家計支出構成",
                "月次 / 年平均",
            ),
            (
                "通信利用動向調査",
                "総務省 (v2_external_internet_usage)",
                "Section 07 ネット利用率",
                "年次",
            ),
            (
                "地域企業データ",
                "地域企業データベース",
                "Section 05 法人セグメント",
                "都度同期",
            ),
        ]
    };
    for (i, (name, source, purpose, freq)) in sources.iter().enumerate() {
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        html.push_str(&format!(
            "<tr{}><td class=\"num bold\">{:02}</td><td><strong>{}</strong></td>\
             <td><span class=\"dim\">{}</span></td><td>{}</td><td><span class=\"dim\">{}</span></td></tr>\n",
            row_class,
            i + 1,
            escape_html(name),
            escape_html(source),
            escape_html(purpose),
            escape_html(freq)
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str("<p class=\"caption\">e-Stat = 政府統計の総合窓口 (https://www.e-stat.go.jp/)。各テーブルの取得 SQL とカラム定義は内部 docs を参照。</p>\n");

    // 2026-05-14 撤去 (ユーザー判断):
    //   表 8-B「主要 集計定義」を全撤去。
    //   - 「給与の月給換算」「給与解析率」等の内部運用定義はレポート受領側が
    //     関知すべき情報ではない (Section 03 等で必要な閾値は本文に統合済み)。

    // -- 免責事項 (so-what 風 navy 帯)
    // 2026-05-14:
    //   - 旧「2. データ範囲の制約 (CSV / ローカル DB)」を撤去。レポート受領側
    //     は CSV 経由であることを意識しない設計のため、内部前提を表に出さない。
    //   - 番号を 1〜3 に詰める。
    html.push_str("<div class=\"block-title block-title-spaced\">免責 &nbsp;解釈上の前提</div>\n");
    html.push_str(
        "<div class=\"so-what\" style=\"margin-top:4mm;\">\
         <div class=\"sw-label\">DISCLAIMER</div>\
         <div class=\"sw-body\">\
         <strong>1. 相関 ≠ 因果。</strong> 本レポートが示す指標間の関係は <strong>相関</strong> であり、\
         因果関係を証明するものではありません。施策実施判断は現場文脈と合わせて行ってください。<br>\
         <strong>2. 数値の鮮度。</strong> 公開統計の更新サイクル (5 年 / 年次 / 月次) を考慮し、\
         直近の事象とのタイムラグを認識してください。最低賃金は毎年 10 月発効、国勢調査は 5 年に一度。<br>\
         <strong>3. 取扱区分。</strong> 本資料は <strong>機密 / 社外秘</strong> として扱い、\
         外部への持ち出しは社内規定に従ってください。\
         </div></div>\n",
    );

    // (改版・問合せ セクションは 2026-05-14 削除)
    let _ = variant;
    let _ = now;

    html.push_str("</section>\n");
}
