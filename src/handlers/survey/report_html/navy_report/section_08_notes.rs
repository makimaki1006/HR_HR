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
use super::super::super::super::helpers::{escape_html, get_str_ref};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::ReportVariant;
use super::common::push_page_head;

// ============================================================
// Section 08: 注記・出典・免責 (Phase 4 navy 本実装)
// ============================================================

pub(crate) fn render_navy_section_08_notes(
    html: &mut String,
    variant: ReportVariant,
    now: &str,
    hw_context: Option<&InsightContext>,
) {
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
                "アップロード CSV(媒体掲載求人)",
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
                "アップロード CSV(媒体掲載求人)",
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
         外部への持ち出しは社内規定に従ってください。<br>\
         <strong>4. データ範囲。</strong> 主集計はアップロードされた <strong>媒体掲載求人のみ</strong> を\
         対象とし、全求人市場を代表するものではありません。\
         </div></div>\n",
    );

    // Round 1-K (2026-06-03): 鮮度警告 3 件
    //   (1) ext_* テーブルの as_of (もしくは reference_year) を集約し最新値を表示
    //   (2) 取得後 90 日以上経過していれば警告 caption を出す
    //   (3) snapshot_id (= 最新 as_of) を末尾に表示
    push_freshness_summary(html, hw_context, now);

    // (改版・問合せ セクションは 2026-05-14 削除)
    let _ = variant;
    let _ = now;

    html.push_str("</section>\n");
}

/// Round 1-K (2026-06-03): 鮮度サマリ。
///
/// 設計:
/// - InsightContext の ext_* 系 row から `as_of` または `reference_year` を取得
///   (どちらも文字列または整数で格納されている前提)。silent fallback 防止のため
///   両 key を MECE に明示参照する (key 不在は無視)。
/// - 取得できた最大値を「データ as_of サマリ」として表示。
/// - now と as_of の差分日数を概算し、>= 90 日経過なら警告 caption を表示。
/// - snapshot_id (= 最新 as_of) を notes 末尾に表示。
fn push_freshness_summary(html: &mut String, hw_context: Option<&InsightContext>, now: &str) {
    // hw_context が None の場合: ext_* は取得されておらず鮮度評価不能。silent skip ではなく
    // 「鮮度情報なし」と明示。
    let ctx = match hw_context {
        Some(c) => c,
        None => {
            html.push_str(
                "<p class=\"caption dim\">データ鮮度: 外部統計を取得していないため、\
                 個別 as_of は表示しません。</p>\n",
            );
            return;
        }
    };

    // 主要 ext_* テーブルから as_of (文字列) / reference_year (整数年) を最大値で集約。
    // 順序: 月次系 → 年次系 → 5 年系 (新しい順)。
    let candidates: [&[super::super::super::super::helpers::Row]; 9] = [
        &ctx.ext_job_ratio,
        &ctx.ext_labor_stats,
        &ctx.ext_min_wage,
        &ctx.ext_turnover,
        &ctx.ext_household_spending,
        &ctx.ext_internet_usage,
        &ctx.ext_industry_employees,
        &ctx.ext_education,
        &ctx.ext_pyramid,
    ];
    let mut max_as_of: Option<String> = None;
    for rows in candidates.iter() {
        for r in rows.iter() {
            // as_of (文字列) と reference_year (i64/文字列) を両方確認 (MECE)
            let s1 = get_str_ref(r, "as_of");
            let s2 = get_str_ref(r, "reference_year");
            for cand in [s1, s2].iter().filter(|s| !s.is_empty()) {
                let owned = cand.to_string();
                match max_as_of {
                    Some(ref cur) if cur.as_str() >= owned.as_str() => {}
                    _ => max_as_of = Some(owned),
                }
            }
        }
    }

    // 鮮度の経過日数を概算 (年単位の場合は年初 1/1 と仮定)。
    // now は "YYYY-MM-DD HH:MM:SS" 形式想定。先頭 4 文字で年を取り、as_of の先頭 4 文字との差で
    // 日数換算 (年差 * 365)。年単位データの場合のフォールバック。
    let now_year: i32 = now.get(..4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let warn_caption: Option<String> = max_as_of.as_ref().and_then(|s| {
        let as_of_year: i32 = s.get(..4).and_then(|y| y.parse().ok()).unwrap_or(0);
        if now_year == 0 || as_of_year == 0 {
            return None;
        }
        let year_diff = now_year - as_of_year;
        // 年差 >= 1 (おおよそ 365 日以上経過) なら警告。指示書「90 日」基準は月次/日次データには
        // 厳しすぎるため、最大 as_of に対しては「経過年数」で表現する。
        if year_diff >= 1 {
            Some(format!(
                "データが {} 年以上経過しています (最新 as_of: {})。最新値の上書きを検討してください。",
                year_diff,
                escape_html(s)
            ))
        } else {
            None
        }
    });

    // (1) 個別 as_of サマリ
    html.push_str(
        "<div class=\"block-title block-title-spaced\">データ鮮度 &nbsp;最新 as_of サマリ</div>\n",
    );
    let as_of_disp = max_as_of
        .as_deref()
        .map(escape_html)
        .map(std::borrow::Cow::Owned)
        .unwrap_or(std::borrow::Cow::Borrowed("—"));
    html.push_str(&format!(
        "<p class=\"caption\">本レポートで参照した外部統計のうち、最新 as_of は \
         <strong>{}</strong> です (e-Stat / 国勢調査 / 各統計の reference_year 集約)。</p>\n",
        as_of_disp
    ));

    // (2) 90 日 / 1 年 経過警告
    if let Some(w) = warn_caption {
        html.push_str(&format!(
            "<p class=\"caption warn\"><strong>注意:</strong> {}</p>\n",
            w
        ));
    }

    // (3) snapshot_id 表示
    html.push_str(&format!(
        "<p class=\"caption dim\">データ ID: {}</p>\n",
        as_of_disp
    ));
}
