use super::fetch::DetailRow;
use crate::handlers::competitive::{escape_html, truncate_str};

/// 初期ページHTML（テンプレート読み込み + 変数置換）
pub(crate) fn render_jobmap_page(
    job_type: &str,
    prefecture: &str,
    municipality: &str,
    prefecture_options: &str,
) -> String {
    include_str!("../../../templates/tabs/jobmap.html")
        .replace("{{JOB_TYPE}}", &escape_html(job_type))
        .replace("{{PREFECTURE}}", &escape_html(prefecture))
        .replace("{{MUNICIPALITY}}", &escape_html(municipality))
        .replace("{{PREFECTURE_OPTIONS}}", prefecture_options)
}

/// 求人詳細カードHTML
pub(crate) fn render_detail_card(d: &DetailRow) -> String {
    let salary_display = if d.salary_min > 0 && d.salary_max > 0 {
        format!(
            "{} {}&nbsp;〜&nbsp;{}",
            escape_html(&d.salary_type),
            format_yen(d.salary_min),
            format_yen(d.salary_max)
        )
    } else if d.salary_min > 0 {
        format!("{} {}〜", escape_html(&d.salary_type), format_yen(d.salary_min))
    } else {
        "記載なし".to_string()
    };

    let mut html = String::with_capacity(2048);
    html.push_str(r#"<div class="space-y-2 text-sm">"#);

    // ヘッドライン
    if !d.headline.is_empty() {
        html.push_str(&format!(
            r#"<div class="text-base font-bold text-blue-300 border-b border-gray-600 pb-1">{}</div>"#,
            escape_html(&d.headline)
        ));
    }

    // 求人番号（Hello Work固有）
    if !d.job_number.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">求人番号</span><span class="font-mono text-cyan-300">{}</span></div>"#,
            escape_html(&d.job_number)
        ));
    }

    // 施設名
    html.push_str(&format!(
        r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">事業所名</span><span class="font-medium text-white">{}</span></div>"#,
        escape_html(&d.facility_name)
    ));

    // 所在地
    html.push_str(&format!(
        r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">所在地</span><span>{} {}</span></div>"#,
        escape_html(&d.prefecture),
        escape_html(&d.municipality)
    ));

    // アクセス
    if !d.access.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">アクセス</span><span>{}</span></div>"#,
            escape_html(&d.access)
        ));
    }

    // ハローワーク管轄（Hello Work固有）
    if !d.hello_work_office.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">管轄HW</span><span class="text-xs text-gray-300">{}</span></div>"#,
            escape_html(&d.hello_work_office)
        ));
    }

    // 産業分類
    if !d.job_type.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">産業</span><span>{}</span></div>"#,
            escape_html(&d.job_type)
        ));
    }

    // 雇用形態
    html.push_str(&format!(
        r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">雇用形態</span><span class="px-2 py-0.5 rounded text-xs {}">{}</span></div>"#,
        emp_badge_class(&d.employment_type),
        escape_html(&d.employment_type)
    ));

    // 給与
    html.push_str(&format!(
        r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">給与</span><span class="text-yellow-300 font-medium">{}</span></div>"#,
        salary_display
    ));

    // 仕事内容
    if !d.job_description.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">仕事内容</span><span class="text-xs">{}</span></div>"#,
            escape_html(&truncate_str(&d.job_description, 200))
        ));
    }

    // 応募要件
    if !d.requirements.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">応募要件</span><span class="text-xs">{}</span></div>"#,
            escape_html(&truncate_str(&d.requirements, 150))
        ));
    }

    // 勤務時間
    if !d.working_hours.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">勤務時間</span><span class="text-xs">{}</span></div>"#,
            escape_html(&truncate_str(&d.working_hours, 100))
        ));
    }

    // 休日
    if !d.holidays.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">休日</span><span class="text-xs">{}</span></div>"#,
            escape_html(&truncate_str(&d.holidays, 100))
        ));
    }

    // 待遇
    if !d.benefits.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">待遇</span><span class="text-xs">{}</span></div>"#,
            escape_html(&truncate_str(&d.benefits, 150))
        ));
    }

    // 募集理由（Hello Work固有）
    if !d.recruitment_reason.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2"><span class="text-gray-400 w-24 flex-shrink-0">募集理由</span><span class="text-xs text-gray-300">{}</span></div>"#,
            escape_html(&d.recruitment_reason)
        ));
    }

    // セグメント情報
    if !d.tier3_label_short.is_empty() {
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2 border-t border-gray-700 pt-1 mt-1"><span class="text-gray-400 w-24 flex-shrink-0">分類</span><span class="text-xs text-purple-300">{}</span></div>"#,
            escape_html(&d.tier3_label_short)
        ));
    }

    html.push_str("</div>");
    html
}

fn emp_badge_class(emp: &str) -> &'static str {
    match emp {
        "正職員" | "正社員" | "フルタイム" => "bg-green-700 text-green-200",
        "契約職員" | "契約社員" => "bg-blue-700 text-blue-200",
        "パート・バイト" | "パートタイム" => "bg-pink-700 text-pink-200",
        "業務委託" | "派遣" => "bg-purple-700 text-purple-200",
        _ => "bg-gray-700 text-gray-300",
    }
}

pub(crate) fn format_yen(n: i64) -> String {
    if n == 0 {
        return "\u{2212}".to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    format!("\u{00a5}{}", formatted)
}

/// 職種データが未連携の場合の表示
pub(crate) fn render_no_data_message(job_type: &str) -> String {
    format!(
        r#"<div class="p-8 text-center">
            <div class="text-6xl mb-4">🗺️</div>
            <h2 class="text-2xl font-bold text-white mb-2">求人地図</h2>
            <div class="bg-yellow-900/30 border border-yellow-700 rounded-lg p-6 max-w-lg mx-auto">
                <p class="text-yellow-300 text-lg font-medium mb-2">データ未連携</p>
                <p class="text-gray-300">
                    「<span class="text-white font-medium">{}</span>」の求人地図データはまだ連携されていません。
                </p>
                <p class="text-gray-400 text-sm mt-3">
                    ヘッダーで産業を選択するか、都道府県を選択してください。
                </p>
            </div>
        </div>"#,
        escape_html(job_type)
    )
}
