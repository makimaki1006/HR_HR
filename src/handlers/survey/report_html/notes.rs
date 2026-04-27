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
///
/// UI-3 強化（2026-04-26）:
/// - カテゴリ別ボックスに整理（データソース / スコープ / 統計手法 / 相関≠因果 / 更新頻度）
/// - 各カテゴリにアイコン + 色分け
/// - 冒頭に「本レポートを正しく読むための前提」サマリ追加
/// - 既存の <ol> 番号付きリスト互換のため、既存テストで参照される文言は内部で保持
pub(super) fn render_section_notes(html: &mut String, now: &str) {
    html.push_str("<section class=\"section\" role=\"region\" aria-labelledby=\"notes-title\">\n");
    html.push_str("<h2 id=\"notes-title\">第6章 注記・出典・免責</h2>\n");

    // === 冒頭サマリ ===
    html.push_str(
        "<div class=\"report-notes-leadin\">\
         本レポートを正しく読むための前提: \
         記載される数値は <strong>HW 掲載求人＋アップロード CSV</strong> の範囲内であり、\
         非公開求人・職業紹介事業者経由・全求人市場を代表しません。\
         また「傾向」「相関」は <strong>因果関係を主張しない</strong> 観測です。</div>\n",
    );

    // === カテゴリ 1: データソース ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-data\">\n");
    html.push_str("<h3>データソース</h3>\n");
    html.push_str("<ul>\n");
    html.push_str("<li>アップロード CSV（媒体経由のデータ）</li>\n");
    html.push_str("<li>ハローワーク公開データ（hellowork.db / postings テーブル）</li>\n");
    html.push_str(
        "<li>外部企業 DB（地域注目企業データ由来、業種マッピングは industry_mapping を参照）</li>\n",
    );
    html.push_str("<li>e-Stat 政府統計（最低賃金・欠員補充率・人口統計）</li>\n");
    html.push_str("</ul>\n</div>\n");

    // === カテゴリ 2: スコープ制約 ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-scope\">\n");
    html.push_str("<h3>\u{26A0}\u{FE0F} スコープ制約</h3>\n"); // 警告アイコンは機能的に残す
    html.push_str("<ul>\n");
    html.push_str(
        "<li><strong>データスコープ</strong>: 本レポートはアップロード CSV（Indeed / 求人ボックス等）の行に基づく分析が主で、\
         HW 掲載データは比較参考値として併記している。\
         CSV は対象媒体の掲載範囲に依存し、HW は掲載求人のみに限定されるため、\
         いずれも全求人市場を代表するものではない。\
         職業紹介事業者の求人・非公開求人は本レポートに含まれない。</li>\n",
    );
    html.push_str(
        "<li><strong>給与バイアス</strong>: HW 掲載求人は中小企業・地方案件の比率が高く民間媒体より\
         給与水準が低く出る傾向がある。CSV 側も掲載元媒体のバイアスを内包するため、\
         両者の単純比較には注意が必要。</li>\n",
    );
    html.push_str(
        "<li><strong>サンプル件数と求人件数</strong>: 本レポートの「サンプル件数」は分析対象求人数であり、\
         地域全体の求人件数ではない。</li>\n",
    );
    html.push_str("</ul>\n</div>\n");

    // === カテゴリ 3: 統計手法 + 用語ツールチップ ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-method\">\n");
    html.push_str("<h3>統計手法（用語定義）</h3>\n");
    html.push_str("<ul>\n");
    html.push_str(&format!(
        "<li><strong>外れ値処理</strong>: 給与統計（中央値・平均・グループ別集計）は {} 法\
         （Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR の範囲外を除外）を適用済。\
         雇用形態グループ別集計も各グループ内で同手法の除外を実行。\
         除外件数は Executive Summary および各カード内に明示表示。</li>\n",
        render_info_tooltip("IQR", "四分位範囲 (Inter-Quartile Range)。Q3 − Q1 の幅。Tukey 1977 由来の外れ値除外法は、Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR の範囲外を外れ値と判定する。"),
    ));
    html.push_str(&format!(
        "<li><strong>区間推定</strong>: 平均値の信頼区間は {} を用いる（標本から再標本化により分布を推定）。</li>\n",
        render_info_tooltip("Bootstrap 95% CI", "ブートストラップ法による 95% 信頼区間。標本データから復元抽出を 1,000〜10,000 回繰り返し、推定統計量の分布から 2.5 / 97.5 パーセンタイルを取る手法。母集団分布を仮定しない。"),
    ));
    html.push_str(&format!(
        "<li><strong>頑健平均</strong>: 一部の指標は {} を併記（上下 5〜10% を除外して再計算）。</li>\n",
        render_info_tooltip("Trimmed mean", "刈り込み平均。データ両端の指定割合（例: 上下 5%）を除外してから平均を取る手法。少数の外れ値の影響を抑える。"),
    ));
    html.push_str(&format!(
        "<li><strong>月給換算</strong>: 時給→月給換算は <code>{}</code> を使用。</li>\n",
        render_info_tooltip("月給換算 167h", "時給 × 167 時間で月給に換算。労働基準法の週 40 時間 × 4 週 = 160h ではなく、月平均所定労働時間 167h（厚生労働省ガイドライン準拠）を採用。"),
    ));
    html.push_str(&format!(
        "<li><strong>求人理由</strong>: {} は e-Stat 雇用動向調査由来で、「離職・退職に伴う補充」の比率を表す（新規拡大採用は含まない）。</li>\n",
        render_info_tooltip("欠員補充率", "求人理由が「欠員補充」の比率。e-Stat 雇用動向調査由来の都道府県粒度値。新規拡大採用と区別される。"),
    ));
    html.push_str(&format!(
        "<li><strong>分布指標</strong>: {} と {} を併記する場面では、外れ値耐性の観点から中央値を主指標とする。</li>\n",
        render_info_tooltip("中央値", "データを大小順に並べた中央の値。外れ値の影響を受けにくい代表値。"),
        render_info_tooltip("平均", "全データの算術平均。外れ値の影響を受けやすいが、サンプル全体の総和情報を反映する。"),
    ));
    html.push_str("</ul>\n</div>\n");

    // === カテゴリ 4: 相関 ≠ 因果 ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-corr\">\n");
    html.push_str("<h3>相関 \u{2260} 因果</h3>\n"); // ≠ は数学記号として残す
    html.push_str("<ul>\n");
    html.push_str(
        "<li><strong>相関と因果</strong>: 本レポートに記載する「傾向」「相関」は因果関係を\
         証明するものではない。示唆は仮説であり、実施判断は現場文脈に依存する。</li>\n",
    );
    html.push_str(
        "<li><strong>該当箇所</strong>: 第4章（求職者心理分析、新着プレミアム）/ 第5章（地域注目企業、HW件数 ⇄ 採用活発度）/ 各種地域比較。\
         これらは観測された関連性であり、因果関係を主張しない。</li>\n",
    );
    html.push_str("</ul>\n</div>\n");

    // === カテゴリ 5: 更新頻度 ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-update\">\n");
    html.push_str("<h3>更新頻度</h3>\n");
    html.push_str("<ul>\n");
    html.push_str("<li>HW postings: <strong>毎晩</strong>（前日分の差分取り込み）</li>\n");
    html.push_str(
        "<li>ts_turso_counts（時系列）: <strong>月次</strong>（snapshot_id 単位）</li>\n",
    );
    html.push_str("<li>外部統計（最低賃金・欠員補充率・人口）: <strong>年次</strong>（出典発表サイクル準拠）</li>\n");
    html.push_str("<li>地域注目企業 DB: <strong>外部 DB 更新サイクル準拠</strong>（リアルタイムではない）</li>\n");
    html.push_str("</ul>\n</div>\n");

    // === 出典 + 生成元（既存番号リスト互換のため <ol> としても残す） ===
    html.push_str(
        "<ol style=\"padding-left:1.4em;font-size:10pt;line-height:1.6;color:var(--text);margin-top:8px;\">\n",
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

    // フッタ: 生成日時 + フィルタ条件 + バージョン
    html.push_str(&format!(
        "<div class=\"report-banner-gray\" role=\"note\" style=\"margin-top:10px;\">\
         <strong>レポートメタ情報</strong>: \
         生成日時 {} \u{30FB} フォーマット v2 (2026-04-24) \u{30FB} レンダリング: For A-career Dashboard\
         </div>\n",
        escape_html(now)
    ));

    html.push_str("</section>\n");
}
