//! Tab 深掘り (2026-04-26): 媒体分析タブ以外の 7 タブ + サブタブの
//! 因果断定 / 将来断定文言の **逆証明テスト** (feedback_correlation_not_causation.md 準拠)
//!
//! 本テスト群は「修正前文言が出ない」+「修正後の中立文言が出る」の二段で
//! ロジック退行を逆証明する (memory: feedback_reverse_proof_tests.md)。
//!
//! 対象 (修正の根拠は docs/audit_2026_04_24/tab_deepdive_results.md):
//!   1. insight/render.rs:1302 開業-廃業ギャップ「成長市場 / 減少局面」断定 → 削除
//!   2. insight/render.rs:1499 高齢化率「拡大基調 / 漸増の見込み」将来断定 → 削除
//!   3. company/render.rs:947 採用リスク「良好です / 厳しい状態」断定 → 削除
//!   4. company/fetch.rs:909 給与提案「応募数増加が見込めます」因果断定 → 削除
//!   5. analysis/render/subtab1_recruit_trend.rs:149 「健全な雇用構造です」断定 → 削除

#![cfg(test)]

use std::fs;
use std::path::Path;

/// プロジェクトルート (env!("CARGO_MANIFEST_DIR")) を起点にファイル全文を読む
fn read_src(rel_path: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let p = Path::new(manifest).join(rel_path);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("failed to read {:?}: {}", p, e))
}

// ---------------------------------------------------------------------------
// 1. 総合診断 / トレンド: insight/render.rs 開業-廃業ギャップ解釈
// ---------------------------------------------------------------------------

/// 修正前文言「新規参入が活発な成長市場」が **完全に消えている** ことを逆証明
#[test]
fn reverse_proof_insight_render_opening_closing_no_seichoshijou() {
    let src = read_src("src/handlers/insight/render.rs");
    assert!(
        !src.contains("新規参入が活発な成長市場"),
        "insight/render.rs: 旧文言「新規参入が活発な成長市場」が残っている (\
         feedback_correlation_not_causation.md 違反)"
    );
}

/// 修正前文言「企業減少局面。既存企業の採用枠確保に注意。」が消えていることを逆証明
#[test]
fn reverse_proof_insight_render_opening_closing_no_genshokyokumen() {
    let src = read_src("src/handlers/insight/render.rs");
    assert!(
        !src.contains("企業減少局面。既存企業の採用枠確保に注意。"),
        "insight/render.rs: 旧文言「企業減少局面」断定が残っている"
    );
}

/// 修正後文言「相対的に多い可能性があります」が出ることを逆証明 (削除に対する逆方向の証明)
#[test]
fn reverse_proof_insight_render_opening_closing_has_kanousei() {
    let src = read_src("src/handlers/insight/render.rs");
    assert!(
        src.contains("新規参入が相対的に多い可能性があります"),
        "insight/render.rs: 新文言「新規参入が相対的に多い可能性があります」が見つからない"
    );
    assert!(
        src.contains("既存企業の採用枠確保に留意が必要な可能性があります"),
        "insight/render.rs: 新文言「採用枠確保に留意が必要な可能性があります」が見つからない"
    );
}

// ---------------------------------------------------------------------------
// 2. 総合診断 / トレンド: insight/render.rs 高齢化率解釈
// ---------------------------------------------------------------------------

/// 修正前文言「拡大基調」「漸増の見込み」「構造的に高く」が消えていることを逆証明
#[test]
fn reverse_proof_insight_render_aging_no_kakudai_kicho() {
    let src = read_src("src/handlers/insight/render.rs");
    assert!(
        !src.contains("介護需要は今後も拡大基調"),
        "insight/render.rs: 旧文言「介護需要は今後も拡大基調」(将来断定) が残っている"
    );
    assert!(
        !src.contains("中長期で漸増の見込み"),
        "insight/render.rs: 旧文言「漸増の見込み」(将来断定) が残っている"
    );
    assert!(
        !src.contains("介護職採用需要が構造的に高く、長期的に供給逼迫が続く可能性。"),
        "insight/render.rs: 旧文言の断定パターンが残っている"
    );
}

/// 修正後文言「可能性があります」が含まれることを逆証明
#[test]
fn reverse_proof_insight_render_aging_has_kanousei() {
    let src = read_src("src/handlers/insight/render.rs");
    assert!(
        src.contains("介護需要が増加傾向となる可能性があり"),
        "insight/render.rs: 新文言「増加傾向となる可能性」が見つからない"
    );
    assert!(
        src.contains("中長期で漸増する可能性があります"),
        "insight/render.rs: 新文言「漸増する可能性」が見つからない"
    );
}

// ---------------------------------------------------------------------------
// 3. 企業検索: company/render.rs 採用リスクグレード説明
// ---------------------------------------------------------------------------

/// 修正前文言「採用環境は良好です」「採用環境は非常に厳しい状態です」が消えていることを逆証明
#[test]
fn reverse_proof_company_render_hiring_risk_no_dantei() {
    let src = read_src("src/handlers/company/render.rs");
    assert!(
        !src.contains(
            "採用環境は良好です。給与水準・地域特性ともに人材確保に有利な条件が揃っています。"
        ),
        "company/render.rs: 旧文言「採用環境は良好です」(断定) が残っている"
    );
    assert!(
        !src.contains("採用環境は非常に厳しい状態です。早急な対策が必要です。"),
        "company/render.rs: 旧文言「採用環境は非常に厳しい状態です」(断定) が残っている"
    );
    assert!(
        !src.contains("採用リスクが高い状態です。複数の指標で不利な条件が重なっています。"),
        "company/render.rs: 旧文言「採用リスクが高い状態です」(断定) が残っている"
    );
}

/// 修正後文言「相対的に〜の可能性があります」が出ることを逆証明
#[test]
fn reverse_proof_company_render_hiring_risk_has_kanousei() {
    let src = read_src("src/handlers/company/render.rs");
    assert!(
        src.contains("採用環境が相対的に有利な可能性があります"),
        "company/render.rs: 新文言「相対的に有利な可能性があります」が見つからない"
    );
    assert!(
        src.contains("採用リスクが相対的に高い可能性があります"),
        "company/render.rs: 新文言「相対的に高い可能性」が見つからない"
    );
}

// ---------------------------------------------------------------------------
// 4. 企業検索: company/fetch.rs 給与提案 (sales pitch)
// ---------------------------------------------------------------------------

/// 修正前文言「給与改善により応募数増加が見込めます」が消えていることを逆証明
#[test]
fn reverse_proof_company_fetch_salary_pitch_no_inga_dantei() {
    let src = read_src("src/handlers/company/fetch.rs");
    assert!(
        !src.contains("給与改善により応募数増加が見込めます。"),
        "company/fetch.rs: 旧文言「給与改善により応募数増加が見込めます」(因果断定) が残っている"
    );
    assert!(
        !src.contains("給与面での競争力は高い状態です。"),
        "company/fetch.rs: 旧文言「給与面での競争力は高い状態です」(断定) が残っている"
    );
}

/// 修正後文言「相関であり因果は別途検証要」「可能性があります」が出ることを逆証明
#[test]
fn reverse_proof_company_fetch_salary_pitch_has_correlation_note() {
    let src = read_src("src/handlers/company/fetch.rs");
    assert!(
        src.contains("応募数が増加する可能性があります（相関であり因果は別途検証要）"),
        "company/fetch.rs: 新文言（相関注記付き）が見つからない"
    );
    assert!(
        src.contains("給与面での競争力が相対的に高い可能性があります"),
        "company/fetch.rs: 新文言「相対的に高い可能性」が見つからない"
    );
}

// ---------------------------------------------------------------------------
// 5. 詳細分析: analysis/render/subtab1_recruit_trend.rs 産業多様性
// ---------------------------------------------------------------------------

/// 修正前文言「健全な雇用構造です」が消えていることを逆証明
#[test]
fn reverse_proof_analysis_subtab1_resilience_no_kenzen_dantei() {
    let src = read_src("src/handlers/analysis/render/subtab1_recruit_trend.rs");
    assert!(
        !src.contains("特定産業への依存リスクが低い健全な雇用構造です。"),
        "analysis/subtab1: 旧文言「健全な雇用構造です」(断定) が残っている"
    );
}

/// 修正後文言「相対的に低い傾向」「保証するものではありません」が出ることを逆証明
#[test]
fn reverse_proof_analysis_subtab1_resilience_has_neutral() {
    let src = read_src("src/handlers/analysis/render/subtab1_recruit_trend.rs");
    assert!(
        src.contains("特定産業への依存リスクが相対的に低い傾向がみられます"),
        "analysis/subtab1: 新文言「相対的に低い傾向」が見つからない"
    );
    assert!(
        src.contains("雇用構造の健全性そのものを保証するものではありません"),
        "analysis/subtab1: 新文言（健全性保証否定）が見つからない"
    );
}

// ---------------------------------------------------------------------------
// 6. 横断: 媒体分析以外でも HW スコープ注記の存在件数が一定数以上であること
// ---------------------------------------------------------------------------

/// HW 限定スコープ注記が複数タブに分布していることを件数下限で逆証明 (回帰防止)
#[test]
fn reverse_proof_hw_scope_note_distribution_floor() {
    let files = [
        "src/handlers/company/fetch.rs",
        "src/handlers/company/render.rs",
        "src/handlers/insight/render.rs",
        "src/handlers/analysis/render/subtab1_recruit_trend.rs",
    ];
    let mut hits_total = 0usize;
    for f in &files {
        let src = read_src(f);
        if src.contains("HW")
            && (src.contains("HW指標ベース")
                || src.contains("HW参考")
                || src.contains("HW求人ベース")
                || src.contains("HW掲載求人ベース")
                || src.contains("HW掲載"))
        {
            hits_total += 1;
        }
    }
    assert!(
        hits_total >= 3,
        "媒体分析以外の主要タブで HW スコープ注記が 3 ファイル未満 (見つかった: {}). \
         feedback_hw_data_scope.md 違反の可能性",
        hits_total
    );
}
