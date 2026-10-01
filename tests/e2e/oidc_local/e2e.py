"""Google OIDC ログインのローカル E2E (実ブラウザ Chromium)。

前提: 偽 Google (fake_google.py) が 127.0.0.1:9300、アプリが localhost:8080 で
RENDER=1 (本番相当: セッション Cookie は Secure + SameSite=Strict) で動いていること。
引数: mode (enabled / disabled) と スクリーンショットの出力先。手順は README.md。
"""
import json, os, sys, urllib.request
from playwright.sync_api import sync_playwright

APP, IDP = "http://localhost:8080", "http://127.0.0.1:9300"
OUT = sys.argv[2]
mode = sys.argv[1]
results = []


def rec(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)


def scenario(name):
    urllib.request.urlopen(f"{IDP}/_scenario?name={name}").read()


def idp_log():
    return json.loads(urllib.request.urlopen(f"{IDP}/_log").read())


def logged_in(page):
    """/ を開いて /login に戻されなければログイン済み"""
    page.goto(APP + "/")
    return not page.url.rstrip("/").endswith("/login")


def google_login(ctx):
    page = ctx.new_page()
    page.goto(APP + "/login")
    page.click("#google-login")
    page.wait_for_load_state("networkidle")
    return page


with sync_playwright() as p:
    # 手元に playwright 版と合うブラウザが無い場合は CHROMIUM_PATH で既存の chrome.exe を指定する
    exe = os.environ.get("CHROMIUM_PATH")
    b = p.chromium.launch(executable_path=exe, headless=True) if exe else p.chromium.launch(headless=True)
    if mode == "disabled":
        ctx = b.new_context()
        page = ctx.new_page()
        page.goto(APP + "/login")
        rec("未設定: ログイン画面に Google ボタンが無い", page.locator("#google-login").count() == 0)
        r = ctx.request.get(APP + "/auth/google/login", max_redirects=0)
        rec("未設定: /auth/google/login は 404", r.status == 404, f"status={r.status}")
        r = ctx.request.get(APP + "/auth/google/callback?code=x&state=y", max_redirects=0)
        rec("未設定: /auth/google/callback は 404", r.status == 404, f"status={r.status}")
        page.fill("#email", "hanako@f-a-c.co.jp")
        page.fill("#password", "testpass")
        page.click("form[action='/login'] button[type=submit]")
        page.wait_for_load_state("networkidle")
        rec("未設定: パスワードログインは従来どおり成功", logged_in(page), page.url)
        page.screenshot(path=f"{OUT}/disabled_after_password_login.png")
    else:
        # --- S1 正常系 (本番相当の Strict Cookie でループしないこと) ---
        scenario("ok")
        ctx = b.new_context()
        page = ctx.new_page()
        page.goto(APP + "/login")
        page.screenshot(path=f"{OUT}/s1_login_page.png")
        rec("S1 ログイン画面に Google ボタンがある", page.locator("#google-login").count() == 1)
        page.click("#google-login")
        page.wait_for_url("**/authorize**")
        page.click("#choose")
        page.wait_for_load_state("networkidle")
        page.wait_for_timeout(500)
        rec("S1 最終 URL がダッシュボード (/)", page.url.rstrip("/") == APP, page.url)
        page.screenshot(path=f"{OUT}/s1_after_login.png")
        cookies = {c["name"]: c for c in ctx.cookies(APP)}
        sess = [c for c in cookies.values() if c["name"] != "hrhr_oidc_tx"]
        rec("S1 セッション Cookie が Secure + SameSite=Strict",
            any(c["secure"] and c["sameSite"] == "Strict" for c in sess),
            str([(c["name"], c["secure"], c["sameSite"]) for c in sess]))
        rec("S1 state 用 Cookie は消えている",
            not any("oidc" in n for n in cookies), str(list(cookies)))
        rec("S1 ページ再読込でもログイン状態", logged_in(page), page.url)
        log = idp_log()
        auth = next(x["authorize_params"] for x in log if "authorize_params" in x)
        tok = next(x["token_checks"] for x in log if "token_checks" in x)
        rec("S1 認可要求に hd / PKCE S256 / nonce / prompt が付く",
            auth.get("hd") == "f-a-c.co.jp" and auth.get("code_challenge_method") == "S256"
            and len(auth.get("nonce", "")) >= 32 and auth.get("prompt") == "select_account"
            and auth.get("scope") == "openid email", json.dumps(auth)[:160])
        rec("S1 token 交換: PKCE・client_secret・redirect_uri・code 1 回きり すべて正",
            all(tok.values()), json.dumps(tok))
        cb = next(x["authorize_params"] for x in log if "authorize_params" in x)

        # --- S2 コールバック URL の使い回し (リプレイ) ---
        ctx2 = b.new_context()
        r = ctx2.request.get(APP + "/auth/google/callback?code=reused&state=" + cb["state"], max_redirects=0)
        rec("S2 別ブラウザで callback を叩く (state Cookie なし) → 400", r.status == 400, f"status={r.status}")
        rec("S2 その後もログインされていない", not logged_in(ctx2.new_page()))
        ctx2.close()

        # --- S3〜S8 拒否されるべきもの ---
        for sc, expect, label in [
            ("gmail", 403, "hd=gmail.com"),
            ("unverified", 403, "email_verified=false"),
            ("badnonce", 403, "nonce 不一致"),
            ("expired", 403, "exp 過去"),
            ("wrongaud", 403, "aud 不一致"),
            ("cancel", 400, "利用者がキャンセル"),
        ]:
            scenario(sc)
            c = b.new_context()
            pg = c.new_page()
            pg.goto(APP + "/login")
            pg.click("#google-login")
            pg.wait_for_url("**/authorize**")
            with pg.expect_response(lambda r: "/auth/google/callback" in r.url) as ri:
                pg.click("#choose")
            status = ri.value.status
            pg.wait_for_load_state("networkidle")
            body = pg.inner_text("body")[:120].replace("\n", " ")
            rec(f"拒否 {label} → {expect}", status == expect, f"status={status} 画面='{body}'")
            if sc == "gmail":
                pg.screenshot(path=f"{OUT}/s3_rejected_gmail.png")
            rec(f"拒否 {label} → ログインされていない", not logged_in(pg))
            c.close()

        # --- S9 パスワードログインの回帰 (OIDC 有効時も従来どおり) ---
        c = b.new_context()
        pg = c.new_page()
        pg.goto(APP + "/login")
        pg.fill("#email", "hanako@f-a-c.co.jp")
        pg.fill("#password", "testpass")
        pg.click("form[action='/login'] button[type=submit]")
        pg.wait_for_load_state("networkidle")
        rec("S9 OIDC 有効時もパスワードログインは成功", logged_in(pg), pg.url)
        c2 = b.new_context()
        pg2 = c2.new_page()
        pg2.goto(APP + "/login")
        pg2.fill("#email", "hanako@f-a-c.co.jp")
        pg2.fill("#password", "wrong")
        pg2.click("form[action='/login'] button[type=submit]")
        pg2.wait_for_load_state("networkidle")
        rec("S9 誤パスワードは拒否", not logged_in(pg2))
    b.close()

fails = [r for r in results if not r[1]]
print(f"\n=== {mode}: {len(results) - len(fails)} passed / {len(fails)} failed ===")
sys.exit(1 if fails else 0)
