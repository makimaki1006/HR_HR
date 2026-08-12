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
                        ユーザーがエクスポートした求人媒体CSVをアップロードし、公的求人データ・外部統計と突き合わせて地域別の相対比較を行います。
                    </p>
                </div>
                <div class="text-xs text-slate-500 text-right">
                    <div>対応形式: 主要求人媒体 CSV</div>
                    <div>文字コード: UTF-8（CSV/TXT）</div>
                    <!-- 2026-08-10: 「レポートの見方」ボタンを削除（モーダルごと撤去） -->
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
                        <div class="text-xs font-semibold text-white mb-0.5">公的データと比較</div>
                        <div class="text-[11px] text-slate-400">「公的求人データと比較」で比較レポート生成</div>
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
                        <span class="text-[10px] text-slate-500 ml-2">CSVをエクスポートした媒体を選んでください</span>
                    </label>
                    <!-- 2026-08-10: 選択肢を実際に使う 3 媒体だけに絞った（「自動判定」
                         「その他 / 手動編集」カードを削除）。既定は Indeed。
                         2026-07-20 の事故（Indeed SP の CSV を PC 扱いで取り込み、年間休日
                         §04・人気タグ §05 が丸ごと欠落）を再発させないため、サーバ側
                         parse_csv に Indeed PC ⇄ SP の取り違えだけを自動補正するガードを
                         入れてある (upload.rs)。 -->
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
                        <label class="source-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors min-h-[72px]" data-source="indeed_sp">
                            <input type="radio" name="source_type" value="indeed_sp" class="mt-1" aria-describedby="src-indeed-sp-desc">
                            <div>
                                <div class="text-sm font-bold text-white flex items-center gap-1.5">
                                    <span class="inline-block w-3 h-3 rounded-full bg-cyan-500" aria-hidden="true"></span>
                                    Indeed (SP)
                                </div>
                                <div id="src-indeed-sp-desc" class="text-[10px] text-slate-400 mt-0.5">Indeed スマホ版スクレイピング (年間休日 + 人気タグ取得可)</div>
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
                公的機関に掲載された求人との比較は相対的な参考値であり、採用判断の唯一の根拠としないでください。
            </div>
        </section>

        <!-- 2026-08-10: 分析結果を「対応CSV列の例」より上に置く。CSV を投げた直後の
             ユーザーはレポートを見たい状態なので、参考情報が結果より上に来ないようにした。 -->
        <div id="survey-result"></div>

        <!-- サンプルCSV列の折畳展開（参考情報なので最下部） -->
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

        <!-- 2026-08-10: 「レポートの見方」モーダルを削除。記載内容が現行レポートと
             乖離しており、更新されるまで表示しない（認知負荷削減）。 -->
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
    // 2026-07-22: アップロードのジョブ化。ファイル送信後すぐ job_id を受け取り、
    // ドロップゾーンそのものを読み込み表示に切り替える (段階: CSV解析→AI補完→集計)。
    // 完成した分析結果は下部パネルに差し込み、ドロップゾーンは元に戻す。
    function submitSurveyCSV(file) {
        if (!file) return;
        var status = document.getElementById('upload-status');
        var target = document.getElementById('survey-result');
        var dz = document.getElementById('drop-zone');
        if (dz && !window._dzOriginal) window._dzOriginal = dz.innerHTML;
        status.innerHTML = '<div class="text-sm text-blue-400">アップロード中: ' + file.name + '...</div>';
        var t0 = Date.now();
        // ドロップゾーンの位置 (=ユーザーが見ている場所) に大きく進捗を表示する
        function showProgress(stage) {
            var s = Math.floor((Date.now() - t0) / 1000);
            var box =
                '<div style="width:48px;height:48px;border:5px solid #334155;border-top-color:#38bdf8;border-radius:50%;animation:spin 1s linear infinite;margin:0 auto 16px"></div>' +
                '<div class="text-slate-100 text-lg font-bold mb-1">' + stage + '</div>' +
                '<div class="text-slate-400 text-sm">経過 ' + Math.floor(s / 60) + '分' + (s % 60) + '秒 — 完了すると下に分析結果が表示されます</div>' +
                '<style>@keyframes spin{to{transform:rotate(360deg)}}</style>';
            if (dz) { dz.innerHTML = box; } else { target.innerHTML = '<div class="stat-card">' + box + '</div>'; }
        }
        function restoreDropzone() {
            if (dz && window._dzOriginal) dz.innerHTML = window._dzOriginal;
        }
        function showError(msg) {
            if (dz) {
                dz.innerHTML = '<div class="text-red-400 text-base font-bold mb-2">' + msg + '</div>' +
                    '<div class="text-slate-400 text-sm">数秒後に元の画面に戻ります。もう一度お試しください。</div>';
                setTimeout(restoreDropzone, 4000);
            } else {
                target.innerHTML = '<div class="stat-card"><p class="text-red-400 text-sm">' + msg + '</p></div>';
            }
            status.textContent = 'エラー';
            status.className = 'mt-3 text-sm text-red-400';
        }
        showProgress('アップロード中');
        var fd = new FormData();
        fd.append('csv_file', file);
        // ユーザー明示指定を同送信（自動判定より優先）
        var src = document.querySelector('input[name="source_type"]:checked');
        var wage = document.querySelector('input[name="wage_mode"]:checked');
        if (src) fd.append('source_type', src.value);
        if (wage) fd.append('wage_mode', wage.value);
        fetch('/api/survey/upload/start', { method: 'POST', body: fd })
            .then(function(r) { return r.json(); })
            .then(function(j) {
                if (!j.job_id) { showError(j.error || 'アップロードに失敗しました'); return; }
                status.textContent = '処理中…';
                status.className = 'mt-3 text-sm text-blue-400';
                var timer = setInterval(function() {
                    fetch('/api/survey/job/status/' + j.job_id)
                        .then(function(r) { return r.json(); })
                        .then(function(st) {
                            if (st.state === 'running' || st.state === 'queued') {
                                showProgress(st.message || '処理中');
                            }
                            if (st.state === 'done') {
                                clearInterval(timer);
                                fetch('/report/survey/job/result/' + j.job_id)
                                    .then(function(r) { return r.text(); })
                                    .then(function(serverHtml) {
                                        restoreDropzone();
                                        target.innerHTML = serverHtml;
                                        if (typeof htmx !== 'undefined') htmx.process(target);
                                        setTimeout(function() {
                                            if (typeof window.initECharts === 'function') window.initECharts(target);
                                            // データ探索（動的）パネルの起動。innerHTML 挿入は
                                            // htmx:afterSettle を発火しないため明示的に呼ぶ
                                            // (2026-08-04 レビューで発見: これが無いとパネルは
                                            //  本番のアップロード経路で一度も初期化されない)。
                                            if (typeof window.surveyExploreScan === 'function') window.surveyExploreScan(target);
                                            // 2026-08-12: レポート作成の 2 択（すべて載せる / 内容を選ぶ）の
                                            // 初期表示を合わせる。結果は innerHTML 挿入なので
                                            // DOMContentLoaded では間に合わない。
                                            if (typeof window.syncReportMode === 'function') window.syncReportMode();
                                            if (typeof window.syncSectionCount === 'function') window.syncSectionCount();
                                        }, 50);
                                        status.textContent = '完了';
                                        status.className = 'mt-3 text-sm text-emerald-400';
                                        target.scrollIntoView({ behavior: 'smooth', block: 'start' });
                                    });
                            }
                            if (st.state === 'failed') {
                                clearInterval(timer);
                                showError(st.message || '処理に失敗しました');
                            }
                        })
                        .catch(function() {});
                }, 1500);
            })
            .catch(function(e) {
                showError('アップロードエラーが発生しました');
            });
    }

    // ==== 折りたたみセクション (details.survey-fold) を開いたときのチャート描画 ====
    // 2026-08-10: 分析結果の各セクションを既定で折りたたみにしたため、閉じている
    // 間はチャート要素の高さが 0 になる。app.js は offsetHeight===0 の要素を
    // スキップするので、開いた瞬間に初期化とリサイズをやり直す必要がある。
    // toggle イベントはバブリングしないのでキャプチャフェーズで受ける。
    document.addEventListener('toggle', function(e) {
        var d = e.target;
        if (!d || d.tagName !== 'DETAILS' || !d.open) return;
        if (!d.classList || !d.classList.contains('survey-fold')) return;
        // データ探索パネルは survey_explore.js が閉じた状態で init 済み（サイズ 0）。
        // scan() の再実行はリスナー二重登録になるため、resize だけで足りる。
        if (typeof window.initECharts === 'function') window.initECharts(d);
        if (typeof echarts !== 'undefined') {
            d.querySelectorAll('.echart, [data-explore-chart]').forEach(function(el) {
                var c = echarts.getInstanceByDom(el);
                if (c) c.resize();
            });
        }
    }, true);

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

    // 2026-08-10: 並び順を「レポート出力が最上部」に変更。CSV を投げた時点で
    // ユーザーはレポートを求めている状態なので、まず出力導線を出す。
    // 数値セクション（エグゼクティブサマリ以下）は既定で折りたたみ、
    // 見たい人だけが開く形にした（認知負荷削減）。

    // 1. アクションボタン（レポート出力導線）
    html.push_str(&render_action_bar(session_id));

    // 1b. コンサル準備パネル（社内用、2026-07-10 フェーズB）
    html.push_str(&render_consult_prep_panel(session_id));

    html.push_str(r#"<div id="survey-integration-result"></div>"#);

    // 2. エグゼクティブサマリ（折りたたみ）
    html.push_str(&render_tldr(agg, seeker));

    // 3. 給与サマリカード（主要KPI）
    html.push_str(&render_salary_summary(agg));

    // 4. 給与分布・雇用形態分布（チャート群）
    html.push_str(&render_distribution_charts(agg));

    // 4a. データ探索（動的）。static/js/survey_explore.js が
    //     /api/survey/report?session_id=… から集計を取得して描画・操作を担う。
    //     レポートと同じキャッシュ済み集計を使うため数字は必ず一致する (2026-08-04)。
    html.push_str(&render_dynamic_explore_section(session_id));

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
        r#"<details class="stat-card border-l-4 border-blue-500 survey-fold" id="survey-executive-summary" data-total="{total_raw}">
            <summary class="text-lg font-bold text-white flex items-center gap-2 cursor-pointer select-none hover:text-blue-200">
                <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/></svg>
                エグゼクティブサマリ
                <span class="text-xs font-normal text-slate-500 ml-1">分析対象 {total}件</span>
            </summary>
            <div class="flex items-start justify-between flex-wrap gap-3 mb-4 mt-3">
                <div>
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
                本サマリはアップロードされたCSVのみに基づく参考指標です。
            </div>
        </details>"#,
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

/// データ探索（動的）セクション。
///
/// 2026-08-04: レポート出力の一方通行だった集計を、アプリ内で並べ替え・ビン幅変更
/// しながら見られるようにする。描画とイベントは static/js/survey_explore.js。
/// チャート要素は app.js の data-chart-config 自動初期化と衝突しないよう
/// .echart クラスを使わない。
fn render_dynamic_explore_section(session_id: &str) -> String {
    format!(
        r#"<details id="survey-explore" data-session-id="{sid}" class="stat-card survey-fold">
        <summary class="text-sm font-semibold text-slate-200 mb-1 border-l-4 border-emerald-500 pl-2 cursor-pointer select-none hover:text-white">データ探索（動的）</summary>
        <p class="text-[11px] text-slate-500 mb-3 mt-2">レポートと同じ集計を、この画面で操作しながら確認できます。給与の数値は外れ値（IQR法）除外後、市区町村は求人件数の多い上位15件です。</p>
        <p data-explore-status class="text-[11px] text-amber-400 mb-2">集計を読み込んでいます…</p>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-[11px] text-slate-400">市区町村別（求人件数の多い上位15件）</span>
                    <select data-explore-control="muni-metric" class="text-[11px] bg-slate-800 text-slate-200 border border-slate-600 rounded px-1 py-0.5">
                        <option value="count">件数</option>
                        <option value="median_salary">給与中央値</option>
                        <option value="avg_salary">給与平均</option>
                    </select>
                    <select data-explore-control="muni-order" class="text-[11px] bg-slate-800 text-slate-200 border border-slate-600 rounded px-1 py-0.5">
                        <option value="desc">多い順</option>
                        <option value="asc">少ない順</option>
                    </select>
                </div>
                <div data-explore-chart="municipality" style="height:340px"></div>
            </div>
            <div>
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-[11px] text-slate-400">給与ヒストグラム（月給換算）</span>
                    <select data-explore-control="bin-width" class="text-[11px] bg-slate-800 text-slate-200 border border-slate-600 rounded px-1 py-0.5">
                        <option value="25000">2.5万円刻み</option>
                        <option value="50000" selected>5万円刻み</option>
                        <option value="100000">10万円刻み</option>
                    </select>
                </div>
                <div data-explore-chart="histogram" style="height:340px"></div>
            </div>
            <div>
                <div class="mb-1"><span class="text-[11px] text-slate-400">雇用形態の内訳</span></div>
                <div data-explore-chart="employment" style="height:300px"></div>
            </div>
            <div>
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-[11px] text-slate-400">給与が高い方の条件タグ（全体平均との差）</span>
                    <select data-explore-control="tag-min-count" class="text-[11px] bg-slate-800 text-slate-200 border border-slate-600 rounded px-1 py-0.5">
                        <option value="5" selected>5件以上のタグ</option>
                        <option value="10">10件以上のタグ</option>
                        <option value="20">20件以上のタグ</option>
                    </select>
                </div>
                <div data-explore-chart="tags" style="height:300px"></div>
            </div>
        </div>
    </details>"#,
        sid = crate::handlers::helpers::escape_html(session_id)
    )
}

fn render_action_bar(session_id: &str) -> String {
    format!(
        r##"<section class="stat-card" id="survey-action-bar" data-session-id="{sid}">
            <h3 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-emerald-500 pl-2">次のアクション</h3>
            <!-- プライマリ動線: 公的求人データとの比較（最も目立たせる）。
                 2026-08-10: 画面上の「HW」表記を廃止し平易な語に統一（社内略語を表に出さない）。 -->
            <div class="mb-3">
                <button hx-get="/api/survey/integrate?session_id={sid}"
                        hx-target="#survey-integration-result" hx-swap="innerHTML"
                        id="btn-hw-integrate"
                        class="group w-full sm:w-auto inline-flex items-center justify-center gap-2 px-6 py-3 bg-gradient-to-r from-blue-600 to-blue-500 hover:from-blue-500 hover:to-blue-400 text-white rounded-lg text-base font-bold shadow-lg shadow-blue-500/20 transition-all hover:shadow-blue-500/40 min-h-[44px]"
                        title="この地域の公的求人データ・外部統計・企業データと突合した比較レポートを生成します">
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>
                    公的求人データと比較
                    <span class="hidden group-hover:inline text-[10px] opacity-75 ml-1">（地域×公的求人×統計の比較レポート）</span>
                </button>
            </div>
            <!-- 2026-08-12: レポート出力を「すべて載せる」か「内容を選ぶ」かの 2 択に整理。
                 標準(market_intelligence) と 本編(sp) のボタンは撤去（URL は温存）。
                 詳細(extended) を唯一のレポートとし、章を選ぶ場合も同じ詳細版に載せる。
                 章ごとの説明文は生成済みレポートの実出力から起こしたもので、
                 レポート本文には手を入れていない（画面側の案内のみ）。 -->
            <div class="mb-3 p-3 bg-slate-900/40 rounded border border-slate-700" role="group" aria-label="レポート出力">
                <div class="text-xs font-semibold text-slate-200 mb-2 flex items-center gap-1.5">
                    <svg class="w-4 h-4 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"/></svg>
                    レポートを作成
                </div>
                <div id="report-mode-cards" role="radiogroup" aria-label="レポートの作り方" class="grid grid-cols-1 sm:grid-cols-2 gap-2 mb-3">
                    <label class="report-mode-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors" data-mode="all">
                        <input type="radio" name="report_mode" value="all" class="mt-1" checked onchange="syncReportMode()">
                        <div>
                            <div class="text-sm font-bold text-white">すべて載せる</div>
                            <div class="text-[11px] text-slate-400 mt-0.5">全10章。まずはこちらで問題ありません</div>
                        </div>
                    </label>
                    <label class="report-mode-card flex items-start gap-2 p-3 bg-slate-800/40 border border-slate-700 rounded cursor-pointer hover:border-blue-500 transition-colors" data-mode="pick">
                        <input type="radio" name="report_mode" value="pick" class="mt-1" onchange="syncReportMode()">
                        <div>
                            <div class="text-sm font-bold text-white">内容を選ぶ</div>
                            <div class="text-[11px] text-slate-400 mt-0.5">要らない章を外して短くします</div>
                        </div>
                    </label>
                </div>
                <div id="report-mode-all">
                    <a href="/report/survey?session_id={sid}&variant=extended" target="_blank" rel="noopener"
                       onclick="return openVariantReport(event, '{sid}', 'extended')"
                       data-variant="extended"
                       class="inline-flex items-center gap-2 px-4 py-2.5 bg-indigo-700 hover:bg-indigo-600 text-white rounded text-sm font-bold transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-indigo-400"
                       aria-label="レポートPDFを新しいタブで開く">
                        <span class="text-base" aria-hidden="true">📊</span> レポートを作成（全10章）
                    </a>
                </div>
                <div id="report-mode-pick" class="hidden">
                    <p class="text-[11px] text-slate-400 mb-2">載せる章を選んでください。<strong class="text-slate-200">表紙・要約・出典は常に入ります。</strong></p>
                    <div class="flex flex-wrap gap-2 mb-2" role="group" aria-label="よく使う組み合わせ">
                        <button type="button" onclick="applySectionPreset('full')"
                                class="px-2.5 py-1 text-[11px] rounded bg-slate-700 hover:bg-slate-600 text-slate-200 transition-colors">すべて選ぶ</button>
                        <button type="button" onclick="applySectionPreset('minimal')"
                                class="px-2.5 py-1 text-[11px] rounded bg-slate-700 hover:bg-slate-600 text-slate-200 transition-colors">すべて外す</button>
                    </div>
                    <div class="grid grid-cols-1 lg:grid-cols-2 gap-x-4 gap-y-1" role="group" aria-label="載せる章">
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="02" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">地域の基礎データ</span><br><span class="text-[10px] text-slate-400">可住地面積・人口密度・件数が多い市区町村・通勤の流入元</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="03" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">給与の分布</span><br><span class="text-[10px] text-slate-400">下限と上限それぞれの分布と分位点、雇用形態別の給与</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="075" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">年間休日×給与の詳細</span><br><span class="text-[10px] text-slate-400">休日数別の給与、給与×休日の散布図、個別求人の具体例、セグメント別統計</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="076" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">人気求人の傾向</span><br><span class="text-[10px] text-slate-400">人気タグ別の月給・年間休日の比較（Indeed (SP) のCSVのときだけ出ます）</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="06" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">働き手の年齢・人口構成</span><br><span class="text-[10px] text-slate-400">人口構造の主要指標、年齢階級別の人口ピラミッド</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="05" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">地域の企業構造</span><br><span class="text-[10px] text-slate-400">規模×動向の法人セグメント、産業大分類の構成</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="04" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">採用市場の需給</span><br><span class="text-[10px] text-slate-400">採用難度の指標、事業所統計、開業率・廃業率</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="07" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">最低賃金・暮らしのデータ</span><br><span class="text-[10px] text-slate-400">最低賃金の推移、家計支出、通勤圏、昼夜間人口</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="09" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">採用マーケット分析</span><br><span class="text-[10px] text-slate-400">配信の優先度、通勤の届く範囲、生活コストで補正した給与の魅力度</span></span></label>
                        <label class="pick-row flex items-start gap-2 p-2 rounded hover:bg-slate-800/60 cursor-pointer"><input type="checkbox" class="section-pick mt-0.5" value="10" checked onchange="syncSectionCount()"><span><span class="text-xs text-slate-100">採用環境の詳細分析</span><br><span class="text-[10px] text-slate-400">働き手の将来推計、地域相場との給与比較、転職意向、採用のネック診断</span></span></label>
                    </div>
                    <div class="mt-3 flex items-center gap-3 flex-wrap">
                        <button type="button" onclick="return buildSectionsReport(event, '{sid}')"
                                class="inline-flex items-center gap-2 px-4 py-2.5 bg-emerald-700 hover:bg-emerald-600 text-white rounded text-sm font-bold transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-emerald-400"
                                aria-label="選んだ内容でレポートを新しいタブで開く">
                            <span class="text-base" aria-hidden="true">🧾</span> 選んだ内容でレポートを作成
                        </button>
                        <span id="section-count" class="text-xs text-slate-300" aria-live="polite">選択中: 10 / 10 章</span>
                    </div>
                </div>
                <div class="mt-3 pt-2 border-t border-slate-700/60">
                    <a href="/report/survey?session_id={sid}&variant=guide" target="_blank" rel="noopener"
                       onclick="return openGuideReport(event, '{sid}')"
                       data-variant="guide"
                       class="inline-flex items-center gap-1.5 px-4 py-2 bg-amber-700 hover:bg-amber-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-amber-400"
                       aria-label="解説資料を新しいタブで開く"
                       title="解説資料: レポートに添える読み解きガイド。給与・年間休日・勤務地の3つの論点について「確認できた事実 → 比較 → 確認していただきたいこと」の順で整理します。">
                        <span class="text-base" aria-hidden="true">📖</span>
                        <span class="flex flex-col items-start leading-tight">
                            <span>解説資料を作成</span>
                            <span class="text-[10px] opacity-80 font-normal">レポートに添える読み解きガイド</span>
                        </span>
                    </a>
                </div>
                <p class="text-[11px] text-amber-300 mt-2 leading-relaxed">📌 ヘッダー上部で選択中の都道府県 / 市区町村 / 業種が、レポートに自動で適用されます。</p>
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
                統合分析を最初に実行することを推奨します。公的求人・外部統計と突き合わせた相対評価により、本CSVの位置付けが明確になります。
            </p>
        </section>
        <!-- 2026-05-19: downloadReportHtml は templates/dashboard_inline.html へ移動。
             HTMX 動的挿入の <script> は eval されない (openVariantReport と同根本原因)。 -->"##,
        sid = session_id
    )
}

// =============================================================================
// セクション: コンサル準備パネル (社内用、2026-07-10 フェーズB)
// =============================================================================

/// 商談準備レポート (社内用) の生成パネル。
/// buildConsultBrief / downloadConsultEvidencePack は templates/dashboard_inline.html に
/// window 登録済み (HTMX 動的挿入の <script> は eval されないため、openVariantReport と同方式)。
fn render_consult_prep_panel(session_id: &str) -> String {
    format!(
        r##"<section class="stat-card" id="consult-prep-panel" data-session-id="{sid}">
            <h3 class="text-sm font-semibold text-slate-200 mb-1 border-l-4 border-rose-500 pl-2 flex items-center gap-2">
                <span aria-hidden="true">🔒</span> コンサル準備 (社内用)
                <span class="text-[10px] font-normal px-1.5 py-0.5 rounded bg-rose-900/60 text-rose-300 border border-rose-700">顧客配布不可</span>
            </h3>
            <p class="text-[11px] text-slate-400 mb-3 leading-relaxed">
                面談前の仮説整理用の商談準備レポートを生成します。市場データから仮説・矛盾・質問を整理した<strong class="text-slate-300">社内専用</strong>の資料です。任意入力があると仮説の精度が上がります。
            </p>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3 mb-3">
                <div>
                    <label for="consult-hiring-count" class="block text-[11px] text-slate-400 mb-1">採用予定人数 (任意)</label>
                    <input type="number" id="consult-hiring-count" min="1" placeholder="例: 3"
                           class="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded text-sm text-white placeholder-slate-500">
                </div>
                <div>
                    <label for="consult-deadline" class="block text-[11px] text-slate-400 mb-1">採用期限 (任意)</label>
                    <input type="text" id="consult-deadline" placeholder="例: 2026年9月末"
                           class="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded text-sm text-white placeholder-slate-500">
                </div>
                <div>
                    <label class="block text-[11px] text-slate-400 mb-1">自社の給与条件 (任意、円)</label>
                    <div class="flex items-center gap-2">
                        <input type="number" id="consult-salary-min" min="0" placeholder="下限 例: 250000"
                               class="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded text-sm text-white placeholder-slate-500" aria-label="自社給与の下限">
                        <span class="text-slate-500 text-xs">〜</span>
                        <input type="number" id="consult-salary-max" min="0" placeholder="上限 例: 300000"
                               class="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded text-sm text-white placeholder-slate-500" aria-label="自社給与の上限">
                    </div>
                </div>
                <div>
                    <label for="consult-note" class="block text-[11px] text-slate-400 mb-1">メモ (任意)</label>
                    <input type="text" id="consult-note" placeholder="例: 夜勤なし希望・車通勤可"
                           class="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded text-sm text-white placeholder-slate-500">
                </div>
            </div>
            <div class="flex flex-wrap gap-2" role="group" aria-label="コンサル準備の出力">
                <button type="button" onclick="return buildConsultBrief('{sid}')"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-rose-700 hover:bg-rose-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-rose-400"
                        title="仮説・矛盾・面談質問・複合考察を整理した社内用の商談準備レポート (最大8ページ) を新しいタブで開きます">
                    <span class="text-base" aria-hidden="true">📝</span> 商談準備レポートを作成
                </button>
                <button type="button" onclick="return downloadConsultEvidencePack('{sid}')"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-slate-400"
                        title="商談準備レポートの根拠データ (証拠・シグナル・仮説) をJSON形式でダウンロードします">
                    <span class="text-base" aria-hidden="true">🗂</span> 証拠データJSON
                </button>
                <a href="/consult/hearing_sheet?session_id={sid}" target="_blank" rel="noopener"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-slate-400"
                        title="面談で確認する項目を並べた社内用のヒアリングシート (印刷用) を新しいタブで開きます">
                    <span class="text-base" aria-hidden="true">🖨</span> ヒアリングシート (印刷用)
                </a>
                <a href="/consult/hearing?session_id={sid}" target="_blank" rel="noopener"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-slate-400"
                        title="面談で確認した採用状況を入力・保存します (社内用)">
                    <span class="text-base" aria-hidden="true">✍</span> ヒアリング入力
                </a>
                <a href="/consult/hypothesis_review?session_id={sid}" target="_blank" rel="noopener"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-slate-400"
                        title="面談前に整理した仮説を、ヒアリング回答をもとに支持・否定・保留へ更新します (社内用)">
                    <span class="text-base" aria-hidden="true">🔎</span> 仮説の確認・更新
                </a>
                <a href="/consult/action_memo?session_id={sid}" target="_blank" rel="noopener"
                        class="inline-flex items-center gap-1.5 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-slate-400"
                        title="お打ち合わせ内容と市場データにもとづく整理として、優先施策とKPIをまとめたメモを開きます (顧客共有可)">
                    <span class="text-base" aria-hidden="true">📝</span> アクションメモ
                </a>
            </div>
        </section>"##,
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
        r#"<details class="stat-card survey-fold" id="survey-salary-stats">
            <summary class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2 flex items-center gap-2 cursor-pointer select-none hover:text-white">
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
    // 2026-05-21: statistics.rs:319-323 が英語 ("high"/"medium"/"low"/"very_low")
    // で reliability を返しているにも関わらず、ここの match は日本語キー想定
    // ("高"/"中") + escape_html(&stats.reliability) で生の英語表示 → 全件 gray +
    // 英語残。英語キーに揃えて color + 日本語 label を 1 経路で取得するよう修正。
    let (reliability_color, reliability_label): (&str, &str) = match stats.reliability.as_str() {
        "high" => ("text-emerald-400", "高"),
        "medium" => ("text-amber-400", "中"),
        "low" => ("text-orange-400", "低"),
        "very_low" => ("text-red-400", "極低"),
        other => ("text-slate-400", other), // 想定外の値はそのまま (silent fallback で英語残検知用)
    };
    write!(
        html,
        r#"<div class="flex items-center gap-3 mt-3 text-xs">
            <span class="text-slate-500">信頼性:</span>
            <span class="font-bold {rc}">{rel}</span>
            <span class="text-slate-600">(有効 n={n})</span>
        </div>"#,
        rc = reliability_color,
        rel = escape_html(reliability_label),
        n = stats.count,
    )
    .unwrap();

    html.push_str(r#"<p class="text-[11px] text-slate-600 mt-2 border-t border-slate-800 pt-2">月給換算は時給×167h/月（厚労省「就業条件総合調査 2024」基準）、年俸÷12で統一。中央値は外れ値の影響を受けにくいため、平均より実勢に近い目安として推奨されます。</p>"#);
    html.push_str("</details>");
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
    html.push_str(r#"<details class="stat-card survey-fold" id="survey-distribution">
        <summary class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2 cursor-pointer select-none hover:text-white">分布<span class="ml-2 text-[10px] font-normal text-slate-500">件数集計（生値ベース・IQR 未適用）</span></summary>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-3">"#);

    // 給与帯分布
    if !agg.by_salary_range.is_empty() {
        html.push_str(&render_salary_range_chart(agg));
    }

    // 雇用形態分布
    if !agg.by_employment_type.is_empty() {
        html.push_str(&render_employment_type_chart(agg));
    }

    html.push_str("</div></details>");
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

    let config_str = chart.to_string().replace('\'', "&#x27;");

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

    let config_str = chart.to_string().replace('\'', "&#x27;");

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

    // 件数ベースのデータ配列（ECharts heatmap 用）
    //
    // 2026-08-10 修正: 以前は series.label.formatter / tooltip.formatter に
    // "function(p){...}" という文字列を入れ、app.js 側で new Function() に
    // 復元していた。しかし本アプリの CSP (lib.rs) は script-src に 'unsafe-eval'
    // を含まないため new Function() が例外になり、app.js の catch に握り潰されて
    // formatter が「文字列のまま」ECharts に渡っていた。結果、各セルとツールチップ
    // に関数のソースコードがそのまま表示されていた（ユーザー報告の「謎のテキスト」）。
    // CSP は緩めず、データ項目ごとに静的な文字列 formatter を持たせて解決する。
    let mut data: Vec<serde_json::Value> = Vec::new();
    let mut max_val: i64 = 1;
    let mut covered = 0usize;

    for (name, row, col) in PREFECTURE_GRID {
        let cnt = pref_count.get(name).copied().unwrap_or(0) as i64;
        if cnt > 0 {
            covered += 1;
        }
        max_val = max_val.max(cnt);
        // セル内表示用の短縮名: 末尾の 都/道/府/県 を落として先頭 2 文字
        let short: String = name
            .trim_end_matches(['都', '道', '府', '県'])
            .chars()
            .take(2)
            .collect();
        data.push(json!({
            "value": [*col as i64, *row as i64, cnt],
            "name": name,
            "label": {"formatter": short},
            "tooltip": {"formatter": format!("{name}<br/>掲載: {cnt}件")},
        }));
    }

    // 県別給与中央値マップ（オプショナル）
    let pref_salary: std::collections::HashMap<&str, i64> = agg
        .by_prefecture_salary
        .iter()
        .map(|p| (p.name.as_str(), p.avg_salary))
        .collect();

    // ECharts heatmap config
    let chart = json!({
        // formatter はデータ項目側に静的文字列で持たせる（CSP で new Function 不可のため）
        "tooltip": {"position": "top"},
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
            "label": {"show": true, "color": "#e2e8f0", "fontSize": 9},
            "itemStyle": {"borderColor": "#334155", "borderWidth": 1},
            "emphasis": {"itemStyle": {"shadowBlur": 10, "shadowColor": "rgba(59,130,246,0.5)"}}
        }]
    });

    let config_str = chart.to_string().replace('\'', "&#x27;");

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
        r##"<details class="stat-card survey-fold" id="survey-prefecture-heatmap" data-pref-count="{covered}">
            <summary class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2 flex items-center gap-2 cursor-pointer select-none hover:text-white">
                都道府県別ヒートマップ
                <span class="text-[10px] font-normal text-slate-500" tabindex="0" title="47都道府県を地理的に配置したヒートマップ。色濃度が掲載件数を表します。データのある県のみ着色。">ⓘ</span>
                <span class="ml-auto text-[10px] font-normal text-slate-500">対象: {covered}/47県</span>
            </summary>
            <div class="grid grid-cols-1 lg:grid-cols-3 gap-3 mt-3">
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
        </details>"##
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

        let config_str = chart.to_string().replace('\'', "&#x27;");
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

        let config_str = chart.to_string().replace('\'', "&#x27;");
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

            let config_str = chart.to_string().replace('\'', "&#x27;");
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
                    <div>・公的求人データとの比較は「公的求人データと比較」で実施されますが、公的機関に掲載される求人は全求人の一部であり産業偏り（IT・通信は少ない等）があります。</div>
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

    /// 2026-08-12: レポートの入口を「すべて載せる」「内容を選ぶ」の 2 択に整理した。
    ///
    /// 標準(market_intelligence) と 本編(sp) のボタンは撤去。本編は章立てが詳細版と
    /// 完全に同一で（実出力で確認済み）、並べる意味が無かった。URL は温存しているので
    /// クエリを直接叩けば従来どおり生成できる。
    #[test]
    fn action_bar_offers_two_ways_to_build_a_report() {
        let html = render_action_bar("test_session_123");

        // 入口は 2 つ。ラジオで切り替える
        assert!(
            html.contains(r#"name="report_mode" value="all""#),
            "「すべて載せる」の選択肢が必要"
        );
        assert!(
            html.contains(r#"name="report_mode" value="pick""#),
            "「内容を選ぶ」の選択肢が必要"
        );
        assert!(
            html.contains("すべて載せる") && html.contains("内容を選ぶ"),
            "2 つの入口のラベルが必要"
        );

        // 出力はどちらも詳細版
        assert!(
            html.contains("variant=extended"),
            "レポートは詳細版 (extended) で出す"
        );

        // 撤去したボタンが復活していないこと（逆証明）
        for gone in [
            "variant=market_intelligence",
            "variant=sp",
            "variant=full",
            "variant=public",
            "variant=ver10",
        ] {
            assert!(
                !html.contains(gone),
                "{gone} はタブ UI から撤去済みのはず（URL 互換のみ維持）"
            );
        }
        assert!(
            !html.contains("標準レポートを作成") && !html.contains("本編レポートを作成"),
            "標準 / 本編 のボタンは撤去済みのはず"
        );
    }

    /// 「出力後に初めて中身が分かる」状態を解消するため、章ごとに何が載るかを事前に示す。
    #[test]
    fn section_picker_explains_what_each_section_contains() {
        let html = render_action_bar("sid");
        // 章名だけでなく、実際に出力される図表の中身が説明として添えられていること。
        // 文言は生成済みレポートの実出力から起こしている。
        let expected = [
            ("地域の基礎データ", "可住地面積・人口密度"),
            ("給与の分布", "雇用形態別の給与"),
            ("年間休日×給与の詳細", "個別求人の具体例"),
            ("人気求人の傾向", "人気タグ別"),
            ("働き手の年齢・人口構成", "人口ピラミッド"),
            ("地域の企業構造", "産業大分類の構成"),
            ("採用市場の需給", "開業率・廃業率"),
            ("最低賃金・暮らしのデータ", "最低賃金の推移"),
            ("採用マーケット分析", "通勤の届く範囲"),
            ("採用環境の詳細分析", "働き手の将来推計"),
        ];
        for (title, detail) in expected {
            assert!(html.contains(title), "章名『{title}』が必要");
            assert!(
                html.contains(detail),
                "章『{title}』に中身の説明『{detail}』が必要（出力後に気づく状態を防ぐ）"
            );
        }
        // 選択結果が分かること
        assert!(
            html.contains(r#"id="section-count""#),
            "選択中の章数の表示が必要"
        );
        // 常に入るものが明示されていること
        assert!(
            html.contains("表紙・要約・出典は常に入ります"),
            "常時含まれる章の説明が必要"
        );
    }

    #[test]
    fn action_bar_buttons_have_min_height_for_mobile() {
        // スマホでもタップしやすいサイズ (min-height:44px)
        let html = render_action_bar("sid");
        let count = html.matches("min-h-[44px]").count();
        assert!(
            count >= 3,
            "レポート作成 / 選んで作成 / 解説資料 の各ボタンに min-h-[44px] が必要 (found {})",
            count
        );
    }

    #[test]
    fn action_bar_keeps_filter_propagation_guidance() {
        let html = render_action_bar("sid");
        assert!(
            html.contains("都道府県 / 市区町村 / 業種が、レポートに自動で適用されます"),
            "ヘッダーのフィルタが効く旨の案内は残す"
        );
    }

    /// 解説資料はレポートとは別の成果物なので、統合せず残す
    #[test]
    fn action_bar_keeps_guide_button() {
        let html = render_action_bar("sid");
        assert!(html.contains("解説資料を作成"), "解説資料のボタンは残す");
        assert!(html.contains("variant=guide"), "解説資料の導線が必要");
    }

    // ---- セクション選択パネル (2026-07-10) ----

    #[test]
    fn action_bar_section_picker_has_10_checkboxes() {
        // 選択可能な 10 セクション分のチェックボックス (class=section-pick) が出る。
        let html = render_action_bar("sid");
        let count = html.matches("class=\"section-pick").count();
        assert_eq!(
            count, 10,
            "section picker should expose exactly 10 checkboxes (found {})",
            count
        );
        // 各コードの value が存在する
        for code in ["02", "03", "04", "05", "06", "07", "075", "076", "09", "10"] {
            assert!(
                html.contains(&format!("value=\"{}\"", code)),
                "checkbox for section {} missing",
                code
            );
        }
    }

    /// 2026-08-12: ショートカットを「すべて選ぶ / すべて外す」の 2 つに整理。
    /// 旧「標準セット」は、どの章が入るのか名前から分からず選びようがなかった。
    #[test]
    fn action_bar_section_picker_has_two_shortcuts() {
        let html = render_action_bar("sid");
        assert!(html.contains("すべて選ぶ"), "全選択のショートカットが必要");
        assert!(html.contains("すべて外す"), "全解除のショートカットが必要");
        assert!(
            html.contains("applySectionPreset('full')")
                && html.contains("applySectionPreset('minimal')"),
            "ショートカットは applySectionPreset を呼ぶ"
        );
        assert!(
            !html.contains("標準セット"),
            "中身の分からない「標準セット」は撤去済みのはず"
        );
    }

    #[test]
    fn action_bar_section_picker_build_button_calls_js() {
        // 「選んだ内容でレポートを作成」ボタンは buildSectionsReport(event, sid) を呼ぶ。
        let html = render_action_bar("sid_xyz");
        assert!(
            html.contains("選んだ内容でレポートを作成"),
            "build button label missing"
        );
        assert!(
            html.contains("buildSectionsReport(event, 'sid_xyz')"),
            "build button should call buildSectionsReport with session id"
        );
        // 常時含まれる注記
        assert!(
            html.contains("表紙・要約・出典は常に入ります"),
            "常時含まれる章の注記が必要"
        );
        // 入口のラベル
        assert!(
            html.contains("内容を選ぶ"),
            "collapsible panel heading missing"
        );
    }

    /// データ探索（動的）パネルの3点契約 (2026-08-04):
    /// (1) パネルHTMLが session_id を data 属性で持つ
    /// (2) ダッシュボードが survey_explore.js を読み込む
    /// (3) JS が参照する data 属性名がパネル側と一致する
    /// どれか1つだけ変えると「エラーなくパネルが出ない」silent 故障になるため固定する。
    #[test]
    fn dynamic_explore_panel_contract_holds() {
        let html = render_dynamic_explore_section("sid_abc");
        assert!(html.contains(r#"id="survey-explore""#));
        assert!(html.contains(r#"data-session-id="sid_abc""#));
        for chart in ["municipality", "histogram", "employment", "tags"] {
            assert!(
                html.contains(&format!(r#"data-explore-chart="{chart}""#)),
                "チャート枠 {chart} がパネルにない"
            );
        }
        // app.js の自動初期化 (.echart) と衝突しないこと
        assert!(
            !html.contains("class=\"echart\""),
            "動的パネルは data-chart-config 自動初期化と分離するべき"
        );

        let dashboard = include_str!("../../../templates/dashboard_inline.html");
        assert!(
            dashboard.contains("/static/js/survey_explore.js"),
            "ダッシュボードが survey_explore.js を読み込んでいない"
        );

        let js = include_str!("../../../static/js/survey_explore.js");
        assert!(
            js.contains("#survey-explore[data-session-id]"),
            "JS の起点セレクタがパネルと一致しない"
        );
        for chart in ["municipality", "histogram", "employment", "tags"] {
            assert!(
                js.contains(&format!("data-explore-chart='{chart}'")),
                "JS がチャート枠 {chart} を参照していない"
            );
        }
        assert!(
            js.contains("/api/survey/report?session_id="),
            "JS のデータ源が /api/survey/report でない"
        );
        // 起動経路の契約 (2026-08-04 レビューで発見した silent 故障の回帰):
        // innerHTML + htmx.process() は htmx:afterSettle を発火しないため、
        // アップロード完了ハンドラが window.surveyExploreScan を明示的に呼ぶこと、
        // JS 側がそれを公開していることの両方が必要。
        assert!(
            js.contains("window.surveyExploreScan = scan"),
            "JS が起動入口 window.surveyExploreScan を公開していない"
        );
        let upload_form = render_upload_form();
        assert!(
            upload_form.contains("window.surveyExploreScan"),
            "アップロード完了ハンドラが surveyExploreScan を呼んでいない (パネルが一度も初期化されない)"
        );
    }
}
