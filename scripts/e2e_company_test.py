# -*- coding: utf-8 -*-
"""企業分析タブのE2Eテスト

テスト項目:
1. タブ表示 — 検索ボックスが表示される
2. 企業検索 — 「ヤマト」で結果が返る
3. プロフィール表示 — KPIカード、チャート、示唆が表示される
4. HW求人マッチング — 求人テーブルが表示される（or「掲載なし」メッセージ）
5. 近隣企業 — 郵便番号エリアの企業テーブルが表示される
6. 近隣企業クリック — 別企業のプロフィールに遷移
7. 印刷レポート — /report/company/{corp} がHTML返却
"""
import sys
import io
import time
import os
import json

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

from playwright.sync_api import sync_playwright

BASE = "http://localhost:9216"
DIR = os.path.dirname(os.path.abspath(__file__))
SS_DIR = os.path.join(os.path.dirname(DIR), "scripts", "screenshots")
os.makedirs(SS_DIR, exist_ok=True)

PASSED = 0
FAILED = 0
RESULTS = []


def test(name, condition, detail=""):
    global PASSED, FAILED
    if condition:
        PASSED += 1
        RESULTS.append(f"  PASS: {name}")
    else:
        FAILED += 1
        RESULTS.append(f"  FAIL: {name} — {detail}")


def main():
    global PASSED, FAILED

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # ログイン
        page.goto(BASE)
        time.sleep(2)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "test123")
        page.click('button[type="submit"]')
        time.sleep(5)

        # === テスト1: タブ表示 ===
        page.evaluate("""document.querySelectorAll('.tab-btn').forEach(function(b){
            if(b.textContent.indexOf('企業分析')>=0)b.click()
        })""")
        time.sleep(4)

        search_input = page.query_selector('#company-search-input')
        test("T1: 企業分析タブ表示", search_input is not None, "検索ボックスが見つからない")

        page.screenshot(path=os.path.join(SS_DIR, "e2e_company_01_tab.png"))

        # === テスト2: 企業検索 ===
        page.evaluate("""htmx.ajax('GET', '/api/company/search?q=ヤマト', {
            target: '#company-search-results', swap: 'innerHTML'
        })""")
        time.sleep(4)

        results = page.query_selector_all('[hx-get*="/api/company/profile/"]')
        test("T2: 企業検索結果", len(results) > 0, f"検索結果: {len(results)}件")

        page.screenshot(path=os.path.join(SS_DIR, "e2e_company_02_search.png"))

        # === テスト3: プロフィール表示 ===
        if results:
            # 最初の結果の法人番号を取得
            corp_num = results[0].get_attribute('hx-get').split('/')[-1]
            page.evaluate(f"""htmx.ajax('GET', '/api/company/profile/{corp_num}', {{
                target: '#company-profile-area', swap: 'innerHTML'
            }})""")
            time.sleep(8)

            page.screenshot(path=os.path.join(SS_DIR, "e2e_company_03_profile_top.png"))

            # KPIカード確認
            stat_values = page.evaluate("""
                Array.from(document.querySelectorAll('#company-profile-area .stat-value'))
                    .map(function(e){ return e.textContent.trim(); })
            """)
            test("T3a: KPIカード表示", len(stat_values) >= 4, f"stat-value数: {len(stat_values)}")

            # チャート確認
            echart_count = page.evaluate("""
                document.querySelectorAll('#company-profile-area .echart').length
            """)
            test("T3b: EChartsチャート", echart_count >= 3, f"echart数: {echart_count}")

            # 企業名確認
            company_name = page.evaluate("""
                var h = document.querySelector('#company-profile-area h3');
                h ? h.textContent.trim() : ''
            """)
            test("T3c: 企業名表示", len(company_name) > 0, f"企業名: {company_name}")

            # スクロールして中間部
            page.evaluate("window.scrollTo(0, 800)")
            time.sleep(2)
            page.screenshot(path=os.path.join(SS_DIR, "e2e_company_04_profile_mid.png"))

            # === テスト4: HW求人マッチング ===
            hw_section = page.evaluate("""
                var el = document.querySelector('#company-profile-area');
                if (!el) '';
                else el.innerHTML.indexOf('ハローワーク求人') >= 0 ? 'found' : 'not_found'
            """)
            test("T4: HW求人セクション存在", hw_section == "found", f"HW section: {hw_section}")

            # 求人テーブルか「掲載なし」メッセージ
            hw_content = page.evaluate("""
                var el = document.querySelector('#company-profile-area');
                if (!el) '';
                var idx = el.innerHTML.indexOf('ハローワーク求人');
                if (idx < 0) 'no_section';
                else if (el.innerHTML.indexOf('求人掲載なし') > 0) 'no_postings';
                else if (el.innerHTML.indexOf('マッチした求人') > 0) 'has_postings';
                else 'unknown'
            """)
            test("T4b: HW求人内容", hw_content in ["no_postings", "has_postings"], f"content: {hw_content}")

            # スクロールして下部
            page.evaluate("window.scrollTo(0, 1600)")
            time.sleep(2)
            page.screenshot(path=os.path.join(SS_DIR, "e2e_company_05_profile_bottom.png"))

            # === テスト5: 近隣企業 ===
            page.evaluate("window.scrollTo(0, document.body.scrollHeight)")
            time.sleep(2)

            nearby_section = page.evaluate("""
                var el = document.querySelector('#company-profile-area');
                if (!el) '';
                else el.innerHTML.indexOf('近隣企業') >= 0 ? 'found' : 'not_found'
            """)
            test("T5: 近隣企業セクション", nearby_section == "found", f"nearby: {nearby_section}")

            nearby_rows = page.evaluate("""
                var rows = document.querySelectorAll('#company-profile-area tr[hx-get]');
                rows ? rows.length : 0
            """)
            test("T5b: 近隣企業行数", nearby_rows > 0, f"rows: {nearby_rows}")

            page.screenshot(path=os.path.join(SS_DIR, "e2e_company_06_nearby.png"))

            # === テスト6: 近隣企業クリック ===
            if nearby_rows > 0:
                nearby_corp = page.evaluate("""
                    var row = document.querySelector('#company-profile-area tr[hx-get]');
                    row ? row.getAttribute('hx-get').split('/').pop() : ''
                """)
                if nearby_corp:
                    page.evaluate(f"""htmx.ajax('GET', '/api/company/profile/{nearby_corp}', {{
                        target: '#company-profile-area', swap: 'innerHTML'
                    }})""")
                    time.sleep(6)

                    new_name = page.evaluate("""
                        var h = document.querySelector('#company-profile-area h3');
                        h ? h.textContent.trim() : ''
                    """)
                    test("T6: 近隣企業遷移", len(new_name) > 0 and new_name != company_name,
                         f"遷移先: {new_name}")

                    page.screenshot(path=os.path.join(SS_DIR, "e2e_company_07_nearby_profile.png"))

            # === テスト7: 印刷レポート ===
            report_page = ctx.new_page()
            report_page.goto(f"{BASE}/report/company/{corp_num}")
            time.sleep(6)

            report_title = report_page.evaluate("document.title")
            test("T7a: レポートページタイトル", "企業分析" in report_title, f"title: {report_title}")

            report_body = report_page.evaluate("document.body.innerText.substring(0, 200)")
            test("T7b: レポート内容", len(report_body) > 50, f"body length: {len(report_body)}")

            report_page.screenshot(path=os.path.join(SS_DIR, "e2e_company_08_report.png"))
            report_page.close()

        browser.close()

    # 結果出力
    print(f"\n{'='*50}")
    print(f"企業分析タブ E2Eテスト結果: {PASSED} passed, {FAILED} failed")
    print(f"{'='*50}")
    for r in RESULTS:
        print(r)
    print(f"\nスクリーンショット: {SS_DIR}")

    return FAILED == 0


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
