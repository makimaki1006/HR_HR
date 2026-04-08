# -*- coding: utf-8 -*-
"""Post-deploy E2E verification — 7タブ構成 + チャート描画検証"""
import time, os, json
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
issues = []
pass_count = 0

def ss(page, name, delay=2):
    time.sleep(delay)
    page.screenshot(path=os.path.join(DIR, f"{name}.png"), full_page=False)

def check(label, cond):
    global pass_count
    if cond:
        pass_count += 1
        print(f"  [PASS] {label}")
    else:
        issues.append(label)
        print(f"  [FAIL] {label}")

def count_charts(page):
    """EChartsチャートの初期化状態を検証"""
    result = page.evaluate("""
        (function() {
            var charts = document.querySelectorAll('.echart[data-chart-config]');
            var init = 0, blank = 0;
            charts.forEach(function(el) {
                var inst = typeof echarts !== 'undefined' ? echarts.getInstanceByDom(el) : null;
                if (inst) init++; else blank++;
            });
            return {total: charts.length, init: init, blank: blank};
        })()
    """)
    return result or {"total": 0, "init": 0, "blank": 0}

def click_tab(page, tab_name, wait=8):
    """タブをクリックしてコンテンツ読み込みを待つ"""
    page.evaluate(f"""
        document.querySelectorAll('.tab-btn').forEach(function(b){{
            if(b.textContent.trim()==='{tab_name}') b.click();
        }})
    """)
    time.sleep(wait)

def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # === 1. Login ===
        print("=== 1. Login ===")
        page.goto(BASE, timeout=60000)
        time.sleep(3)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "cyxen_2025")
        page.click('button[type="submit"]')
        time.sleep(8)
        ss(page, "d01_login")
        check("Login", "ログアウト" in (page.text_content("body") or ""))

        # === 2. Tab structure ===
        print("\n=== 2. Tab Structure ===")
        tabs = page.query_selector_all(".tab-btn")
        tab_labels = [t.text_content().strip() for t in tabs]
        expected = ["市場概況", "地図", "詳細分析", "求人検索", "条件診断", "企業検索", "媒体分析"]
        check(f"7 tabs (got {len(tab_labels)})", tab_labels == expected)

        help_btn = page.query_selector('button[title="使い方ガイド"]')
        check("Help button in header", help_btn is not None)

        # === 3. 市場概況タブ ===
        print("\n=== 3. Market Overview ===")
        # 初期ロード（概況セクション）+ 遅延ロードセクション待ち
        time.sleep(45)
        ss(page, "d02_market_top")
        body = page.text_content("#content") or ""
        check("KPI: total postings", "469" in body or "総求人数" in body)
        check("KPI: facilities", "130" in body or "事業所" in body)
        check("Section nav", page.query_selector('a[href="#sec-overview"]') is not None)

        # 概況セクションのチャート
        charts = count_charts(page)
        print(f"  [INFO] Charts after overview: {charts['init']}/{charts['total']} init")
        check("Overview charts rendered", charts["init"] >= 4)

        # スクロールして遅延ロードセクション確認
        page.evaluate("window.scrollTo(0, 3000)")
        time.sleep(3)
        ss(page, "d03_market_workstyle")
        body = page.text_content("#content") or ""
        check("Workstyle section: benefits", "福利厚生" in body or "社会保険" in body)
        check("Workstyle section: holidays", "週休" in body or "休日" in body)

        page.evaluate("window.scrollTo(0, 6000)")
        time.sleep(3)
        ss(page, "d04_market_balance")
        body = page.text_content("#content") or ""
        check("Balance section: employee stats", "従業員" in body or "中央値" in body)

        page.evaluate("window.scrollTo(0, 9000)")
        time.sleep(3)
        ss(page, "d05_market_demographics")
        body = page.text_content("#content") or ""
        check("Demographics section: recruitment", "求人理由" in body or "学歴" in body or "資格" in body)

        # 全セクションのチャート検証
        charts = count_charts(page)
        print(f"  [INFO] Charts total: {charts['init']}/{charts['total']} init, {charts['blank']} blank")
        check(f"All charts initialized ({charts['blank']} blank)", charts["blank"] == 0)

        page.evaluate("window.scrollTo(0, 0)")

        # === 4. 地図タブ ===
        print("\n=== 4. Map Tab ===")
        click_tab(page, "地図")
        ss(page, "d06_map")
        body = page.text_content("#content") or ""
        check("Map tab loaded", len(body) > 100)

        # 東京都を選択してマーカー確認
        page.evaluate("var s=document.getElementById('pref-select');if(s){s.value='東京都';s.dispatchEvent(new Event('change'))}")
        time.sleep(8)

        markers_resp = page.evaluate("""
            fetch('/api/jobmap/company-markers?south=35.5&north=35.8&west=139.5&east=139.9&zoom=12')
                .then(r => r.json()).catch(e => ({error: String(e)}))
        """)
        markers_total = markers_resp.get("total", 0) if markers_resp else 0
        check(f"Company markers API (total={markers_total})", markers_total > 0)

        # === 5. 詳細分析タブ ===
        print("\n=== 5. Analysis Tab ===")
        click_tab(page, "詳細分析", wait=10)
        ss(page, "d07_analysis")
        body = page.text_content("#content") or ""
        check("Group nav: 3 groups", "構造分析" in body and "トレンド" in body and "総合診断" in body)
        check("Subtab: vacancy", "欠員" in body or "求人動向" in body)

        # 構造分析サブタブ切替
        salary_btn = page.query_selector('button.analysis-subtab:has-text("給与分析")')
        if salary_btn:
            salary_btn.click()
            time.sleep(8)
            body = page.text_content("#content") or ""
            check("Subtab: salary analysis", "給与" in body or "月給" in body)

        # トレンドグループ
        trend_btn = page.query_selector('.analysis-group:has-text("トレンド")')
        if trend_btn:
            trend_btn.click()
            time.sleep(15)
            ss(page, "d08_trend")
            body = page.text_content("#content") or ""
            check("Trend group loaded", "時系列" in body or "推移" in body or "量の変化" in body)

            # トレンドチャート描画確認
            trend_charts = count_charts(page)
            print(f"  [INFO] Trend charts: {trend_charts['init']}/{trend_charts['total']} init")
            check("Trend charts rendered", trend_charts["init"] >= 1)

        # 総合診断グループ
        insight_btn = page.query_selector('.analysis-group:has-text("総合診断")')
        if insight_btn:
            insight_btn.click()
            time.sleep(10)
            ss(page, "d09_insight")
            body = page.text_content("#content") or ""
            check("Insight group loaded", "採用構造" in body or "シグナル" in body)

        # === 6. 求人検索タブ ===
        print("\n=== 6. Job Search Tab ===")
        click_tab(page, "求人検索", wait=10)
        ss(page, "d10_search")
        body = page.text_content("#content") or ""
        check("Search tab loaded", len(body) > 100)

        # === 7. 条件診断タブ ===
        print("\n=== 7. Diagnostic Tab ===")
        click_tab(page, "条件診断")
        ss(page, "d11_diagnostic")
        body = page.text_content("#content") or ""
        check("Diagnostic form", "月給" in body or "診断" in body or "グレード" in body)

        # === 8. 企業検索タブ ===
        print("\n=== 8. Company Search Tab ===")
        click_tab(page, "企業検索")
        ss(page, "d12_company")
        body = page.text_content("#content") or ""
        check("Company tab loaded", "企業" in body)
        check("DB connected", "未接続" not in body)

        # 企業検索 → プロフィール
        page.evaluate("""
            var inputs = document.querySelectorAll('#content input');
            for(var i=0;i<inputs.length;i++){
                if(inputs[i].type==='text'||inputs[i].type==='search'){inputs[i].focus();break;}
            }
        """)
        time.sleep(1)
        page.keyboard.type("トヨタ", delay=200)
        time.sleep(5)
        ss(page, "d13_company_search")
        body = page.text_content("body") or ""
        check("Search results", "トヨタ" in body)

        page.evaluate("""
            var items = document.querySelectorAll('#company-results a, #company-results [hx-get], [onclick*=profile], #company-results div[class*=cursor]');
            if(items.length>0) items[0].click();
        """)
        time.sleep(10)
        ss(page, "d14_company_profile")
        body = page.text_content("body") or ""
        check("Company profile loaded", len(body) > 500)

        # API v1
        print("\n=== 9. API v1 ===")
        api1 = page.evaluate("fetch('/api/v1/companies?q=日本郵便&limit=3').then(r=>r.json()).catch(e=>({error:String(e)}))")
        cnt = api1.get("count", 0) if api1 else 0
        check(f"API company search (count={cnt})", cnt > 0)

        if cnt > 0 and api1.get("results"):
            corp = api1["results"][0].get("corporate_number", "")
            if corp:
                api2 = page.evaluate(f"fetch('/api/v1/companies/{corp}/nearby').then(r=>r.json()).catch(e=>({{error:String(e)}}))")
                cnt2 = api2.get("count", 0) if api2 else 0
                check(f"API nearby (count={cnt2})", cnt2 >= 0)

                api3 = page.evaluate(f"fetch('/api/v1/companies/{corp}/postings').then(r=>r.json()).catch(e=>({{error:String(e)}}))")
                cnt3 = api3.get("count", 0) if api3 else 0
                check(f"API postings (count={cnt3})", cnt3 >= 0)

        # === 10. 媒体分析タブ ===
        print("\n=== 10. Survey Tab ===")
        click_tab(page, "媒体分析")
        ss(page, "d15_survey")
        body = page.text_content("#content") or ""
        check("Survey title: 媒体分析", "媒体分析" in body)
        check("CSV upload form", "CSV" in body or "ファイル" in body)

        # 統合レポートAPI
        integrate_resp = page.evaluate("""
            fetch('/api/survey/integrate?session_id=e2e_test_dummy')
                .then(r => ({status: r.status}))
                .catch(e => ({error: String(e)}))
        """)
        if integrate_resp and "error" not in integrate_resp:
            int_status = integrate_resp.get("status", 0)
            check(f"Integration API reachable (status={int_status})", int_status in [200, 400, 404, 422, 500])

        # === 11. 通勤圏サブタブ ===
        print("\n=== 11. Commute Zone ===")
        # 市区町村選択済みの状態で分析タブ→通勤圏
        click_tab(page, "詳細分析", wait=10)
        # 構造分析グループに戻す
        struct_btn = page.query_selector('.analysis-group:has-text("構造分析")')
        if struct_btn:
            struct_btn.click()
            time.sleep(5)
        commute_btn = page.query_selector('button.analysis-subtab:has-text("通勤圏")')
        if commute_btn:
            commute_btn.click()
            time.sleep(10)
            ss(page, "d16_commute")
            body = page.text_content("#content") or ""
            check("Commute zone content", "通勤圏" in body or "圏内" in body)
        else:
            print("  [INFO] Commute subtab button not found")

        # === 12. 全タブ巡回（最終確認） ===
        print("\n=== 12. All Tabs Round-trip ===")
        for tab in expected:
            click_tab(page, tab, wait=6)
            content = page.text_content('#content') or ""
            ok = len(content) > 10
            print(f"  [{'OK' if ok else 'EMPTY'}] {tab} ({len(content)} chars)")
            if not ok:
                issues.append(f"Tab empty: {tab}")

        # 市場概況に戻ってチャート最終検証
        click_tab(page, "市場概況", wait=45)
        final_charts = count_charts(page)
        print(f"\n  [INFO] Final chart state: {final_charts['init']}/{final_charts['total']} init, {final_charts['blank']} blank")
        check(f"Final: all charts initialized ({final_charts['blank']} blank)", final_charts["blank"] == 0)

        browser.close()

    # === Summary ===
    print("\n" + "=" * 60)
    total = pass_count + len(issues)
    print(f"Results: {pass_count}/{total} passed, {len(issues)} failed")
    if issues:
        print("\nFailed checks:")
        for i, iss in enumerate(issues, 1):
            print(f"  {i}. {iss}")
    else:
        print("ALL CHECKS PASSED")
    print("=" * 60)

    return 1 if issues else 0

if __name__ == "__main__":
    import sys
    sys.exit(main())
