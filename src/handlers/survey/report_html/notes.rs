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
    html.push_str("<section class=\"section page-start\" role=\"region\" aria-labelledby=\"notes-title\">\n");
    html.push_str("<h2 id=\"notes-title\">第6章 注記・出典・免責</h2>\n");

    // === 冒頭サマリ ===
    // 2026-05-08 Round 2.6: variant 非依存ブロックの HW 文言を中立化。
    //   「HW 掲載求人」→「公開求人」(出典欄では postings テーブル名で具体化)
    html.push_str(
        "<div class=\"report-notes-leadin\">\
         本レポートを正しく読むための前提: \
         記載される数値は <strong>公開求人＋アップロード CSV</strong> の範囲内であり、\
         非公開求人・職業紹介事業者経由・全求人市場を代表しません。\
         また「傾向」「相関」は <strong>因果関係を主張しない</strong> 観測です。</div>\n",
    );

    // === カテゴリ 1: データソース ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-data\">\n");
    html.push_str("<h3>データソース</h3>\n");
    html.push_str("<ul>\n");
    html.push_str("<li>アップロード CSV（媒体経由のデータ）</li>\n");
    html.push_str("<li>公開求人データ（hellowork.db / postings テーブル）</li>\n");
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
        "<li><strong>データスコープ</strong>: 本レポートはアップロード CSV の行に基づく分析が主で、\
         公開求人データは比較参考値として併記している。\
         CSV は対象媒体の掲載範囲に依存し、公開求人データは掲載分のみに限定されるため、\
         いずれも全求人市場を代表するものではない。\
         職業紹介事業者の求人・非公開求人は本レポートに含まれない。</li>\n",
    );
    html.push_str(
        "<li><strong>給与バイアス</strong>: 公開求人は中小企業・地方案件の比率が高く民間媒体より\
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
        "<li><strong>該当箇所</strong>: 第4章（求職者心理分析、新着プレミアム）/ 第5章（地域注目企業、公開求人件数 ⇄ 採用活発度）/ 各種地域比較。\
         これらは観測された関連性であり、因果関係を主張しない。</li>\n",
    );
    // タスク3-A: 相関 vs 因果の具体例を追加
    html.push_str(
        "<li><strong>具体例（読み違いを避けるために）</strong>: 「失業率が高い → 採用しやすい」と短絡しないこと。\
         高失業率の背景には「景気悪化」「特定産業偏重」「世帯外労働意欲の低さ」「年齢構成の偏り」など複数の要因があり、\
         その地域で<em>この瞬間に応募が集まるかどうか</em>は別問題です。\
         同様に「給与が高い求人 → 応募が多い」も常に成り立つわけではなく、\
         職種ミスマッチ・通勤距離・勤務条件等が応募意欲を大きく左右します。</li>\n",
    );
    html.push_str("</ul>\n</div>\n");

    // === タスク3-B: 本レポートで「分からないこと」（データの限界の明示） ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-limit\" \
        style=\"margin-top:8px;padding:8px 12px;border-left:4px solid #f59e0b;background:#fffbeb;border-radius:4px;\">\n");
    html.push_str("<h3>\u{26A0} 本レポートで「分からないこと」</h3>\n");
    html.push_str("<ul>\n");
    html.push_str("<li><strong>個別求職者の応募意向</strong>（CSV / 公開求人データは求人側の情報のみであり、応募者側の意思決定要因は含まれない）</li>\n");
    html.push_str("<li><strong>求人媒体の選定理由・運用期間・予算配分</strong>（媒体側の運用ノウハウは観測不可）</li>\n");
    html.push_str("<li><strong>採用後の定着率・ミスマッチ事例</strong>（時系列追跡なし、入社後データは保有していない）</li>\n");
    html.push_str("<li><strong>競合企業の採用戦略</strong>（地域注目企業の比較表は構造比較のみで戦略意図は推測不可）</li>\n");
    html.push_str("<li><strong>非公開求人・職業紹介事業者経由の求人</strong>（公開データのみが対象範囲）</li>\n");
    html.push_str("<li><strong>個別案件の採用成否予測</strong>（地域マクロ統計から個別求人の結果は予測できない）</li>\n");
    html.push_str("</ul>\n</div>\n");

    // === タスク3-C: このレポートが活きる場面（推奨ユースケース） ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-usecase\" \
        style=\"margin-top:8px;padding:8px 12px;border-left:4px solid #10b981;background:#ecfdf5;border-radius:4px;\">\n");
    html.push_str("<h3>\u{1F4A1} このレポートが活きる場面</h3>\n");
    html.push_str("<ol>\n");
    html.push_str("<li><strong>新規進出地域の採用戦略策定</strong>: 該当地域のデモグラフィック・サイコグラフィック・賃金水準を把握し、\
         初期の求人設計の前提条件を揃える。</li>\n");
    // 2026-05-08 Round 2-1: variant 非依存ブロックなので、HW 言及を中立化。
    //   「HW 市場との適合度」→「公開求人市場との適合度」(出典欄に HW 公開求人記載で具体化)
    html.push_str("<li><strong>既存地域の媒体運用の見直し</strong>: 給与水準・新着比率・主要雇用形態の構成比を確認し、\
         CSV 媒体の掲載傾向と公開求人市場との適合度を比較する。</li>\n");
    html.push_str("<li><strong>社内提案資料の客観データ補強</strong>: 公開統計（e-Stat）・公開求人データを引用し、\
         主観的な肌感覚に頼らない数値ベースの提案を構築する。</li>\n");
    html.push_str("</ol>\n");
    html.push_str("<p style=\"font-size:9.5pt;color:#6b7280;margin:6px 0 0;\">\u{203B} 個別案件の「採用成否予測」「内定承諾率の見積り」には別途現場ヒアリング・候補者プロファイル分析が必要です。\
         本レポートは<strong>地域マクロの俯瞰</strong>を提供するものであり、個別ケースの判断材料ではありません。</p>\n");
    html.push_str("</div>\n");

    // === カテゴリ 5: 更新頻度 ===
    html.push_str("<div class=\"report-notes-category report-notes-cat-update\">\n");
    html.push_str("<h3>更新頻度</h3>\n");
    html.push_str("<ul>\n");
    html.push_str("<li>公開求人データ: <strong>毎晩</strong>（前日分の差分取り込み）</li>\n");
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
        "<li><strong>出典</strong>: データ源 - アップロード CSV / 公開求人データ / \
         地域注目企業データベース / e-Stat。</li>\n",
    );
    html.push_str(&format!(
        "<li><strong>生成元</strong>: 株式会社For A-career / 生成日時: {}</li>\n",
        escape_html(now)
    ));
    html.push_str("</ol>\n");

    // 2026-04-30: ユーザー指摘により「レポートメタ情報」「フォーマット v2」「レンダリング」等の
    // 内部実装情報は不要のため削除。生成日時はカバーページと screen-footer に既出。
    let _ = now;

    html.push_str("</section>\n");
}

// =====================================================================
// UX 強化テスト（2026-04-28）: タスク 3-A / 3-B / 3-C
// =====================================================================
#[cfg(test)]
mod notes_enhancement_tests {
    use super::*;

    /// タスク3-A: 相関 vs 因果の具体例が含まれること
    #[test]
    fn test_notes_contains_correlation_example() {
        let mut html = String::new();
        render_section_notes(&mut html, "2026-04-28 12:00:00");

        // 「失業率が高い → 採用しやすい」は短絡しないことの注意例
        assert!(
            html.contains("失業率"),
            "具体例（失業率）の記述が含まれること"
        );
        assert!(
            html.contains("短絡しない") || html.contains("短絡"),
            "「短絡しない」の注意喚起が含まれること"
        );
        // 別問題であることの明示
        assert!(
            html.contains("別問題"),
            "「別問題」と明示されること"
        );
    }

    /// タスク3-B: 「分からないこと」セクションの主要キーワードが含まれること
    #[test]
    fn test_notes_contains_unknowns_section() {
        let mut html = String::new();
        render_section_notes(&mut html, "2026-04-28 12:00:00");

        assert!(
            html.contains("分からないこと"),
            "「分からないこと」見出しが含まれること"
        );
        assert!(
            html.contains("応募意向"),
            "「応募意向」が分からないことに含まれること"
        );
        assert!(
            html.contains("定着率"),
            "「定着率」が分からないことに含まれること"
        );
        assert!(
            html.contains("非公開求人"),
            "「非公開求人」が範囲外として明示されること"
        );
        assert!(
            html.contains("採用成否予測"),
            "「採用成否予測」は予測対象外と明示されること"
        );
    }

    /// タスク3-C: 「使う場面」セクションの主要キーワードが含まれること
    #[test]
    fn test_notes_contains_usecase_section() {
        let mut html = String::new();
        render_section_notes(&mut html, "2026-04-28 12:00:00");

        assert!(
            html.contains("活きる場面"),
            "「活きる場面」見出しが含まれること"
        );
        assert!(
            html.contains("新規進出地域"),
            "ユースケース1: 新規進出地域の記述"
        );
        assert!(
            html.contains("媒体運用の見直し"),
            "ユースケース2: 媒体運用の見直しの記述"
        );
        assert!(
            html.contains("提案資料") || html.contains("提案"),
            "ユースケース3: 提案資料の記述"
        );
    }

    /// 既存互換: 冒頭サマリ・カテゴリ別ボックス等の既存出力が引き続き含まれること
    #[test]
    fn test_notes_existing_structure_preserved() {
        let mut html = String::new();
        render_section_notes(&mut html, "2026-04-28 12:00:00");

        // 既存テストで参照される文言（互換性確保）
        assert!(
            html.contains("第6章 注記・出典・免責"),
            "既存の見出しが維持されること"
        );
        assert!(
            html.contains("データソース"),
            "データソースカテゴリが維持されること"
        );
        assert!(
            html.contains("スコープ制約"),
            "スコープ制約カテゴリが維持されること"
        );
        assert!(
            html.contains("相関") && html.contains("因果"),
            "相関≠因果カテゴリが維持されること"
        );
    }
}
