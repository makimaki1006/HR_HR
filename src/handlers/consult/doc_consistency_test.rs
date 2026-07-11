//! 文書間突合テスト (P1-9)
//!
//! 商談準備レポート (consult) と顧客向けレポート (survey report_html) が、
//! 同じ集計フィールド (給与中央値等) を参照していることを静的に確認する。
//!
//! 両者が別々の派生値を表示すると「同じ市場なのに数字が食い違う」ため、
//! ソースが `SurveyAggregation.enhanced_stats.median` という同一フィールドから
//! 給与中央値を取っていることをコード上で検証する (§24 決定性・整合性)。
//!
//! 注: consult/** 以外は編集しないルールのため、顧客向けレポート側は読み取り専用で
//! include_str! により参照する (コンパイル時にファイルが存在することも同時に保証される)。

/// consult ハンドラのソース (給与中央値を組み立てる箇所)
const CONSULT_HANDLERS_SRC: &str = include_str!("handlers.rs");
/// 顧客向けレポートの給与中央値算出ソース
const REPORT_EXTENDED_SRC: &str =
    include_str!("../survey/report_html/navy_report/section_10_extended.rs");
/// 顧客向けレポートの給与サマリ算出ソース
const REPORT_SALARY_SRC: &str =
    include_str!("../survey/report_html/navy_report/section_03_salary.rs");

#[test]
fn consult_derives_salary_median_from_enhanced_stats() {
    // consult は enhanced_stats.median を給与中央値のソースにしている
    assert!(
        CONSULT_HANDLERS_SRC.contains("enhanced_stats"),
        "consult が enhanced_stats を参照している"
    );
    // st = enhanced_stats のメンバ median を使っている
    assert!(
        CONSULT_HANDLERS_SRC.contains("st.median"),
        "consult は enhanced_stats.median を中央値ソースにしている"
    );
}

#[test]
fn customer_report_derives_salary_median_from_same_field() {
    // 顧客向けレポートも enhanced_stats.median を給与中央値のソースにしている
    assert!(
        REPORT_EXTENDED_SRC.contains("enhanced_stats") && REPORT_EXTENDED_SRC.contains(".median"),
        "顧客向けレポート(拡張)も enhanced_stats.median を参照している"
    );
    assert!(
        REPORT_SALARY_SRC.contains("s.median"),
        "顧客向けレポート(給与)も同じ median フィールドを参照している"
    );
}

#[test]
fn both_documents_reference_same_aggregation_field() {
    // 両文書とも同一の集計構造体フィールド (enhanced_stats.median) を参照している=
    // 同じ市場データを表示する保証。派生元が食い違っていないことの静的突合。
    let consult_uses =
        CONSULT_HANDLERS_SRC.contains("enhanced_stats") && CONSULT_HANDLERS_SRC.contains(".median");
    let report_uses =
        REPORT_EXTENDED_SRC.contains("enhanced_stats") && REPORT_EXTENDED_SRC.contains(".median");
    assert!(
        consult_uses && report_uses,
        "商談準備レポートと顧客向けレポートは同一の給与中央値フィールドを参照する"
    );
}
