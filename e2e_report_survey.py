# -*- coding: utf-8 -*-
"""
/report/survey E2E検証
- CSVアップロード → レポート出力までの実ブラウザ動作を確認
- ECharts描画・テーブルソート・印刷プレビューを実証
"""
import os, time, csv, random
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
CSV_PATH = os.path.join(DIR, "_survey_mock.csv")

# ---- 1. モックCSV生成 (Indeed風) ----
def make_mock_csv():
    """30件程度のIndeed風CSVを生成"""
    companies = ["株式会社ABC", "株式会社DEF", "株式会社GHI", "株式会社JKL", "株式会社MNO"]
    locations = [
        ("東京都", "千代田区"), ("東京都", "新宿区"), ("東京都", "渋谷区"),
        ("東京都", "港区"), ("神奈川県", "横浜市中区"),
    ]
    emp_types = ["正社員", "契約社員", "パート・アルバイト", "派遣社員"]
    tags_pool = [
        "未経験可,週休2日,残業少なめ",
        "経験者優遇,昇給あり,交通費支給",
        "未経験可,社保完備,土日休み",
        "経験者歓迎,年間休日120日",
        "未経験可,研修制度,マイカー通勤可",
    ]

    rows = []
    random.seed(42)
    for i in range(30):
        company = random.choice(companies)
        pref, muni = random.choice(locations)
        emp = random.choice(emp_types)
        tags = random.choice(tags_pool)
        if emp == "パート・アルバイト":
            salary = f"時給{random.randint(1100, 1500)}円"
        else:
            min_sal = random.randint(20, 35)
            max_sal = min_sal + random.randint(2, 10)
            salary = f"月給{min_sal}万円~{max_sal}万円"
        rows.append({
            "求人タイトル": f"営業職 No.{i+1}",
            "企業名": company,
            "勤務地": f"{pref}{muni}",
            "給与": salary,
            "雇用形態": emp,
            "タグ": tags,
            "URL": f"https://example.com/job/{i+1}",
            "新着": "新着" if i < 5 else "",
        })
    with open(CSV_PATH, "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    return len(rows)

def ss(page, name, full=False):
    time.sleep(2)
    path = os.path.join(DIR, f"rep_{name}.png")
    page.screenshot(path=path, full_page=full)
    print(f"    [screenshot] rep_{name}.png")

def check(label, cond):
    status = "PASS" if cond else "FAIL"
    icon = "OK" if cond else "NG"
    print(f"  [{icon}] [{status}] {label}")
    return cond

def main():
    n = make_mock_csv()
    print(f"[INFO] mock CSV: {n}件 → {CSV_PATH}")

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=False, slow_mo=500)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        console_errors = []
        page.on("console", lambda m: console_errors.append(m.text) if m.type == "error" else None)

        # === 1. ログイン ===
        print("\n=== 1. ログイン ===")
        page.goto(BASE, timeout=60000)
        time.sleep(3)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "cyxen_2025")
        page.click('button[type="submit"]')
        time.sleep(8)
        check("ログイン成功", "ログアウト" in (page.text_content("body") or ""))

        # === 2. 媒体分析タブへ（HTMX直接トリガー） ===
        print("\n=== 2. 媒体分析タブ表示 ===")
        # 先にタブ要素の状態を確認
        tab_info = page.evaluate("""
            (function(){
                var btns = document.querySelectorAll('.tab-btn');
                return Array.from(btns).map(function(b){
                    return {
                        text: b.textContent.trim(),
                        hxget: b.getAttribute('hx-get'),
                        target: b.getAttribute('hx-target')
                    };
                });
            })()
        """)
        print(f"  [INFO] タブ一覧: {tab_info}")

        # htmxがロードされるまで待機
        page.wait_for_function("typeof htmx !== 'undefined'", timeout=15000)
        time.sleep(1)

        # Playwrightの実クリックで媒体分析タブを選択
        survey_btn = None
        for btn in page.query_selector_all('.tab-btn'):
            text = btn.text_content() or ""
            if "媒体" in text:
                survey_btn = btn
                break
        if survey_btn is None:
            print("  [ERROR] 媒体分析タブボタンが見つからない")
            browser.close()
            return
        survey_btn.click()
        clicked = "/tab/survey (real click)"

        # HTMXイベント終了を明示的に待機
        time.sleep(2)
        content_state = page.evaluate("""
            (function(){
                var c = document.getElementById('content');
                return {
                    hasContent: !!c,
                    textLen: c ? (c.textContent||'').length : 0,
                    hasSurveyForm: !!document.querySelector('#survey-upload-form'),
                    hasFileInput: !!document.querySelector('input[type=\"file\"]'),
                    allInputs: document.querySelectorAll('input').length,
                    surveyKeyword: (document.body.textContent||'').indexOf('媒体分析')
                };
            })()
        """)
        print(f"  [INFO] タブクリック後のDOM状態: {content_state}")

        # 直接 fetch して生レスポンスを確認
        raw_resp = page.evaluate("""
            fetch('/tab/survey', {credentials: 'include'})
                .then(r => r.text())
                .then(t => ({status: 'ok', length: t.length, snippet: t.substring(0, 200)}))
                .catch(e => ({status: 'err', msg: String(e)}))
        """)
        print(f"  [INFO] /tab/survey 直接fetch: {raw_resp}")

        # もう一度 innerHTML 書き換えを試す
        if content_state.get('textLen', 0) == 0:
            print("  [WARN] タブクリックで#contentが空のため、直接fetchで埋める")
            page.evaluate("""
                fetch('/tab/survey', {credentials: 'include'})
                    .then(r => r.text())
                    .then(t => { document.getElementById('content').innerHTML = t; })
            """)
            time.sleep(5)
        print(f"  [INFO] クリックしたタブのhx-get: {clicked}")
        # HTMX読み込みを確実に待機：file input が現れるまで最大20秒
        for wait_sec in range(20):
            has_file = page.evaluate("!!document.querySelector('input[type=\"file\"]')")
            if has_file:
                print(f"  [INFO] file input 検出 ({wait_sec+1}秒)")
                break
            time.sleep(1)
        else:
            print("  [WARN] 20秒待っても file input が出現しなかった")
        time.sleep(2)
        ss(page, "01_survey_tab")
        body = page.text_content("body") or ""
        check("調査タブ内容表示", "CSV" in body or "アップロード" in body or "媒体" in body)

        # === 3. CSVアップロード ===
        print("\n=== 3. CSVアップロード ===")
        ss(page, "02a_before_upload")  # アップロード前の状態記録

        # file input と form の情報を取得
        upload_info = page.evaluate("""
            (function(){
                var f = document.querySelector('input[type=\"file\"]');
                var form = f ? f.closest('form') : null;
                return {
                    hasFile: !!f,
                    formAction: form ? form.getAttribute('action') : null,
                    formMethod: form ? form.getAttribute('method') : null,
                    formHxPost: form ? form.getAttribute('hx-post') : null,
                    formHxEncoding: form ? form.getAttribute('hx-encoding') : null,
                    formHxTarget: form ? form.getAttribute('hx-target') : null,
                    buttonCount: form ? form.querySelectorAll('button').length : 0
                };
            })()
        """)
        print(f"  [INFO] uploadフォーム: {upload_info}")

        if not upload_info.get('hasFile'):
            print("  [ERROR] file input が見つからない（媒体分析タブの読み込みが未完了）")
            browser.close()
            return

        file_input = page.query_selector('input[type="file"]')
        file_input.set_input_files(CSV_PATH)
        time.sleep(2)
        ss(page, "02b_file_selected")

        # onchange発火しない場合に備えて submitSurveyCSV を手動で呼ぶ
        page.evaluate(
            "(function(){var i=document.querySelector('input[type=\"file\"]');"
            "if(i&&i.files&&i.files[0]&&typeof window.submitSurveyCSV==='function')"
            "{window.submitSurveyCSV(i.files[0]);}})()"
        )
        time.sleep(15)
        ss(page, "02c_after_upload")

        body = page.text_content("body") or ""
        check("分析結果表示", "総求人数" in body or "サマリー" in body)
        check("KPIカード", "件" in body)

        # === 4. 「印刷用レポート出力」ボタンを探して新タブで開く ===
        print("\n=== 4. レポート出力ボタン ===")
        # セッションIDを取得してURL直打ち（同タブで開く）
        session_link = page.query_selector('a[href*="/report/survey"]')
        if not session_link:
            print("  [ERROR] /report/survey へのリンクが見つからない")
            ss(page, "02b_no_link", full=True)
            browser.close()
            return
        href = session_link.get_attribute("href")
        print(f"  [INFO] report URL: {href}")
        report_url = href if href.startswith("http") else BASE + href

        # 新しいページでレポートを開く
        report_page = ctx.new_page()
        report_page.on("console", lambda m: console_errors.append(f"[report] {m.text}") if m.type == "error" else None)
        report_page.goto(report_url, timeout=60000)
        time.sleep(8)

        # === 5. レポート各セクション確認 ===
        print("\n=== 5. レポート描画確認 ===")
        ss(report_page, "03_report_top")

        rbody = report_page.text_content("body") or ""
        check("サマリーセクション", "サマリー" in rbody)
        check("給与分布セクション", "給与分布" in rbody)
        check("雇用形態セクション", "雇用形態" in rbody)
        check("散布図 or 相関分析", "相関" in rbody or "散布" in rbody)
        check("市区町村別給与", "市区町村" in rbody)
        check("最低賃金比較", "最低賃金" in rbody)
        check("企業分析", "企業分析" in rbody or "ランキング" in rbody)
        check("タグ分析", "タグ" in rbody)
        check("求職者心理", "求職者" in rbody or "レンジ" in rbody)

        # === 5b. データ妥当性検証（目視で見逃したバグを検出する） ===
        print("\n=== 5b. データ妥当性検証 ===")
        # 給与ヒストグラムの「0万」バケットが総件数の10%未満であること（時給混入バグ検出）
        zero_bucket_issue = report_page.evaluate("""
            (function(){
                var result = {checked: false, zero_ratios: []};
                var charts = document.querySelectorAll('[data-chart-config]');
                charts.forEach(function(el, idx){
                    try {
                        var cfg = JSON.parse(el.getAttribute('data-chart-config'));
                        if (cfg.xAxis && cfg.xAxis.data && cfg.series && cfg.series[0] && cfg.series[0].type === 'bar') {
                            var labels = cfg.xAxis.data;
                            var values = cfg.series[0].data;
                            var zeroIdx = labels.findIndex(function(l){return l === '0万' || l === '0';});
                            if (zeroIdx >= 0) {
                                var total = values.reduce(function(a,b){return a+b;}, 0);
                                var zeroVal = values[zeroIdx] || 0;
                                var ratio = total > 0 ? zeroVal/total : 0;
                                result.zero_ratios.push({chart: idx, zero: zeroVal, total: total, ratio: ratio});
                                result.checked = true;
                            }
                        }
                    } catch(e) {}
                });
                return result;
            })()
        """)
        print(f"  [INFO] 給与ヒストグラム0バケット検証: {zero_bucket_issue}")
        for r in zero_bucket_issue.get('zero_ratios', []):
            check(
                f"給与ヒストグラム chart[{r['chart']}] の0バケットが10%未満 (実際: {r['ratio']*100:.1f}%)",
                r['ratio'] < 0.1
            )

        # KPIサマリーの平均月給が妥当な範囲（5万〜200万）
        kpi_values = report_page.evaluate("""
            (function(){
                var cards = document.querySelectorAll('.kpi-card');
                return Array.from(cards).map(function(c){
                    var label = c.querySelector('.kpi-label');
                    var value = c.querySelector('.kpi-value');
                    return {
                        label: label ? label.textContent.trim() : '',
                        value: value ? value.textContent.trim() : ''
                    };
                });
            })()
        """)
        print(f"  [INFO] KPIカード: {kpi_values}")
        for kpi in kpi_values:
            if "月給" in kpi.get('label', '') or "時給" in kpi.get('label', ''):
                # 数値抽出
                import re
                m = re.search(r'([\d.]+)', kpi.get('value', ''))
                if m:
                    val = float(m.group(1))
                    check(f"KPI {kpi['label']} が妥当な範囲 ({val})", 5.0 <= val <= 300.0)

        # === 6. ECharts実描画確認 ===
        print("\n=== 6. ECharts描画検証 ===")
        echart_count = report_page.evaluate(
            "document.querySelectorAll('[data-chart-config]').length"
        )
        print(f"  [INFO] data-chart-config数: {echart_count}")
        check(f"ECharts configが配置されている (>=3)", echart_count >= 3)

        # ECharts.instanceが存在するか (= 初期化済み)
        initialized = report_page.evaluate("""
            (function() {
                if (typeof echarts === 'undefined') return 0;
                var count = 0;
                document.querySelectorAll('[data-chart-config]').forEach(function(el){
                    var inst = echarts.getInstanceByDom(el);
                    if (inst) count++;
                });
                return count;
            })()
        """)
        print(f"  [INFO] ECharts初期化済み数: {initialized}")
        check(f"ECharts実際に初期化 (>=3)", initialized >= 3)

        # SVGレンダラーの確認
        svg_rendered = report_page.evaluate(
            "document.querySelectorAll('[data-chart-config] svg').length"
        )
        print(f"  [INFO] SVG子要素数: {svg_rendered}")
        check(f"SVGチャート描画 (>=3)", svg_rendered >= 3)

        # === 7. スクリーンショット各セクション ===
        print("\n=== 7. 各セクションのスクリーンショット ===")
        # スクロールして取得
        for i, y in enumerate([0, 800, 1600, 2400, 3200, 4000]):
            report_page.evaluate(f"window.scrollTo(0, {y})")
            time.sleep(2)
            ss(report_page, f"04_section_{i+1}")

        # 全ページスクリーンショット
        report_page.evaluate("window.scrollTo(0, 0)")
        time.sleep(2)
        ss(report_page, "05_full", full=True)

        # === 8. テーブルソート動作 ===
        print("\n=== 8. テーブルソート動作 ===")
        sortable = report_page.query_selector_all('.sortable-table')
        print(f"  [INFO] sortable-table数: {len(sortable)}")
        check("ソート可能テーブル存在 (>=3)", len(sortable) >= 3)

        if sortable:
            # 最初のテーブルの2列目ヘッダーをクリック
            first_th = report_page.query_selector('.sortable-table th:nth-child(2)')
            if first_th:
                first_th.click()
                time.sleep(1)
                has_sort = report_page.evaluate(
                    "document.querySelector('.sortable-table th.sort-asc, .sortable-table th.sort-desc') !== null"
                )
                check("ソートクラス付与", has_sort)

        # === 9. コンソールエラーチェック ===
        print("\n=== 9. コンソールエラー ===")
        real_errors = [e for e in console_errors if "favicon" not in e.lower() and "manifest" not in e.lower()]
        print(f"  [INFO] エラー数: {len(real_errors)}")
        for e in real_errors[:5]:
            print(f"    - {e[:200]}")
        check("致命的なエラーなし", len(real_errors) == 0)

        # === 10. 印刷プレビュー（JavaScriptでemulate）===
        print("\n=== 10. 印刷モード検証 ===")
        report_page.emulate_media(media="print")
        time.sleep(2)
        ss(report_page, "06_print_preview", full=True)
        # 印刷時に .no-print が隠れているか
        hidden = report_page.evaluate(
            "(function(){var el=document.querySelector('.no-print');"
            "if(!el) return 'no-element';"
            "return getComputedStyle(el).display;})()"
        )
        print(f"  [INFO] .no-print display in print: {hidden}")
        check(".no-print が印刷時に非表示", hidden == "none")
        report_page.emulate_media(media="screen")

        # === サマリー ===
        print("\n" + "="*50)
        print("E2E検証完了")
        print("="*50)
        time.sleep(3)
        browser.close()

if __name__ == "__main__":
    main()
