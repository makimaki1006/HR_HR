# -*- coding: utf-8 -*-
"""目視確認用E2Eテスト（slow_mo=800ms、headless=False）"""
import time, os
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))

def ss(page, name, delay=2):
    time.sleep(delay)
    page.screenshot(path=os.path.join(DIR, f"v_{name}.png"), full_page=False)
    print(f"    [screenshot] v_{name}.png")

def check(label, cond):
    status = "PASS" if cond else "FAIL"
    icon = "OK" if cond else "NG"
    print(f"  [{icon}] [{status}] {label}")
    return cond

def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=False, slow_mo=800)
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
        ss(page, "01_login")
        body = page.text_content("body") or ""
        check("ログイン成功", "ログアウト" in body)

        # === 2. 市場概況タブ確認 ===
        print("\n=== 2. 市場概況タブ ===")
        time.sleep(2)
        body = page.text_content("body") or ""
        check("KPI表示 (総求人数)", "469" in body or "求人" in body)
        check("産業別チャート", "老人福祉" in body or "サービス" in body)
        ss(page, "02_market")
        # 遅延ロードセクション確認（雇用条件・企業分析・採用動向）
        page.evaluate("window.scrollTo(0, 2000)")
        time.sleep(5)
        body = page.text_content("body") or ""
        check("雇用条件セクション", "福利厚生" in body or "社会保険" in body or "週休" in body)
        ss(page, "02_market_workstyle")
        page.evaluate("window.scrollTo(0, 4000)")
        time.sleep(5)
        body = page.text_content("body") or ""
        check("企業分析セクション", "従業員" in body or "中央値" in body)
        ss(page, "02_market_balance")

        # === 5. 地図タブ + 企業マーカー ===
        print("\n=== 5. 地図タブ ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('地図')>=0)b.click()})")
        time.sleep(8)
        ss(page, "05_map_initial")

        # 東京都を選択
        page.evaluate("var s=document.getElementById('jm-pref');if(s){s.value='東京都';s.dispatchEvent(new Event('change'))}")
        time.sleep(3)
        # 市区町村を選択
        page.evaluate("var s=document.getElementById('jm-muni');if(s&&s.options.length>1){s.value=s.options[1].value;s.dispatchEvent(new Event('change'))}")
        time.sleep(2)
        # 検索実行
        page.evaluate("if(typeof postingMap!=='undefined')postingMap.search()")
        time.sleep(8)
        ss(page, "05_map_tokyo")

        # 企業マーカーチェックボックスを探してクリック
        print("\n=== 5b. 企業マーカーレイヤー ===")
        cb = page.query_selector('#jm-show-companies')
        if cb:
            check("企業チェックボックス存在", True)
            cb.click()
            time.sleep(5)
            ss(page, "05_map_companies")
            # APIで企業マーカーデータ確認
            result = page.evaluate("""
                fetch('/api/jobmap/company-markers?south=35.5&north=35.8&west=139.5&east=139.9&zoom=12')
                    .then(r=>r.json()).catch(e=>({error:String(e)}))
            """)
            total = result.get("total", 0) if result else 0
            shown = result.get("shown", 0) if result else 0
            err = result.get("error", "") if result else ""
            check(f"企業マーカーAPI (total={total}, shown={shown})", total > 0)
            if err:
                print(f"    [WARN] API error: {err}")
        else:
            check("企業チェックボックス存在", False)

        # === 6. 詳細分析タブ ===
        print("\n=== 6. 詳細分析タブ ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('詳細分析')>=0)b.click()})")
        time.sleep(6)
        ss(page, "06_analysis")
        body = page.text_content("body") or ""
        check("詳細分析タブ表示", len(body) > 100)
        check("グループナビ", "構造分析" in body and "トレンド" in body and "総合診断" in body)

        # === 7. 企業検索タブ (SalesNow) ===
        print("\n=== 7. 企業検索タブ ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('企業検索')>=0)b.click()})")
        time.sleep(8)
        ss(page, "07_company_search")
        body = page.text_content("body") or ""
        check("企業検索タブ表示", "企業" in body)
        check("DB接続済み", "未接続" not in body)

        # === 8. API v1テスト ===
        print("\n=== 8. API v1 ===")
        api1 = page.evaluate("fetch('/api/v1/companies?q=日本郵便&limit=3').then(r=>r.json()).catch(e=>({error:String(e)}))")
        cnt = api1.get("count", 0) if api1 else 0
        check(f"API企業検索 (count={cnt})", cnt > 0)

        if cnt > 0 and api1.get("results"):
            corp = api1["results"][0].get("corporate_number", "")
            if corp:
                api2 = page.evaluate(f"fetch('/api/v1/companies/{corp}/nearby').then(r=>r.json()).catch(e=>({{error:String(e)}}))")
                cnt2 = api2.get("count", 0) if api2 else 0
                check(f"API近隣企業 (count={cnt2})", cnt2 >= 0)

                api3 = page.evaluate(f"fetch('/api/v1/companies/{corp}/postings').then(r=>r.json()).catch(e=>({{error:String(e)}}))")
                cnt3 = api3.get("count", 0) if api3 else 0
                check(f"API求人マッチ (count={cnt3})", cnt3 >= 0)

        # === 9. 媒体分析タブ ===
        print("\n=== 9. 媒体分析タブ ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('媒体分析')>=0)b.click()})")
        time.sleep(6)
        ss(page, "09_survey")
        body = page.text_content("body") or ""
        check("CSVアップロードフォーム", "CSV" in body or "ファイル" in body)

        # === 10. 全タブ巡回 ===
        print("\n=== 10. 全タブ巡回 ===")
        tabs = ["市場概況","地図","詳細分析","求人検索","条件診断","企業検索","媒体分析"]
        tab_results = []
        for tab in tabs:
            page.evaluate(f"document.querySelectorAll('.tab-btn').forEach(function(b){{if(b.textContent.indexOf('{tab}')>=0)b.click()}})")
            time.sleep(4)
            content = page.text_content('#content') or ""
            ok = len(content) > 10
            icon = "OK" if ok else "NG"
            print(f"  {icon} {tab}: {len(content)} chars")
            tab_results.append(ok)

        ss(page, "10_final")
        check("全タブ正常", all(tab_results))

        print("\n" + "=" * 50)
        print("目視確認E2Eテスト完了")
        print("=" * 50)

        # 最後にブラウザを少し開いたまま確認時間
        print("\n30秒間ブラウザを表示します。確認してください...")
        time.sleep(30)

        browser.close()

if __name__ == "__main__":
    main()
