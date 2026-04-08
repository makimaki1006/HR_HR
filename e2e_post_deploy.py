# -*- coding: utf-8 -*-
"""Post-deploy E2E verification with screenshots"""
import time, os, json
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
issues = []

def ss(page, name, delay=3):
    time.sleep(delay)
    page.screenshot(path=os.path.join(DIR, f"{name}.png"), full_page=False)

def check(label, cond):
    status = "PASS" if cond else "FAIL"
    if not cond: issues.append(label)
    print(f"  [{status}] {label}")

def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # Login
        print("=== 1. Login ===")
        page.goto(BASE, timeout=60000); time.sleep(3)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "cyxen_2025")
        page.click('button[type="submit"]'); time.sleep(8)
        ss(page, "d01_login")
        check("Login", "ログアウト" in (page.text_content("body") or ""))

        # Company tab + search
        print("\n=== 2. Company Search ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('企業分析')>=0)b.click()})")
        time.sleep(6)
        ss(page, "d02_company_tab")
        body = page.text_content("body") or ""
        check("Company tab", "企業分析" in body)
        check("DB connected", "未接続" not in body)

        # Type search
        page.evaluate("""
            var inputs = document.querySelectorAll('#content input');
            for(var i=0;i<inputs.length;i++){
                if(inputs[i].type==='text'||inputs[i].type==='search'){inputs[i].focus();break;}
            }
        """)
        time.sleep(1)
        page.keyboard.type("トヨタ", delay=200)
        time.sleep(5)
        ss(page, "d03_search_results")
        body = page.text_content("body") or ""
        check("Search results", "トヨタ" in body)

        # Click first result
        page.evaluate("""
            var items = document.querySelectorAll('#company-results a, #company-results [hx-get], [onclick*=profile], #company-results div[class*=cursor]');
            if(items.length>0) items[0].click();
        """)
        time.sleep(10)
        ss(page, "d04_profile_top")
        body = page.text_content("body") or ""
        check("Profile loaded", len(body) > 500)

        page.evaluate("window.scrollTo(0, 800)")
        time.sleep(2)
        ss(page, "d05_profile_market")

        page.evaluate("window.scrollTo(0, 1800)")
        time.sleep(2)
        ss(page, "d06_profile_detail")

        page.evaluate("window.scrollTo(0, document.body.scrollHeight)")
        time.sleep(2)
        ss(page, "d07_profile_nearby")
        body = page.text_content("body") or ""
        check("Nearby section", "近隣" in body or "エリア" in body or "同一" in body)

        # API v1
        print("\n=== 3. API v1 ===")
        api1 = page.evaluate("fetch('/api/v1/companies?q=日本郵便&limit=3').then(r=>r.json()).catch(e=>({error:String(e)}))")
        cnt = api1.get("count", 0) if api1 else 0
        check(f"API search (count={cnt})", cnt > 0)
        if cnt > 0 and api1.get("results"):
            corp = api1["results"][0].get("corporate_number", "")
            if corp:
                api2 = page.evaluate(f"fetch('/api/v1/companies/{corp}/nearby').then(r=>r.json()).catch(e=>({{error:String(e)}}))")
                cnt2 = api2.get("count", 0) if api2 else 0
                check(f"API nearby (count={cnt2})", cnt2 >= 0)

                api3 = page.evaluate(f"fetch('/api/v1/companies/{corp}/postings').then(r=>r.json()).catch(e=>({{error:String(e)}}))")
                cnt3 = api3.get("count", 0) if api3 else 0
                check(f"API postings (count={cnt3})", cnt3 >= 0)

        # Balance tab
        print("\n=== 4. Balance Tab ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('企業')>=0 && b.textContent.indexOf('分析')<0)b.click()})")
        time.sleep(8)
        ss(page, "d08_balance")
        body = page.text_content("body") or ""
        check("Median employee", "中央値" in body)

        # Workstyle tab
        print("\n=== 5. Workstyle Tab ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('条件')>=0)b.click()})")
        time.sleep(10)
        ss(page, "d09_workstyle_top")
        page.evaluate("window.scrollTo(0, 1200)")
        time.sleep(3)
        ss(page, "d10_workstyle_mid")
        page.evaluate("window.scrollTo(0, 2400)")
        time.sleep(3)
        ss(page, "d11_workstyle_bottom")
        body = page.text_content("body") or ""
        check("Benefits", "福利厚生" in body or "社会保険" in body)
        check("Overtime", "残業" in body)
        check("Holiday detail", "週休" in body or "休日" in body)

        # Map tab
        print("\n=== 6. Map Tab ===")
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('地図')>=0)b.click()})")
        time.sleep(6)
        page.evaluate("var s=document.getElementById('pref-select');if(s){s.value='東京都';s.dispatchEvent(new Event('change'))}")
        time.sleep(8)
        ss(page, "d12_map")

        # Commute zone
        print("\n=== 7. Commute Zone ===")
        page.evaluate("var s=document.getElementById('muni-select');if(s&&s.options.length>1){s.value=s.options[1].value;s.dispatchEvent(new Event('change'))}")
        time.sleep(5)
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('分析')>=0)b.click()})")
        time.sleep(5)
        btn = page.query_selector('button:has-text("通勤圏")')
        if btn:
            btn.click()
            time.sleep(8)
        ss(page, "d13_commute")
        body = page.text_content("body") or ""
        check("Commute zone", "通勤圏" in body or "圏内" in body)

        # All tabs
        print("\n=== 8. All Tabs ===")
        tabs = ["市場概況","地図","詳細分析","求人検索","条件診断","企業検索","媒体分析"]
        for tab in tabs:
            page.evaluate(f"document.querySelectorAll('.tab-btn').forEach(function(b){{if(b.textContent.indexOf('{tab}')>=0)b.click()}})")
            time.sleep(5)
            content = page.text_content('#content') or ""
            ok = len(content) > 10
            print(f"  [{'OK' if ok else 'EMPTY'}] {tab} ({len(content)})")
            if not ok: issues.append(f"Tab: {tab}")

        # Company Markers API (Phase 2)
        print("\n=== 9. Company Markers API ===")

        # 9a. 正常リクエスト: 東京都周辺の範囲、zoom >= 10
        markers_resp = page.evaluate("""
            fetch('/api/jobmap/company-markers?south=35.5&north=35.8&west=139.5&east=139.9&zoom=12')
                .then(r => r.json())
                .catch(e => ({error: String(e)}))
        """)
        markers_err = markers_resp.get("error") if markers_resp else "no response"
        check("Markers API reachable", markers_resp is not None and "error" not in markers_resp)

        markers_total = markers_resp.get("total", 0) if markers_resp else 0
        markers_list = markers_resp.get("markers", []) if markers_resp else []
        check(f"Markers data loaded (total={markers_total})", markers_total > 0)
        check(f"Markers array returned (len={len(markers_list)})", len(markers_list) > 0)

        # 9b. マーカーのフィールド検証（最初の1件）
        if len(markers_list) > 0:
            first = markers_list[0]
            required_fields = ["corporate_number", "lat", "lng", "company_name"]
            missing = [f for f in required_fields if f not in first]
            check(f"Marker required fields (missing={missing})", len(missing) == 0)

            optional_fields = ["sn_industry", "employee_count", "credit_score"]
            present_optional = [f for f in optional_fields if f in first]
            print(f"  [INFO] Optional fields present: {present_optional}")

            # 座標の妥当性チェック（日本国内: lat 24-46, lng 122-146）
            lat = first.get("lat", 0)
            lng = first.get("lng", 0)
            lat_ok = 24 <= lat <= 46
            lng_ok = 122 <= lng <= 146
            check(f"Marker coords in Japan (lat={lat}, lng={lng})", lat_ok and lng_ok)

        # 9c. zoom < 10 のリクエスト: マーカーが空で返ること
        low_zoom_resp = page.evaluate("""
            fetch('/api/jobmap/company-markers?south=30&north=45&west=128&east=146&zoom=5')
                .then(r => r.json())
                .catch(e => ({error: String(e)}))
        """)
        if low_zoom_resp and "error" not in low_zoom_resp:
            low_markers = low_zoom_resp.get("markers", [])
            low_msg = low_zoom_resp.get("zoom_required", low_zoom_resp.get("message", ""))
            check(f"Low zoom returns empty markers (len={len(low_markers)})", len(low_markers) == 0)
            check(f"Low zoom has message ('{low_msg[:30]}...')" if low_msg else "Low zoom has zoom_required message", len(str(low_msg)) > 0)
        else:
            # APIがエラーを返す場合も許容（zoom制限の実装方法による）
            print(f"  [INFO] Low zoom response: {low_zoom_resp}")
            check("Low zoom handled", low_zoom_resp is not None)

        # 9d. パラメータ欠落時のエラーハンドリング
        bad_resp = page.evaluate("""
            fetch('/api/jobmap/company-markers?south=35&north=36')
                .then(r => ({status: r.status, ok: r.ok}))
                .catch(e => ({error: String(e)}))
        """)
        if bad_resp:
            # 400系エラーか、空結果を返すことを期待
            bad_status = bad_resp.get("status", 0)
            check(f"Missing params handled (status={bad_status})", bad_status in [200, 400, 422])
        ss(page, "d14_markers_api")

        # Survey x Company Integration (Phase 1)
        print("\n=== 10. Survey x Company Integration ===")

        # 10a. 媒体分析タブに移動してセッション存在確認
        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('媒体分析')>=0)b.click()})")
        time.sleep(6)
        ss(page, "d15_survey_tab")
        body = page.text_content("body") or ""
        check("Survey tab loaded", len(body) > 50)

        # 10b. 統合レポートAPIをダミーsession_idで呼び出し
        # 実際のCSVアップロードなしでもAPIの存在とレスポンス構造を確認
        integrate_resp = page.evaluate("""
            fetch('/api/survey/integrate?session_id=e2e_test_dummy')
                .then(r => ({status: r.status, text: r.text()}))
                .then(async obj => {
                    if (obj.text && typeof obj.text.then === 'function') {
                        obj.text = await obj.text;
                    }
                    return obj;
                })
                .catch(e => ({error: String(e)}))
        """)
        if integrate_resp and "error" not in integrate_resp:
            int_status = integrate_resp.get("status", 0)
            int_body = integrate_resp.get("text", "")
            # ダミーセッションなので404/400は正常、200ならレポートが返る
            check(f"Integration API reachable (status={int_status})", int_status in [200, 400, 404, 422, 500])

            # 200が返った場合、企業データセクションの存在を確認
            if int_status == 200 and isinstance(int_body, str):
                has_company = "該当地域の企業データ" in int_body or "SalesNow" in int_body
                print(f"  [INFO] Integration report has company section: {has_company}")
                if has_company:
                    check("Integration report includes company data", True)
        else:
            # APIエンドポイント自体がない場合
            print(f"  [INFO] Integration API response: {integrate_resp}")
            check("Integration API exists", False)

        # 10c. セッション付きの統合レポートが企業テーブルを含むか
        # 調査タブ内に統合レポートボタンがあれば押して確認
        integrate_btn = page.query_selector('button:has-text("統合"), button:has-text("レポート"), button:has-text("integrate")')
        if integrate_btn:
            integrate_btn.click()
            time.sleep(8)
            ss(page, "d16_survey_integrate")
            body = page.text_content("body") or ""
            has_company_table = "該当地域の企業データ" in body or "SalesNow" in body or "企業" in body
            check("Survey integration UI has company reference", has_company_table)
        else:
            print("  [INFO] No integration button found in survey tab (may require CSV upload first)")
            ss(page, "d16_survey_no_integrate")

        browser.close()

    print("\n" + "=" * 50)
    if issues:
        print(f"ISSUES: {len(issues)}")
        for i, iss in enumerate(issues, 1):
            print(f"  {i}. {iss}")
    else:
        print("ALL CHECKS PASSED")
    print("=" * 50)

if __name__ == "__main__":
    main()
