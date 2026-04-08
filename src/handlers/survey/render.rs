//! HTMX HTML描画（3フェーズUI）

use super::super::helpers::{escape_html, format_number};
use super::aggregator::SurveyAggregation;
use super::job_seeker::JobSeekerAnalysis;

/// Phase A: アップロードフォーム
pub(crate) fn render_upload_form() -> String {
    r#"<div class="space-y-6">
        <h2 class="text-xl font-bold text-white">媒体分析</h2>
        <p class="text-xs text-slate-500">Indeed / 求人ボックスのCSVをアップロードして、HWデータ・外部統計と統合した競合分析レポートを生成します</p>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-4">CSVファイルをアップロード</h3>
            <form id="survey-upload-form" enctype="multipart/form-data">
                <div id="drop-zone" class="border-2 border-dashed border-slate-600 rounded-lg p-8 text-center cursor-pointer hover:border-blue-500 transition-colors"
                     ondragover="event.preventDefault();this.classList.add('border-blue-500')"
                     ondragleave="this.classList.remove('border-blue-500')"
                     ondrop="event.preventDefault();this.classList.remove('border-blue-500');handleDrop(event)">
                    <svg class="w-8 h-8 mx-auto mb-2 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"/>
                    </svg>
                    <div class="text-slate-400 text-sm mb-2">CSVファイルをドラッグ&ドロップ</div>
                    <div class="text-slate-500 text-xs mb-3">または</div>
                    <label class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded text-sm cursor-pointer transition-colors">
                        ファイルを選択
                        <input type="file" name="csv_file" accept=".csv,.txt" class="hidden" onchange="submitSurveyCSV(this.files[0])">
                    </label>
                    <div class="text-slate-600 text-xs mt-4">対応形式: Indeed, 求人ボックス (CSV/UTF-8)</div>
                </div>
            </form>
            <div id="upload-status" class="mt-3"></div>
        </div>
        <div id="survey-result"></div>
    </div>
    <script>
    function handleDrop(e) {
        var files = e.dataTransfer.files;
        if (files.length > 0) submitSurveyCSV(files[0]);
    }
    function submitSurveyCSV(file) {
        if (!file) return;
        var status = document.getElementById('upload-status');
        status.innerHTML = '<div class="text-sm text-blue-400">アップロード中: ' + file.name + '...</div>';
        var fd = new FormData();
        fd.append('csv_file', file);
        fetch('/api/survey/upload', { method: 'POST', body: fd })
            .then(function(r) { return r.text(); })
            .then(function(serverHtml) {
                // Server-rendered HTML (pre-escaped, XSS safe)
                var target = document.getElementById('survey-result');
                target.innerHTML = serverHtml;
                // Re-process HTMX attributes in dynamically inserted content
                if (typeof htmx !== 'undefined') htmx.process(target);
                status.textContent = '完了';
                status.className = 'mt-3 text-sm text-green-400';
            })
            .catch(function(e) {
                status.textContent = 'アップロードエラーが発生しました';
                status.className = 'mt-3 text-sm text-red-400';
            });
    }
    </script>"#.to_string()
}

/// Phase B: 分析結果
pub(crate) fn render_analysis_result(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    session_id: &str,
) -> String {
    let mut html = String::with_capacity(8_000);

    // サマリーカード
    html.push_str(r#"<div class="space-y-4 mt-4">"#);
    html.push_str(r#"<h3 class="text-lg font-bold text-white">分析結果</h3>"#);

    // KPIカード
    html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
    render_kpi(&mut html, "総求人数", &format_number(agg.total_count as i64), "text-blue-400");
    render_kpi(&mut html, "新着率", &format!("{:.1}%", if agg.total_count > 0 { agg.new_count as f64 / agg.total_count as f64 * 100.0 } else { 0.0 }), "text-emerald-400");
    render_kpi(&mut html, "給与パース率", &format!("{:.0}%", agg.salary_parse_rate * 100.0), "text-amber-400");
    render_kpi(&mut html, "住所パース率", &format!("{:.0}%", agg.location_parse_rate * 100.0), "text-cyan-400");
    html.push_str("</div>");

    // 主要地域
    if let Some(pref) = &agg.dominant_prefecture {
        html.push_str(&format!(
            r#"<div class="stat-card"><span class="text-slate-400 text-sm">主要地域: </span><span class="text-white font-medium">{}</span>"#,
            escape_html(pref)
        ));
        if let Some(muni) = &agg.dominant_municipality {
            html.push_str(&format!(r#" <span class="text-blue-400">{}</span>"#, escape_html(muni)));
        }
        html.push_str("</div>");
    }

    // 給与統計
    if let Some(stats) = &agg.enhanced_stats {
        html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">給与統計</h4>"#);
        html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
        render_kpi(&mut html, "平均月給", &format!("{}円", format_number(stats.mean)), "text-white");
        render_kpi(&mut html, "中央値", &format!("{}円", format_number(stats.median)), "text-white");
        render_kpi(&mut html, "最低", &format!("{}円", format_number(stats.min)), "text-slate-400");
        render_kpi(&mut html, "最高", &format!("{}円", format_number(stats.max)), "text-slate-400");
        html.push_str("</div>");

        // Bootstrap CI
        if let Some(ci) = &stats.bootstrap_ci {
            html.push_str(&format!(
                r#"<div class="text-xs text-slate-500 mt-2">95%信頼区間: {}円 〜 {}円 (n={}, {}回リサンプリング)</div>"#,
                format_number(ci.lower), format_number(ci.upper), ci.sample_size, ci.iterations
            ));
        }
        // トリム平均
        if let Some(tm) = &stats.trimmed_mean {
            html.push_str(&format!(
                r#"<div class="text-xs text-slate-500">トリム平均(10%): {}円 (外れ値{}件除外)</div>"#,
                format_number(tm.trimmed_mean), tm.removed_count
            ));
        }
        html.push_str(&format!(
            r#"<div class="text-xs text-slate-600 mt-1">信頼性: {} (n={})</div>"#,
            stats.reliability, stats.count
        ));
        html.push_str("</div>");
    }

    // 求職者心理分析
    if let Some(perception) = &seeker.salary_range_perception {
        html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">求職者心理分析</h4>"#);
        html.push_str(&format!(
            r#"<div class="text-sm text-white">期待給与（推定）: <span class="text-amber-400 font-bold">{}円</span></div>
            <div class="text-[10px] text-slate-600">※ 求職者は給与レンジの下限〜1/3地点を期待値とする傾向（下限+レンジ幅×33%）</div>"#,
            format_number(perception.expected_point)
        ));
        html.push_str(&format!(
            r#"<div class="text-xs text-slate-500 mt-1">給与レンジ平均: {}円 〜 {}円（幅: {}円）</div>"#,
            format_number(perception.avg_lower), format_number(perception.avg_upper), format_number(perception.avg_range_width)
        ));
        html.push_str(&format!(
            r#"<div class="text-xs text-slate-600">レンジ幅: 狭い{}件 / 中程度{}件 / 広い{}件</div>"#,
            perception.narrow_count, perception.medium_count, perception.wide_count
        ));
        html.push_str("</div>");
    }

    // 未経験タグ分析
    if let Some(inexp) = &seeker.inexperience_analysis {
        if let Some(gap) = inexp.salary_gap {
            html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">未経験可タグ分析</h4>"#);
            let color = if gap > 0 { "text-red-400" } else { "text-green-400" };
            html.push_str(&format!(
                r#"<div class="text-sm text-white">経験者vs未経験者 給与差: <span class="{color} font-bold">{:+}円</span></div>"#,
                gap
            ));
            html.push_str(&format!(
                r#"<div class="text-xs text-slate-500">未経験可: {}件 (平均{}円) / その他: {}件 (平均{}円)</div>"#,
                inexp.inexperience_count,
                inexp.inexperience_avg_salary.map(|v| format_number(v)).unwrap_or_default(),
                inexp.experience_count,
                inexp.experience_avg_salary.map(|v| format_number(v)).unwrap_or_default(),
            ));
            html.push_str("</div>");
        }
    }

    // 地域分布 Top 10
    if !agg.by_prefecture.is_empty() {
        html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">地域分布 Top 10</h4>"#);
        for (pref, count) in agg.by_prefecture.iter().take(10) {
            let pct = *count as f64 / agg.total_count as f64 * 100.0;
            html.push_str(&format!(
                r#"<div class="flex items-center gap-2 mb-1">
                    <span class="text-xs text-white w-20 shrink-0">{}</span>
                    <div class="flex-1 bg-slate-700 rounded h-3">
                        <div class="bg-blue-500 rounded h-3" style="width:{pct:.1}%"></div>
                    </div>
                    <span class="text-xs text-slate-400 w-16 text-right">{} ({pct:.1}%)</span>
                </div>"#,
                escape_html(pref), count
            ));
        }
        html.push_str("</div>");
    }

    // 統合レポート生成ボタン
    html.push_str(&format!(
        r##"<div class="stat-card text-center">
            <button hx-get="/api/survey/integrate?session_id={session_id}" hx-target="#survey-integration-result" hx-swap="innerHTML"
                class="px-6 py-3 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-sm font-medium transition-colors">
                🔗 HWデータ・外部統計と統合レポートを生成
            </button>
            <p class="text-xs text-slate-500 mt-2">主要地域のHW求人データ・人口統計と統合して分析します</p>
        </div>
        <div id="survey-integration-result"></div>"##
    ));

    html.push_str("</div>");
    html
}

fn render_kpi(html: &mut String, label: &str, value: &str, color: &str) {
    html.push_str(&format!(
        r#"<div class="stat-card text-center">
            <div class="text-lg font-bold {color}">{value}</div>
            <div class="text-xs text-slate-500">{label}</div>
        </div>"#
    ));
}
