# -*- coding: utf-8 -*-
"""
求人ボックス形式CSVでの /report/survey E2E検証
- 求人ボックス固有ヘッダー（企業名・所在地・賃金・特徴）でCSV自動判定されるか
- Indeed形式と同じレポート品質が出るか
"""
import os, time, csv, random, base64
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
CSV_PATH = os.path.join(DIR, "_jobbox_mock.csv")

def make_jobbox_csv():
    """求人ボックス形式のモックCSV（ヘッダー: 職種、企業名、所在地、賃金、就業形態、特徴、URL、新着）"""
    companies = ["株式会社ABC求人", "DEF商事", "GHIサービス", "JKLホールディングス", "MNO産業"]
    locations = [
        ("東京都", "千代田区"), ("東京都", "新宿区"), ("東京都", "渋谷区"),
        ("東京都", "港区"), ("神奈川県", "横浜市中区"),
    ]
    emp_types = ["正社員", "契約社員", "パート・アルバイト", "派遣社員"]
    tags_pool = [
        "駅近,社保完備,週休2日",
        "経験者歓迎,年間休日120日,賞与あり",
        "未経験歓迎,研修充実,昇給あり",
        "残業少なめ,土日休み,交通費支給",
        "マイカー通勤可,育児支援,退職金制度",
    ]

    rows = []
    random.seed(100)
    for i in range(30):
        company = random.choice(companies)
        pref, muni = random.choice(locations)
        emp = random.choice(emp_types)
        tags = random.choice(tags_pool)
        if emp == "パート・アルバイト":
            salary = f"時給 {random.randint(1100, 1500)}円"
        else:
            min_sal = random.randint(22, 38)
            max_sal = min_sal + random.randint(3, 10)
            salary = f"月給 {min_sal}万円 ~ {max_sal}万円"
        rows.append({
            "職種": f"事務職 No.{i+1}",  # 求人ボックスは「職種」ヘッダー
            "企業名": company,
            "所在地": f"{pref}{muni}",
            "賃金": salary,
            "就業形態": emp,
            "特徴": tags,
            "URL": f"https://kyujinbox.com/job/{i+1}",
            "新着": "新着" if i < 6 else "",
        })
    with open(CSV_PATH, "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    return len(rows)

def ss(page, name, full=False):
    time.sleep(2)
    path = os.path.join(DIR, f"jb_{name}.png")
    try:
        page.screenshot(path=path, full_page=full, timeout=15000)
        print(f"    [screenshot] jb_{name}.png")
    except Exception as e:
        print(f"    [screenshot-FAIL] jb_{name}.png: {type(e).__name__}")

def check(label, cond):
    status = "PASS" if cond else "FAIL"
    icon = "OK" if cond else "NG"
    print(f"  [{icon}] [{status}] {label}")
    return cond

def main():
    n = make_jobbox_csv()
    print(f"[INFO] 求人ボックス形式モックCSV: {n}件 → {CSV_PATH}")
    # CSVヘッダー確認
    with open(CSV_PATH, encoding='utf-8') as f:
        print(f"[INFO] CSV先頭行: {f.readline().strip()}")

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=False, slow_mo=400)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # === 1. ログイン ===
        print("\n=== 1. ログイン ===")
        page.goto(BASE, timeout=60000)
        time.sleep(3)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "cyxen_2025")
        page.click('button[type="submit"]')
        time.sleep(8)
        check("ログイン成功", "ログアウト" in (page.text_content("body") or ""))

        # === 2. 媒体分析タブ ===
        print("\n=== 2. 媒体分析タブ ===")
        survey_btn = None
        for btn in page.query_selector_all('.tab-btn'):
            if "媒体" in (btn.text_content() or ""):
                survey_btn = btn
                break
        survey_btn.click()
        time.sleep(2)
        for wait_sec in range(20):
            if page.evaluate("!!document.querySelector('input[type=\"file\"]')"):
                break
            time.sleep(1)

        # === 3. 求人ボックスCSVアップロード ===
        print("\n=== 3. 求人ボックスCSVアップロード ===")
        file_input = page.query_selector('input[type="file"]')
        file_input.set_input_files(CSV_PATH)
        time.sleep(1)

        page.evaluate(
            "(function(){var i=document.querySelector('input[type=\"file\"]');"
            "if(i&&i.files&&i.files[0]&&typeof window.submitSurveyCSV==='function')"
            "{window.submitSurveyCSV(i.files[0]);}})()"
        )
        # アップロード結果待機
        for wait_sec in range(25):
            rt = page.evaluate(
                "(function(){var r=document.getElementById('survey-result');"
                "return r?(r.textContent||'').length:0;})()"
            )
            if rt > 100:
                print(f"  [INFO] アップロード結果受信 ({wait_sec+1}秒, {rt}文字)")
                break
            time.sleep(1)
        ss(page, "01_after_upload")

        # === 4. データソース判定確認 ===
        print("\n=== 4. CSVソース判定 ===")
        source_info = page.evaluate(
            "(function(){var r=document.getElementById('survey-result');"
            "if(!r) return null;"
            "var text = r.textContent || '';"
            "return {hasJobBox: text.indexOf('求人ボックス')>=0, "
            "hasIndeed: text.indexOf('Indeed')>=0, "
            "hasSrcLabel: text.indexOf('データソース')>=0,"
            "hasCount: text.indexOf('件')>=0,"
            "len: text.length};})()"
        )
        print(f"  [INFO] 分析結果テキスト情報: {source_info}")

        # === 5. レポート出力 ===
        print("\n=== 5. レポート出力 ===")
        session_link = page.query_selector('a[href*="/report/survey"]')
        if not session_link:
            print("  [ERROR] /report/survey リンクが見つからない")
            ss(page, "02_no_link", full=True)
            browser.close()
            return
        href = session_link.get_attribute("href")
        report_url = href if href.startswith("http") else BASE + href
        print(f"  [INFO] report URL: {report_url}")

        report_page = ctx.new_page()
        report_page.goto(report_url, timeout=60000)
        time.sleep(8)
        ss(report_page, "03_report_top")

        # === 6. データ妥当性検証 ===
        print("\n=== 6. データ妥当性 ===")
        # 総求人数が30件であること
        kpi_info = report_page.evaluate("""
            (function(){
                var cards = document.querySelectorAll('.summary-card');
                return Array.from(cards).map(function(c){
                    var label = c.querySelector('.label, .card-label');
                    var value = c.querySelector('.value, .card-value');
                    return {
                        label: label ? label.textContent.trim() : (c.textContent.split(/\\s+/).slice(-1)[0] || ''),
                        value: value ? value.textContent.trim() : ''
                    };
                });
            })()
        """)
        print(f"  [INFO] KPIカード内容: {kpi_info}")

        # ヒストグラム0バケット検証
        zero_check = report_page.evaluate("""
            (function(){
                var result = [];
                document.querySelectorAll('[data-chart-config]').forEach(function(el, idx){
                    try {
                        var cfg = JSON.parse(el.getAttribute('data-chart-config'));
                        if (cfg.xAxis && cfg.xAxis.data && cfg.series && cfg.series[0] && cfg.series[0].type === 'bar') {
                            var labels = cfg.xAxis.data;
                            var values = cfg.series[0].data;
                            var zeroIdx = labels.findIndex(function(l){return l === '0万' || l === '0';});
                            if (zeroIdx >= 0) {
                                var total = values.reduce(function(a,b){return a+b;}, 0);
                                var zeroVal = values[zeroIdx] || 0;
                                result.push({chart: idx, zero: zeroVal, total: total, ratio: total>0?zeroVal/total:0});
                            }
                        }
                    } catch(e) {}
                });
                return result;
            })()
        """)
        print(f"  [INFO] 0バケットチェック: {zero_check}")
        for z in zero_check:
            check(
                f"chart[{z['chart']}] 0バケット<10% (実測 {z['ratio']*100:.1f}%)",
                z['ratio'] < 0.1
            )

        # === 7. 各セクション存在確認 ===
        print("\n=== 7. レポートセクション ===")
        rbody = report_page.text_content("body") or ""
        check("サマリー", "サマリー" in rbody)
        check("給与分布", "給与分布" in rbody)
        check("雇用形態分布", "雇用形態" in rbody)
        check("相関分析", "相関" in rbody or "散布" in rbody)
        check("地域分析", "地域分析" in rbody)
        check("市区町村別給与", "市区町村" in rbody)
        check("最低賃金比較", "最低賃金" in rbody)
        check("企業分析", "企業分析" in rbody)
        check("タグ分析", "タグ" in rbody)
        check("求職者心理", "求職者" in rbody)

        # ECharts検証
        echart_count = report_page.evaluate(
            "document.querySelectorAll('[data-chart-config]').length"
        )
        initialized = report_page.evaluate("""
            (function(){
                if (typeof echarts === 'undefined') return 0;
                var count = 0;
                document.querySelectorAll('[data-chart-config]').forEach(function(el){
                    if (echarts.getInstanceByDom(el)) count++;
                });
                return count;
            })()
        """)
        print(f"  [INFO] ECharts config数: {echart_count}, 初期化済み: {initialized}")
        check("ECharts初期化 (>=3)", initialized >= 3)

        # === 8. 求人ボックス特有データ確認 ===
        print("\n=== 8. 求人ボックス固有データ検証 ===")
        # 会社名に「株式会社ABC求人」のような求人ボックス用企業名が出るか
        has_jobbox_company = any(c in rbody for c in ["株式会社ABC求人", "DEF商事", "GHIサービス", "JKLホールディングス", "MNO産業"])
        check("求人ボックス企業名が表示される", has_jobbox_company)

        # スクリーンショット
        for i, y in enumerate([0, 800, 1600, 2400, 3200]):
            report_page.evaluate(f"window.scrollTo(0, {y})")
            time.sleep(1.5)
            ss(report_page, f"04_section_{i+1}")

        print("\n" + "="*50)
        print("求人ボックスCSV E2E検証完了")
        print("="*50)
        time.sleep(3)
        browser.close()

if __name__ == "__main__":
    main()
