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
use super::super::super::super::helpers::{escape_html, get_i64_opt, get_str_ref};
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
                "年間休日 (求人説明文抽出)",
                "アップロード CSV (自由記述解析)",
                "Section 07.5 年間休日 × 給与",
                "都度",
            ),
            (
                "Indeed (SP) 人気タグ",
                "アップロード CSV (Indeed SP 表示列)",
                "Section 07.6 表示優先度シグナル",
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
                "SSDSE-A 地理指標",
                "SSDSE-A (v2_external_geography)",
                "Section 02 地理規模 (可住地面積 / 人口密度)",
                "年次",
            ),
            (
                "事業所統計",
                "総務省 (v2_external_establishments)",
                "Section 04 採用競合規模",
                "年次",
            ),
            (
                "開廃業動態",
                "総務省 (v2_external_business_dynamics)",
                "Section 04 市場成長性",
                "年次",
            ),
            (
                "社会人口統計体系",
                "e-Stat (v2_external_labor_stats)",
                "Section 06 就業構造内訳",
                "年次",
            ),
            (
                "住民基本台帳 人口移動報告",
                "総務省 (v2_external_migration)",
                "Section 06 人口移動",
                "年次",
            ),
            (
                "人口動態統計",
                "厚生労働省 (v2_external_vital_statistics)",
                "Section 06 自然増減",
                "年次",
            ),
            (
                "国勢調査 昼夜間人口",
                "e-Stat (v2_external_daytime_population)",
                "Section 07 昼夜間人口比",
                "5 年",
            ),
            (
                "国勢調査 世帯集計",
                "e-Stat (v2_external_households)",
                "Section 07 世帯構成",
                "5 年",
            ),
            (
                "住宅・土地統計調査",
                "総務省 e-Stat (v2_external_rental_housing)",
                "Section 07 家賃水準",
                "5 年",
            ),
            (
                "医療・福祉施設",
                "厚生労働省 (v2_external_medical_welfare)",
                "Section 07 施設密度",
                "年次",
            ),
            (
                "社会生活統計",
                "SSDSE (v2_external_social_life)",
                "Section 07 施設・参加率",
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
                "年間休日 (求人説明文抽出)",
                "アップロード CSV (自由記述解析)",
                "Section 07.5 年間休日 × 給与",
                "都度",
            ),
            (
                "Indeed (SP) 人気タグ",
                "アップロード CSV (Indeed SP 表示列)",
                "Section 07.6 表示優先度シグナル",
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
                "SSDSE-A 地理指標",
                "SSDSE-A (v2_external_geography)",
                "Section 02 地理規模 (可住地面積 / 人口密度)",
                "年次",
            ),
            (
                "事業所統計",
                "総務省 (v2_external_establishments)",
                "Section 04 採用競合規模",
                "年次",
            ),
            (
                "開廃業動態",
                "総務省 (v2_external_business_dynamics)",
                "Section 04 市場成長性",
                "年次",
            ),
            (
                "社会人口統計体系",
                "e-Stat (v2_external_labor_stats)",
                "Section 06 就業構造内訳",
                "年次",
            ),
            (
                "住民基本台帳 人口移動報告",
                "総務省 (v2_external_migration)",
                "Section 06 人口移動",
                "年次",
            ),
            (
                "人口動態統計",
                "厚生労働省 (v2_external_vital_statistics)",
                "Section 06 自然増減",
                "年次",
            ),
            (
                "国勢調査 昼夜間人口",
                "e-Stat (v2_external_daytime_population)",
                "Section 07 昼夜間人口比",
                "5 年",
            ),
            (
                "国勢調査 世帯集計",
                "e-Stat (v2_external_households)",
                "Section 07 世帯構成",
                "5 年",
            ),
            (
                "住宅・土地統計調査",
                "総務省 e-Stat (v2_external_rental_housing)",
                "Section 07 家賃水準",
                "5 年",
            ),
            (
                "医療・福祉施設",
                "厚生労働省 (v2_external_medical_welfare)",
                "Section 07 施設密度",
                "年次",
            ),
            (
                "社会生活統計",
                "SSDSE (v2_external_social_life)",
                "Section 07 施設・参加率",
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
/// 設計 (2026-07-03 修正):
/// - InsightContext の ext_* 系 row から基準年 (INTEGER) を取得し最大値を集約。
/// - 実カラムは各テーブルで異なる (grep 確認):
///     * `fiscal_year`     … v2_external_job_openings_ratio / labor_stats(賃金構造) /
///                            minimum_wage_history / turnover (trend/fetch.rs:227-306)
///     * `reference_year`  … v2_external_household_spending (subtab5_phase4.rs:567)
///     * `year`            … v2_external_internet_usage (subtab5_phase4_7.rs:186)
///     * `survey_year`     … 一部ライフ系 (MECE 予備)
///   旧実装は `as_of` (全テーブルに不在) と `reference_year` を **文字列限定の
///   `get_str_ref`** で読んでいたため、INTEGER 年値を拾えず常に空欄 (as_of=— /
///   データ ID=—) になっていた。§07 と同様に `get_i64_opt` で数値対応させる。
/// - 年値をまたぐ日付文字列カラム (`as_of` / `reference_date`) は現行 9 候補には
///   存在しないが、将来追加に備え先頭 4 桁を年として MECE に併読する。
/// - 集約できた最大年を「最新 as_of」欄に表示 (例: 2024)。
/// - now の年と最大年の差が >= 1 年なら警告 caption を表示。
/// - データ ID (= 最新基準年) を notes 末尾に表示。
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

    // 主要 ext_* テーブルから基準年 (INTEGER) を最大値で集約。
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
    // 数値年カラム (テーブルごとに列名が異なるため MECE に全キー探索)。
    const YEAR_KEYS: [&str; 4] = ["fiscal_year", "reference_year", "year", "survey_year"];
    // 将来的な日付文字列カラム (現行 9 候補には不在だが先頭 4 桁を年として併読)。
    const DATE_KEYS: [&str; 2] = ["as_of", "reference_date"];

    let mut max_year: Option<i64> = None;
    let mut consider = |y: i64| {
        // 妥当な西暦のみ採用 (ヘッダー残骸や 0 を弾く)。
        if (1900..=2100).contains(&y) {
            max_year = Some(match max_year {
                Some(cur) if cur >= y => cur,
                _ => y,
            });
        }
    };
    for rows in candidates.iter() {
        for r in rows.iter() {
            for key in YEAR_KEYS.iter() {
                if let Some(y) = get_i64_opt(r, key) {
                    consider(y);
                }
            }
            for key in DATE_KEYS.iter() {
                let s = get_str_ref(r, key);
                if let Some(y) = s.get(..4).and_then(|p| p.parse::<i64>().ok()) {
                    consider(y);
                }
            }
        }
    }

    // now は "YYYY-MM-DD HH:MM:SS" 形式想定。先頭 4 文字で年を取得。
    let now_year: i64 = now.get(..4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let warn_caption: Option<String> = max_year.and_then(|as_of_year| {
        if now_year == 0 {
            return None;
        }
        let year_diff = now_year - as_of_year;
        // 年差 >= 1 なら警告。月次/日次データには「90 日」基準は厳しすぎるため、
        // 最大基準年に対しては「経過年数」で表現する。
        if year_diff >= 1 {
            Some(format!(
                "最新の基準年が {} 年で、現時点から {} 年以上経過しています。最新値の上書きを検討してください。",
                as_of_year, year_diff
            ))
        } else {
            None
        }
    });

    // (1) 最新基準年サマリ
    html.push_str(
        "<div class=\"block-title block-title-spaced\">データ鮮度 &nbsp;最新 as_of サマリ</div>\n",
    );
    let as_of_disp: std::borrow::Cow<'static, str> = match max_year {
        Some(y) => std::borrow::Cow::Owned(format!("{} 年", y)),
        None => std::borrow::Cow::Borrowed("—"),
    };
    html.push_str(&format!(
        "<p class=\"caption\">本レポートで参照した外部統計のうち、最新の基準年は \
         <strong>{}</strong> です (e-Stat / 国勢調査 / 各統計の fiscal_year・reference_year・year 集約)。</p>\n",
        as_of_disp
    ));

    // (2) 経過年数 警告
    if let Some(w) = warn_caption {
        html.push_str(&format!(
            "<p class=\"caption warn\"><strong>注意:</strong> {}</p>\n",
            escape_html(&w)
        ));
    }

    // (3) データ ID (= 最新基準年) 表示
    html.push_str(&format!(
        "<p class=\"caption dim\">データ ID: {}</p>\n",
        as_of_disp
    ));
}
