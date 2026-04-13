# -*- coding: utf-8 -*-
"""CSV→/report/survey→PDF生成（ad-hoc 目視確認用）"""
import os, time, csv, random
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
CSV_PATH = os.path.join(DIR, "_mixed_mock.csv")

def make_mixed_csv():
    """正社員+パート+契約を含む混合CSV（雇用形態別比較のテストデータ）"""
    companies = ["株式会社Alpha", "Beta商事", "Gamma産業", "Delta工業", "Epsilon技研"]
    locations = [("東京都", "千代田区"), ("東京都", "新宿区"), ("東京都", "渋谷区")]
    # 正社員20件、パート20件、契約10件
    emp_types = ["正社員"] * 20 + ["パート・アルバイト"] * 20 + ["契約社員"] * 10
    random.seed(999)
    random.shuffle(emp_types)
    tags_pool = [
        "週休2日,交通費支給,社保完備",
        "未経験可,研修制度,昇給あり",
        "残業少なめ,土日休み,年間休日120日",
    ]
    rows = []
    for i, emp in enumerate(emp_types):
        pref, muni = random.choice(locations)
        if emp == "パート・アルバイト":
            salary = f"時給{random.randint(1100,1400)}円"
        elif emp == "正社員":
            min_s = random.randint(22, 35)
            max_s = min_s + random.randint(3, 8)
            salary = f"月給{min_s}万円~{max_s}万円"
        else:  # 契約
            min_s = random.randint(20, 28)
            max_s = min_s + random.randint(2, 6)
            salary = f"月給{min_s}万円~{max_s}万円"
        rows.append({
            "求人タイトル": f"職種 No.{i+1}",
            "企業名": random.choice(companies),
            "勤務地": f"{pref}{muni}",
            "給与": salary,
            "雇用形態": emp,
            "タグ": random.choice(tags_pool),
            "URL": f"https://example.com/{i}",
            "新着": "新着" if i < 10 else "",
        })
    with open(CSV_PATH, "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)

def main():
    make_mixed_csv()
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()
        page.goto(BASE, timeout=60000)
        time.sleep(2)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "cyxen_2025")
        page.click('button[type="submit"]')
        time.sleep(6)

        # 媒体分析タブクリック
        page.evaluate("""
            (function(){
                var btns = document.querySelectorAll('.tab-btn');
                for (var b of btns) if (b.textContent.indexOf('媒体')>=0) { b.click(); return; }
            })()
        """)
        time.sleep(2)
        # file input 待機
        for _ in range(20):
            if page.evaluate("!!document.querySelector('input[type=\"file\"]')"):
                break
            time.sleep(1)

        # CSVアップロード
        fi = page.query_selector('input[type="file"]')
        fi.set_input_files(CSV_PATH)
        time.sleep(1)
        page.evaluate("""
            (function(){
                var i=document.querySelector('input[type="file"]');
                if (i && i.files[0] && window.submitSurveyCSV) window.submitSurveyCSV(i.files[0]);
            })()
        """)
        # 結果待機
        for _ in range(25):
            rt = page.evaluate("(document.getElementById('survey-result')||{textContent:''}).textContent.length")
            if rt > 100: break
            time.sleep(1)
        time.sleep(2)

        # レポートリンク取得
        link = page.query_selector('a[href*="/report/survey"]')
        href = link.get_attribute("href")
        report_url = href if href.startswith("http") else BASE + href
        print(f"Report URL: {report_url}")

        # 新タブで開いてPDF出力
        rp = ctx.new_page()
        rp.goto(report_url, timeout=60000)
        time.sleep(10)
        rp.emulate_media(media="print")
        time.sleep(2)
        pdf_path = os.path.join(DIR, "report_survey_mixed.pdf")
        rp.pdf(path=pdf_path, format="A4", print_background=True, margin={"top":"8mm","bottom":"8mm","left":"10mm","right":"10mm"})
        print(f"PDF生成: {pdf_path} ({os.path.getsize(pdf_path):,} bytes)")

        # スクショも
        rp.emulate_media(media="screen")
        for i, y in enumerate([0, 900, 1800, 2700, 3600]):
            rp.evaluate(f"window.scrollTo(0,{y})")
            time.sleep(1)
            rp.screenshot(path=os.path.join(DIR, f"mixed_{i+1}.png"), full_page=False)
        browser.close()

if __name__ == "__main__":
    main()
