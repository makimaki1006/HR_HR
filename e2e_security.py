# -*- coding: utf-8 -*-
"""
E2E セキュリティ検証スクリプト
対象: https://hr-hw.onrender.com (Rust Axum + HTMX + ECharts)

検証カテゴリ:
  1. XSS注入 (CSVアップロード → survey-result / ECharts tooltip / report HTML)
  2. 巨大ファイルアップロード (1MB / 10MB / 50MB / 100MB)
  3. 文字コード異常 (Shift-JIS / UTF-16 / UTF-8 BOM / Latin-1 / 不正バイト列)
  4. SQLインジェクション (/api/set_prefecture, /api/company/search, /api/insight/widget/*)
  5. CSRF (別Origin偽装ヘッダー)
  6. ファイル形式偽装 (.csv中身が EXE / PDF / ZIP)

注意事項:
  - 攻撃ペイロードは自社サーバー検証用のみ
  - 読み取り系が中心。書き込みは「拒否されること」を期待
  - セッション停止時は health check で復旧待機
  - 結果は現状記録のみ、修正は別タスク
"""
import os
import sys
import time
import csv
import io
import json
import traceback
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
EMAIL = "test@f-a-c.co.jp"
PASSWORD = "cyxen_2025"
DIR = os.path.dirname(os.path.abspath(__file__))
TMP = os.path.join(DIR, "_sec_tmp")
os.makedirs(TMP, exist_ok=True)

# 結果集計
RESULTS = []  # (category, label, status, detail)  status in {SECURE, VULNERABLE, INCONCLUSIVE}


def record(category, label, status, detail=""):
    RESULTS.append((category, label, status, detail))
    icon = {"SECURE": "OK", "VULNERABLE": "NG", "INCONCLUSIVE": "??"}.get(status, "??")
    print(f"  [{icon}] [{status}] {label}" + (f" - {detail}" if detail else ""))


def section(title):
    print(f"\n=== {title} ===")


# -------------------------------------------------------------
# CSV 生成ヘルパー
# -------------------------------------------------------------
SURVEY_HEADER = ["求人タイトル", "企業名", "勤務地", "給与", "雇用形態", "タグ", "URL", "新着"]


def write_survey_csv(path, rows, encoding="utf-8", bom=False, raw_bytes=None):
    if raw_bytes is not None:
        with open(path, "wb") as f:
            f.write(raw_bytes)
        return
    buf = io.StringIO()
    w = csv.writer(buf)
    w.writerow(SURVEY_HEADER)
    for r in rows:
        w.writerow(r)
    data = buf.getvalue()
    if encoding.lower() == "utf-8":
        b = data.encode("utf-8")
        if bom:
            b = b"\xef\xbb\xbf" + b
    elif encoding.lower() in ("shift-jis", "shift_jis", "cp932"):
        b = data.encode("cp932", errors="replace")
    elif encoding.lower() in ("utf-16", "utf-16-le"):
        b = data.encode("utf-16")  # with BOM
    elif encoding.lower() in ("latin-1", "latin1"):
        b = data.encode("latin-1", errors="replace")
    else:
        b = data.encode(encoding, errors="replace")
    with open(path, "wb") as f:
        f.write(b)


def make_xss_csv(path):
    payloads = [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "javascript:alert(1)",
        "\"><svg/onload=alert(1)>",
        "{{7*7}}",
        "${7*7}",
        "<iframe src=javascript:alert(1)>",
    ]
    rows = []
    for i, p in enumerate(payloads):
        rows.append([
            f"営業職XSS-{i}",
            p,  # 企業名にXSS
            "東京都千代田区",
            "月給25万円~30万円",
            "正社員",
            p,  # タグにもXSS
            "https://example.com/",
            "新着",
        ])
    # 普通のデータも数件
    for i in range(3):
        rows.append([
            f"営業職通常{i}", "株式会社ABC", "東京都新宿区",
            "月給25万円~30万円", "正社員", "未経験可", "https://example.com/", ""
        ])
    write_survey_csv(path, rows)
    return payloads


def make_size_csv(path, rows_count):
    rows = []
    for i in range(rows_count):
        rows.append([
            f"営業職No{i}", f"株式会社{i % 50}", "東京都千代田区",
            "月給25万円~30万円", "正社員", "未経験可,週休2日",
            f"https://example.com/j/{i}", ""
        ])
    write_survey_csv(path, rows)


def file_size_mb(path):
    return os.path.getsize(path) / (1024 * 1024)


# -------------------------------------------------------------
# ログイン & health check
# -------------------------------------------------------------
def login(page):
    page.goto(BASE, timeout=60000)
    time.sleep(2)
    try:
        page.fill('input[name="email"]', EMAIL, timeout=10000)
        page.fill('input[name="password"]', PASSWORD, timeout=10000)
        page.click('button[type="submit"]', timeout=10000)
        time.sleep(5)
    except Exception as e:
        print(f"  [WARN] ログインフォーム操作失敗: {e}")
        return False
    body = page.text_content("body") or ""
    return "ログアウト" in body or "都道府県" in body


def health_check(page):
    """簡易ヘルスチェック: / にアクセスして 200 系応答なら OK"""
    try:
        resp = page.request.get(BASE, timeout=30000)
        return resp.status < 500
    except Exception:
        return False


# -------------------------------------------------------------
# Survey タブ遷移（XSS/アップロード検証用）
# -------------------------------------------------------------
def goto_survey_tab(page):
    """媒体分析タブを開き、file input が出現するまで待機"""
    try:
        page.wait_for_function("typeof htmx !== 'undefined'", timeout=15000)
    except Exception:
        pass
    time.sleep(1)
    btns = page.query_selector_all('.tab-btn')
    for b in btns:
        t = b.text_content() or ""
        if "媒体" in t:
            try:
                b.click()
            except Exception:
                pass
            break
    # file input 出現待機
    for _ in range(20):
        if page.evaluate("!!document.querySelector('input[type=\"file\"]')"):
            return True
        time.sleep(1)
    # fallback: 直接 fetch
    page.evaluate(
        "fetch('/tab/survey', {credentials: 'include'})"
        ".then(r => r.text()).then(t => { document.getElementById('content').innerHTML = t; })"
    )
    time.sleep(3)
    return page.evaluate("!!document.querySelector('input[type=\"file\"]')")


# =============================================================
# 1. XSS 検証
# =============================================================
def test_xss(page):
    section("1. XSS注入検証")
    path = os.path.join(TMP, "xss.csv")
    payloads = make_xss_csv(path)

    # dialog フック（alert 発火なら VULNERABLE）
    dialog_fired = {"count": 0, "messages": []}

    def on_dialog(d):
        dialog_fired["count"] += 1
        dialog_fired["messages"].append(d.message)
        try:
            d.dismiss()
        except Exception:
            pass

    page.on("dialog", on_dialog)

    try:
        if not goto_survey_tab(page):
            for p in payloads:
                record("XSS", f"{p!r}", "INCONCLUSIVE", "survey タブ未遷移")
            return
        fi = page.query_selector('input[type="file"]')
        if fi is None:
            for p in payloads:
                record("XSS", f"{p!r}", "INCONCLUSIVE", "file input 未検出")
            return
        fi.set_input_files(path)
        time.sleep(1)
        # submit
        submitted = False
        if page.evaluate("typeof window.submitSurveyCSV === 'function'"):
            try:
                page.evaluate("window.submitSurveyCSV()")
                submitted = True
            except Exception:
                pass
        if not submitted:
            try:
                page.evaluate("""
                    (function(){
                        var f = document.querySelector('input[type=\"file\"]').closest('form');
                        if (f) { f.requestSubmit ? f.requestSubmit() : f.submit(); }
                    })()
                """)
                submitted = True
            except Exception:
                pass
        # レスポンス描画待機（最大25秒、ナビゲーション例外を捕捉）
        rendered_html = ""
        for _ in range(25):
            time.sleep(1)
            try:
                html = page.evaluate(
                    "(function(){var r=document.getElementById('survey-result');"
                    "return r ? r.innerHTML : '';})()"
                ) or ""
            except Exception:
                # Execution context destroyed (navigation in progress) - retry
                continue
            if html and len(html) > 100:
                rendered_html = html
                break
        try:
            body_text = page.text_content("body") or ""
        except Exception:
            body_text = ""

        for p in payloads:
            # 生のペイロードがそのままHTMLに入っていたら VULNERABLE
            # （エスケープされていれば &lt;script&gt; になっているはず）
            # 注: "{{7*7}}" / "${7*7}" は HTML 的に無害だがテンプレート評価されたら "49" が現れる
            if p == "{{7*7}}" or p == "${7*7}":
                if "49" in rendered_html and p not in rendered_html and "{{7*7}}" not in body_text and "${7*7}" not in body_text:
                    record("XSS", p, "VULNERABLE", "テンプレ評価で49出現")
                else:
                    # ペイロードがそのまま表示 or エスケープされていればOK
                    record("XSS", p, "SECURE", "テンプレ未評価")
                continue
            # script/img/svg/iframe 系
            lowered = rendered_html.lower()
            raw_present = p.lower() in lowered
            escaped_present = ("&lt;" in lowered) and (
                p.replace("<", "&lt;").replace(">", "&gt;")[:20].lower() in lowered
            )
            if raw_present and not escaped_present:
                record("XSS", p, "VULNERABLE", "生タグがHTMLに存在")
            else:
                record("XSS", p, "SECURE", "エスケープ or 未描画")

        # alert 発火判定
        if dialog_fired["count"] > 0:
            record("XSS", "alert dialog", "VULNERABLE",
                   f"{dialog_fired['count']}件発火: {dialog_fired['messages']}")
        else:
            record("XSS", "alert dialog", "SECURE", "dialog 0件")
    finally:
        page.remove_listener("dialog", on_dialog)


# =============================================================
# 2. 巨大ファイル
# =============================================================
def test_large_files(page, ctx):
    section("2. 巨大ファイルアップロード")
    sizes = [
        ("1MB", 2000),      # ~1MB
        ("10MB", 20000),    # ~10MB
        ("50MB", 100000),   # ~50MB (拒否期待)
    ]
    cookies = ctx.cookies()
    cookie_header = "; ".join(f"{c['name']}={c['value']}" for c in cookies)

    for label, rows_count in sizes:
        path = os.path.join(TMP, f"size_{label}.csv")
        try:
            make_size_csv(path, rows_count)
        except Exception as e:
            record("LARGE", label, "INCONCLUSIVE", f"CSV生成失敗: {e}")
            continue
        actual_mb = file_size_mb(path)
        try:
            # APIRequestContext で直接 POST
            with open(path, "rb") as f:
                data = f.read()
            resp = page.request.post(
                f"{BASE}/api/survey/upload",
                multipart={"file": {"name": os.path.basename(path),
                                    "mimeType": "text/csv",
                                    "buffer": data}},
                headers={"Cookie": cookie_header},
                timeout=120000,
            )
            status = resp.status
            body_snip = ""
            try:
                body_snip = resp.text()[:200]
            except Exception:
                pass
            # health check
            alive = health_check(page)
            if label == "50MB":
                if status in (400, 413, 422, 500, 503):
                    record("LARGE", f"{label} ({actual_mb:.1f}MB)", "SECURE",
                           f"HTTP {status} 拒否 alive={alive}")
                elif status == 200:
                    record("LARGE", f"{label} ({actual_mb:.1f}MB)", "VULNERABLE",
                           f"HTTP 200 受理 alive={alive}")
                else:
                    record("LARGE", f"{label} ({actual_mb:.1f}MB)", "INCONCLUSIVE",
                           f"HTTP {status} alive={alive}")
            else:
                record("LARGE", f"{label} ({actual_mb:.1f}MB)",
                       "SECURE" if alive else "VULNERABLE",
                       f"HTTP {status} alive={alive}")
        except Exception as e:
            alive = health_check(page)
            record("LARGE", f"{label} ({actual_mb:.1f}MB)",
                   "INCONCLUSIVE" if alive else "VULNERABLE",
                   f"例外: {type(e).__name__} alive={alive}")

    # 100MB はディスク負荷大なので、dummy バイナリ（ランダムでない）で生成
    path100 = os.path.join(TMP, "size_100MB.bin")
    try:
        with open(path100, "wb") as f:
            chunk = b"A" * (1024 * 1024)
            for _ in range(100):
                f.write(chunk)
        actual_mb = file_size_mb(path100)
        with open(path100, "rb") as f:
            data = f.read()
        resp = page.request.post(
            f"{BASE}/api/survey/upload",
            multipart={"file": {"name": "big.csv", "mimeType": "text/csv", "buffer": data}},
            headers={"Cookie": cookie_header},
            timeout=180000,
        )
        alive = health_check(page)
        if resp.status in (400, 413, 422, 500, 503):
            record("LARGE", f"100MB ({actual_mb:.0f}MB)", "SECURE",
                   f"HTTP {resp.status} alive={alive}")
        elif resp.status == 200:
            record("LARGE", f"100MB ({actual_mb:.0f}MB)", "VULNERABLE",
                   f"HTTP 200 受理 alive={alive}")
        else:
            record("LARGE", f"100MB ({actual_mb:.0f}MB)", "INCONCLUSIVE",
                   f"HTTP {resp.status} alive={alive}")
    except Exception as e:
        alive = health_check(page)
        record("LARGE", "100MB",
               "INCONCLUSIVE" if alive else "VULNERABLE",
               f"例外: {type(e).__name__} alive={alive}")


# =============================================================
# 3. 文字コード異常
# =============================================================
def test_encodings(page, ctx):
    section("3. 文字コード異常")
    cookies = ctx.cookies()
    cookie_header = "; ".join(f"{c['name']}={c['value']}" for c in cookies)

    cases = []
    # Shift-JIS
    p = os.path.join(TMP, "enc_sjis.csv")
    write_survey_csv(p, [["営業職SJ", "株式会社日本語", "東京都", "月給25万円", "正社員", "タグ", "https://e.com/", ""]],
                     encoding="cp932")
    cases.append(("Shift-JIS", p, "decode_error_or_mojibake"))

    # UTF-16 with BOM
    p = os.path.join(TMP, "enc_utf16.csv")
    write_survey_csv(p, [["営業UTF16", "株式会社", "東京都", "月給25万円", "正社員", "タグ", "https://e.com/", ""]],
                     encoding="utf-16")
    cases.append(("UTF-16 BOM", p, "decode_error"))

    # UTF-8 with BOM
    p = os.path.join(TMP, "enc_utf8bom.csv")
    write_survey_csv(p, [["営業UTF8B", "株式会社", "東京都", "月給25万円", "正社員", "タグ", "https://e.com/", ""]],
                     encoding="utf-8", bom=True)
    cases.append(("UTF-8 BOM", p, "accept"))

    # Latin-1 (日本語なし、西欧文字混在)
    p = os.path.join(TMP, "enc_latin1.csv")
    write_survey_csv(p, [["Sales", "Cafe", "Paris", "1000", "FT", "tag", "https://e.com/", ""]],
                     encoding="latin-1")
    cases.append(("Latin-1", p, "accept_or_error"))

    # 不正バイト列
    p = os.path.join(TMP, "enc_garbage.csv")
    write_survey_csv(p, [], raw_bytes=b"\xff\xfe\xff\xfe" * 100)
    cases.append(("不正バイト列", p, "error"))

    for label, path, expect in cases:
        try:
            with open(path, "rb") as f:
                data = f.read()
            resp = page.request.post(
                f"{BASE}/api/survey/upload",
                multipart={"file": {"name": os.path.basename(path),
                                    "mimeType": "text/csv",
                                    "buffer": data}},
                headers={"Cookie": cookie_header},
                timeout=60000,
            )
            status = resp.status
            body = ""
            try:
                body = resp.text()[:300]
            except Exception:
                pass
            alive = health_check(page)
            # DBエラー/スタックトレース露出チェック
            leak = any(k in body.lower() for k in ["panic", "sqlx::", "postgres", "sqlite error", "stacktrace", "thread 'main'"])
            if not alive:
                record("ENC", label, "VULNERABLE", f"サーバー停止 HTTP {status}")
            elif leak:
                record("ENC", label, "VULNERABLE", f"内部エラー露出 HTTP {status}")
            elif expect == "accept" and status == 200:
                record("ENC", label, "SECURE", f"HTTP {status} 受理")
            elif status in (200, 400, 415, 422, 500):
                record("ENC", label, "SECURE", f"HTTP {status} サーバー健全")
            else:
                record("ENC", label, "INCONCLUSIVE", f"HTTP {status}")
        except Exception as e:
            alive = health_check(page)
            record("ENC", label,
                   "INCONCLUSIVE" if alive else "VULNERABLE",
                   f"例外: {type(e).__name__}")


# =============================================================
# 4. SQLインジェクション
# =============================================================
def test_sqli(page, ctx):
    section("4. SQLインジェクション")
    cookies = ctx.cookies()
    cookie_header = "; ".join(f"{c['name']}={c['value']}" for c in cookies)

    cases = [
        # (method, url, kind, payload)
        ("POST", "/api/set_prefecture", "form", {"prefecture": "' OR '1'='1"}),
        ("POST", "/api/set_prefecture", "form", {"prefecture": "東京都'; DROP TABLE users--"}),
        ("GET", "/api/company/search?q=' OR 1=1--", "query", None),
        ("GET", "/api/company/search?q=%27%20UNION%20SELECT%20NULL--", "query", None),
        ("GET", "/api/insight/widget/overview'%20OR%201=1", "query", None),
        ("GET", "/api/insight/widget/' OR 1=1 --", "query", None),
    ]

    for method, path, kind, payload in cases:
        label = f"{method} {path[:60]}"
        try:
            if method == "POST":
                resp = page.request.post(
                    BASE + path,
                    form=payload,
                    headers={"Cookie": cookie_header},
                    timeout=30000,
                )
            else:
                resp = page.request.get(
                    BASE + path,
                    headers={"Cookie": cookie_header},
                    timeout=30000,
                )
            status = resp.status
            body = ""
            try:
                body = resp.text()
            except Exception:
                pass
            body_low = body.lower()
            # DBエラーキーワード露出チェック
            leak_keys = ["sqlite", "sqlx", "syntax error", "panic", "no such table",
                         "postgres", "unrecognized token", "near \""]
            leak = any(k in body_low for k in leak_keys)
            alive = health_check(page)
            if leak:
                record("SQLi", label, "VULNERABLE", f"HTTP {status} DBエラー露出: {[k for k in leak_keys if k in body_low]}")
            elif status in (400, 404, 422, 401, 403):
                record("SQLi", label, "SECURE", f"HTTP {status} 拒否")
            elif status == 200:
                # 200 でも中身が空/正常なら SECURE と判定
                if len(body) < 50000 and not leak:
                    record("SQLi", label, "SECURE", f"HTTP 200 正常応答 len={len(body)}")
                else:
                    record("SQLi", label, "INCONCLUSIVE", f"HTTP 200 len={len(body)}")
            else:
                record("SQLi", label, "INCONCLUSIVE",
                       f"HTTP {status} alive={alive}")
        except Exception as e:
            record("SQLi", label, "INCONCLUSIVE", f"例外: {type(e).__name__}")

    # Session cookie 改ざん
    try:
        tampered = "; ".join(f"{c['name']}=TAMPERED_' OR 1=1--" for c in cookies if "session" in c['name'].lower())
        if not tampered:
            tampered = "session=TAMPERED_' OR 1=1--"
        resp = page.request.get(
            f"{BASE}/api/insight/widget/overview",
            headers={"Cookie": tampered},
            timeout=30000,
        )
        status = resp.status
        if status in (401, 403, 400, 302):
            record("SQLi", "session cookie改ざん", "SECURE", f"HTTP {status}")
        elif status == 200:
            record("SQLi", "session cookie改ざん", "VULNERABLE",
                   "改ざんcookieで200応答 (認証迂回の可能性)")
        else:
            record("SQLi", "session cookie改ざん", "INCONCLUSIVE", f"HTTP {status}")
    except Exception as e:
        record("SQLi", "session cookie改ざん", "INCONCLUSIVE", f"例外: {type(e).__name__}")


# =============================================================
# 5. CSRF
# =============================================================
def test_csrf(page, ctx):
    section("5. CSRF検証")
    cookies = ctx.cookies()
    cookie_header = "; ".join(f"{c['name']}={c['value']}" for c in cookies)
    evil_origin = "https://evil.example.com"

    # 5-1. POST /api/set_prefecture (curlで正確にOriginを偽装)
    import subprocess
    try:
        result = subprocess.run(
            ["curl", "-s", "-X", "POST",
             "-H", f"Origin: {evil_origin}",
             "-H", f"Referer: {evil_origin}/attack.html",
             "-H", f"Cookie: {cookie_header}",
             f"{BASE}/api/set_prefecture",
             "-d", "prefecture=東京都",
             "-o", "/dev/null",
             "-w", "%{http_code}"],
            capture_output=True, text=True, timeout=30
        )
        status_str = result.stdout.strip()
        status = int(status_str) if status_str.isdigit() else 0
        if status in (403, 401, 400):
            record("CSRF", "POST /api/set_prefecture (evil Origin)", "SECURE",
                   f"HTTP {status} 拒否")
        elif status in (200, 204, 302):
            record("CSRF", "POST /api/set_prefecture (evil Origin)", "VULNERABLE",
                   f"HTTP {status} 受理 (CSRF未対策)")
        else:
            record("CSRF", "POST /api/set_prefecture (evil Origin)", "INCONCLUSIVE",
                   f"HTTP {status}")
    except Exception as e:
        record("CSRF", "POST /api/set_prefecture", "INCONCLUSIVE", f"例外: {type(e).__name__}")

    # 5-2. POST /api/survey/upload (curlで正確にOriginを偽装)
    try:
        csv_path = os.path.join(TMP, "csrf.csv")
        make_size_csv(csv_path, 10)
        import subprocess
        result = subprocess.run(
            ["curl", "-s", "-X", "POST",
             "-H", f"Origin: {evil_origin}",
             "-H", f"Referer: {evil_origin}/x.html",
             "-H", f"Cookie: {cookie_header}",
             f"{BASE}/api/survey/upload",
             "-F", f"csv_file=@{csv_path}",
             "-o", "/dev/null",
             "-w", "%{http_code}"],
            capture_output=True, text=True, timeout=60
        )
        status_str = result.stdout.strip()
        status = int(status_str) if status_str.isdigit() else 0
        if status in (403, 401, 400):
            record("CSRF", "POST /api/survey/upload (evil Origin)", "SECURE",
                   f"HTTP {status} 拒否")
        elif status == 200:
            record("CSRF", "POST /api/survey/upload (evil Origin)", "VULNERABLE",
                   f"HTTP {status} 受理")
        else:
            record("CSRF", "POST /api/survey/upload (evil Origin)", "INCONCLUSIVE",
                   f"HTTP {status}")
    except Exception as e:
        record("CSRF", "POST /api/survey/upload", "INCONCLUSIVE", f"例外: {type(e).__name__}")


# =============================================================
# 6. ファイル形式偽装
# =============================================================
def test_file_spoof(page, ctx):
    section("6. ファイル形式偽装")
    cookies = ctx.cookies()
    cookie_header = "; ".join(f"{c['name']}={c['value']}" for c in cookies)

    spoofs = [
        ("EXE as .csv", b"MZ\x90\x00\x03\x00\x00\x00" + b"\x00" * 1024),
        ("PDF as .csv", b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n" + b"dummy pdf content\n" * 100),
        ("ZIP as .csv", b"PK\x03\x04\x14\x00\x00\x00" + b"\x00" * 1024),
    ]
    for label, raw in spoofs:
        path = os.path.join(TMP, f"spoof_{label.replace(' ', '_')}.csv")
        with open(path, "wb") as f:
            f.write(raw)
        try:
            resp = page.request.post(
                f"{BASE}/api/survey/upload",
                multipart={"file": {"name": os.path.basename(path),
                                    "mimeType": "text/csv",
                                    "buffer": raw}},
                headers={"Cookie": cookie_header},
                timeout=30000,
            )
            status = resp.status
            body = ""
            try:
                body = resp.text()[:300]
            except Exception:
                pass
            alive = health_check(page)
            if not alive:
                record("SPOOF", label, "VULNERABLE", f"サーバー停止 HTTP {status}")
            elif status in (400, 415, 422, 500):
                record("SPOOF", label, "SECURE", f"HTTP {status} 拒否/エラー応答")
            elif status == 200:
                # CSVパーサが何かしら返すケース。中身を処理していなければ SECURE とみなす
                if len(body) < 2000:
                    record("SPOOF", label, "SECURE", f"HTTP 200 (軽量応答 len={len(body)})")
                else:
                    record("SPOOF", label, "INCONCLUSIVE", f"HTTP 200 len={len(body)}")
            else:
                record("SPOOF", label, "INCONCLUSIVE", f"HTTP {status}")
        except Exception as e:
            record("SPOOF", label, "INCONCLUSIVE", f"例外: {type(e).__name__}")


# =============================================================
# main
# =============================================================
def main():
    print(f"[INFO] Target: {BASE}")
    print(f"[INFO] Tmp dir: {TMP}")

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, slow_mo=200)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # ログイン
        print("\n[INFO] ログイン中...")
        if not login(page):
            print("[ERROR] ログイン失敗。中止")
            browser.close()
            return
        print("[INFO] ログイン成功")

        # 各テスト実行（例外で全体停止しないよう個別try）
        for fn, args in [
            (test_xss, (page,)),
            (test_large_files, (page, ctx)),
            (test_encodings, (page, ctx)),
            (test_sqli, (page, ctx)),
            (test_csrf, (page, ctx)),
            (test_file_spoof, (page, ctx)),
        ]:
            try:
                if not health_check(page):
                    print(f"[WARN] health check 失敗、30秒待機後リトライ...")
                    time.sleep(30)
                fn(*args)
            except Exception as e:
                print(f"[ERROR] {fn.__name__} 例外: {e}")
                traceback.print_exc()

        browser.close()

    # サマリー
    print("\n" + "=" * 50)
    secure = sum(1 for r in RESULTS if r[2] == "SECURE")
    vuln = sum(1 for r in RESULTS if r[2] == "VULNERABLE")
    inc = sum(1 for r in RESULTS if r[2] == "INCONCLUSIVE")
    print(f"Security Summary: SECURE: {secure} / VULNERABLE: {vuln} / INCONCLUSIVE: {inc}")
    print("=" * 50)

    if vuln > 0:
        print("\n[VULNERABLE 詳細]")
        for cat, label, status, detail in RESULTS:
            if status == "VULNERABLE":
                print(f"  - [{cat}] {label}: {detail}")

    # JSON 出力
    out = os.path.join(DIR, "e2e_security_result.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump({
            "summary": {"secure": secure, "vulnerable": vuln, "inconclusive": inc},
            "results": [
                {"category": c, "label": l, "status": s, "detail": d}
                for c, l, s, d in RESULTS
            ],
        }, f, ensure_ascii=False, indent=2)
    print(f"\n[INFO] 詳細結果: {out}")

    sys.exit(0 if vuln == 0 else 1)


if __name__ == "__main__":
    main()
