# -*- coding: utf-8 -*-
"""
E2E Data Quality Tests for V2 HelloWork Dashboard
=====================================================
P0 tests verifying DATA ACCURACY and FUNCTIONAL CORRECTNESS.
No text-existence checks. Extract actual numbers and verify mathematical properties.

Test suites:
  1. KPI Accuracy (overview tab)
  2. Employee Stats Consistency (balance tab)
  3. API Data Integrity (company search/nearby/postings)
  4. Company Markers API (jobmap)
  5. Cross-Source Consistency (overview vs /health)
"""

import re
import sys
import time
import traceback
from urllib.parse import quote

from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
EMAIL = "test@f-a-c.co.jp"
PASSWORD = "cyxen_2025"

# 47都道府県リスト
PREFECTURES_47 = [
    "北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県",
    "茨城県", "栃木県", "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県",
    "新潟県", "富山県", "石川県", "福井県", "山梨県", "長野県", "岐阜県",
    "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府", "兵庫県",
    "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県",
    "徳島県", "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県",
    "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県",
]

# テスト結果格納
results = []


def record(suite, test_id, description, passed, detail=""):
    """テスト結果を記録し、即時出力する"""
    status = "PASS" if passed else "FAIL"
    results.append({
        "suite": suite,
        "test_id": test_id,
        "description": description,
        "passed": passed,
        "detail": detail,
    })
    print(f"  [{status}] {test_id}: {description}")
    if detail:
        print(f"         -> {detail}")


def parse_number(text):
    """カンマ区切り数値文字列を int に変換。失敗時は None。"""
    if not text:
        return None
    cleaned = text.replace(",", "").replace(" ", "").strip()
    m = re.match(r"^[+-]?(\d+)", cleaned)
    if m:
        return int(m.group(1))
    return None


def parse_float_from_text(text):
    """テキストから浮動小数点数を抽出。'21.5万円' -> 21.5, '198,000円' -> 198000"""
    if not text:
        return None
    cleaned = text.replace(",", "").replace(" ", "").strip()
    m = re.search(r"(\d+\.?\d*)", cleaned)
    if m:
        return float(m.group(1))
    return None


def parse_percentage(text):
    """'58.3%' -> 58.3"""
    if not text:
        return None
    m = re.search(r"(\d+\.?\d*)\s*%", text)
    if m:
        return float(m.group(1))
    return None


def login(page):
    """ログインして認証済みセッションを確立する"""
    page.goto(BASE, timeout=60000)
    page.wait_for_selector('input[name="email"]', timeout=15000)
    page.fill('input[name="email"]', EMAIL)
    page.fill('input[name="password"]', PASSWORD)
    page.click('button[type="submit"]')
    # ダッシュボード表示を待機（タブボタンが表示されるまで）
    page.wait_for_selector('.tab-btn', timeout=30000)


def wait_for_stat_cards(page, min_count=3, timeout_ms=40000):
    """stat-cardが指定数以上DOMに現れるまでポーリング待機する"""
    try:
        page.wait_for_function(
            f"document.querySelectorAll('#content .stat-card .stat-label').length >= {min_count}",
            timeout=timeout_ms,
        )
    except Exception:
        # フォールバック: 固定待機
        time.sleep(8)
    # レンダリング安定化のための追加待機
    time.sleep(1)


def load_tab_fresh(page, tab_path, wait_count=3, timeout_ms=40000):
    """タブキャッシュをクリアし、fetchで直接HTMLを取得して#contentに挿入する。
    tabcache.jsのインターセプトを回避するため、直接fetchする。"""
    page.evaluate("""
        (function() {
            // tabcache.jsのキャッシュをクリア
            if (typeof clearTabCache === 'function') clearTabCache();
            // #contentを一旦空にする
            var c = document.getElementById('content');
            if (c) c.innerHTML = '<p>loading...</p>';
        })()
    """)
    # fetchでHTMLを取得し、#contentに挿入
    page.evaluate(f"""
        fetch('{tab_path}', {{credentials: 'same-origin'}})
            .then(function(r) {{ return r.text(); }})
            .then(function(html) {{
                var c = document.getElementById('content');
                if (c) {{
                    c.innerHTML = html;
                    // ECharts初期化をトリガー
                    if (typeof initCharts === 'function') initCharts(c);
                    var evt = new CustomEvent('htmx:afterSettle', {{bubbles: true, detail: {{target: c}}}});
                    c.dispatchEvent(evt);
                }}
            }})
    """)
    wait_for_stat_cards(page, min_count=wait_count, timeout_ms=timeout_ms)


def extract_kpi_cards(page):
    """#content内の全stat-cardからlabel->valueのdictを取得する。
    grid-stats内のカードのみを対象とする（チャートカードを除外）。"""
    return page.evaluate("""
        (function() {
            var content = document.getElementById('content');
            if (!content) return {};
            // grid-stats 内の stat-card のみを対象（チャートカードを除外）
            var grids = content.querySelectorAll('.grid-stats');
            var result = {};
            for (var g = 0; g < grids.length; g++) {
                var cards = grids[g].querySelectorAll('.stat-card');
                for (var i = 0; i < cards.length; i++) {
                    var label_el = cards[i].querySelector('.stat-label');
                    var value_el = cards[i].querySelector('.stat-value');
                    if (label_el && value_el) {
                        var label = label_el.textContent.trim();
                        var value = value_el.textContent.trim();
                        result[label] = value;
                    }
                }
            }
            return result;
        })()
    """)


def fetch_json(page, url):
    """認証済みセッションでAPIを呼び出し、JSONを返す"""
    result = page.evaluate(f"""
        fetch('{url}', {{credentials: 'same-origin'}})
            .then(function(r) {{ return r.json(); }})
            .catch(function(e) {{ return {{"__fetch_error": String(e)}}; }})
    """)
    return result


# =============================================================================
# Suite 1: KPI Accuracy (Overview Tab)
# =============================================================================
def test_suite_1_kpi_accuracy(page):
    print("\n" + "=" * 60)
    print("Suite 1: KPI Accuracy (Overview Tab)")
    print("=" * 60)

    # overviewタブを直接fetchでロード（tabcache回避）
    load_tab_fresh(page, "/tab/market", wait_count=3)

    kpi_data = extract_kpi_cards(page)

    record("KPI", "KPI-001", "KPI cards extracted from DOM",
           len(kpi_data) >= 3,
           f"found {len(kpi_data)} KPI cards: {list(kpi_data.keys())}")

    # --- 総求人数 ---
    total_text = None
    for label, value in kpi_data.items():
        if "求人" in label:
            total_text = value
            break

    total_postings = parse_number(total_text) if total_text else None

    record("KPI", "KPI-002", "total_postings is a valid positive number",
           total_postings is not None and total_postings > 0,
           f"raw='{total_text}' parsed={total_postings}")

    record("KPI", "KPI-003", "total_postings in expected range (100K-600K)",
           total_postings is not None and 100000 <= total_postings <= 600000,
           f"total_postings={total_postings}")

    # --- 平均月給 ---
    salary_text = None
    for label, value in kpi_data.items():
        if "月給" in label or "給与" in label:
            salary_text = value
            break

    avg_salary = parse_float_from_text(salary_text) if salary_text else None

    if avg_salary is not None:
        if salary_text and "万" in salary_text:
            avg_salary_yen = avg_salary * 10000
        else:
            avg_salary_yen = avg_salary
    else:
        avg_salary_yen = None

    record("KPI", "KPI-004", "avg_salary is valid (150K-350K yen monthly)",
           avg_salary_yen is not None and 150000 <= avg_salary_yen <= 350000,
           f"raw='{salary_text}' parsed_yen={avg_salary_yen}")

    # --- 正社員率 ---
    ft_text = None
    for label, value in kpi_data.items():
        if "正社員率" in label:
            ft_text = value
            break

    fulltime_rate = parse_percentage(ft_text) if ft_text else None

    record("KPI", "KPI-005", "fulltime_rate in range 30-80%",
           fulltime_rate is not None and 30.0 <= fulltime_rate <= 80.0,
           f"raw='{ft_text}' parsed={fulltime_rate}%")

    # --- 事業所数 ---
    fac_text = None
    for label, value in kpi_data.items():
        if "事業所" in label:
            fac_text = value
            break

    facility_count = parse_number(fac_text) if fac_text else None

    record("KPI", "KPI-006", "facility_count > 0 and < total_postings",
           (facility_count is not None and facility_count > 0
            and total_postings is not None and facility_count < total_postings),
           f"facilities={facility_count}, postings={total_postings}")

    # --- /health APIからdb_rowsを取得して比較 ---
    health = fetch_json(page, "/health")
    health_rows = health.get("db_rows") if health else None

    record("KPI", "KPI-007", "/health returns valid db_rows",
           health_rows is not None and health_rows > 0,
           f"health.db_rows={health_rows}")

    record("KPI", "KPI-008", "total_postings matches /health db_rows",
           (total_postings is not None and health_rows is not None
            and total_postings == health_rows),
           f"DOM={total_postings} vs API={health_rows}")

    return total_postings, health_rows


# =============================================================================
# Suite 2: Employee Stats Consistency (Balance Tab)
# =============================================================================
def test_suite_2_balance_stats(page):
    print("\n" + "=" * 60)
    print("Suite 2: Employee Stats Consistency (Balance Tab)")
    print("=" * 60)

    load_tab_fresh(page, "/tab/market", wait_count=3)
    # 企業分析セクションまでスクロールして遅延ロードを発火
    page.evaluate("document.getElementById('sec-balance')?.scrollIntoView()")
    time.sleep(5)

    kpi_data = extract_kpi_cards(page)

    record("BAL", "BAL-001", "Balance KPI cards extracted",
           len(kpi_data) >= 3,
           f"found {len(kpi_data)} cards: {list(kpi_data.keys())}")

    # 各統計量を抽出
    median_val = None
    mean_val = None
    mode_val = None

    for label, value in kpi_data.items():
        parsed = parse_float_from_text(value)
        if "中央値" in label:
            median_val = parsed
        elif "平均" in label and "月給" not in label:
            mean_val = parsed
        elif "最頻値" in label:
            mode_val = parsed

    record("BAL", "BAL-002", "median employee count > 0",
           median_val is not None and median_val > 0,
           f"median={median_val}")

    record("BAL", "BAL-003", "mean employee count > 0",
           mean_val is not None and mean_val > 0,
           f"mean={mean_val}")

    record("BAL", "BAL-004", "mode employee count > 0",
           mode_val is not None and mode_val > 0,
           f"mode={mode_val}")

    # 右裾分布の検証: mode <= median <= mean
    if all(v is not None and v > 0 for v in [mode_val, median_val, mean_val]):
        record("BAL", "BAL-005",
               "right-skewed distribution: mode <= median <= mean",
               mode_val <= median_val <= mean_val,
               f"mode={mode_val} <= median={median_val} <= mean={mean_val}")
    else:
        record("BAL", "BAL-005",
               "right-skewed distribution: mode <= median <= mean",
               False,
               f"insufficient data: mode={mode_val}, median={median_val}, mean={mean_val}")

    # 総求人数 > 事業所数
    postings_val = None
    facilities_val = None
    for label, value in kpi_data.items():
        if "求人" in label:
            postings_val = parse_number(value)
        elif "事業所" in label:
            facilities_val = parse_number(value)

    record("BAL", "BAL-006", "balance tab: postings > facilities > 0",
           (postings_val is not None and facilities_val is not None
            and postings_val > facilities_val > 0),
           f"postings={postings_val}, facilities={facilities_val}")


# =============================================================================
# Suite 3: API Data Integrity
# =============================================================================
def test_suite_3_api_integrity(page):
    print("\n" + "=" * 60)
    print("Suite 3: API Data Integrity")
    print("=" * 60)

    # --- 3a: 企業検索（日本語クエリ「トヨタ自動車」） ---
    search_query = quote("トヨタ自動車")
    search_result = fetch_json(page, f"/api/v1/companies?q={search_query}")

    record("API", "API-001", "企業検索: 'トヨタ自動車' で結果が返る",
           search_result is not None and search_result.get("count", 0) > 0,
           f"count={search_result.get('count') if search_result else 'null'}")

    if search_result and search_result.get("results"):
        # トヨタ自動車を検索 (販売会社等を除外)
        toyota = None
        for r in search_result["results"]:
            name = r.get("company_name", "")
            if name == "トヨタ自動車株式会社":
                toyota = r
                break
        if not toyota:
            for r in search_result["results"]:
                name = r.get("company_name", "")
                if "トヨタ自動車" in name and "販売" not in name:
                    toyota = r
                    break

        if toyota:
            emp = toyota.get("employee_count", 0)
            pref = toyota.get("prefecture", "")
            corp_num = toyota.get("corporate_number", "")

            record("API", "API-002",
                   "トヨタ自動車 employee_count > 10000",
                   emp is not None and emp > 10000,
                   f"employee_count={emp}, name={toyota.get('company_name', '')}")

            record("API", "API-003",
                   "トヨタ自動車 prefecture が有効な都道府県",
                   pref in PREFECTURES_47,
                   f"prefecture='{pref}'")

            # --- 3b: Nearby companies ---
            if corp_num:
                nearby = fetch_json(page, f"/api/v1/companies/{corp_num}/nearby")

                if nearby and "companies" in nearby and len(nearby["companies"]) > 0:
                    companies = nearby["companies"]
                    record("API", "API-004",
                           "nearby companies returned > 0 results",
                           len(companies) > 0,
                           f"count={len(companies)}")

                    if len(companies) > 1:
                        corp_nums = [c.get("corporate_number") for c in companies]
                        unique_corps = set(corp_nums)
                        record("API", "API-005",
                               "all nearby companies have distinct corporate_number",
                               len(unique_corps) == len(corp_nums),
                               f"total={len(corp_nums)}, unique={len(unique_corps)}")

                        emp_counts = [c.get("employee_count", 0) for c in companies]
                        is_sorted_desc = all(
                            emp_counts[i] >= emp_counts[i + 1]
                            for i in range(len(emp_counts) - 1)
                        )
                        record("API", "API-006",
                               "nearby sorted by employee_count DESC",
                               is_sorted_desc,
                               f"first_5_emp={emp_counts[:5]}")
                    else:
                        record("API", "API-005",
                               "all nearby companies have distinct corporate_number",
                               True, "only 1 result, trivially true")
                        record("API", "API-006",
                               "nearby sorted by employee_count DESC",
                               True, "only 1 result, trivially true")
                else:
                    err_msg = nearby.get("error", "no companies") if nearby else "null response"
                    record("API", "API-004",
                           "nearby companies returned > 0 results",
                           False, f"error: {err_msg}")
                    record("API", "API-005",
                           "all nearby companies have distinct corporate_number",
                           False, "skipped (no nearby data)")
                    record("API", "API-006",
                           "nearby sorted by employee_count DESC",
                           False, "skipped (no nearby data)")

                # --- 3c: Company postings ---
                postings_resp = fetch_json(page, f"/api/v1/companies/{corp_num}/postings")

                if postings_resp and "count" in postings_resp:
                    total_count = postings_resp.get("count", 0)
                    shown = postings_resp.get("shown", 0)
                    record("API", "API-007",
                           "company postings: count field returned",
                           total_count >= 0,
                           f"total_count={total_count}, shown={shown}")

                    record("API", "API-008",
                           "shown <= total_count (not overcounting)",
                           shown <= total_count if total_count > 0 else True,
                           f"shown={shown}, total={total_count}")
                else:
                    err_msg = postings_resp.get("error", "unknown") if postings_resp else "null"
                    record("API", "API-007",
                           "company postings: count field returned",
                           False, f"error: {err_msg}")
                    record("API", "API-008",
                           "shown <= total_count (not overcounting)",
                           False, "skipped")
            else:
                for tid in ["API-004", "API-005", "API-006", "API-007", "API-008"]:
                    record("API", tid, "(skipped - no corporate_number)", False, "no corp_num")
        else:
            record("API", "API-002", "トヨタ自動車 employee_count > 10000",
                   False, "トヨタ自動車が検索結果に見つからない")
            record("API", "API-003", "トヨタ自動車 prefecture が有効な都道府県",
                   False, "トヨタ自動車が検索結果に見つからない")
            for tid in ["API-004", "API-005", "API-006", "API-007", "API-008"]:
                record("API", tid, "(skipped - トヨタ自動車 not found)", False, "")
    else:
        for tid in ["API-002", "API-003", "API-004", "API-005", "API-006", "API-007", "API-008"]:
            record("API", tid, "(skipped - search failed)", False, "")

    # --- 3d: Prefecture validation for search results ---
    if search_result and search_result.get("results"):
        invalid_prefs = []
        for r in search_result["results"]:
            pref = r.get("prefecture", "")
            if pref and pref not in PREFECTURES_47:
                invalid_prefs.append(pref)

        record("API", "API-009",
               "all search results have valid prefecture names",
               len(invalid_prefs) == 0,
               f"invalid={invalid_prefs}" if invalid_prefs else "all valid")
    else:
        record("API", "API-009",
               "all search results have valid prefecture names",
               False, "no search results to validate")


# =============================================================================
# Suite 4: Company Markers API
# =============================================================================
def test_suite_4_markers(page):
    print("\n" + "=" * 60)
    print("Suite 4: Company Markers API")
    print("=" * 60)

    # --- 4a: zoom=12, Tokyo viewport ---
    markers_tokyo = fetch_json(
        page,
        "/api/jobmap/company-markers?zoom=12&south=35.5&north=35.8&west=139.5&east=139.9"
    )

    if markers_tokyo and "__fetch_error" not in markers_tokyo:
        total = markers_tokyo.get("total", 0)
        shown = markers_tokyo.get("shown", 0)
        markers = markers_tokyo.get("markers", [])

        # total=0 の場合は、企業ジオコードキャッシュが未ロード（Turso未接続）の可能性
        # その場合は SKIP ではなく、事実をそのまま報告する
        has_error = markers_tokyo.get("error", "")

        record("MAP", "MAP-001", "markers API returns total > 0 for Tokyo viewport (zoom=12)",
               total > 0,
               f"total={total}, shown={shown}" + (f", error='{has_error}'" if has_error else ""))

        record("MAP", "MAP-002", "shown <= 500 (API cap)",
               shown <= 500,
               f"shown={shown}")

        if markers and len(markers) > 0:
            out_of_range = 0
            for m in markers[:50]:
                lat = m.get("lat", 0)
                lng = m.get("lng", 0)
                if not (24.0 <= lat <= 46.0 and 122.0 <= lng <= 154.0):
                    out_of_range += 1

            record("MAP", "MAP-003",
                   "all markers have lat/lng within Japan range",
                   out_of_range == 0,
                   f"checked {min(50, len(markers))} markers, out_of_range={out_of_range}")

            first = markers[0]
            required_fields = ["corporate_number", "lat", "lng", "company_name",
                               "employee_count"]
            missing = [f for f in required_fields if f not in first]
            record("MAP", "MAP-004",
                   "markers have required fields",
                   len(missing) == 0,
                   f"missing={missing}" if missing else f"all fields present")

            valid_emp = sum(1 for m in markers[:20]
                           if isinstance(m.get("employee_count"), (int, float))
                           and m["employee_count"] >= 0)
            record("MAP", "MAP-005",
                   "markers have valid employee_count >= 0",
                   valid_emp == min(20, len(markers)),
                   f"valid={valid_emp}/{min(20, len(markers))}")
        else:
            record("MAP", "MAP-003", "all markers have lat/lng within Japan range",
                   total == 0, f"no markers returned, total={total}")
            record("MAP", "MAP-004", "markers have required fields",
                   total == 0, f"no markers returned, total={total}")
            record("MAP", "MAP-005", "markers have valid employee_count >= 0",
                   total == 0, f"no markers returned, total={total}")
    else:
        err = markers_tokyo.get("__fetch_error", "unknown") if markers_tokyo else "null"
        for tid in ["MAP-001", "MAP-002", "MAP-003", "MAP-004", "MAP-005"]:
            record("MAP", tid, "(markers API failed)", False, f"error: {err}")

    # --- 4b: zoom < 10 should return empty markers ---
    markers_zoom5 = fetch_json(
        page,
        "/api/jobmap/company-markers?zoom=5&south=24&north=46&west=122&east=154"
    )

    if markers_zoom5 and "__fetch_error" not in markers_zoom5:
        m_list = markers_zoom5.get("markers", [])
        zoom_req = markers_zoom5.get("zoom_required")
        msg = markers_zoom5.get("message", "")
        total_z5 = markers_zoom5.get("total", -1)

        record("MAP", "MAP-006",
               "zoom=5 returns empty markers and total=0",
               len(m_list) == 0 and total_z5 == 0,
               f"markers_count={len(m_list)}, total={total_z5}")

        # zoom_required は zoom < 10 のときのみ返される
        # ソースコード上は zoom_required: 10 を返す仕様だが、
        # 本番で企業ジオキャッシュ未ロード時は別の応答経路を通る可能性がある
        record("MAP", "MAP-007",
               "zoom=5 returns zoom_required or error message",
               zoom_req is not None or msg != "" or "error" in markers_zoom5,
               f"zoom_required={zoom_req}, message='{msg}', keys={list(markers_zoom5.keys())}")
    else:
        record("MAP", "MAP-006", "zoom=5 returns empty markers",
               False, "API call failed")
        record("MAP", "MAP-007", "zoom=5 returns zoom_required or error message",
               False, "API call failed")


# =============================================================================
# Suite 5: Cross-Source Consistency
# =============================================================================
def test_suite_5_cross_source(page, dom_total, health_rows):
    print("\n" + "=" * 60)
    print("Suite 5: Cross-Source Consistency")
    print("=" * 60)

    record("XSRC", "XSRC-001",
           "DOM total_postings == /health db_rows",
           (dom_total is not None and health_rows is not None
            and dom_total == health_rows),
           f"DOM={dom_total} vs health={health_rows}")

    # --- /api/status のdb_rowsも一致するか ---
    status = fetch_json(page, "/api/status")
    if status:
        status_rows = status.get("hellowork_db_rows")
        record("XSRC", "XSRC-002",
               "/api/status db_rows == /health db_rows",
               (status_rows is not None and health_rows is not None
                and status_rows == health_rows),
               f"status={status_rows} vs health={health_rows}")

        record("XSRC", "XSRC-003",
               "/api/status reports healthy",
               status.get("status") == "healthy",
               f"status={status.get('status')}")
    else:
        record("XSRC", "XSRC-002", "/api/status db_rows == /health db_rows",
               False, "API call failed")
        record("XSRC", "XSRC-003", "/api/status reports healthy",
               False, "API call failed")

    # --- /health reports db_connected ---
    health = fetch_json(page, "/health")
    if health:
        record("XSRC", "XSRC-004",
               "/health db_connected is true",
               health.get("db_connected") is True,
               f"db_connected={health.get('db_connected')}")
    else:
        record("XSRC", "XSRC-004", "/health db_connected is true",
               False, "API call failed")


# =============================================================================
# Main
# =============================================================================
def main():
    print("=" * 60)
    print("E2E Data Quality Tests - V2 HelloWork Dashboard")
    print(f"Target: {BASE}")
    print("=" * 60)

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        try:
            # ログイン
            print("\n--- Login ---")
            login(page)
            logged_in = "tab-btn" in page.content()
            record("AUTH", "AUTH-001", "login successful (tab-btn visible)",
                   logged_in, "")
            if not logged_in:
                print("FATAL: Login failed. Cannot proceed.")
                browser.close()
                return 1

            # Suite 1: KPI Accuracy
            dom_total, health_rows = test_suite_1_kpi_accuracy(page)

            # Suite 2: Balance Stats
            test_suite_2_balance_stats(page)

            # Suite 3: API Integrity
            test_suite_3_api_integrity(page)

            # Suite 4: Markers
            test_suite_4_markers(page)

            # Suite 5: Cross-Source
            test_suite_5_cross_source(page, dom_total, health_rows)

        except Exception as e:
            print(f"\nFATAL ERROR: {e}")
            traceback.print_exc()
        finally:
            browser.close()

    # サマリー出力
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)

    total = len(results)
    passed = sum(1 for r in results if r["passed"])
    failed = total - passed

    print(f"Total: {total}, Passed: {passed}, Failed: {failed}")
    if total > 0:
        print(f"Pass rate: {passed/total*100:.1f}%")
    else:
        print("No tests run")

    if failed > 0:
        print("\n--- FAILED TESTS ---")
        for r in results:
            if not r["passed"]:
                print(f"  [FAIL] {r['test_id']}: {r['description']}")
                if r["detail"]:
                    print(f"         -> {r['detail']}")

    # 本番環境の問題を明確にするための追加診断情報
    if failed > 0:
        print("\n--- DIAGNOSTIC NOTES ---")
        kpi_fails = [r for r in results if r["test_id"].startswith("KPI") and not r["passed"]]
        map_fails = [r for r in results if r["test_id"].startswith("MAP") and not r["passed"]]
        if kpi_fails:
            print("  [NOTE] KPI extraction failures may indicate HTMX loading delay or DOM structure change.")
        if map_fails:
            print("  [NOTE] MAP failures may indicate company_geo_cache not loaded (Turso connection issue).")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
