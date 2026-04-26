//! 雇用形態の統一分類モジュール (P2 #2 / 2026-04-26)
//!
//! survey/aggregator.rs の `classify_emp_group_label` と
//! recruitment_diag/mod.rs の `expand_employment_type` で分類が乖離していた問題を解消する。
//!
//! - 契約社員/業務委託: survey 側「正社員」/ diag 側「その他」 → 本モジュールでは Other に統一
//! - パート労働者/有期雇用派遣パート/無期雇用派遣パート: PartTime
//! - 正社員/正職員: Regular
//!
//! # MEMORY 遵守
//! - `feedback_test_data_validation.md`: 要素存在ではなくデータ妥当性を逆証明する。
//!
//! # 注意 (Phase 互換)
//! 既存の `recruitment_diag::expand_employment_type` / `survey::classify_emp_group_label`
//! は後方互換のため残す。本モジュールは新規呼出と将来の置換で使用する。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmpGroup {
    /// 正社員 (月給ベースの集計対象)
    Regular,
    /// パート/アルバイト (時給ベースの集計対象)
    PartTime,
    /// 契約/派遣/業務委託/正社員以外 (multi-modal 報酬)
    Other,
}

impl EmpGroup {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Regular => "正社員",
            Self::PartTime => "パート",
            Self::Other => "その他",
        }
    }
}

/// HW postings.employment_type 文字列 → EmpGroup
///
/// パート判定が先に来るのは「有期雇用派遣パート」のような重複キーワード
/// (派遣 + パート) を「パート」優先で扱うため。
pub fn classify(emp: &str) -> EmpGroup {
    if emp.contains("パート") || emp.contains("アルバイト") {
        EmpGroup::PartTime
    } else if (emp.contains("正社員") || emp.contains("正職員")) && !emp.contains("以外") {
        EmpGroup::Regular
    } else {
        // 契約/業務委託/派遣/正社員以外 → Other に集約
        EmpGroup::Other
    }
}

/// UI 3 区分 → DB の employment_type 値リスト (IN 句用)
///
/// `recruitment_diag::expand_employment_type` の意味を本モジュールに移し、
/// 業務委託 を Other に追加することで survey 集計との整合性を確保する。
pub fn expand_to_db_values(group: EmpGroup) -> Vec<&'static str> {
    match group {
        EmpGroup::Regular => vec!["正社員", "正職員"],
        EmpGroup::PartTime => vec![
            "パート労働者",
            "有期雇用派遣パート",
            "無期雇用派遣パート",
        ],
        EmpGroup::Other => vec!["正社員以外", "派遣", "契約社員", "業務委託"],
    }
}

/// UI 文字列 → EmpGroup (UI セレクトの 3 値専用)
pub fn from_ui_value(ui: &str) -> Option<EmpGroup> {
    match ui {
        "正社員" => Some(EmpGroup::Regular),
        "パート" => Some(EmpGroup::PartTime),
        "その他" => Some(EmpGroup::Other),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // 逆証明テスト (feedback_reverse_proof_tests.md / feedback_test_data_validation.md)
    // 「契約社員 = 正社員」「業務委託 = 正社員」だった survey 側の旧分類は誤り。
    // 本モジュール導入後は Other に統一される。
    // ========================================================================

    #[test]
    fn classify_regular_seishain() {
        // 修正前 (survey/aggregator.rs:678): 「正社員」のみ Regular
        // 修正後: 同じ Regular (変更なし、整合性確認)
        assert_eq!(classify("正社員"), EmpGroup::Regular);
        assert_eq!(classify("正職員"), EmpGroup::Regular);
    }

    #[test]
    fn classify_seishain_igai_is_other_not_regular() {
        // 修正前 (survey/aggregator.rs:678): "正社員" を含むため Regular に分類されていた
        // 修正後: "以外" 含みは Other に分類される (重要な不整合修正)
        assert_eq!(classify("正社員以外"), EmpGroup::Other);
    }

    #[test]
    fn classify_contract_worker_is_other_not_regular() {
        // 修正前 (survey/aggregator.rs:680): 「契約」を含むため Regular に分類 (誤り)
        // 修正後: Other に分類 → survey の月給バケットに混入しなくなる
        assert_eq!(classify("契約社員"), EmpGroup::Other);
    }

    #[test]
    fn classify_gyomu_itaku_is_other_not_regular() {
        // 修正前 (survey/aggregator.rs:681): 「業務委託」を含むため Regular に分類 (誤り)
        // 修正後: Other → 業務委託の固定報酬が正社員月給中央値を歪めない
        assert_eq!(classify("業務委託"), EmpGroup::Other);
    }

    #[test]
    fn classify_part_priority_over_dispatch() {
        // 「有期雇用派遣パート」 (派遣 + パート) はパート優先扱い
        assert_eq!(classify("パート労働者"), EmpGroup::PartTime);
        assert_eq!(classify("有期雇用派遣パート"), EmpGroup::PartTime);
        assert_eq!(classify("無期雇用派遣パート"), EmpGroup::PartTime);
        assert_eq!(classify("アルバイト"), EmpGroup::PartTime);
    }

    #[test]
    fn classify_pure_dispatch_is_other() {
        assert_eq!(classify("派遣"), EmpGroup::Other);
    }

    #[test]
    fn expand_other_includes_contract_and_gyomu_itaku() {
        // 修正前 (recruitment_diag/mod.rs:78): その他 = [正社員以外, 派遣, 契約社員] (3件)
        // 修正後: Other = [正社員以外, 派遣, 契約社員, 業務委託] (4件、業務委託追加)
        let v = expand_to_db_values(EmpGroup::Other);
        assert!(v.contains(&"正社員以外"));
        assert!(v.contains(&"派遣"));
        assert!(v.contains(&"契約社員"));
        assert!(v.contains(&"業務委託"));
        assert_eq!(v.len(), 4, "Other group must include 4 db values");
    }

    #[test]
    fn expand_part_includes_three_dispatch_part_variants() {
        let v = expand_to_db_values(EmpGroup::PartTime);
        assert_eq!(v.len(), 3);
        assert!(v.contains(&"パート労働者"));
        assert!(v.contains(&"有期雇用派遣パート"));
        assert!(v.contains(&"無期雇用派遣パート"));
    }

    #[test]
    fn expand_regular_includes_seishain_and_seishokuin() {
        // 修正前 (recruitment_diag/mod.rs:76): 正社員 = ["正社員"] のみ
        // 修正後: ["正社員", "正職員"] (正職員も Regular に含む)
        let v = expand_to_db_values(EmpGroup::Regular);
        assert!(v.contains(&"正社員"));
        assert!(v.contains(&"正職員"));
    }

    #[test]
    fn from_ui_value_three_options() {
        assert_eq!(from_ui_value("正社員"), Some(EmpGroup::Regular));
        assert_eq!(from_ui_value("パート"), Some(EmpGroup::PartTime));
        assert_eq!(from_ui_value("その他"), Some(EmpGroup::Other));
        assert_eq!(from_ui_value(""), None);
        assert_eq!(from_ui_value("不明"), None);
    }

    #[test]
    fn label_consistency_with_from_ui_value() {
        for ui in ["正社員", "パート", "その他"] {
            let group = from_ui_value(ui).unwrap();
            assert_eq!(group.label(), ui);
        }
    }
}
