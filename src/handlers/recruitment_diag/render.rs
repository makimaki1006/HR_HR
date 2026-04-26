//! 採用診断タブの HTML レンダリング
//!
//! Agent D がテンプレート本体を実装済み。本モジュールはテンプレ置換用の
//! コンテキスト生成のみに責務を限定する。
//!
//! テンプレ側トークン:
//! - `{{PREFECTURE_OPTIONS}}` : 47 都道府県の option HTML
//!
//! 業種・雇用形態セレクタは Agent D がテンプレ内にハードコードした select 要素を
//! そのまま使用する（本ファイルからの注入なし）。業種/雇用形態マスタヘルパは
//! 将来拡張（自動テスト・API バリデーション）用途で `job_type_options_html` /
//! `emp_type_options_html` を残す。

use std::fmt::Write as _;

/// 採用診断タブの初期ページ HTML を生成。
///
/// Agent D テンプレとの接続は `{{PREFECTURE_OPTIONS}}` のみ。
pub(crate) fn render_diag_page(prefecture_options: &str) -> String {
    include_str!("../../../templates/tabs/recruitment_diag.html")
        .replace("{{PREFECTURE_OPTIONS}}", prefecture_options)
}

/// 業種選択肢 HTML。UI マスタ 13 分類を固定で返す（DB に依存しない）。
///
/// Agent D テンプレが独自に select をハードコードしたため現状未使用だが、
/// 将来の API 入力バリデーション / テスト用途で残す。
#[allow(dead_code)]
pub(crate) fn job_type_options_html(selected: &str) -> String {
    use crate::handlers::helpers::escape_html;
    const JOB_TYPES: &[&str] = &[
        "老人福祉・介護",
        "サービス業",
        "小売業",
        "その他",
        "建設業",
        "医療",
        "教育・保育",
        "製造業",
        "飲食業",
        "運輸業",
        "派遣・人材",
        "IT・通信",
        "宿泊業",
    ];
    let mut html = String::from(r#"<option value="">-- 業種を選択 --</option>"#);
    for jt in JOB_TYPES {
        let sel = if *jt == selected { " selected" } else { "" };
        write!(html,
            r#"<option value="{v}"{sel}>{v}</option>"#,
            v = escape_html(jt),
            sel = sel
        ).unwrap();
    }
    html
}

/// 雇用形態選択肢 HTML。UI マスタ 3 統合を固定で返す。
///
/// Agent D テンプレが独自に select をハードコードしたため現状未使用だが、
/// 将来の API 入力バリデーション / テスト用途で残す。
#[allow(dead_code)]
pub(crate) fn emp_type_options_html(selected: &str) -> String {
    use crate::handlers::helpers::escape_html;
    const EMP_TYPES: &[(&str, &str)] = &[
        ("正社員", "正社員"),
        ("パート", "パート（パート労働者＋派遣パート）"),
        ("その他", "その他（正社員以外＋派遣）"),
    ];
    let mut html = String::from(r#"<option value="">-- 雇用形態を選択 --</option>"#);
    for (v, label) in EMP_TYPES {
        let sel = if *v == selected { " selected" } else { "" };
        write!(html,
            r#"<option value="{v}"{sel}>{label}</option>"#,
            v = escape_html(v),
            label = escape_html(label),
            sel = sel
        ).unwrap();
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_type_options_contains_13_types() {
        let html = job_type_options_html("");
        // 13 分類 + 1 プレースホルダ = 14 option
        assert_eq!(html.matches("<option").count(), 14);
        assert!(html.contains("老人福祉・介護"));
        assert!(html.contains("IT・通信"));
    }

    #[test]
    fn job_type_options_marks_selected() {
        let html = job_type_options_html("医療");
        assert!(html.contains(r#"value="医療" selected"#));
    }

    #[test]
    fn emp_type_options_3_categories() {
        let html = emp_type_options_html("");
        // 3 統合 + 1 プレースホルダ = 4 option
        assert_eq!(html.matches("<option").count(), 4);
        assert!(html.contains("正社員"));
        assert!(html.contains("パート"));
        assert!(html.contains("その他"));
    }
}
