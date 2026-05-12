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

use std::fmt::Write as _;
// =============================================================================
// Phase A: アップロードフォーム
// =============================================================================

/// 初期表示: CSVアップロードフォーム
pub(crate) fn render_upload_form() -> String {
    r##"<div class="space-y-6" id="survey-root" data-survey-ui-version="ui1-2026-04-26">
        <!-- ヘッダー -->
        <header class="stat-card">
            <div class="flex items-start justify-between flex-wrap gap-3">
                <div>
                    <h2 class="text-xl font-bold text-white">媒体分析
                        <span class="text-blue-400 text-base font-normal">求人媒体 CSV 取込</span>
                    </h2>
                    <p class="text-xs text-slate-400 mt-1">
                        ユーザーがエクスポートした求人媒体CSVをアップロードし、HWデータ・外部統計と突き合わせて地域別の相対比較を行います。
                    </p>
                </div>
                <div class="text-xs text-slate-500 text-right">
                    <div>対応形式: 主要求人媒体 CSV</div>
                    <div>文字コード: UTF-8（CSV/TXT）</div>
                </div>
            </div>
        </header>

        <!-- 使い方ステップ表示（番号付き図） -->
        <section class="stat-card" id="survey-howto-steps" aria-label="使い方ステップ">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-slate-500 pl-2">使い方</h3>
            <ol class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3" role="list">
                <li class="bg-slate-800/40 rounded p-3 flex gap-3 items-start">
                    <div class="flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 text-white text-sm font-bold flex items-center justify-center" aria-hidden="true">1</div>
                    <div>
                        <div class="text-xs font-semibold text-white mb-0.5">CSVエクスポート</div>
                        <div class="text-[11px] text-slate-400">利用中の求人媒体またはExcelで求人一覧をUTF-8 CSVに出力</div>
                    </div>
                </li>
                <li class="bg-slate-800/40 rounded p-3 flex gap-3 items-start">
                    <div class="flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 text-white text-sm font-bold flex items-center justify-center" aria-hidden="true">2</div>
                    <div>
                        <div class="text-xs font-semibold text-white mb-0.5">アップロード</div>
                        <div class="text-[11px] text-slate-400">下のドロップゾーンへドラッグ＆ドロップで自動パース</div>
                    </div>
                </li>
                <li class="bg-slate-800/40 rounded p-3 flex gap-3 items-start">
                    <div class="flex-shrink-0 w-8 h-8 rounded-full bg-emerald-600 text-white text-sm font-bold flex items-center justify-center" aria-hidden="true">3</div>
                    <div>
                        <div class="text-xs font-semibold text-white mb-0.5">サマリ確認</div>
                        <div class="text-[11px] text-slate-400">給与中央値・地域分布・雇用形態を即時表示</div>
                    </div>
                </li>
                <li class="bg-slate-800/40 rounded p-3 flex gap-3 items-start">
                    <div class="flex-shrink-0 w-8 h-8 rounded-full bg-emerald-600 text-white text-sm font-bold flex items-center justify-center" aria-hidden="true">4</div>
                    <div>
                        <div class="text-xs font-semibold text-white mb-0.5">HW統合分析</div>
                        <div class="text-[11px] text-slate-400">「HWデータと統合分析」で比較レポート生成</div>
                    </div>
                </li>
            </ol>
        </section>

        <!-- アップロードセクション -->
        <section class="stat-card">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2">
                CSVファイルをアップロード
            </h3>
            <form id="survey-upload-form" enctype="multipart/form-data">
                <!-- ソース媒体: ラジオカード形式で視覚化 -->
                <div class="mb-4" id="source-type-cards" role="radiogroup" aria-label="ソース媒体">
                    <label class="block text-xs text-slate-400 mb-2">ソース媒体 <span class="text-red-400" aria-label="必須">*</span>
                        <span class="text-[10px] text-slate-500 ml-2">列名マッピングの精度向上のため明示指定してください</span>
                    </label>
                    <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
                        <label class="source-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-source="indeed">
                            <input type="radio" name="source_type" value="indeed" class="mt-1" checked aria-describedby="src-indeed-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5">
                                    <span class="inline-block w-3 h-3 rounded-full bg-blue-500" aria-hidden="true"></span>
                                    Indeed
                                </div>
                                <div id="src-indeed-desc" class="text-[10px] text-slate-400 mt-0.5">広域求人サイト・列名は英字混在</div>
                            </div>
                        </label>
                        <label class="source-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-source="jobbox">
                            <input type="radio" name="source_type" value="jobbox" class="mt-1" aria-describedby="src-jobbox-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5">
                                    <span class="inline-block w-3 h-3 rounded-full bg-emerald-500" aria-hidden="true"></span>
                                    求人ボックス
                                </div>
                                <div id="src-jobbox-desc" class="text-[10px] text-slate-400 mt-0.5">国内求人ポータル・日本語列名</div>
                            </div>
                        </label>
                        <label class="source-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-source="other">
                            <input type="radio" name="source_type" value="other" class="mt-1" aria-describedby="src-other-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5">
                                    <span class="inline-block w-3 h-3 rounded-full bg-amber-500" aria-hidden="true"></span>
                                    その他 / 手動編集
                                </div>
                                <div id="src-other-desc" class="text-[10px] text-slate-400 mt-0.5">汎用CSV・列名フリー</div>
                            </div>
                        </label>
                    </div>
                </div>

                <!-- 給与単位: ラジオカード形式で視覚化 -->
                <div class="mb-4" id="wage-mode-cards" role="radiogroup" aria-label="給与単位">
                    <label class="block text-xs text-slate-400 mb-2">給与単位 <span class="text-red-400" aria-label="必須">*</span>
                        <span class="text-[10px] text-slate-500 ml-2">分析結果の単位が直感と一致するよう選択</span>
                    </label>
                    <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
                        <label class="wage-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-wage="monthly">
                            <input type="radio" name="wage_mode" value="monthly" class="mt-1" checked aria-describedby="wage-monthly-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5" aria-hidden="true">
                                    <span class="inline-block w-2 h-2 rounded-full bg-blue-500"></span>
                                    月給ベース
                                </div>
                                <div id="wage-monthly-desc" class="text-[10px] text-slate-400 mt-0.5">正社員・契約社員 など長期雇用中心</div>
                            </div>
                        </label>
                        <label class="wage-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-wage="hourly">
                            <input type="radio" name="wage_mode" value="hourly" class="mt-1" aria-describedby="wage-hourly-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5" aria-hidden="true">
                                    <span class="inline-block w-2 h-2 rounded-full bg-amber-500"></span>
                                    時給ベース
                                </div>
                                <div id="wage-hourly-desc" class="text-[10px] text-slate-400 mt-0.5">パート・アルバイト・派遣 中心</div>
                            </div>
                        </label>
                        <label class="wage-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-wage="auto">
                            <input type="radio" name="wage_mode" value="auto" class="mt-1" aria-describedby="wage-auto-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5" aria-hidden="true">
                                    <span class="inline-block w-2 h-2 rounded-full bg-slate-400"></span>
                                    自動判定
                                </div>
                                <div id="wage-auto-desc" class="text-[10px] text-slate-400 mt-0.5">雇用形態ごとに時給/月給を切替</div>
                            </div>
                        </label>
                    </div>
                </div>

                <!-- ドロップゾーン（強化版） -->
                <div id="drop-zone"
                     class="border-2 border-dashed border-slate-600 rounded-lg p-10 text-center cursor-pointer hover:border-blue-500 hover:bg-slate-800/30 transition-all duration-200"
                     role="button" tabindex="0" aria-label="CSVファイルをドラッグ＆ドロップ、またはクリックで選択"
                     ondragover="event.preventDefault();this.classList.add('border-blue-500','bg-blue-500/10','scale-[1.01]')"
                     ondragleave="this.classList.remove('border-blue-500','bg-blue-500/10','scale-[1.01]')"
                     ondrop="event.preventDefault();this.classList.remove('border-blue-500','bg-blue-500/10','scale-[1.01]');handleDrop(event)">
                    <svg class="w-12 h-12 mx-auto mb-3 text-blue-400 animate-pulse" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"/>
                    </svg>
                    <div class="text-slate-200 text-base font-semibold mb-1">CSVをここにドロップ</div>
                    <div class="text-slate-500 text-xs mb-4">または</div>
                    <label class="inline-flex items-center gap-2 px-5 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded text-sm font-medium cursor-pointer transition-colors min-h-[44px]">
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                        </svg>
                        ファイルを選択
                        <input type="file" name="csv_file" accept=".csv,.txt" class="hidden" onchange="submitSurveyCSV(this.files[0])">
                    </label>
                    <div class="text-slate-500 text-xs mt-4">対応形式: 主要求人媒体 CSV (UTF-8)</div>
                </div>
            </form>
            <div id="upload-status" class="mt-3" aria-live="polite"></div>
            <div class="text-xs text-slate-600 mt-3 border-t border-slate-800 pt-3">
                アップロードしたCSVはブラウザセッション内でのみ処理され、永続保存されません。
                HW掲載求人との比較は相対的な参考値であり、採用判断の唯一の根拠としないでください。
            </div>
        </section>

        <!-- サンプルCSV列の折畳展開 -->
        <section class="stat-card" id="survey-csv-samples">
            <details>
                <summary class="cursor-pointer text-sm font-semibold text-slate-200 border-l-4 border-slate-500 pl-2 select-none hover:text-white">
                    対応CSV列の例（クリックで展開）
                </summary>
                <div class="mt-3 grid grid-cols-1 lg:grid-cols-2 gap-3">
                    <div class="bg-slate-900/50 rounded p-3">
                        <div class="text-xs font-semibold text-blue-400 mb-2">英字列名形式 CSV 主要列 (例)</div>
                        <table class="w-full text-[11px] text-slate-300">
                            <thead><tr class="border-b border-slate-700"><th class="text-left py-1 pr-2">列名</th><th class="text-left py-1">用途</th></tr></thead>
                            <tbody class="text-slate-400">
                                <tr><td class="py-0.5 pr-2 font-mono">Job Title</td><td>求人タイトル</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">Location</td><td>勤務地（都道府県・市区町村）</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">Salary</td><td>給与（時給/月給/年俸）</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">Job Type</td><td>雇用形態</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">Date Posted</td><td>掲載日</td></tr>
                            </tbody>
                        </table>
                    </div>
                    <div class="bg-slate-900/50 rounded p-3">
                        <div class="text-xs font-semibold text-emerald-400 mb-2">日本語列名形式 CSV 主要列 (例)</div>
                        <table class="w-full text-[11px] text-slate-300">
                            <thead><tr class="border-b border-slate-700"><th class="text-left py-1 pr-2">列名</th><th class="text-left py-1">用途</th></tr></thead>
                            <tbody class="text-slate-400">
                                <tr><td class="py-0.5 pr-2 font-mono">求人タイトル</td><td>タイトル</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">勤務地</td><td>都道府県・市区町村</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">給与</td><td>時給/月給/年俸表記</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">雇用形態</td><td>正社員・パート 等</td></tr>
                                <tr><td class="py-0.5 pr-2 font-mono">掲載日</td><td>日付</td></tr>
                            </tbody>
                        </table>
                    </div>
                </div>
                <p class="text-[11px] text-slate-600 mt-3">列名が一致しない場合も自動マッピングを試行します。マッピング失敗時は「データ品質」セクションでパース率を確認してください。</p>
            </details>
        </section>

        <div id="survey-result"></div>
    </div>
    <script>
    // ラジオカード選択時のハイライト（source/wage 共通）
    (function() {
        function syncCards(groupSelector, activeClasses) {
            document.querySelectorAll(groupSelector).forEach(function(card) {
                var input = card.querySelector('input[type="radio"]');
                if (!input) return;
                var apply = function() {
                    document.querySelectorAll(groupSelector).forEach(function(c) {
                        c.classList.remove('border-blue-500','bg-blue-500/10','ring-1','ring-blue-500');
                    });
                    if (input.checked) {
                        card.classList.add('border-blue-500','bg-blue-500/10','ring-1','ring-blue-500');
                    }
                };
                input.addEventListener('change', apply);
                apply();
            });
        }
        syncCards('.source-card');
        syncCards('.wage-card');
    })();
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
        // ユーザー明示指定を同送信（自動判定より優先）
        var src = document.querySelector('input[name="source_type"]:checked');
        var wage = document.querySelector('input[name="wage_mode"]:checked');
        if (src) fd.append('source_type', src.value);
        if (wage) fd.append('wage_mode', wage.value);
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

    // 4b. 都道府県別 KPI ヒートマップ（新規）
    html.push_str(&render_prefecture_heatmap_section(agg));

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

    // 中央値と期待値のギャップを評価（読み手の判断材料）
    let median_val = agg.enhanced_stats.as_ref().map(|s| s.median).unwrap_or(0);
    let expected_val = seeker.expected_salary.unwrap_or(0);
    let (gap_pct, gap_color, gap_label) = if median_val > 0 && expected_val > 0 {
        let pct = (median_val - expected_val) as f64 / expected_val as f64 * 100.0;
        if pct >= 5.0 {
            (
                pct,
                "text-emerald-400",
                "求職者期待値より中央値が高水準です。応募集まりやすい給与帯。",
            )
        } else if pct <= -5.0 {
            (
                pct,
                "text-rose-400",
                "求職者期待値より中央値が低水準です。給与訴求の強化を検討。",
            )
        } else {
            (
                pct,
                "text-amber-400",
                "求職者期待値とほぼ同等です。差別化要素を給与以外でも訴求してください。",
            )
        }
    } else {
        (
            0.0,
            "text-slate-500",
            "（期待値推定不能のため比較スキップ）",
        )
    };
    let gap_pct_text = if median_val > 0 && expected_val > 0 {
        format!("{:+.1}%", gap_pct)
    } else {
        "—".to_string()
    };

    // 新着率の色判定（高いほど鮮度良）
    let new_rate_color = if new_rate >= 30.0 {
        "text-emerald-400"
    } else if new_rate >= 15.0 {
        "text-amber-400"
    } else {
        "text-slate-400"
    };

    write!(html,
        r#"<section class="stat-card border-l-4 border-blue-500" id="survey-executive-summary" data-total="{total_raw}">
            <div class="flex items-start justify-between flex-wrap gap-3 mb-4">
                <div>
                    <h3 class="text-lg font-bold text-white flex items-center gap-2">
                        <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/></svg>
                        エグゼクティブサマリ
                    </h3>
                    <p class="text-xs text-slate-500 mt-0.5">アップロードCSVから抽出した「この地域・職種で見るべき主要KPI」</p>
                </div>
                <div class="text-xs text-slate-500 text-right">
                    <div>分析対象: <span class="text-white font-semibold">{total}件</span></div>
                    <div>新着率: <span class="font-semibold {new_rate_color}">{new_rate:.1}%</span></div>
                </div>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3" id="survey-kpi-grid">
                <!-- KPI 1: 主要地域 -->
                <div class="p-4 bg-slate-800/60 rounded border border-slate-700/50" data-kpi="region">
                    <div class="flex items-center justify-between mb-2">
                        <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
                            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 11a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
                            主要地域
                        </div>
                        <span class="kpi-info" tabindex="0" role="button" aria-label="主要地域の説明" title="掲載件数が最も多い都道府県・市区町村。求人の主戦場を示します。">ⓘ</span>
                    </div>
                    <div class="text-base font-bold text-white truncate" title="{region}">{region}</div>
                    <div class="text-[10px] text-slate-500 mt-1">最多掲載エリア（CSV基準）</div>
                </div>
                <!-- KPI 2: 給与中央値 -->
                <div class="p-4 bg-slate-800/60 rounded border border-slate-700/50" data-kpi="median">
                    <div class="flex items-center justify-between mb-2">
                        <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
                            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/></svg>
                            給与中央値
                        </div>
                        <span class="kpi-info" tabindex="0" role="button" aria-label="給与中央値の説明" title="50パーセンタイル。外れ値の影響を受けにくく、平均より実勢に近い指標です（時給・年俸は月給換算後）。">ⓘ</span>
                    </div>
                    <div class="text-2xl font-bold text-emerald-400 leading-tight">{median}</div>
                    <div class="text-[10px] text-slate-500 mt-1">月給換算（時給×167h / 年俸÷12）</div>
                </div>
                <!-- KPI 3: 求職者期待値 -->
                <div class="p-4 bg-slate-800/60 rounded border border-slate-700/50" data-kpi="expected">
                    <div class="flex items-center justify-between mb-2">
                        <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
                            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/></svg>
                            求職者期待値
                        </div>
                        <span class="kpi-info" tabindex="0" role="button" aria-label="求職者期待値の説明" title="レンジ下限 + 幅 × 1/3 で推定。求職者が現実的に意識する応募ライン（一般的応募行動モデル）。">ⓘ</span>
                    </div>
                    <div class="text-2xl font-bold text-amber-400 leading-tight">{expected}</div>
                    <div class="text-[10px] text-slate-500 mt-1">推定モデル（応募行動研究ベース）</div>
                </div>
                <!-- KPI 4: 中央値 vs 期待値 ギャップ -->
                <div class="p-4 bg-slate-800/60 rounded border border-slate-700/50" data-kpi="gap">
                    <div class="flex items-center justify-between mb-2">
                        <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
                            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6"/></svg>
                            期待値ギャップ
                        </div>
                        <span class="kpi-info" tabindex="0" role="button" aria-label="ギャップの説明" title="(中央値 − 期待値) ÷ 期待値 × 100。プラスは求職者期待を上回る訴求力、マイナスは応募集まりにくさのシグナル。">ⓘ</span>
                    </div>
                    <div class="text-2xl font-bold {gap_color} leading-tight">{gap_pct_text}</div>
                    <div class="text-[10px] text-slate-500 mt-1">中央値 − 期待値 の相対差</div>
                </div>
            </div>
            <!-- 読み方吹き出し -->
            <div class="mt-4 p-3 bg-slate-900/50 rounded border-l-2 border-blue-500" id="survey-summary-readout">
                <div class="text-[11px] text-slate-400 mb-1">この画面の読み方</div>
                <div class="text-xs text-slate-200">{gap_label}</div>
            </div>
            <div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
                本サマリはアップロードされたCSVのみに基づく参考指標です。HW掲載求人との比較は下部「HWデータと統合分析」で確認してください。
            </div>
        </section>"#,
        total = format_number(agg.total_count as i64),
        total_raw = agg.total_count,
        new_rate = new_rate,
        new_rate_color = new_rate_color,
        region = escape_html(&region_text),
        median = median_text,
        expected = expected_text,
        gap_pct_text = gap_pct_text,
        gap_color = gap_color,
        gap_label = gap_label,
    ).unwrap();

    html
}

// =============================================================================
// セクション: アクションバー
// =============================================================================

fn render_action_bar(session_id: &str) -> String {
    format!(
        r##"<section class="stat-card" id="survey-action-bar" data-session-id="{sid}">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-emerald-500 pl-2">次のアクション</h3>
            <!-- プライマリ動線: HW統合分析（最も目立たせる） -->
            <div class="mb-3">
                <button hx-get="/api/survey/integrate?session_id={sid}"
                        hx-target="#survey-integration-result" hx-swap="innerHTML"
                        id="btn-hw-integrate"
                        class="group w-full sm:w-auto inline-flex items-center justify-center gap-2 px-6 py-3 bg-gradient-to-r from-blue-600 to-blue-500 hover:from-blue-500 hover:to-blue-400 text-white rounded-lg text-base font-bold shadow-lg shadow-blue-500/20 transition-all hover:shadow-blue-500/40 min-h-[44px]"
                        title="この地域のHW求人・外部統計・企業データと突合した比較レポートを生成します">
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>
                    HWデータと統合分析
                    <span class="hidden group-hover:inline text-[10px] opacity-75 ml-1">（地域×HW×統計の比較レポート）</span>
                </button>
            </div>
            <!-- PDF出力: 通常導線は採用コンサルレポートに一本化。旧 full/public は URL 互換のみ維持。 -->
            <div class="mb-3 p-3 bg-slate-900/40 rounded border border-slate-700" role="group" aria-label="PDFレポート出力">
                <div class="text-xs font-semibold text-slate-200 mb-1.5 flex items-center gap-1.5">
                    <svg class="w-4 h-4 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"/></svg>
                    PDFレポート出力
                    <span class="text-[10px] font-normal text-emerald-300">採用コンサル版に統一</span>
                </div>
                <div class="flex flex-wrap gap-2">
                    <!-- Legacy full/public variants remain URL-compatible only; the tab UI exposes one report. -->
                    <a href="/report/survey?session_id={sid}&variant=market_intelligence" target="_blank" rel="noopener"
                       onclick="return openVariantReport(event, '{sid}', 'market_intelligence')"
                       data-variant="market_intelligence"
                       class="inline-flex items-center gap-1.5 px-4 py-2 bg-purple-700 hover:bg-purple-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-purple-400"
                       aria-label="採用コンサルレポートPDFを新しいタブで開く（採用マーケットインテリジェンス版）"
                       title="採用コンサルレポート: 採用マーケットインテリジェンス（職業×地域 / 常住地ベース vs 従業地ベース / 検証済み推定 β）を含む拡張版。コンサル提案・配信地域選定向け。ヘッダーで選択中の都道府県/市区町村/業種が自動的に適用されます。">
                        <span class="text-base" aria-hidden="true">📊</span>
                        <span class="flex flex-col items-start leading-tight">
                            <span>採用コンサルレポート PDF</span>
                            <span class="text-[10px] opacity-80 font-normal">マーケットインテリジェンス / コンサル提案向け</span>
                        </span>
                    </a>
                </div>
                <p class="text-[11px] text-slate-400 mt-2 leading-relaxed">
                    通常のPDF出力は<strong class="text-slate-200">採用コンサルレポート</strong>に統一しました。旧「HW併載版」「公開データ中心版」は混乱防止のため媒体分析タブには表示しません。<br><strong class="text-amber-300">📌 ヘッダー上部で選択中の都道府県/市区町村/業種が PDF に自動適用されます。</strong>
                </p>
                <script>
                /* 2026-04-29: グローバルフィルタの値を読んでレポート URL に付与 */
                if (typeof window.openVariantReport !== 'function') {{
                    window.openVariantReport = function(ev, sid, variant) {{
                        try {{
                            var pref = (document.getElementById('pref-select') || {{}}).value || '';
                            var muni = (document.getElementById('muni-select') || {{}}).value || '';
                            var industries = (typeof _selectedIndustryRaws !== 'undefined' && Array.isArray(_selectedIndustryRaws))
                                ? _selectedIndustryRaws : [];
                            var industry = industries.length > 0 ? industries[0] : '';
                            var url = '/report/survey?session_id=' + encodeURIComponent(sid)
                                + '&variant=' + encodeURIComponent(variant);
                            if (pref && pref !== '全国') url += '&pref=' + encodeURIComponent(pref);
                            if (muni && muni !== 'すべて') url += '&muni=' + encodeURIComponent(muni);
                            if (industry) url += '&industry=' + encodeURIComponent(industry);
                            if (ev) ev.preventDefault();
                            window.open(url, '_blank', 'noopener');
                            return false;
                        }} catch (e) {{
                            console.error('openVariantReport failed', e);
                            return true; /* fallback to static href */
                        }}
                    }};
                }}
                </script>
            </div>
            <!-- セカンダリ動線: ボタングループ化（HTMLダウンロード + 別CSV） -->
            <div class="flex flex-wrap gap-2" role="group" aria-label="その他の出力">
                <button type="button" onclick="downloadReportHtml('{sid}')"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-indigo-700 hover:bg-indigo-600 text-white rounded text-sm font-medium transition-colors min-h-[44px]"
                        title="HTMLファイルをダウンロード。後からブラウザで開いて印刷・編集が可能">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                    HTMLダウンロード <span class="text-[10px] opacity-75">編集可</span>
                </button>
                <a href="#" onclick="document.getElementById('survey-result').innerHTML='';document.getElementById('survey-root').scrollIntoView({{behavior:'smooth'}});return false;"
                   class="inline-flex items-center gap-1.5 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors min-h-[44px]"
                   title="アップロード画面に戻り、別のCSVを取り込み直します">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
                    別のCSVをアップロード
                </a>
            </div>
            <p class="text-[11px] text-slate-500 mt-3">
                統合分析を最初に実行することを推奨します。HW・外部統計と突き合わせた相対評価により、本CSVの位置付けが明確になります。
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
        r#"<section class="stat-card" id="survey-salary-stats">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2 flex items-center gap-2">
                給与統計（月給換算）
                <span class="text-[10px] font-normal text-slate-500" tabindex="0" title="IQR×1.5 (Tukey法) で外れ値を除外した統計値。中央値は外れ値の影響を受けにくく、実勢に近い指標です。">ⓘ</span>
                <span class="ml-2 text-[10px] font-normal text-slate-500">外れ値除外（IQR法）</span>
            </h3>
            <div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#,
    );

    render_kpi_card(
        &mut html,
        "中央値",
        &format!("{}円", format_number(stats.median)),
        "text-emerald-400",
        "50パーセンタイル / 推奨指標",
    );
    render_kpi_card(
        &mut html,
        "平均",
        &format!("{}円", format_number(stats.mean)),
        "text-amber-300",
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
    write!(
        html,
        r#"<div class="flex items-center gap-3 mt-3 text-xs">
            <span class="text-slate-500">信頼性:</span>
            <span class="font-bold {rc}">{rel}</span>
            <span class="text-slate-600">(有効 n={n})</span>
        </div>"#,
        rc = reliability_color,
        rel = escape_html(&stats.reliability),
        n = stats.count,
    )
    .unwrap();

    html.push_str(r#"<p class="text-[11px] text-slate-600 mt-2 border-t border-slate-800 pt-2">月給換算は時給×167h/月（厚労省「就業条件総合調査 2024」基準）、年俸÷12で統一。中央値は外れ値の影響を受けにくいため、平均より実勢に近い目安として推奨されます。</p>"#);
    html.push_str("</section>");
    html
}

// =============================================================================
// セクション: 分布チャート（給与帯 + 雇用形態）
// =============================================================================

fn render_distribution_charts(agg: &SurveyAggregation) -> String {
    let mut html = String::with_capacity(4_000);
    // 2026-04-26 Fix-A: ラベル整合性修正。「分布」(by_salary_range / by_employment_type) は
    // パース直後の生レコードを件数集計しており、IQR は適用されていない。
    // 旧ラベル「外れ値除外（IQR法）適用済」は事実と異なるため「件数集計（生値ベース）」に変更。
    // IQR は給与統計（mean/median/Q1/Q3）と雇用形態グループ別集計の数値計算側のみに適用。
    html.push_str(r#"<section>
        <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2">分布<span class="ml-2 text-[10px] font-normal text-slate-500">件数集計（生値ベース・IQR 未適用）</span></h3>
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

    // 中央値・平均の縦線オーバーレイと IQR シェード
    let mut readout = String::from("破線: 中央値（緑）/ 平均（橙）");
    if let Some(stats) = &agg.enhanced_stats {
        chart["series"][0]["markLine"] = json!({
            "silent": true,
            "symbol": "none",
            "lineStyle": {"type": "dashed", "width": 2},
            "label": {"color": "#e2e8f0", "fontSize": 10},
            "data": [
                {"yAxis": stats.median, "name": "中央値", "lineStyle": {"color": "#10b981"}},
                {"yAxis": stats.mean, "name": "平均", "lineStyle": {"color": "#f59e0b"}}
            ]
        });

        // IQR (Q1-Q3) シェード表示
        if let Some(q) = &stats.quartiles {
            chart["series"][0]["markArea"] = json!({
                "silent": true,
                "itemStyle": {"color": "rgba(16, 185, 129, 0.08)"},
                "label": {"show": true, "color": "#10b981", "fontSize": 10, "position": "insideTop"},
                "data": [[
                    {"yAxis": q.q1, "name": "IQR (Q1-Q3)"},
                    {"yAxis": q.q3}
                ]]
            });

            // 中央値・期待値の差を読み手に伝える
            readout = format!(
                "中央値 {}円が「ボリュームゾーン」。IQR (Q1〜Q3) 範囲は {}円〜{}円で、求人の中央50%がこの帯に集中しています。",
                format_number(stats.median),
                format_number(q.q1),
                format_number(q.q3)
            );
        }
    }

    let config_str = chart.to_string().replace('\'', "&#39;");

    // 外れ値除外件数のビジュアル（除外前/後の比較バー）
    let outlier_bar = if agg.outliers_removed_total > 0 && agg.salary_values_raw_count > 0 {
        let raw = agg.salary_values_raw_count;
        let kept = raw.saturating_sub(agg.outliers_removed_total);
        let raw_pct = 100.0;
        let kept_pct = kept as f64 / raw as f64 * 100.0;
        format!(
            r#"<div class="mt-3 p-2 bg-slate-900/40 rounded text-[11px]" id="outlier-removal-bar">
                <div class="flex items-center justify-between mb-1">
                    <span class="text-slate-400">外れ値除外（IQR×1.5 / Tukey法）</span>
                    <span class="text-amber-400">{removed}件除外 / 残{kept}件</span>
                </div>
                <div class="space-y-1">
                    <div class="flex items-center gap-2">
                        <span class="text-slate-500 w-12">除外前</span>
                        <div class="flex-1 h-2 bg-slate-700 rounded overflow-hidden"><div class="h-full bg-slate-400" style="width:{raw_pct:.0}%"></div></div>
                        <span class="text-slate-300 w-14 text-right">{raw_n}件</span>
                    </div>
                    <div class="flex items-center gap-2">
                        <span class="text-slate-500 w-12">除外後</span>
                        <div class="flex-1 h-2 bg-slate-700 rounded overflow-hidden"><div class="h-full bg-emerald-500" style="width:{kept_pct:.1}%"></div></div>
                        <span class="text-emerald-300 w-14 text-right">{kept_n}件</span>
                    </div>
                </div>
            </div>"#,
            removed = agg.outliers_removed_total,
            kept = format_number(kept as i64),
            raw_pct = raw_pct,
            kept_pct = kept_pct,
            raw_n = format_number(raw as i64),
            kept_n = format_number(kept as i64),
        )
    } else {
        String::new()
    };

    format!(
        r#"<div class="stat-card" data-chart="salary-range">
            <div class="flex items-start justify-between mb-2 gap-2">
                <h4 class="text-xs font-semibold text-slate-300">給与帯分布
                    <span class="ml-1 text-[10px] text-slate-500" tabindex="0" title="緑線=中央値、橙線=平均、緑シェード=IQR (Q1-Q3) で求人の中央50%が集中する帯">ⓘ</span>
                </h4>
                <div class="text-[10px] text-slate-500 flex gap-2">
                    <span class="inline-flex items-center gap-1"><span class="inline-block w-2 h-2 bg-emerald-400 rounded-full" aria-hidden="true"></span>中央値</span>
                    <span class="inline-flex items-center gap-1"><span class="inline-block w-2 h-2 bg-amber-300 rounded-full" aria-hidden="true"></span>平均</span>
                </div>
            </div>
            <div class="echart" style="height:300px" data-chart-config='{config_str}'></div>
            <div class="mt-2 p-2 bg-blue-500/5 border-l-2 border-blue-500/40 rounded text-[11px] text-slate-300">
                <span class="text-blue-400 font-semibold">読み方:</span> {readout}
            </div>
            {outlier_bar}
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

    // 100% stacked bar (横長の帯) を補助として追加。比率を1次元で直感的に把握できる。
    let total: usize = agg.by_employment_type.iter().map(|(_, v)| *v).sum();
    let mut stacked_html = String::from(
        r#"<div class="mt-3 mb-2" data-stack="employment-100"><div class="flex h-4 rounded overflow-hidden border border-slate-700" role="img" aria-label="雇用形態100%帯">"#,
    );
    if total > 0 {
        for (i, (name, val)) in agg.by_employment_type.iter().enumerate() {
            let pct = *val as f64 / total as f64 * 100.0;
            if pct < 0.1 {
                continue;
            }
            write!(
                stacked_html,
                r#"<div style="width:{pct:.2}%;background:{color}" title="{name} {val}件 ({pct:.1}%)"></div>"#,
                pct = pct,
                color = colors[i % colors.len()],
                name = escape_html(name),
                val = val,
            )
            .unwrap();
        }
    }
    stacked_html.push_str(r#"</div></div>"#);

    // ボリューム最多の雇用形態を抽出して読み方に反映
    let dominant = agg
        .by_employment_type
        .iter()
        .max_by_key(|(_, v)| *v)
        .map(|(n, v)| {
            let pct = if total > 0 {
                *v as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            format!("最多は「{}」({:.1}%)", n, pct)
        })
        .unwrap_or_else(|| "—".to_string());

    format!(
        r#"<div class="stat-card" data-chart="employment-type">
            <div class="flex items-start justify-between mb-2 gap-2">
                <h4 class="text-xs font-semibold text-slate-300">雇用形態分布
                    <span class="ml-1 text-[10px] text-slate-500" tabindex="0" title="掲載件数の比率。給与単位（月給/時給）は雇用形態によって異なるため、比較時は単位を確認してください。">ⓘ</span>
                </h4>
                <div class="text-[10px] text-slate-500">n={total}</div>
            </div>
            <div class="echart" style="height:280px" data-chart-config='{config_str}'></div>
            {stacked_html}
            <div class="mt-2 p-2 bg-blue-500/5 border-l-2 border-blue-500/40 rounded text-[11px] text-slate-300">
                <span class="text-blue-400 font-semibold">読み方:</span> {dominant}。雇用形態の偏りは給与水準や応募層に直結します。
            </div>
        </div>"#
    )
}

// =============================================================================
// セクション: 都道府県別 KPI ヒートマップ（8x9 グリッド近似配置）
// =============================================================================

/// 47都道府県の配置 (row, col) - 日本地図を 8 row × 12 col のグリッドで近似
/// row 0=北、row 7=南。col は 0(西)〜11(東)。九州・沖縄を col 0-2 に配置。
/// 注: 同じセルに複数県が来る箇所があるが、ECharts heatmap は data 配列で全件描画。
const PREFECTURE_GRID: &[(&str, usize, usize)] = &[
    ("北海道", 0, 11),
    ("青森県", 1, 10),
    ("秋田県", 1, 9),
    ("岩手県", 1, 11),
    ("山形県", 2, 9),
    ("宮城県", 2, 10),
    ("福島県", 2, 11),
    ("新潟県", 3, 8),
    ("群馬県", 3, 9),
    ("栃木県", 3, 10),
    ("茨城県", 3, 11),
    ("石川県", 4, 7),
    ("富山県", 4, 8),
    ("長野県", 4, 9),
    ("埼玉県", 4, 10),
    ("東京都", 4, 11),
    ("千葉県", 5, 11),
    ("福井県", 5, 7),
    ("岐阜県", 5, 8),
    ("山梨県", 5, 9),
    ("神奈川県", 5, 10),
    ("滋賀県", 6, 7),
    ("愛知県", 6, 8),
    ("静岡県", 6, 9),
    ("京都府", 6, 6),
    ("奈良県", 7, 7),
    ("三重県", 7, 8),
    ("大阪府", 7, 6),
    ("兵庫県", 6, 5),
    ("和歌山県", 7, 5),
    ("鳥取県", 5, 4),
    ("岡山県", 6, 4),
    ("島根県", 5, 3),
    ("広島県", 6, 3),
    ("山口県", 7, 3),
    ("香川県", 7, 4),
    ("徳島県", 7, 5),
    ("愛媛県", 6, 2),
    ("高知県", 7, 4),
    ("福岡県", 6, 1),
    ("佐賀県", 6, 0),
    ("長崎県", 7, 0),
    ("熊本県", 7, 1),
    ("大分県", 5, 2),
    ("宮崎県", 7, 2),
    ("鹿児島県", 5, 1),
    ("沖縄県", 5, 0),
];

fn render_prefecture_heatmap_section(agg: &SurveyAggregation) -> String {
    if agg.by_prefecture.is_empty() {
        return String::new();
    }

    // 県名 → 件数 のマップ
    let pref_count: std::collections::HashMap<&str, usize> = agg
        .by_prefecture
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();

    // 件数ベースのデータ配列（ECharts heatmap 用 [col, row, value, name]）
    let mut data: Vec<serde_json::Value> = Vec::new();
    let mut max_val: i64 = 1;
    let mut covered = 0usize;

    for (name, row, col) in PREFECTURE_GRID {
        let cnt = pref_count.get(name).copied().unwrap_or(0) as i64;
        if cnt > 0 {
            covered += 1;
        }
        max_val = max_val.max(cnt);
        data.push(json!([*col as i64, *row as i64, cnt, name]));
    }

    // 県別給与中央値マップ（オプショナル）
    let pref_salary: std::collections::HashMap<&str, i64> = agg
        .by_prefecture_salary
        .iter()
        .map(|p| (p.name.as_str(), p.avg_salary))
        .collect();

    // ECharts heatmap config
    let chart = json!({
        "tooltip": {
            "position": "top",
            "formatter": "function(p){return p.data[3]+'<br/>掲載: '+p.data[2]+'件';}"
        },
        "grid": {"left": "3%", "right": "3%", "top": "3%", "bottom": "12%", "containLabel": true},
        "xAxis": {
            "type": "category",
            "show": false,
            "data": ["c0","c1","c2","c3","c4","c5","c6","c7","c8","c9","c10","c11","c12"],
            "splitArea": {"show": false}
        },
        "yAxis": {
            "type": "category",
            "show": false,
            "data": ["r0","r1","r2","r3","r4","r5","r6","r7"],
            "inverse": true,
            "splitArea": {"show": false}
        },
        "visualMap": {
            "min": 0,
            "max": max_val,
            "calculable": true,
            "orient": "horizontal",
            "left": "center",
            "bottom": "0%",
            "textStyle": {"color": "#94a3b8", "fontSize": 10},
            "inRange": {"color": ["#1e293b", "#1e40af", "#3b82f6", "#10b981", "#f59e0b"]}
        },
        "series": [{
            "type": "heatmap",
            "data": data,
            "label": {"show": true, "color": "#e2e8f0", "fontSize": 9, "formatter": "function(p){return p.data[3].replace(/[県府都道]$/,'').slice(0,2);}"},
            "itemStyle": {"borderColor": "#334155", "borderWidth": 1},
            "emphasis": {"itemStyle": {"shadowBlur": 10, "shadowColor": "rgba(59,130,246,0.5)"}}
        }]
    });

    let config_str = chart.to_string().replace('\'', "&#39;");

    // 補助テーブル（Top 5 + 給与中央値）
    let mut table_html = String::from(
        r#"<table class="w-full text-[11px] text-slate-300 mt-2"><thead><tr class="border-b border-slate-700"><th class="text-left py-1">都道府県</th><th class="text-right py-1">掲載件数</th><th class="text-right py-1">平均給与</th></tr></thead><tbody>"#,
    );
    for (name, cnt) in agg.by_prefecture.iter().take(5) {
        let sal = pref_salary
            .get(name.as_str())
            .map(|v| format!("{}円", format_number(*v)))
            .unwrap_or_else(|| "—".to_string());
        write!(
            table_html,
            r#"<tr class="border-b border-slate-800"><td class="py-1">{name}</td><td class="text-right text-emerald-400">{cnt}件</td><td class="text-right text-amber-400">{sal}</td></tr>"#,
            name = escape_html(name),
            cnt = format_number(*cnt as i64),
            sal = sal,
        )
        .unwrap();
    }
    table_html.push_str("</tbody></table>");

    format!(
        r##"<section class="stat-card" id="survey-prefecture-heatmap" data-pref-count="{covered}">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2 flex items-center gap-2">
                都道府県別ヒートマップ
                <span class="text-[10px] font-normal text-slate-500" tabindex="0" title="47都道府県を地理的に配置したヒートマップ。色濃度が掲載件数を表します。データのある県のみ着色。">ⓘ</span>
                <span class="ml-auto text-[10px] font-normal text-slate-500">対象: {covered}/47県</span>
            </h3>
            <div class="grid grid-cols-1 lg:grid-cols-3 gap-3">
                <div class="lg:col-span-2 bg-slate-900/40 rounded p-2">
                    <div class="echart" style="height:340px" data-chart-config='{config_str}'></div>
                </div>
                <div class="bg-slate-900/40 rounded p-3">
                    <div class="text-xs font-semibold text-slate-300 mb-1">掲載件数 Top 5</div>
                    {table_html}
                    <p class="text-[10px] text-slate-500 mt-2">クリックで都道府県別の詳細統計を「地域・タグの内訳」セクションで確認できます。</p>
                </div>
            </div>
            <div class="mt-3 p-2 bg-blue-500/5 border-l-2 border-blue-500/40 rounded text-[11px] text-slate-300">
                <span class="text-blue-400 font-semibold">読み方:</span> 色が濃いほど掲載件数が多い地域。空白セルはデータなし（0件）です。求人の地理的偏在を一目で確認できます。
            </div>
        </section>"##
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
        write!(
            html,
            r#"<div class="bg-slate-900/50 rounded p-3">
                <h4 class="text-xs font-semibold text-slate-300 mb-2">都道府県別 掲載件数</h4>
                <div class="echart" style="height:400px" data-chart-config='{config_str}'></div>
            </div>"#
        )
        .unwrap();
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
        write!(
            html,
            r#"<div class="bg-slate-900/50 rounded p-3">
                <h4 class="text-xs font-semibold text-slate-300 mb-2">求人タグ 頻出Top 15</h4>
                <div class="echart" style="height:400px" data-chart-config='{config_str}'></div>
            </div>"#
        )
        .unwrap();
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
        write!(html,
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
        ).unwrap();

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
            write!(
                html,
                r#"<div class="bg-slate-900/50 rounded p-3">
                    <h4 class="text-xs font-semibold text-slate-300 mb-2">給与レンジ幅 分布</h4>
                    <div class="echart" style="height:280px" data-chart-config='{config_str}'></div>
                </div>"#
            )
            .unwrap();
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

            write!(html,
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
            ).unwrap();
        }
    }

    html.push_str(
        r#"</div>
        </details>
    </section>"#,
    );

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
        write!(html,
            r#"<div class="p-2 bg-slate-900/50 rounded">
                <div class="text-slate-400 text-[11px] mb-1">95%信頼区間（Bootstrap）</div>
                <div>{lower}円 〜 {upper}円</div>
                <div class="text-[10px] text-slate-600 mt-1">n={n} / {iter}回リサンプリング / 平均の不確実性範囲</div>
            </div>"#,
            lower = format_number(ci.lower),
            upper = format_number(ci.upper),
            n = ci.sample_size,
            iter = ci.iterations,
        ).unwrap();
    }

    if let Some(tm) = &stats.trimmed_mean {
        write!(html,
            r#"<div class="p-2 bg-slate-900/50 rounded">
                <div class="text-slate-400 text-[11px] mb-1">トリム平均（10%）</div>
                <div>{val}円</div>
                <div class="text-[10px] text-slate-600 mt-1">外れ値{rm}件を除外した平均（ロバスト指標）</div>
            </div>"#,
            val = format_number(tm.trimmed_mean),
            rm = tm.removed_count,
        ).unwrap();
    }

    if let Some(q) = &stats.quartiles {
        write!(
            html,
            r#"<div class="p-2 bg-slate-900/50 rounded">
                <div class="text-slate-400 text-[11px] mb-1">四分位</div>
                <div>Q1: {q1}円 / Q2(中央値): {q2}円 / Q3: {q3}円 / IQR: {iqr}円</div>
            </div>"#,
            q1 = format_number(q.q1),
            q2 = format_number(q.q2),
            q3 = format_number(q.q3),
            iqr = format_number(q.iqr),
        )
        .unwrap();
    }

    html.push_str(
        r#"</div>
        </details>
    </section>"#,
    );

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
    write!(
        html,
        r#"<div class="p-3 bg-slate-800/50 rounded text-center">
            <div class="text-[11px] text-slate-500 mb-1">{label}</div>
            <div class="text-sm font-bold {color}">{value}</div>
            <div class="text-[10px] text-slate-600 mt-0.5">{note}</div>
        </div>"#,
        label = escape_html(label),
        value = value,
        color = value_color,
        note = escape_html(note),
    )
    .unwrap();
}

// =============================================================================
// テスト: PDF出力モード切替 UI (2026-04-29)
// =============================================================================

#[cfg(test)]
mod variant_ui_tests {
    use super::*;

    #[test]
    fn action_bar_exposes_single_market_intelligence_report_button() {
        let html = render_action_bar("test_session_123");
        assert!(
            html.contains("採用コンサルレポート PDF"),
            "action bar should contain the unified consulting report button"
        );
        assert!(
            html.contains("variant=market_intelligence"),
            "action bar should link to the market_intelligence report"
        );
        assert!(
            !html.contains("variant=full") && !html.contains("variant=public"),
            "full/public report variants must not be exposed in the tab UI"
        );
    }

    #[test]
    fn action_bar_unified_report_has_accessible_label() {
        let html = render_action_bar("test_session_456");
        assert!(
            html.contains("採用コンサルレポートPDFを新しいタブで開く"),
            "unified report button should have aria-label"
        );
    }

    #[test]
    fn action_bar_variant_buttons_have_min_height_for_mobile() {
        // スマホでもタップしやすいサイズ (min-height:44px)
        let html = render_action_bar("sid");
        let count = html.matches("min-h-[44px]").count();
        assert!(
            count >= 1,
            "unified report button should have min-h-[44px] for mobile tappability (found {})",
            count
        );
    }

    #[test]
    fn action_bar_explains_report_unification() {
        let html = render_action_bar("sid");
        assert!(
            html.contains("採用コンサルレポート</strong>に統一"),
            "should explain that PDF output is unified"
        );
        assert!(
            html.contains("旧「HW併載版」「公開データ中心版」は混乱防止のため媒体分析タブには表示しません"),
            "should explain why legacy variants are hidden"
        );
        assert!(
            html.contains("都道府県/市区町村/業種が PDF に自動適用"),
            "should keep filter propagation guidance"
        );
    }
}
