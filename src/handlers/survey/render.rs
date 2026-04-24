//! HTMX HTML描画（媒体分析タブ）
//!
//! UI設計方針:
//! - TL;DR を最上部に配置（n件数・主要地域・給与中央値）
//! - HW比較と詳細統計は折りたたみセクションで段階開示
//! - 絵文字を抑制し、色と縦バー装飾でセクション階層を示す
//! - 「HW掲載求人のみ」「因果ではなく相関」等の注意書きを各カードに明示
//! - カラー意味論: blue=情報 / emerald=良好 / amber=注意 / red=警告（insight::Severity と整合）

use super::super::helpers::{escape_html, format_number};
use super::aggregator::SurveyAggregation;
use super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

// =============================================================================
// Phase A: アップロードフォーム
// =============================================================================

/// 初期表示: CSVアップロードフォーム
pub(crate) fn render_upload_form() -> String {
    r##"<div class="space-y-6" id="survey-root">
        <!-- ヘッダー -->
        <header class="stat-card">
            <div class="flex items-start justify-between flex-wrap gap-3">
                <div>
                    <h2 class="text-xl font-bold text-white">媒体分析
                        <span class="text-blue-400 text-base font-normal">Indeed・求人ボックス CSV 取込</span>
                    </h2>
                    <p class="text-xs text-slate-400 mt-1">
                        他媒体のCSVをアップロードし、HWデータ・外部統計と突き合わせて地域別の相対比較を行います。
                    </p>
                </div>
                <div class="text-xs text-slate-500 text-right">
                    <div>対応形式: Indeed / 求人ボックス</div>
                    <div>文字コード: UTF-8（CSV/TXT）</div>
                </div>
            </div>
        </header>

        <!-- アップロードセクション -->
        <section class="stat-card">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2">
                CSVファイルをアップロード
            </h3>
            <form id="survey-upload-form" enctype="multipart/form-data">
                <div id="drop-zone"
                     class="border-2 border-dashed border-slate-600 rounded-lg p-8 text-center cursor-pointer hover:border-blue-500 hover:bg-slate-800/30 transition-colors"
                     ondragover="event.preventDefault();this.classList.add('border-blue-500','bg-slate-800/30')"
                     ondragleave="this.classList.remove('border-blue-500','bg-slate-800/30')"
                     ondrop="event.preventDefault();this.classList.remove('border-blue-500','bg-slate-800/30');handleDrop(event)">
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
            <div class="text-xs text-slate-600 mt-3 border-t border-slate-800 pt-3">
                アップロードしたCSVはブラウザセッション内でのみ処理され、永続保存されません。
                HW掲載求人との比較は相対的な参考値であり、採用判断の唯一の根拠としないでください。
            </div>
        </section>

        <!-- 使い方ガイド -->
        <section class="stat-card">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-slate-500 pl-2">使い方</h3>
            <ol class="text-xs text-slate-400 space-y-1.5 list-decimal list-inside">
                <li>IndeedまたはExcelで求人リストをCSVエクスポート（UTF-8）</li>
                <li>本画面でCSVをアップロード → 給与・地域・雇用形態を自動パース</li>
                <li>TL;DRと給与分布を確認後、「HWデータと統合分析」で比較レポート生成</li>
                <li>印刷用レポート出力でA4印刷／PDF化が可能</li>
            </ol>
        </section>

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
                var target = document.getElementById('survey-result');
                target.innerHTML = serverHtml;
                if (typeof htmx !== 'undefined') htmx.process(target);
                setTimeout(function() {
                    if (typeof window.initECharts === 'function') window.initECharts(target);
                }, 50);
                status.textContent = '完了';
                status.className = 'mt-3 text-sm text-emerald-400';
                // 結果エリアまでスクロール
                target.scrollIntoView({ behavior: 'smooth', block: 'start' });
            })
            .catch(function(e) {
                status.textContent = 'アップロードエラーが発生しました';
                status.className = 'mt-3 text-sm text-red-400';
            });
    }
    </script>"##.to_string()
}

// =============================================================================
// Phase B: 分析結果（TL;DR + 分布 + 詳細）
// =============================================================================

/// CSVアップロード後の分析結果
pub(crate) fn render_analysis_result(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    session_id: &str,
) -> String {
    let mut html = String::with_capacity(12_000);

    html.push_str(r#"<div class="space-y-6 mt-6" id="survey-analysis">"#);

    // 1. TL;DR (最上部サマリ)
    html.push_str(&render_tldr(agg, seeker));

    // 2. アクションボタン（TL;DR直下に配置 - すぐに次ステップへ進めるように）
    html.push_str(&render_action_bar(session_id));
    html.push_str(r#"<div id="survey-integration-result"></div>"#);

    // 3. 給与サマリカード（主要KPI）
    html.push_str(&render_salary_summary(agg));

    // 4. 給与分布・雇用形態分布（チャート群）
    html.push_str(&render_distribution_charts(agg));

    // 5. 地域・タグ分布（折りたたみ）
    html.push_str(&render_breakdown_section(agg));

    // 6. 求職者心理分析（折りたたみ）
    html.push_str(&render_job_seeker_section(seeker));

    // 7. 詳細統計（折りたたみ: bootstrap CI, trimmed mean 等）
    html.push_str(&render_detailed_stats_section(agg));

    // 8. データ品質と注意事項
    html.push_str(&render_data_quality_section(agg));

    html.push_str("</div>");
    html
}

// =============================================================================
// セクション: TL;DR
// =============================================================================

fn render_tldr(agg: &SurveyAggregation, seeker: &JobSeekerAnalysis) -> String {
    let mut html = String::with_capacity(2_000);

    // 主要地域
    let region_text = match (&agg.dominant_prefecture, &agg.dominant_municipality) {
        (Some(p), Some(m)) => format!("{} {}", p, m),
        (Some(p), None) => p.clone(),
        _ => "地域不明".to_string(),
    };

    // 中央値
    let median_text = agg
        .enhanced_stats
        .as_ref()
        .map(|s| format!("{}円", format_number(s.median)))
        .unwrap_or_else(|| "—".to_string());

    // 期待給与
    let expected_text = seeker
        .expected_salary
        .map(|v| format!("{}円", format_number(v)))
        .unwrap_or_else(|| "—".to_string());

    // 新着率
    let new_rate = if agg.total_count > 0 {
        agg.new_count as f64 / agg.total_count as f64 * 100.0
    } else {
        0.0
    };

    html.push_str(&format!(
        r#"<section class="stat-card border-l-4 border-blue-500">
            <div class="flex items-start justify-between flex-wrap gap-3 mb-3">
                <div>
                    <h3 class="text-lg font-bold text-white">分析サマリ</h3>
                    <p class="text-xs text-slate-500 mt-0.5">アップロードされたCSVから抽出した主要指標</p>
                </div>
                <div class="text-xs text-slate-500 text-right">
                    <div>分析対象: {total}件</div>
                    <div>新着率: {new_rate:.1}%</div>
                </div>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
                <div class="p-3 bg-slate-800/50 rounded">
                    <div class="text-[10px] text-slate-500 mb-1">主要地域</div>
                    <div class="text-sm font-bold text-white truncate">{region}</div>
                    <div class="text-[10px] text-slate-600 mt-0.5">最多掲載エリア</div>
                </div>
                <div class="p-3 bg-slate-800/50 rounded">
                    <div class="text-[10px] text-slate-500 mb-1">給与中央値（月給換算）</div>
                    <div class="text-sm font-bold text-emerald-400">{median}</div>
                    <div class="text-[10px] text-slate-600 mt-0.5">時給・年俸は統一月給換算後</div>
                </div>
                <div class="p-3 bg-slate-800/50 rounded">
                    <div class="text-[10px] text-slate-500 mb-1">求職者期待値（推定）</div>
                    <div class="text-sm font-bold text-amber-400">{expected}</div>
                    <div class="text-[10px] text-slate-600 mt-0.5">レンジ下限+幅×33%の目安</div>
                </div>
            </div>
            <div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
                本サマリはアップロードされたCSVのみに基づく参考指標です。HWデータとの比較は下部「HWデータと統合分析」で確認してください。
            </div>
        </section>"#,
        total = format_number(agg.total_count as i64),
        new_rate = new_rate,
        region = escape_html(&region_text),
        median = median_text,
        expected = expected_text,
    ));

    html
}

// =============================================================================
// セクション: アクションバー
// =============================================================================

fn render_action_bar(session_id: &str) -> String {
    format!(
        r##"<section class="stat-card">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-emerald-500 pl-2">次のアクション</h3>
            <div class="flex gap-3 flex-wrap">
                <button hx-get="/api/survey/integrate?session_id={sid}"
                        hx-target="#survey-integration-result" hx-swap="innerHTML"
                        class="px-5 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded text-sm font-medium transition-colors">
                    HWデータと統合分析
                </button>
                <a href="/report/survey?session_id={sid}" target="_blank"
                   class="px-5 py-2.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-sm font-medium transition-colors inline-block">
                    印刷用レポート表示
                    <span class="text-[10px] opacity-75 block">新しいタブで印刷／PDF化</span>
                </a>
                <button type="button" onclick="downloadReportHtml('{sid}')"
                        class="px-5 py-2.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-sm font-medium transition-colors">
                    📄 HTML ダウンロード
                    <span class="text-[10px] opacity-75 block">編集後ブラウザで印刷 → PDF</span>
                </button>
                <a href="#" onclick="document.getElementById('survey-result').innerHTML='';document.getElementById('survey-root').scrollIntoView({{behavior:'smooth'}});return false;"
                   class="px-5 py-2.5 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors inline-block">
                    別のCSVをアップロード
                </a>
            </div>
            <p class="text-[11px] text-slate-500 mt-3">
                統合分析: この地域のHW求人・外部統計・企業データと突合 ／ 印刷用レポート: A4縦向けPDF用レイアウト
            </p>
        </section>
        <script>
        async function downloadReportHtml(sessionId) {{
            try {{
                const url = sessionId
                    ? '/report/survey/download?session_id=' + encodeURIComponent(sessionId)
                    : '/report/survey/download';
                const res = await fetch(url);
                if (!res.ok) {{ alert('HTMLダウンロードに失敗しました (HTTP ' + res.status + ')'); return; }}
                const blob = await res.blob();
                const blobUrl = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = blobUrl;
                a.download = 'hellowork_report_' + new Date().toISOString().slice(0, 10) + '.html';
                document.body.appendChild(a);
                a.click();
                document.body.removeChild(a);
                URL.revokeObjectURL(blobUrl);
            }} catch (e) {{
                alert('HTMLダウンロードでエラーが発生しました: ' + e.message);
            }}
        }}
        </script>"##,
        sid = session_id
    )
}

// =============================================================================
// セクション: 給与サマリカード
// =============================================================================

fn render_salary_summary(agg: &SurveyAggregation) -> String {
    let stats = match &agg.enhanced_stats {
        Some(s) => s,
        None => {
            return r#"<section class="stat-card">
                <h3 class="text-sm font-semibold text-slate-200 mb-2 border-l-4 border-amber-500 pl-2">給与統計</h3>
                <p class="text-xs text-amber-400">給与パース可能なレコードがありません。CSVの給与列形式を確認してください。</p>
            </section>"#.to_string();
        }
    };

    let mut html = String::with_capacity(2_000);
    html.push_str(
        r#"<section class="stat-card">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2">給与統計（月給換算）</h3>
            <div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#,
    );

    render_kpi_card(
        &mut html,
        "中央値",
        &format!("{}円", format_number(stats.median)),
        "text-emerald-400",
        "50パーセンタイル",
    );
    render_kpi_card(
        &mut html,
        "平均",
        &format!("{}円", format_number(stats.mean)),
        "text-white",
        "算術平均（外れ値影響あり）",
    );
    render_kpi_card(
        &mut html,
        "最低",
        &format!("{}円", format_number(stats.min)),
        "text-slate-300",
        "分布下端",
    );
    render_kpi_card(
        &mut html,
        "最高",
        &format!("{}円", format_number(stats.max)),
        "text-slate-300",
        "分布上端",
    );

    html.push_str("</div>");

    // データ信頼性インジケータ
    let reliability_color = match stats.reliability.as_str() {
        "高" => "text-emerald-400",
        "中" => "text-amber-400",
        _ => "text-slate-400",
    };
    html.push_str(&format!(
        r#"<div class="flex items-center gap-3 mt-3 text-xs">
            <span class="text-slate-500">信頼性:</span>
            <span class="font-bold {rc}">{rel}</span>
            <span class="text-slate-600">(有効 n={n})</span>
        </div>"#,
        rc = reliability_color,
        rel = escape_html(&stats.reliability),
        n = stats.count,
    ));

    html.push_str(r#"<p class="text-[11px] text-slate-600 mt-2 border-t border-slate-800 pt-2">月給換算は時給×173.8h/月、年俸÷12で統一。中央値は外れ値の影響を受けにくいため、平均より実勢に近い目安として推奨されます。</p>"#);
    html.push_str("</section>");
    html
}

// =============================================================================
// セクション: 分布チャート（給与帯 + 雇用形態）
// =============================================================================

fn render_distribution_charts(agg: &SurveyAggregation) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<section>
        <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2">分布</h3>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">"#);

    // 給与帯分布
    if !agg.by_salary_range.is_empty() {
        html.push_str(&render_salary_range_chart(agg));
    }

    // 雇用形態分布
    if !agg.by_employment_type.is_empty() {
        html.push_str(&render_employment_type_chart(agg));
    }

    html.push_str("</div></section>");
    html
}

fn render_salary_range_chart(agg: &SurveyAggregation) -> String {
    let labels: Vec<serde_json::Value> =
        agg.by_salary_range.iter().map(|(l, _)| json!(l)).collect();
    let values: Vec<serde_json::Value> =
        agg.by_salary_range.iter().map(|(_, v)| json!(v)).collect();

    let mut chart = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}, "formatter": "{b}<br/>件数: {c}"},
        "grid": {"left": "10%", "right": "5%", "top": "15%", "bottom": "22%"},
        "xAxis": {
            "type": "category",
            "data": labels,
            "axisLabel": {"color": "#94a3b8", "fontSize": 10, "rotate": 30}
        },
        "yAxis": {
            "type": "value",
            "axisLabel": {"color": "#94a3b8"},
            "splitLine": {"lineStyle": {"color": "#1e293b"}}
        },
        "series": [{
            "type": "bar",
            "data": values,
            "itemStyle": {"color": "#3b82f6", "borderRadius": [4, 4, 0, 0]},
            "label": {"show": true, "position": "top", "color": "#cbd5e1", "fontSize": 10}
        }]
    });

    if let Some(stats) = &agg.enhanced_stats {
        chart["series"][0]["markLine"] = json!({
            "silent": true,
            "symbol": "none",
            "lineStyle": {"type": "dashed", "width": 1},
            "label": {"color": "#e2e8f0", "fontSize": 10},
            "data": [
                {"yAxis": stats.median, "name": "中央値", "lineStyle": {"color": "#10b981"}},
                {"yAxis": stats.mean, "name": "平均", "lineStyle": {"color": "#f59e0b"}}
            ]
        });
    }

    let config_str = chart.to_string().replace('\'', "&#39;");
    format!(
        r#"<div class="stat-card">
            <h4 class="text-xs font-semibold text-slate-300 mb-2">給与帯分布</h4>
            <div class="echart" style="height:300px" data-chart-config='{config_str}'></div>
            <p class="text-[11px] text-slate-600 mt-2">破線: 中央値（緑）/ 平均（橙）</p>
        </div>"#
    )
}

fn render_employment_type_chart(agg: &SurveyAggregation) -> String {
    // カラーブラインドセーフなパレット（insight と整合）
    let colors = [
        "#3b82f6", "#10b981", "#f59e0b", "#8b5cf6", "#ec4899", "#06b6d4", "#ef4444", "#64748b",
    ];
    let pie_data: Vec<serde_json::Value> = agg
        .by_employment_type
        .iter()
        .enumerate()
        .map(|(i, (name, val))| {
            json!({
                "value": val,
                "name": name,
                "itemStyle": {"color": colors[i % colors.len()]}
            })
        })
        .collect();

    let chart = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}<br/>{c}件 ({d}%)"},
        "legend": {
            "bottom": "0%",
            "textStyle": {"color": "#94a3b8", "fontSize": 10},
            "itemWidth": 10,
            "itemHeight": 10
        },
        "series": [{
            "type": "pie",
            "radius": ["45%", "70%"],
            "center": ["50%", "45%"],
            "data": pie_data,
            "label": {"color": "#e2e8f0", "fontSize": 10, "formatter": "{b}\n{d}%"},
            "emphasis": {"itemStyle": {"shadowBlur": 10, "shadowColor": "rgba(0,0,0,0.5)"}}
        }]
    });

    let config_str = chart.to_string().replace('\'', "&#39;");
    format!(
        r#"<div class="stat-card">
            <h4 class="text-xs font-semibold text-slate-300 mb-2">雇用形態分布</h4>
            <div class="echart" style="height:300px" data-chart-config='{config_str}'></div>
            <p class="text-[11px] text-slate-600 mt-2">雇用形態別の掲載件数比率</p>
        </div>"#
    )
}

// =============================================================================
// セクション: 地域・タグ分布（折りたたみ）
// =============================================================================

fn render_breakdown_section(agg: &SurveyAggregation) -> String {
    if agg.by_prefecture.is_empty() && agg.by_tags.is_empty() {
        return String::new();
    }

    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<section class="stat-card">
        <details open>
            <summary class="cursor-pointer text-sm font-semibold text-slate-200 border-l-4 border-blue-500 pl-2 select-none hover:text-white">
                地域・タグの内訳（Top 15）
            </summary>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">"#);

    // 地域分布
    if !agg.by_prefecture.is_empty() {
        let top15: Vec<&(String, usize)> = agg.by_prefecture.iter().take(15).collect();
        let labels: Vec<serde_json::Value> = top15.iter().rev().map(|(l, _)| json!(l)).collect();
        let values: Vec<serde_json::Value> = top15.iter().rev().map(|(_, v)| json!(v)).collect();

        let chart = json!({
            "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}, "formatter": "{b}<br/>件数: {c}"},
            "grid": {"left": "22%", "right": "12%", "top": "5%", "bottom": "5%"},
            "xAxis": {"type": "value", "axisLabel": {"color": "#94a3b8"}, "splitLine": {"lineStyle": {"color": "#1e293b"}}},
            "yAxis": {
                "type": "category",
                "data": labels,
                "axisLabel": {"color": "#e2e8f0", "fontSize": 11}
            },
            "series": [{
                "type": "bar",
                "data": values,
                "itemStyle": {"color": "#10b981", "borderRadius": [0, 4, 4, 0]},
                "label": {"show": true, "position": "right", "color": "#cbd5e1", "fontSize": 10}
            }]
        });

        let config_str = chart.to_string().replace('\'', "&#39;");
        html.push_str(&format!(
            r#"<div class="bg-slate-900/50 rounded p-3">
                <h4 class="text-xs font-semibold text-slate-300 mb-2">都道府県別 掲載件数</h4>
                <div class="echart" style="height:400px" data-chart-config='{config_str}'></div>
            </div>"#
        ));
    }

    // タグ分布
    if !agg.by_tags.is_empty() {
        let top15: Vec<&(String, usize)> = agg.by_tags.iter().take(15).collect();
        let labels: Vec<serde_json::Value> = top15.iter().rev().map(|(l, _)| json!(l)).collect();
        let values: Vec<serde_json::Value> = top15.iter().rev().map(|(_, v)| json!(v)).collect();

        let chart = json!({
            "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}, "formatter": "{b}<br/>件数: {c}"},
            "grid": {"left": "30%", "right": "12%", "top": "5%", "bottom": "5%"},
            "xAxis": {"type": "value", "axisLabel": {"color": "#94a3b8"}, "splitLine": {"lineStyle": {"color": "#1e293b"}}},
            "yAxis": {
                "type": "category",
                "data": labels,
                "axisLabel": {"color": "#e2e8f0", "fontSize": 10}
            },
            "series": [{
                "type": "bar",
                "data": values,
                "itemStyle": {"color": "#f59e0b", "borderRadius": [0, 4, 4, 0]},
                "label": {"show": true, "position": "right", "color": "#cbd5e1", "fontSize": 10}
            }]
        });

        let config_str = chart.to_string().replace('\'', "&#39;");
        html.push_str(&format!(
            r#"<div class="bg-slate-900/50 rounded p-3">
                <h4 class="text-xs font-semibold text-slate-300 mb-2">求人タグ 頻出Top 15</h4>
                <div class="echart" style="height:400px" data-chart-config='{config_str}'></div>
            </div>"#
        ));
    }

    html.push_str(r#"</div>
        <p class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
            タグの頻出は訴求ポイントの傾向を示しますが、件数が多い=重要とは限らず、自社ターゲット層との整合性で判断してください。
        </p>
        </details>
    </section>"#);

    html
}

// =============================================================================
// セクション: 求職者心理分析（折りたたみ）
// =============================================================================

fn render_job_seeker_section(seeker: &JobSeekerAnalysis) -> String {
    if seeker.salary_range_perception.is_none() && seeker.inexperience_analysis.is_none() {
        return String::new();
    }

    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<section class="stat-card">
        <details>
            <summary class="cursor-pointer text-sm font-semibold text-slate-200 border-l-4 border-amber-500 pl-2 select-none hover:text-white">
                求職者心理分析（給与レンジ・未経験可）
            </summary>
            <div class="mt-4 space-y-4">"#);

    // 給与レンジ知覚
    if let Some(perception) = &seeker.salary_range_perception {
        html.push_str(&format!(
            r#"<div class="bg-slate-900/50 rounded p-3">
                <h4 class="text-xs font-semibold text-slate-300 mb-2">給与レンジ知覚モデル</h4>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-2 mb-3">
                    <div class="p-2 bg-slate-800/60 rounded text-center">
                        <div class="text-xs text-slate-500">期待給与（推定）</div>
                        <div class="text-sm font-bold text-amber-400">{expected}円</div>
                    </div>
                    <div class="p-2 bg-slate-800/60 rounded text-center">
                        <div class="text-xs text-slate-500">レンジ平均下限</div>
                        <div class="text-sm text-white">{lower}円</div>
                    </div>
                    <div class="p-2 bg-slate-800/60 rounded text-center">
                        <div class="text-xs text-slate-500">レンジ平均上限</div>
                        <div class="text-sm text-white">{upper}円</div>
                    </div>
                    <div class="p-2 bg-slate-800/60 rounded text-center">
                        <div class="text-xs text-slate-500">レンジ平均幅</div>
                        <div class="text-sm text-white">{width}円</div>
                    </div>
                </div>
                <div class="text-xs text-slate-400 mb-2">
                    レンジ幅の分布: 狭い(&lt;5万){narrow}件 / 中(5〜10万){medium}件 / 広い(&gt;10万){wide}件
                </div>
                <p class="text-[11px] text-slate-600">
                    求職者は給与レンジの下限〜下1/3地点を現実的な期待値とする傾向があります（一般的な応募行動モデル）。
                    上限のみを強調する媒体は応募ギャップを生みやすいため、下限設計が重要です。
                </p>
            </div>"#,
            expected = format_number(perception.expected_point),
            lower = format_number(perception.avg_lower),
            upper = format_number(perception.avg_upper),
            width = format_number(perception.avg_range_width),
            narrow = perception.narrow_count,
            medium = perception.medium_count,
            wide = perception.wide_count,
        ));

        // レンジ幅ドーナツチャート
        let total = perception.narrow_count + perception.medium_count + perception.wide_count;
        if total > 0 {
            let chart = json!({
                "tooltip": {"trigger": "item", "formatter": "{b}<br/>{c}件 ({d}%)"},
                "legend": {
                    "bottom": "0%",
                    "textStyle": {"color": "#94a3b8", "fontSize": 10},
                    "itemWidth": 10,
                    "itemHeight": 10
                },
                "series": [{
                    "type": "pie",
                    "radius": ["45%", "70%"],
                    "center": ["50%", "45%"],
                    "data": [
                        {"value": perception.narrow_count, "name": "狭い (<5万円)", "itemStyle": {"color": "#3b82f6"}},
                        {"value": perception.medium_count, "name": "中程度 (5〜10万円)", "itemStyle": {"color": "#10b981"}},
                        {"value": perception.wide_count, "name": "広い (>10万円)", "itemStyle": {"color": "#f59e0b"}}
                    ],
                    "label": {"color": "#e2e8f0", "fontSize": 10, "formatter": "{b}\n{d}%"}
                }]
            });

            let config_str = chart.to_string().replace('\'', "&#39;");
            html.push_str(&format!(
                r#"<div class="bg-slate-900/50 rounded p-3">
                    <h4 class="text-xs font-semibold text-slate-300 mb-2">給与レンジ幅 分布</h4>
                    <div class="echart" style="height:280px" data-chart-config='{config_str}'></div>
                </div>"#
            ));
        }
    }

    // 未経験タグ
    if let Some(inexp) = &seeker.inexperience_analysis {
        if let Some(gap) = inexp.salary_gap {
            // ギャップの方向: 正=経験者が高い、負=未経験者のほうが高い（稀）
            let (gap_color, gap_label) = if gap > 0 {
                ("text-amber-400", "経験者の方が高い")
            } else if gap < 0 {
                ("text-emerald-400", "未経験者の方が高い（稀）")
            } else {
                ("text-slate-400", "差なし")
            };

            html.push_str(&format!(
                r#"<div class="bg-slate-900/50 rounded p-3">
                    <h4 class="text-xs font-semibold text-slate-300 mb-2">未経験可タグの給与影響</h4>
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-2 mb-3">
                        <div class="p-2 bg-slate-800/60 rounded">
                            <div class="text-xs text-slate-500">経験者向け</div>
                            <div class="text-sm text-white">{exp_cnt}件 / 平均{exp_sal}円</div>
                        </div>
                        <div class="p-2 bg-slate-800/60 rounded">
                            <div class="text-xs text-slate-500">未経験可</div>
                            <div class="text-sm text-white">{inexp_cnt}件 / 平均{inexp_sal}円</div>
                        </div>
                        <div class="p-2 bg-slate-800/60 rounded">
                            <div class="text-xs text-slate-500">給与差</div>
                            <div class="text-sm font-bold {color}">{gap:+}円</div>
                            <div class="text-[10px] text-slate-600">{label}</div>
                        </div>
                    </div>
                    <p class="text-[11px] text-slate-600">
                        「未経験可」タグは参入障壁を下げる反面、給与面で経験者向け求人より低くなる傾向が見られます。
                        これは相関であり、タグの有無が直接給与を決定することを示すものではありません。
                    </p>
                </div>"#,
                exp_cnt = inexp.experience_count,
                exp_sal = inexp.experience_avg_salary.map(format_number).unwrap_or_default(),
                inexp_cnt = inexp.inexperience_count,
                inexp_sal = inexp.inexperience_avg_salary.map(format_number).unwrap_or_default(),
                gap = gap,
                color = gap_color,
                label = gap_label,
            ));
        }
    }

    html.push_str(r#"</div>
        </details>
    </section>"#);

    html
}

// =============================================================================
// セクション: 詳細統計（bootstrap CI / trimmed mean）
// =============================================================================

fn render_detailed_stats_section(agg: &SurveyAggregation) -> String {
    let stats = match &agg.enhanced_stats {
        Some(s) => s,
        None => return String::new(),
    };

    if stats.bootstrap_ci.is_none() && stats.trimmed_mean.is_none() {
        return String::new();
    }

    let mut html = String::with_capacity(1_500);
    html.push_str(r#"<section class="stat-card">
        <details>
            <summary class="cursor-pointer text-sm font-semibold text-slate-200 border-l-4 border-slate-500 pl-2 select-none hover:text-white">
                詳細統計（信頼区間・トリム平均）
            </summary>
            <div class="mt-4 space-y-2 text-xs text-slate-300">"#);

    if let Some(ci) = &stats.bootstrap_ci {
        html.push_str(&format!(
            r#"<div class="p-2 bg-slate-900/50 rounded">
                <div class="text-slate-400 text-[11px] mb-1">95%信頼区間（Bootstrap）</div>
                <div>{lower}円 〜 {upper}円</div>
                <div class="text-[10px] text-slate-600 mt-1">n={n} / {iter}回リサンプリング / 平均の不確実性範囲</div>
            </div>"#,
            lower = format_number(ci.lower),
            upper = format_number(ci.upper),
            n = ci.sample_size,
            iter = ci.iterations,
        ));
    }

    if let Some(tm) = &stats.trimmed_mean {
        html.push_str(&format!(
            r#"<div class="p-2 bg-slate-900/50 rounded">
                <div class="text-slate-400 text-[11px] mb-1">トリム平均（10%）</div>
                <div>{val}円</div>
                <div class="text-[10px] text-slate-600 mt-1">外れ値{rm}件を除外した平均（ロバスト指標）</div>
            </div>"#,
            val = format_number(tm.trimmed_mean),
            rm = tm.removed_count,
        ));
    }

    if let Some(q) = &stats.quartiles {
        html.push_str(&format!(
            r#"<div class="p-2 bg-slate-900/50 rounded">
                <div class="text-slate-400 text-[11px] mb-1">四分位</div>
                <div>Q1: {q1}円 / Q2(中央値): {q2}円 / Q3: {q3}円 / IQR: {iqr}円</div>
            </div>"#,
            q1 = format_number(q.q1),
            q2 = format_number(q.q2),
            q3 = format_number(q.q3),
            iqr = format_number(q.iqr),
        ));
    }

    html.push_str(r#"</div>
        </details>
    </section>"#);

    html
}

// =============================================================================
// セクション: データ品質
// =============================================================================

fn render_data_quality_section(agg: &SurveyAggregation) -> String {
    let salary_rate = agg.salary_parse_rate * 100.0;
    let location_rate = agg.location_parse_rate * 100.0;

    // 品質ステータスの色分け
    let salary_color = if salary_rate >= 80.0 {
        "text-emerald-400"
    } else if salary_rate >= 60.0 {
        "text-amber-400"
    } else {
        "text-red-400"
    };
    let location_color = if location_rate >= 80.0 {
        "text-emerald-400"
    } else if location_rate >= 60.0 {
        "text-amber-400"
    } else {
        "text-red-400"
    };

    let warn = if salary_rate < 60.0 || location_rate < 60.0 {
        r#"<p class="text-[11px] text-amber-400 mt-2">パース率が低いため、統計値の信頼性が限定的です。CSVの列形式（給与表記、住所列）を確認してください。</p>"#
    } else {
        ""
    };

    format!(
        r#"<section class="stat-card">
            <details>
                <summary class="cursor-pointer text-sm font-semibold text-slate-200 border-l-4 border-slate-500 pl-2 select-none hover:text-white">
                    データ品質とスコープ
                </summary>
                <div class="mt-3 grid grid-cols-2 gap-3">
                    <div class="p-2 bg-slate-900/50 rounded">
                        <div class="text-[11px] text-slate-500">給与パース率</div>
                        <div class="text-sm font-bold {sc}">{sr:.1}%</div>
                        <div class="text-[10px] text-slate-600">時給・月給・年俸から月給換算できた割合</div>
                    </div>
                    <div class="p-2 bg-slate-900/50 rounded">
                        <div class="text-[11px] text-slate-500">住所パース率</div>
                        <div class="text-sm font-bold {lc}">{lr:.1}%</div>
                        <div class="text-[10px] text-slate-600">都道府県まで特定できた割合</div>
                    </div>
                </div>
                {warn}
                <div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2 space-y-1">
                    <div>・本分析はアップロードされたCSVのみに基づきます。求人市場全体の代表値ではありません。</div>
                    <div>・HWデータとの比較は「HWデータと統合分析」で実施されますが、HW掲載は全求人の一部であり産業偏り（IT・通信は少ない等）があります。</div>
                    <div>・相関指標（例: 未経験タグと給与差）は因果関係を示すものではありません。</div>
                </div>
            </details>
        </section>"#,
        sc = salary_color,
        sr = salary_rate,
        lc = location_color,
        lr = location_rate,
        warn = warn,
    )
}

// =============================================================================
// 共通ヘルパー
// =============================================================================

fn render_kpi_card(html: &mut String, label: &str, value: &str, value_color: &str, note: &str) {
    html.push_str(&format!(
        r#"<div class="p-3 bg-slate-800/50 rounded text-center">
            <div class="text-[11px] text-slate-500 mb-1">{label}</div>
            <div class="text-sm font-bold {color}">{value}</div>
            <div class="text-[10px] text-slate-600 mt-0.5">{note}</div>
        </div>"#,
        label = escape_html(label),
        value = value,
        color = value_color,
        note = escape_html(note),
    ));
}
