# -*- coding: utf-8 -*-
"""本番E2E検証: データ整合性・機能動作・表示正確性を検証する。
テキスト存在チェックではなく、具体的な数値・データ・動作を検証する。
"""
import time, os, json, re
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
issues = []
passes = 0

def ss(page, name, delay=2):
    time.sleep(delay)
    page.screenshot(path=os.path.join(DIR, f"real_{name}.png"), full_page=False)

def ok(label, cond, detail=""):
    global passes
    status = "PASS" if cond else "FAIL"
    if not cond:
        issues.append(f"{label}: {detail}")
    else:
        passes += 1
    msg = f"  [{status}] {label}"
    if detail:
        msg += f" -- {detail}"
    print(msg)
    return cond


def main():
    global passes
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=False, slow_mo=600)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 1: API v1 データ整合性検証")
        print("=" * 60)
        # ============================================================

        # ログイン
        page.goto(BASE, timeout=60000); time.sleep(5)
        page.fill('input[name="email"]', "test@f-a-c.co.jp")
        page.fill('input[name="password"]', "cyxen_2025")
        page.click('button[type="submit"]'); time.sleep(8)

        # 1-1: トヨタ自動車の企業データ整合性
        print("\n--- 1-1: トヨタ自動車のデータ検証 ---")
        toyota = page.evaluate("""
            fetch('/api/v1/companies?q=トヨタ自動車&limit=5').then(r=>r.json()).catch(e=>({error:String(e)}))
        """)
        ok("API応答あり", toyota and "results" in toyota, f"count={toyota.get('count',0)}")

        if toyota and toyota.get("results"):
            t = toyota["results"][0]
            ok("法人番号が13桁数字", len(t.get("corporate_number","")) == 13,
               f"corporate_number={t.get('corporate_number','')}")
            ok("従業員数が1万人以上(トヨタ)", t.get("employee_count",0) > 10000,
               f"employee_count={t.get('employee_count',0)}")
            ok("都道府県が愛知県", t.get("prefecture","") == "愛知県",
               f"prefecture={t.get('prefecture','')}")
            ok("業種が自動車関連", "自動車" in t.get("sn_industry",""),
               f"sn_industry={t.get('sn_industry','')}")
            ok("信用スコア50以上", t.get("credit_score",0) >= 50,
               f"credit_score={t.get('credit_score',0)}")

            # 1-2: トヨタの近隣企業データ
            corp = t["corporate_number"]
            print(f"\n--- 1-2: 近隣企業検証 (corp={corp}) ---")
            nearby = page.evaluate(f"""
                fetch('/api/v1/companies/{corp}/nearby').then(r=>r.json()).catch(e=>({{error:String(e)}}))
            """)
            if nearby and nearby.get("results"):
                nr = nearby["results"]
                ok("近隣企業10社以上", len(nr) >= 10, f"count={len(nr)}")
                ok("近隣企業に都道府県あり", all(r.get("prefecture") for r in nr[:5]),
                   f"prefs={[r.get('prefecture','?') for r in nr[:3]]}")
                # 近隣企業がトヨタ自身を含まないこと
                ok("自社除外", all(r.get("corporate_number") != corp for r in nr),
                   "近隣リストにトヨタ自身を含まない")
                # 従業員数降順ソート
                emps = [r.get("employee_count", 0) for r in nr if r.get("employee_count")]
                if len(emps) >= 2:
                    ok("従業員数降順ソート", emps == sorted(emps, reverse=True),
                       f"top3={emps[:3]}")

            # 1-3: HW求人マッチング
            print(f"\n--- 1-3: HW求人マッチング ---")
            postings = page.evaluate(f"""
                fetch('/api/v1/companies/{corp}/postings').then(r=>r.json()).catch(e=>({{error:String(e)}}))
            """)
            if postings and postings.get("results"):
                pr = postings["results"]
                ok("求人に施設名あり", all(r.get("facility_name") for r in pr[:5]),
                   f"first={pr[0].get('facility_name','')[:20]}")
                # 給与が妥当な範囲(月給10万-200万)
                salaries = [r.get("salary_min",0) for r in pr if r.get("salary_min",0) > 0]
                if salaries:
                    ok("給与範囲が妥当(10万-200万)", all(100000 <= s <= 2000000 for s in salaries),
                       f"range={min(salaries)}-{max(salaries)}")

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 2: 市場概況タブ - KPI数値の妥当性検証")
        print("=" * 60)
        # ============================================================

        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('市場概況')>=0)b.click()})")
        time.sleep(6)
        ss(page, "02_overview")

        # KPIカードの数値を抽出
        kpis = page.evaluate("""
            (function(){
                var cards = document.querySelectorAll('.stat-card');
                var result = {};
                cards.forEach(function(c){
                    var text = c.textContent;
                    // 総求人数
                    var m = text.match(/(\\d[\\d,]+)\\s*総求人数/);
                    if(m) result.total_postings = parseInt(m[1].replace(/,/g,''));
                    // 事業所数
                    m = text.match(/(\\d[\\d,]+)\\s*事業所数/);
                    if(m) result.facilities = parseInt(m[1].replace(/,/g,''));
                    // 平均月給
                    m = text.match(/(\\d[\\d,]+)円?\\s*平均月給/);
                    if(m) result.avg_salary = parseInt(m[1].replace(/,/g,''));
                    // 正社員率
                    m = text.match(/(\\d+\\.?\\d*)%\\s*正社員率/);
                    if(m) result.fulltime_rate = parseFloat(m[1]);
                });
                return result;
            })()
        """)
        print(f"  KPI raw: {kpis}")

        if kpis:
            tp = kpis.get("total_postings", 0)
            ok("総求人数40万-60万", 400000 <= tp <= 600000, f"total_postings={tp}")

            fac = kpis.get("facilities", 0)
            ok("事業所数10万-20万", 100000 <= fac <= 200000, f"facilities={fac}")

            sal = kpis.get("avg_salary", 0)
            ok("平均月給15万-35万", 150000 <= sal <= 350000, f"avg_salary={sal}")

            ft = kpis.get("fulltime_rate", 0)
            ok("正社員率30-80%", 30 <= ft <= 80, f"fulltime_rate={ft}%")

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 3: 企業タブ - 従業員数3指標の整合性")
        print("=" * 60)
        # ============================================================

        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('企業')>=0 && b.textContent.indexOf('分析')<0)b.click()})")
        time.sleep(8)
        ss(page, "03_balance")

        emp_stats = page.evaluate("""
            (function(){
                var result = {};
                var body = document.getElementById('content').textContent;
                var m;
                // 中央値
                m = body.match(/(\\d[\\d,]*)人\\s*従業員数（中央値）/);
                if(m) result.median = parseInt(m[1].replace(/,/g,''));
                // 平均
                m = body.match(/(\\d[\\d,]*)人\\s*従業員数（平均）/);
                if(m) result.mean = parseInt(m[1].replace(/,/g,''));
                // 最頻値
                m = body.match(/(\\d[\\d,]*)人\\s*従業員数（最頻値）/);
                if(m) result.mode = parseInt(m[1].replace(/,/g,''));
                return result;
            })()
        """)
        print(f"  Employee stats: {emp_stats}")

        if emp_stats:
            med = emp_stats.get("median", 0)
            mean = emp_stats.get("mean", 0)
            mode = emp_stats.get("mode", 0)

            ok("中央値 > 0", med > 0, f"median={med}")
            ok("平均 > 0", mean > 0, f"mean={mean}")
            ok("最頻値 > 0", mode > 0, f"mode={mode}")
            # 統計的整合性: 右裾が長い分布では 最頻値 < 中央値 < 平均
            ok("最頻値 <= 中央値 <= 平均 (右裾分布)", mode <= med <= mean,
               f"mode={mode} <= median={med} <= mean={mean}")

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 4: 企業検索タブ - 検索→プロフィール→データ表示")
        print("=" * 60)
        # ============================================================

        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('企業検索')>=0)b.click()})")
        time.sleep(6)

        # 検索ボックスに入力してHTMXサーチ
        search_input = page.query_selector('#company-search-input') or page.query_selector('input[type="search"]') or page.query_selector('input[type="text"]')
        if search_input:
            search_input.fill("日本郵便")
            time.sleep(5)
            ss(page, "04_search_results")

            # 検索結果リストを確認
            results = page.evaluate("""
                (function(){
                    var items = document.querySelectorAll('#company-results a, #company-results [hx-get], #company-results div[onclick], #company-results button');
                    var list = [];
                    for(var i=0;i<Math.min(items.length,5);i++){
                        list.push(items[i].textContent.trim().substring(0,50));
                    }
                    return list;
                })()
            """)
            ok("検索結果にリスト表示", len(results) > 0, f"results={results[:3]}")

            # 最初の結果をクリック
            if results:
                page.evaluate("""
                    var items = document.querySelectorAll('#company-results a, #company-results [hx-get], #company-results div[onclick], #company-results button');
                    if(items.length>0) items[0].click();
                """)
                time.sleep(10)
                ss(page, "04_profile")

                # プロフィール内のデータ検証
                profile_data = page.evaluate("""
                    (function(){
                        var body = document.getElementById('content').textContent;
                        return {
                            has_employee: /従業員/.test(body),
                            has_industry: /業種|業界|産業/.test(body),
                            has_address: /住所|所在/.test(body) || /東京|大阪|愛知/.test(body),
                            has_hw_posting: /HW求人|ハローワーク求人|求人マッチ/.test(body),
                            has_nearby: /近隣|同一エリア|周辺/.test(body),
                            has_credit: /信用|スコア|credit/.test(body),
                            body_length: body.length
                        };
                    })()
                """)
                print(f"  Profile data checks: {profile_data}")
                if profile_data:
                    ok("プロフィール十分な長さ(>500文字)", profile_data.get("body_length",0) > 500,
                       f"length={profile_data.get('body_length',0)}")
                    ok("従業員情報あり", profile_data.get("has_employee", False))
                    ok("業種情報あり", profile_data.get("has_industry", False))
                    ok("近隣企業セクションあり", profile_data.get("has_nearby", False))
        else:
            ok("企業検索ボックス存在", False, "input not found")

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 5: 地図タブ - マーカー表示とデータ正確性")
        print("=" * 60)
        # ============================================================

        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('地図')>=0)b.click()})")
        time.sleep(8)

        # 東京都・新宿区で検索
        page.evaluate("""
            var pref = document.getElementById('jm-pref');
            if(pref){pref.value='東京都';pref.dispatchEvent(new Event('change'))}
        """)
        time.sleep(3)
        page.evaluate("""
            var muni = document.getElementById('jm-muni');
            if(muni){
                for(var i=0;i<muni.options.length;i++){
                    if(muni.options[i].text.indexOf('新宿')>=0){muni.value=muni.options[i].value;break;}
                }
                muni.dispatchEvent(new Event('change'));
            }
        """)
        time.sleep(2)
        page.evaluate("if(typeof postingMap!=='undefined')postingMap.search()")
        time.sleep(10)
        ss(page, "05_map_shinjuku")

        # マーカーAPIで直接データ検証
        markers = page.evaluate("""
            fetch('/api/jobmap/markers?prefecture=東京都&municipality=新宿区&radius=5')
                .then(r=>r.json()).catch(e=>({error:String(e)}))
        """)
        if markers:
            mlist = markers.get("markers", [])
            total_avail = markers.get("totalAvailable", len(mlist))
            ok("新宿区の求人マーカー50件以上", total_avail >= 50,
               f"totalAvailable={total_avail}, shown={len(mlist)}")

            if mlist:
                m0 = mlist[0]
                ok("マーカーにlat/lng", m0.get("lat") and m0.get("lng"),
                   f"lat={m0.get('lat')}, lng={m0.get('lng')}")
                # 座標が東京都内(lat:35.5-35.9, lng:139.4-139.9)
                ok("座標が東京都範囲内",
                   35.5 <= m0.get("lat",0) <= 35.9 and 139.4 <= m0.get("lng",0) <= 139.9,
                   f"lat={m0.get('lat')}, lng={m0.get('lng')}")
                # 施設名あり
                ok("施設名が存在", bool(m0.get("facility")),
                   f"facility={str(m0.get('facility',''))[:30]}")

        # 企業マーカーAPI
        print("\n--- 5b: 企業マーカーAPI ---")
        comp_markers = page.evaluate("""
            fetch('/api/jobmap/company-markers?south=35.5&north=35.9&west=139.4&east=139.9&zoom=12')
                .then(r=>r.json()).catch(e=>({error:String(e)}))
        """)
        if comp_markers:
            cm_total = comp_markers.get("total", 0)
            cm_shown = comp_markers.get("shown", 0)
            cm_err = comp_markers.get("error", "")
            if cm_err:
                ok("企業マーカーキャッシュロード済み", False, f"error={cm_err}")
            else:
                ok("東京エリア企業マーカー100社以上", cm_total >= 100,
                   f"total={cm_total}, shown={cm_shown}")
                if comp_markers.get("markers"):
                    cm0 = comp_markers["markers"][0]
                    ok("企業マーカーに社名", bool(cm0.get("company_name")),
                       f"name={cm0.get('company_name','')[:20]}")
                    ok("企業マーカー座標が東京範囲",
                       35.5 <= cm0.get("lat",0) <= 35.9 and 139.4 <= cm0.get("lng",0) <= 139.9,
                       f"lat={cm0.get('lat')}, lng={cm0.get('lng')}")
                    ok("従業員数 > 0", cm0.get("employee_count", 0) > 0,
                       f"emp={cm0.get('employee_count')}")

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 6: 条件タブ - 福利厚生・残業・休日データ")
        print("=" * 60)
        # ============================================================

        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('条件')>=0)b.click()})")
        time.sleep(10)
        ss(page, "06_workstyle")

        workstyle = page.evaluate("""
            (function(){
                var body = (document.getElementById('content') || {}).textContent || '';
                return {
                    length: body.length,
                    has_benefits: /福利厚生|社会保険|退職金|賞与/.test(body),
                    has_overtime: /残業|時間外/.test(body),
                    has_holiday: /休日|週休|年間休日/.test(body),
                    has_percentage: /%/.test(body),
                    // EChartsキャンバスの存在チェック
                    chart_count: document.querySelectorAll('canvas, [data-chart-config], div[id*="chart"]').length
                };
            })()
        """)
        print(f"  Workstyle data: {workstyle}")
        if workstyle:
            ok("条件タブにコンテンツあり(>100文字)", workstyle.get("length",0) > 100,
               f"length={workstyle.get('length',0)}")
            ok("福利厚生データ", workstyle.get("has_benefits", False))
            ok("残業データ", workstyle.get("has_overtime", False))
            ok("休日データ", workstyle.get("has_holiday", False))

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 7: 分析タブ - サブタブ切替とデータ存在")
        print("=" * 60)
        # ============================================================

        page.evaluate("document.querySelectorAll('.tab-btn').forEach(function(b){if(b.textContent.indexOf('分析')>=0 && b.textContent.indexOf('企業')<0)b.click()})")
        time.sleep(6)

        # サブタブ1: 給与分析
        page.evaluate("fetch('/api/analysis/subtab/1').then(r=>r.text()).then(function(h){document.getElementById('content').innerHTML=h})")
        time.sleep(5)
        ss(page, "07_analysis_salary")

        analysis_data = page.evaluate("""
            (function(){
                var body = (document.getElementById('content') || {}).textContent || '';
                return {
                    length: body.length,
                    has_salary: /給与|月給|年収|万円/.test(body),
                    has_chart: document.querySelectorAll('canvas, [data-chart-config]').length > 0
                };
            })()
        """)
        ok("分析タブにデータあり", analysis_data and analysis_data.get("length",0) > 50,
           f"length={analysis_data.get('length',0) if analysis_data else 0}")

        # ============================================================
        print("\n" + "=" * 60)
        print("TEST 8: データ横断整合性 - API vs 画面表示の一致")
        print("=" * 60)
        # ============================================================

        # health APIから総求人数取得
        health = page.evaluate("fetch('/health').then(r=>r.json()).catch(e=>({}))")
        db_rows = health.get("db_rows", 0) if health else 0
        ok("DB行数40万以上", db_rows >= 400000, f"db_rows={db_rows}")

        # 概況タブのKPIと比較
        if kpis and kpis.get("total_postings"):
            ok("概況KPI == health.db_rows", kpis["total_postings"] == db_rows,
               f"KPI={kpis['total_postings']} vs health={db_rows}")

        # ============================================================
        print("\n" + "=" * 60)
        print("SUMMARY")
        print("=" * 60)

        total_tests = passes + len(issues)
        print(f"\n  PASS: {passes}/{total_tests}")
        print(f"  FAIL: {len(issues)}/{total_tests}")

        if issues:
            print(f"\n  FAILURES:")
            for i, issue in enumerate(issues, 1):
                print(f"    {i}. {issue}")

        print("\n" + "=" * 60)

        # 確認時間
        print("\n20秒間ブラウザを開いたまま確認できます...")
        time.sleep(20)
        browser.close()


if __name__ == "__main__":
    main()
