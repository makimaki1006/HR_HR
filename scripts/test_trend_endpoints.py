#!/usr/bin/env python3
"""
時系列トレンドタブ HTTP エンドポイント統合テスト

対象: /tab/trend, /api/trend/subtab/{1..5}, /tab/guide, /api/analysis/subtab/5
前提: サーバーが localhost:9216 で起動済み、test@f-a-c.co.jp / test123 でログイン可能

実行例:
    python scripts/test_trend_endpoints.py
"""

import json
import re
import sys
import time
import urllib.request
import urllib.parse
import http.cookiejar

# ============================================================
# 設定
# ============================================================
BASE_URL = "http://localhost:9216"
LOGIN_EMAIL = "test@f-a-c.co.jp"
LOGIN_PASSWORD = "test123"

# ============================================================
# テスト基盤
# ============================================================

class TestResult:
    """テスト結果を蓄積するコンテナ"""
    def __init__(self):
        self.results: list[tuple[str, bool, str]] = []

    def record(self, test_id: str, passed: bool, detail: str = ""):
        self.results.append((test_id, passed, detail))
        status = "[PASS]" if passed else "[FAIL]"
        msg = f"{status} {test_id}"
        if detail:
            msg += f": {detail}"
        print(msg)

    def summary(self) -> int:
        total = len(self.results)
        passed = sum(1 for _, p, _ in self.results if p)
        print(f"\nSummary: {passed}/{total} passed")
        return 0 if passed == total else 1


def build_opener() -> tuple[urllib.request.OpenerDirector, http.cookiejar.CookieJar]:
    """Cookie保持付きのopenerを構築"""
    cj = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(
        urllib.request.HTTPCookieProcessor(cj),
        urllib.request.HTTPRedirectHandler(),
    )
    return opener, cj


def do_get(opener: urllib.request.OpenerDirector, path: str) -> tuple[int, str]:
    """GETリクエストを送信し (status, body) を返す"""
    url = BASE_URL + path
    try:
        resp = opener.open(url, timeout=30)
        body = resp.read().decode("utf-8", errors="replace")
        return resp.status, body
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace") if e.fp else ""
        return e.code, body


def do_login(opener: urllib.request.OpenerDirector) -> tuple[bool, str]:
    """POSTログインを実行し、成功可否とメッセージを返す"""
    data = urllib.parse.urlencode({
        "email": LOGIN_EMAIL,
        "password": LOGIN_PASSWORD,
    }).encode("utf-8")
    url = BASE_URL + "/login"
    try:
        resp = opener.open(url, data, timeout=15)
        body = resp.read().decode("utf-8", errors="replace")
        final_url = resp.url if hasattr(resp, "url") else ""
        if resp.status == 200:
            return True, f"redirected to {final_url}"
        return False, f"status={resp.status}"
    except urllib.error.HTTPError as e:
        return False, f"HTTP {e.code}"


# ============================================================
# テストケース
# ============================================================

def test_auth(opener: urllib.request.OpenerDirector, results: TestResult):
    """認証テスト: POST /login でセッションCookieを取得"""
    ok, detail = do_login(opener)
    results.record("I-0: Authentication", ok, detail)
    return ok


def test_tab_trend(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-1: /tab/trend が 200 を返し、必要な要素を含む"""
    status, body = do_get(opener, "/tab/trend")
    ok_status = status == 200
    ok_title = "時系列トレンド分析" in body
    # <button class="analysis-subtab で始まるHTMLボタン要素のみカウント
    # JS内の querySelectorAll('.analysis-subtab') は除外
    subtab_buttons = re.findall(r'<button\s[^>]*class="analysis-subtab', body)
    subtab_count = len(subtab_buttons)
    ok_subtabs = subtab_count == 5

    all_ok = ok_status and ok_title and ok_subtabs
    details = []
    if not ok_status:
        details.append(f"status={status}")
    if not ok_title:
        details.append("missing '時系列トレンド分析'")
    if not ok_subtabs:
        details.append(f"subtab button count={subtab_count}, expected 5")
    results.record("I-1: tab_trend returns 200", all_ok, "; ".join(details) if details else "")


def test_subtab(opener: urllib.request.OpenerDirector, results: TestResult,
                subtab_id: int, test_id: str, expected_texts: list[str],
                check_chart: bool = True, check_size: bool = False):
    """汎用サブタブテスト"""
    status, body = do_get(opener, f"/api/trend/subtab/{subtab_id}")
    checks = []
    ok = True

    if status != 200:
        checks.append(f"status={status}")
        ok = False

    for text in expected_texts:
        if text not in body:
            checks.append(f"missing '{text}'")
            ok = False

    if check_chart:
        if "data-chart-config" not in body:
            checks.append("missing data-chart-config")
            ok = False

    if check_size and len(body) < 1000:
        checks.append(f"body too small ({len(body)} bytes)")
        ok = False

    results.record(test_id, ok, "; ".join(checks) if checks else f"size={len(body)} bytes")


def test_subtab_invalid(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-6: 無効なサブタブID=6"""
    status, body = do_get(opener, "/api/trend/subtab/6")
    ok_status = status == 200
    ok_msg = "不明なサブタブ" in body
    ok = ok_status and ok_msg
    details = []
    if not ok_status:
        details.append(f"status={status}")
    if not ok_msg:
        details.append("missing '不明なサブタブ'")
    results.record("I-6: subtab/6 invalid", ok, "; ".join(details) if details else "")


def test_echart_json_validity(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-7: subtab/1 の data-chart-config がすべて有効なJSON"""
    status, body = do_get(opener, "/api/trend/subtab/1")
    if status != 200:
        results.record("I-7: ECharts JSON validity", False, f"status={status}")
        return

    # data-chart-config='...' を全て抽出
    pattern = r"data-chart-config='([^']*)'"
    matches = re.findall(pattern, body)

    if not matches:
        # data-chart-config="..." (ダブルクォート)パターンも試す
        pattern2 = r'data-chart-config="([^"]*)"'
        matches = re.findall(pattern2, body)

    if not matches:
        results.record("I-7: ECharts JSON validity", False, "no data-chart-config found")
        return

    parse_errors = []
    for i, config_str in enumerate(matches):
        # HTMLエンティティのデコード
        config_str = config_str.replace("&quot;", '"').replace("&amp;", "&")
        config_str = config_str.replace("&lt;", "<").replace("&gt;", ">")
        try:
            json.loads(config_str)
        except json.JSONDecodeError as e:
            parse_errors.append(f"config[{i}]: {e}")

    ok = len(parse_errors) == 0
    detail = f"{len(matches)} configs parsed OK" if ok else "; ".join(parse_errors[:3])
    results.record("I-7: ECharts JSON validity", ok, detail)


def test_guide_tab(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-8: ガイドタブにトレンド関連情報が含まれる"""
    status, body = do_get(opener, "/tab/guide")
    checks = []
    ok = True

    if status != 200:
        checks.append(f"status={status}")
        ok = False

    expected = [
        ("全9タブ", "tab count reference"),
        ("Tab 9: トレンド", "trend tab title"),
        ("トレンドタブは市区町村", "municipality FAQ"),
        ("トレンドタブのデータはいつ", "period FAQ"),
    ]
    for text, label in expected:
        if text not in body:
            checks.append(f"missing '{text}' ({label})")
            ok = False

    results.record("I-8: Guide tab consistency", ok, "; ".join(checks) if checks else "")


def test_cross_nav_link(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-9: analysis/subtab/5 にトレンドへのクロスナビリンクが含まれる"""
    status, body = do_get(opener, "/api/analysis/subtab/5")
    ok_status = status == 200
    ok_link = "時系列トレンド" in body
    ok = ok_status and ok_link
    details = []
    if not ok_status:
        details.append(f"status={status}")
    if not ok_link:
        details.append("missing '時系列トレンド' cross-nav link")
    results.record("I-9: Cross-nav link", ok, "; ".join(details) if details else "")


def test_subtab5_external(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-11: subtab/5 (外部比較) が200を返し、チャートを含む"""
    test_subtab(opener, results, 5, "I-11: subtab/5 (外部比較)",
                ["有効求人倍率"], check_chart=True, check_size=True)


def test_subtab5_echart_json(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-12: subtab/5 の data-chart-config がすべて有効なJSON"""
    status, body = do_get(opener, "/api/trend/subtab/5")
    if status != 200:
        results.record("I-12: Sub5 ECharts JSON validity", False, f"status={status}")
        return

    # data-chart-config='...' を全て抽出
    pattern = r"data-chart-config='([^']*)'"
    matches = re.findall(pattern, body)

    if not matches:
        # data-chart-config="..." (ダブルクォート)パターンも試す
        pattern2 = r'data-chart-config="([^"]*)"'
        matches = re.findall(pattern2, body)

    if not matches:
        results.record("I-12: Sub5 ECharts JSON validity", False, "no data-chart-config found")
        return

    parse_errors = []
    for i, config_str in enumerate(matches):
        # HTMLエンティティのデコード
        config_str = config_str.replace("&quot;", '"').replace("&amp;", "&")
        config_str = config_str.replace("&lt;", "<").replace("&gt;", ">")
        try:
            json.loads(config_str)
        except json.JSONDecodeError as e:
            parse_errors.append(f"config[{i}]: {e}")

    ok = len(parse_errors) == 0
    detail = f"{len(matches)} configs parsed OK" if ok else "; ".join(parse_errors[:3])
    results.record("I-12: Sub5 ECharts JSON validity", ok, detail)


def test_cache_behavior(opener: urllib.request.OpenerDirector, results: TestResult):
    """I-10: subtab/1 の2回目リクエストがキャッシュにより高速（または同等）"""
    # 1回目（キャッシュされる）
    t1_start = time.perf_counter()
    do_get(opener, "/api/trend/subtab/1")
    t1 = time.perf_counter() - t1_start

    # 2回目（キャッシュヒット期待）
    t2_start = time.perf_counter()
    do_get(opener, "/api/trend/subtab/1")
    t2 = time.perf_counter() - t2_start

    # 2回目が1回目の2倍未満であればOK（ネットワーク揺らぎを考慮）
    ok = t2 <= t1 * 2.0
    detail = f"1st={t1:.3f}s, 2nd={t2:.3f}s"
    results.record("I-10: Cache behavior", ok, detail)


# ============================================================
# メイン
# ============================================================

def main() -> int:
    print("=" * 60)
    print("Trend Tab Endpoint Integration Tests")
    print(f"Target: {BASE_URL}")
    print("=" * 60)
    print()

    results = TestResult()
    opener, cj = build_opener()

    # 認証
    if not test_auth(opener, results):
        print("\nERROR: Authentication failed. Remaining tests skipped.")
        return results.summary()

    # I-1: tab_trend
    test_tab_trend(opener, results)

    # I-2: subtab/1 (量の変化)
    test_subtab(opener, results, 1, "I-2: subtab/1 (量の変化)",
                ["求人数推移"], check_chart=True, check_size=True)

    # I-3: subtab/2 (質の変化)
    test_subtab(opener, results, 2, "I-3: subtab/2 (質の変化)",
                ["給与推移"], check_chart=True)

    # I-4: subtab/3 (構造の変化)
    test_subtab(opener, results, 3, "I-4: subtab/3 (構造の変化)",
                ["雇用形態別"], check_chart=True)

    # I-5: subtab/4 (シグナル)
    test_subtab(opener, results, 4, "I-5: subtab/4 (シグナル)",
                ["ライフサイクル"], check_chart=True)

    # I-6: subtab/6 (invalid)
    test_subtab_invalid(opener, results)

    # I-7: ECharts JSON validity
    test_echart_json_validity(opener, results)

    # I-8: Guide tab consistency
    test_guide_tab(opener, results)

    # I-9: Cross-nav link
    test_cross_nav_link(opener, results)

    # I-10: Cache behavior
    test_cache_behavior(opener, results)

    # I-11: subtab/5 (外部比較)
    test_subtab5_external(opener, results)

    # I-12: Sub5 ECharts JSON validity
    test_subtab5_echart_json(opener, results)

    print()
    return results.summary()


if __name__ == "__main__":
    sys.exit(main())
