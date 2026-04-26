//! 分割: report_html/notes.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;

/// スコープ制約、相関≠因果、データ限界を明示
/// 記載項目の文言は仕様書 4.12 に沿うこと（変更不可）
pub(super) fn render_section_notes(html: &mut String, now: &str) {
    html.push_str("<section class=\"section\" role=\"region\" aria-labelledby=\"notes-title\">\n");
    html.push_str("<h2 id=\"notes-title\">注記・出典・免責</h2>\n");
    html.push_str(
        "<ol style=\"padding-left:1.4em;font-size:10pt;line-height:1.6;color:var(--text);\">\n",
    );
    html.push_str(
        "<li><strong>データスコープ</strong>: 本レポートはアップロード CSV（Indeed / 求人ボックス等）\
        の行に基づく分析が主で、HW 掲載データは比較参考値として併記している。\
        CSV はスクレイピング範囲に依存し、HW は掲載求人のみに限定されるため、\
        いずれも全求人市場を代表するものではない。\
        職業紹介事業者の求人・非公開求人は本レポートに含まれない。</li>\n",
    );
    html.push_str(
        "<li><strong>給与バイアス</strong>: HW 掲載求人は中小企業・地方案件の比率が高く民間媒体より\
        給与水準が低く出る傾向がある。CSV 側も掲載元媒体のバイアスを内包するため、\
        両者の単純比較には注意が必要。</li>\n",
    );
    html.push_str(
        "<li><strong>相関と因果</strong>: 本レポートに記載する「傾向」「相関」は因果関係を\
        証明するものではない。示唆は仮説であり、実施判断は現場文脈に依存する。</li>\n",
    );
    html.push_str(
        "<li><strong>外れ値処理</strong>: 給与統計（中央値・平均・グループ別集計）は IQR 法\
        （Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR の範囲外を除外）を適用済。\
        雇用形態グループ別集計も各グループ内で同手法の除外を実行。\
        除外件数は Executive Summary および各カード内に明示表示。</li>\n",
    );
    html.push_str(
        "<li><strong>サンプル件数と求人件数</strong>: 本レポートの「サンプル件数」は分析対象求人数で\
        あり、地域全体の求人件数ではない。</li>\n",
    );
    html.push_str(
        "<li><strong>出典</strong>: データ源 - アップロード CSV / ハローワーク公開データ / \
        地域注目企業データベース / e-Stat。</li>\n",
    );
    html.push_str(&format!(
        "<li><strong>生成元</strong>: 株式会社For A-career / 生成日時: {}</li>\n",
        escape_html(now)
    ));
    html.push_str("</ol>\n");
    html.push_str("</section>\n");
}
