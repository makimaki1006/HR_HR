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
                    <button type="button" id="btn-open-report-guide"
                        class="mt-2 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold bg-blue-600 hover:bg-blue-500 text-white rounded shadow transition-colors"
                        aria-label="レポートの見方を開く">
                        <span aria-hidden="true">📖</span>
                        <span>レポートの見方</span>
                    </button>
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
                    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2">
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

        <!-- ==================================================================
             レポートの見方 モーダル (2026-07-02 追加)
             ==================================================================
             - トリガー: 右上「📖 レポートの見方」ボタン (btn-open-report-guide)
             - 内容: a. 各 Section の目的 / b. 統計用語 / d. 活用例 (営業・採用)
             - Esc / 背景クリック / × で閉じる。role=dialog + focus trap 相当のシンプル実装。
        -->
        <div id="report-guide-modal" class="hidden fixed inset-0 z-50 flex items-center justify-center p-4"
             role="dialog" aria-modal="true" aria-labelledby="report-guide-title">
            <div id="report-guide-backdrop" class="absolute inset-0 bg-black/70 backdrop-blur-sm"></div>
            <div class="relative bg-slate-900 border border-slate-700 rounded-lg shadow-2xl max-w-4xl w-full max-h-[85vh] overflow-hidden flex flex-col">
                <div class="flex items-start justify-between p-5 border-b border-slate-700">
                    <div>
                        <h2 id="report-guide-title" class="text-lg font-bold text-white flex items-center gap-2">
                            <span aria-hidden="true">📖</span> レポートの見方 (使い方ガイド)
                        </h2>
                        <p class="text-[11px] text-slate-400 mt-1">各セクションの目的、統計用語、営業/採用での活用例。折りたたみを開いて閲覧してください。</p>
                    </div>
                    <button type="button" id="btn-close-report-guide"
                        class="text-slate-400 hover:text-white text-2xl leading-none px-2"
                        aria-label="ガイドを閉じる">×</button>
                </div>

                <div class="overflow-y-auto p-5 space-y-4 text-slate-200 text-sm">

                    <!-- ==== a. 各 Section の目的 ==== -->
                    <details class="bg-slate-800/40 rounded p-3 border border-slate-700" open>
                        <summary class="cursor-pointer font-semibold text-white select-none">
                            <span class="inline-block w-5 text-blue-400">A</span>各セクションの目的・ストーリー
                        </summary>
                        <div class="mt-3 space-y-3 pl-6">

                            <div>
                                <div class="font-semibold text-cyan-300">Section 07.5 — 年間休日 × 給与 詳細</div>
                                <p class="text-slate-300 text-[13px] mt-1">求人ボックス / Indeed (SP) の求人説明文から「年間休日◯◯日」を自動抽出し、給与とのクロスを可視化します。「休みが多い企業は本当に給与が低いのか」「120日以上休みで◯◯万円台の求人はどの層か」といった疑問に答えるセクションです。</p>
                                <ul class="mt-2 text-[12px] text-slate-400 space-y-1">
                                    <li>• <b class="text-slate-200">§07.5-1 サマリー</b>: 平均年間休日 / Q3 / 標準偏差 / 120・125 日以上比率</li>
                                    <li>• <b class="text-slate-200">§07.5-2 分布</b>: 年間休日カテゴリ (〜89 / 90-104 / 105-119 / 120-124 / 125-129 / 130+) 別の構成比 + 給与中央値</li>
                                    <li>• <b class="text-slate-200">§07.5-3 散布図</b>: 給与×年間休日、雇用形態色分け、Pearson r + 回帰直線</li>
                                    <li>• <b class="text-slate-200">§07.5-4 個別求人</b>: 月給制+給与記載+会社名記載の求人リスト (最大100件、年間休日降順)</li>
                                    <li>• <b class="text-slate-200">§07.5-5 セグメント別 給与</b>: 6 カテゴリ × 月給下限/上限 × 平均・中央値・最頻値</li>
                                </ul>
                            </div>

                            <div>
                                <div class="font-semibold text-amber-300">Section 07.6 — 人気度シグナル</div>
                                <p class="text-slate-300 text-[13px] mt-1">Indeed (SP) 固有の「人気」「超人気」タグを集計。Indeed が付与する「人気」ラベル(付与基準は非公開)が給与・年間休日とどう相関するかを可視化します。「人気タグ付きの求人は本当に条件が良いのか」を検証するセクションです。</p>
                                <ul class="mt-2 text-[12px] text-slate-400 space-y-1">
                                    <li>• <b class="text-slate-200">§07.6-1 サマリー</b>: 人気/超人気件数、比率、月給差 (万円)、年間休日差</li>
                                    <li>• <b class="text-slate-200">§07.6-2 中央値比較</b>: 人気タグあり vs なしの月給・年間休日中央値</li>
                                    <li>• <b class="text-slate-200">§07.6-3 タグ別 給与統計</b>: 超人気/人気/なし × 月給下限/上限 × 平均・中央値・最頻値</li>
                                </ul>
                            </div>

                            <div class="text-[11px] text-slate-500 border-t border-slate-700 pt-2 mt-2">
                                ※ 他のセクション (Section 01-07, 08, 09) は HW データ・外部統計との統合分析。上記 07.5/07.6 は媒体 CSV 独自の追加セクションです。
                            </div>
                        </div>
                    </details>

                    <!-- ==== b. 統計用語ガイド ==== -->
                    <details class="bg-slate-800/40 rounded p-3 border border-slate-700">
                        <summary class="cursor-pointer font-semibold text-white select-none">
                            <span class="inline-block w-5 text-emerald-400">B</span>統計用語ガイド (中央値・Q3・r・最頻値・n閾値)
                        </summary>
                        <div class="mt-3 pl-6 space-y-3 text-[13px]">
                            <div>
                                <b class="text-white">平均 vs 中央値</b>
                                <p class="text-slate-300 mt-1">平均は外れ値の影響を受けます (1件の高額求人で全体が引き上げ)。<b class="text-emerald-300">中央値</b>は全データを並べた真ん中の値なので、実態に近い水準を示します。両者の乖離が大きい場合は分布の歪みを疑ってください。</p>
                            </div>
                            <div>
                                <b class="text-white">Q3 (第3四分位)</b>
                                <p class="text-slate-300 mt-1">全データを昇順に並べたとき上位 25% の境界値。「Q3 以上 = 上位 1/4」の意味。§07.5-1 の「Q3=125 日」なら「上位 25% の企業は年間休日 125 日以上」。n ≥ 20 では補間処理を適用、n &lt; 20 は最近接値。</p>
                            </div>
                            <div>
                                <b class="text-white">Pearson 相関係数 r</b>
                                <p class="text-slate-300 mt-1">-1 〜 +1 の範囲。0 に近いほど無相関、±1 に近いほど強い線形関係。目安: |r|&lt;0.2 = ほぼ無相関 / 0.4 = 弱い / 0.6 = 中程度 / 0.8 = 強い。<b class="text-yellow-300">相関 ≠ 因果</b>: 給与と年間休日に相関があっても、片方が原因とは限りません (第三変数の可能性)。</p>
                                <p class="text-slate-400 text-[11px] mt-1">本レポートでは n &lt; 10 は「傾向判定なし」、10 ≤ n &lt; 30 は「参考値」注記付き、n ≥ 30 でのみ確定表示します。</p>
                            </div>
                            <div>
                                <b class="text-white">最頻値 (5 万円ビン)</b>
                                <p class="text-slate-300 mt-1">給与は連続値なので、そのままでは最頻値が出にくい。5 万円刻みでビン化し「20〜24 万」「25〜29 万」…と集計、最も件数の多いビン開始値を最頻値としています。「20.0 万円」= 20 〜 25 万円のビン。同数の場合は最小ビン。</p>
                            </div>
                            <div>
                                <b class="text-white">n閾値 (両群 n ≥ 5)</b>
                                <p class="text-slate-300 mt-1">中央値比較 (§07.6-1 の月給差など) は両群の n が 5 未満だと外れ値 1 件で結果が乱高下します。両群 n ≥ 5 を満たさない場合は「— (n不足)」と表示され、KPI foot に実 n が併記されます。</p>
                            </div>
                            <div>
                                <b class="text-white">重複排除</b>
                                <p class="text-slate-300 mt-1">同一施設の別求人 (経験別 / 雇用形態別 / 給与レンジ別) は別レコードとして残します。会社・職種・勤務地・給与・雇用形態が完全に一致する同時掲載の重複のみ 1 件に集約します。</p>
                            </div>
                        </div>
                    </details>

                    <!-- ==== d. 活用例 (営業・採用) ==== -->
                    <details class="bg-slate-800/40 rounded p-3 border border-slate-700">
                        <summary class="cursor-pointer font-semibold text-white select-none">
                            <span class="inline-block w-5 text-fuchsia-400">C</span>数値の読み替え / 活用例
                        </summary>
                        <div class="mt-3 pl-6 space-y-4 text-[13px]">

                            <div>
                                <div class="text-fuchsia-300 font-semibold">💼 営業提案での活用</div>
                                <ul class="mt-2 space-y-2 text-slate-300 pl-4 list-disc">
                                    <li>
                                        <b class="text-white">競合水準の提示</b>：
                                        「貴社の年間休日 X 日は §07.5-5 の 105-119 日カテゴリで下限中央値 Y 万円が標準。上限中央値 Z 万円まで引き上げると 120-124 日カテゴリと張り合えます」
                                    </li>
                                    <li>
                                        <b class="text-white">人気タグ付き求人との比較</b>：
                                        「§07.6-3 の超人気タグ求人は中央値で月給 +A 万円、休日 +B 日の傾向があります(本媒体データでの相関であり、給与を上げれば人気タグが付くという因果関係ではありません)」
                                    </li>
                                    <li>
                                        <b class="text-white">上限中央値の引用</b>：
                                        「求人票の月給上限は §07.5-1 のこの地域の中央値 X 万円が参考水準。本媒体データ上、そうした水準の求人が多いという参考情報です(実際の応募数・閲覧数を保証するものではありません)」
                                    </li>
                                </ul>
                            </div>

                            <div>
                                <div class="text-fuchsia-300 font-semibold">🧑‍💼 採用実務での活用</div>
                                <ul class="mt-2 space-y-2 text-slate-300 pl-4 list-disc">
                                    <li>
                                        <b class="text-white">候補者面談の材料</b>：
                                        「同エリア/同職種の年間休日中央値は §07.5-1 で N 日。当社の X 日は上位 A% (§07.5-2 分布より) に位置します」
                                    </li>
                                    <li>
                                        <b class="text-white">競合オファーの妥当性判定</b>：
                                        「候補者が『他社で月給 Y 万提示』と言った際、§07.5-5 で該当カテゴリ (年間休日+雇用形態) の上限中央値と比較して現実的な水準か判定」
                                    </li>
                                    <li>
                                        <b class="text-white">求人票改善の優先度</b>：
                                        「§07.5-4 個別求人の上位企業と自社を比較し、給与/休日/勤務地のどこに差があるか特定 → 差別化訴求」
                                    </li>
                                </ul>
                            </div>

                            <div class="text-[11px] text-slate-500 border-t border-slate-700 pt-3 mt-2">
                                <b class="text-slate-400">⚠️ 注意</b>: 数値は媒体スクレイピング時点の状態です。求人ボックス / Indeed (SP) 掲載求人のみが集計対象で、全求人市場を代表しません。相関の話をする際は必ず「本媒体データでは」と限定してください。
                            </div>
                        </div>
                    </details>

                </div>

                <div class="p-4 border-t border-slate-700 flex items-center justify-between text-[11px] text-slate-500">
                    <span>Esc キー / 背景クリックでも閉じられます</span>
                    <button type="button" data-close-guide
                        class="px-3 py-1.5 bg-slate-700 hover:bg-slate-600 text-white rounded text-xs">閉じる</button>
                </div>
            </div>
        </div>
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

    // ==== レポートの見方 モーダル 開閉 (2026-07-02) ====
    (function() {
        var modal = document.getElementById('report-guide-modal');
        var btnOpen = document.getElementById('btn-open-report-guide');
        var btnClose = document.getElementById('btn-close-report-guide');
        var backdrop = document.getElementById('report-guide-backdrop');
        if (!modal || !btnOpen) return;
        function open() {
            modal.classList.remove('hidden');
            document.body.style.overflow = 'hidden';
        }
        function close() {
            modal.classList.add('hidden');
            document.body.style.overflow = '';
        }
        btnOpen.addEventListener('click', open);
        if (btnClose) btnClose.addEventListener('click', close);
        if (backdrop) backdrop.addEventListener('click', close);
        document.querySelectorAll('[data-close-guide]').forEach(function(b) {
            b.addEventListener('click', close);
        });
        document.addEventListener('keydown', function(e) {
            if (e.key === 'Escape' && !modal.classList.contains('hidden')) close();
        });
    })();
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

    // 2b. コンサル準備パネル（社内用、2026-07-10 フェーズB）
    html.push_str(&render_consult_prep_panel(session_id));

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
                    <span class="text-[10px] font-normal text-emerald-300">2種類から選択</span>
                </div>
                <div class="flex flex-wrap gap-2">
                    <!-- 標準レポート: market_intelligence — 従来の動線・URL 不変 -->
                    <a href="/report/survey?session_id={sid}&variant=market_intelligence" target="_blank" rel="noopener"
                       onclick="return openVariantReport(event, '{sid}', 'market_intelligence')"
                       data-variant="market_intelligence"
                       class="inline-flex items-center gap-1.5 px-4 py-2 bg-purple-700 hover:bg-purple-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-purple-400"
                       aria-label="標準レポートPDFを新しいタブで開く"
                       title="標準レポート: いつもの構成（採用マーケットインテリジェンス版）。ヘッダーで選択中の都道府県/市区町村/業種が自動的に適用されます。">
                        <span class="text-base" aria-hidden="true">📄</span>
                        <span class="flex flex-col items-start leading-tight">
                            <span>標準レポートを作成</span>
                            <span class="text-[10px] opacity-80 font-normal">いつもの構成</span>
                        </span>
                    </a>
                    <!-- 詳細レポート: extended — 働き手の将来・給与相場・転職動向を追加 (Section 10) -->
                    <a href="/report/survey?session_id={sid}&variant=extended" target="_blank" rel="noopener"
                       onclick="return openVariantReport(event, '{sid}', 'extended')"
                       data-variant="extended"
                       class="inline-flex items-center gap-1.5 px-4 py-2 bg-indigo-700 hover:bg-indigo-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-indigo-400"
                       aria-label="詳細レポートPDFを新しいタブで開く（データ拡大版）"
                       title="詳細レポート: 働き手の将来・給与相場・転職動向の分析を追加した版。公的統計×今回の求人データのクロス集計（国の将来人口推計等）を含む。データ未投入時は該当セクションをスキップします。">
                        <span class="text-base" aria-hidden="true">📊</span>
                        <span class="flex flex-col items-start leading-tight">
                            <span>詳細レポートを作成 (データ拡大版)</span>
                            <span class="text-[10px] opacity-80 font-normal">働き手の将来・給与相場・転職動向の分析を追加した版</span>
                        </span>
                    </a>
                    <!-- SP版 (仮): sp — 詳細版 + 経営サマリー1ページ/結論バンド/優先アクション表/給与四分位 (試作) -->
                    <a href="/report/survey?session_id={sid}&variant=sp" target="_blank" rel="noopener"
                       onclick="return openVariantReport(event, '{sid}', 'sp')"
                       data-variant="sp"
                       class="inline-flex items-center gap-1.5 px-4 py-2 bg-teal-700 hover:bg-teal-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-teal-400"
                       aria-label="SPレポート (仮) PDFを新しいタブで開く（試作）"
                       title="SPレポート (仮): 詳細版に「持ち歩ける経営サマリー1ページ」「各ページの結論バンド」「優先アクション表」「給与の四分位」を加えた試作版です。レビュー改善を全部入れした試験的なレポートで、内容・体裁は今後変わる可能性があります。">
                        <span class="text-base" aria-hidden="true">🧪</span>
                        <span class="flex flex-col items-start leading-tight">
                            <span>SPレポートを作成 (仮)</span>
                            <span class="text-[10px] opacity-80 font-normal">レビュー改善を全部入れした試作版</span>
                        </span>
                    </a>
                </div>
                <p class="text-[11px] text-slate-400 mt-2 leading-relaxed">
                    PDF出力は<strong class="text-slate-200">2種類</strong>から選択できます。旧「HW併載版」「公開データ中心版」は混乱防止のため媒体分析タブには表示しません。<br><strong class="text-amber-300">📌 ヘッダー上部で選択中の都道府県/市区町村/業種が PDF に自動適用されます。</strong>
                </p>
                <!-- 2026-05-19: openVariantReport は templates/dashboard_inline.html へ移動。
                     HTMX で動的挿入された <script> は eval されないため、ここで定義すると
                     onclick 実行時に ReferenceError → static href (pref/muni 無し) に
                     フォールバック navigate されていた。 -->
                <!-- 2026-07-10: セクション選択パネル (折りたたみ)。
                     チェックした内容だけを詳細版レポートに載せる。applySectionPreset /
                     buildSectionsReport は openVariantReport と同じく templates/dashboard_inline.html
                     に window 登録済み (HTMX 挿入 <script> は eval されないため)。 -->
                <details class="mt-3 rounded border border-slate-700 bg-slate-900/40" id="section-pick-panel">
                    <summary class="cursor-pointer select-none px-3 py-2 text-xs font-semibold text-slate-200 flex items-center gap-1.5">
                        <span aria-hidden="true">📋</span> 出力する内容を選んでレポートを作成
                        <span class="text-[10px] font-normal text-slate-400">（必要なページだけ選べます）</span>
                    </summary>
                    <div class="px-3 pb-3 pt-1">
                        <div class="flex flex-wrap gap-1.5 mb-2" role="group" aria-label="よく使う組み合わせ">
                            <button type="button" onclick="applySectionPreset('standard')"
                                    class="px-2.5 py-1 text-[11px] rounded bg-slate-700 hover:bg-slate-600 text-slate-200 transition-colors"
                                    title="よく使う標準的なページ一式を選びます">標準セット</button>
                            <button type="button" onclick="applySectionPreset('full')"
                                    class="px-2.5 py-1 text-[11px] rounded bg-slate-700 hover:bg-slate-600 text-slate-200 transition-colors"
                                    title="すべてのページを選びます">詳細セット</button>
                            <button type="button" onclick="applySectionPreset('minimal')"
                                    class="px-2.5 py-1 text-[11px] rounded bg-slate-700 hover:bg-slate-600 text-slate-200 transition-colors"
                                    title="表紙・要約・出典だけの最小構成にします">最小 (要約のみ)</button>
                        </div>
                        <div class="grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-1.5" role="group" aria-label="出力するページ">
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="02" checked> 地域の基礎データ</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="03" checked> 給与の分布</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="04" checked> 採用市場の需給</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="05" checked> 地域の企業構造</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="06" checked> 働き手の年齢・人口構成</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="07" checked> 最低賃金・暮らしのデータ</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="075" checked> 年間休日×給与の詳細</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="076" checked> 人気求人の傾向 <span class="text-[10px] text-slate-500">(Indeed SPのみ)</span></label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="09" checked> 採用マーケット分析</label>
                            <label class="flex items-start gap-2 text-xs text-slate-200"><input type="checkbox" class="section-pick mt-0.5" value="10" checked> 採用環境の詳細分析 <span class="text-[10px] text-slate-500">(働き手の将来・給与相場・転職動向)</span></label>
                        </div>
                        <div class="mt-3">
                            <button type="button" onclick="return buildSectionsReport(event, '{sid}')"
                                    class="inline-flex items-center gap-1.5 px-4 py-2 bg-emerald-700 hover:bg-emerald-600 text-white rounded text-sm font-medium transition-colors min-h-[44px] focus:outline-none focus:ring-2 focus:ring-emerald-400"
                                    aria-label="選んだ内容でレポートを新しいタブで開く"
                                    title="チェックした内容だけを詳細版レポートに載せて新しいタブで開きます">
                                <span class="text-base" aria-hidden="true">🧾</span> 選んだ内容でレポートを作成
                            </button>
                        </div>
                        <p class="text-[11px] text-slate-400 mt-2">表紙・要約・出典は常に含まれます。</p>
                    </div>
                </details>
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

    /// 標準レポート (market_intelligence) と詳細レポート (extended) の 2 ボタンが露出することを確認。
    /// full / public はタブ UI に表示しない (URL 互換のみ維持)。
    #[test]
    fn action_bar_exposes_both_report_buttons() {
        let html = render_action_bar("test_session_123");
        // 標準ボタン
        assert!(
            html.contains("標準レポートを作成"),
            "action bar should contain the standard report button"
        );
        assert!(
            html.contains("variant=market_intelligence"),
            "action bar should link to the market_intelligence report (URL unchanged)"
        );
        // 詳細ボタン
        assert!(
            html.contains("詳細レポートを作成 (データ拡大版)"),
            "action bar should contain the extended report button"
        );
        assert!(
            html.contains("variant=extended"),
            "action bar should link to the extended report"
        );
        // full / public は非表示
        assert!(
            !html.contains("variant=full") && !html.contains("variant=public"),
            "full/public report variants must not be exposed in the tab UI"
        );
    }

    #[test]
    fn action_bar_standard_report_has_accessible_label() {
        let html = render_action_bar("test_session_456");
        assert!(
            html.contains("標準レポートPDFを新しいタブで開く"),
            "standard report button should have aria-label"
        );
    }

    #[test]
    fn action_bar_extended_report_has_accessible_label() {
        let html = render_action_bar("test_session_789");
        assert!(
            html.contains("詳細レポートPDFを新しいタブで開く（データ拡大版）"),
            "extended report button should have aria-label"
        );
    }

    #[test]
    fn action_bar_variant_buttons_have_min_height_for_mobile() {
        // スマホでもタップしやすいサイズ (min-height:44px) — 2 ボタン分
        let html = render_action_bar("sid");
        let count = html.matches("min-h-[44px]").count();
        assert!(
            count >= 2,
            "both report buttons should have min-h-[44px] for mobile tappability (found {})",
            count
        );
    }

    #[test]
    fn action_bar_explains_two_report_types() {
        let html = render_action_bar("sid");
        assert!(
            html.contains("2種類"),
            "should explain that two PDF variants are available"
        );
        assert!(
            html.contains(
                "旧「HW併載版」「公開データ中心版」は混乱防止のため媒体分析タブには表示しません"
            ),
            "should explain why legacy variants are hidden"
        );
        assert!(
            html.contains("都道府県/市区町村/業種が PDF に自動適用"),
            "should keep filter propagation guidance"
        );
    }

    #[test]
    fn action_bar_extended_button_describes_content() {
        let html = render_action_bar("sid");
        assert!(
            html.contains("働き手の将来・給与相場・転職動向の分析を追加した版"),
            "extended button sub-label should describe what is added"
        );
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

    #[test]
    fn action_bar_section_picker_has_three_shortcuts() {
        // 標準セット / 詳細セット / 最小 の 3 ショートカット。
        let html = render_action_bar("sid");
        assert!(html.contains("標準セット"), "standard preset missing");
        assert!(html.contains("詳細セット"), "full preset missing");
        assert!(html.contains("最小 (要約のみ)"), "minimal preset missing");
        assert!(
            html.contains("applySectionPreset('standard')")
                && html.contains("applySectionPreset('full')")
                && html.contains("applySectionPreset('minimal')"),
            "preset buttons should call applySectionPreset"
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
            html.contains("表紙・要約・出典は常に含まれます"),
            "always-included note missing"
        );
        // パネル見出し
        assert!(
            html.contains("出力する内容を選んでレポートを作成"),
            "collapsible panel heading missing"
        );
    }
}
