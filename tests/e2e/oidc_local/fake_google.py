"""ローカル E2E 用の偽 Google (OIDC プロバイダ)。127.0.0.1:9300 で待ち受ける。

アプリ (http://localhost:8080) とは別サイトなので、Google からのコールバックと同じ
クロスサイト遷移になる (本番の SameSite=Strict の挙動を確かめるため)。

/_scenario?name=... で次の ID token の中身を切り替える:
  ok / gmail / unverified / badnonce / cancel / expired / wrongaud
/_log で token endpoint が受け取った内容の検査結果を返す。
"""
import base64, hashlib, json, secrets, sys, threading, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import jwt

ROOT = sys.argv[1]  # C:\dev\hr_crm_oidc
ISSUER = "http://127.0.0.1:9300"
KEY = open(f"{ROOT}/tests/fixtures/oidc/test_key_1.pem").read()
JWKS = open(f"{ROOT}/tests/fixtures/oidc/jwks.json").read()
CLIENT_ID, CLIENT_SECRET = "test-client", "test-secret"

state = {"scenario": "ok", "codes": {}, "log": []}
lock = threading.Lock()


def b64url_sha256(s):
    return base64.urlsafe_b64encode(hashlib.sha256(s.encode()).digest()).rstrip(b"=").decode()


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send_json(self, obj, code=200):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def redirect(self, url):
        self.send_response(302)
        self.send_header("Location", url)
        self.end_headers()

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        q = dict(urllib.parse.parse_qsl(u.query))
        if u.path == "/.well-known/openid-configuration":
            return self.send_json({
                "issuer": ISSUER,
                "authorization_endpoint": ISSUER + "/authorize",
                "token_endpoint": ISSUER + "/token",
                "jwks_uri": ISSUER + "/jwks",
            })
        if u.path == "/jwks":
            b = JWKS.encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            return self.wfile.write(b)
        if u.path == "/_scenario":
            with lock:
                state["scenario"] = q.get("name", "ok")
                state["log"].clear()
            return self.send_json({"scenario": state["scenario"]})
        if u.path == "/_log":
            return self.send_json(state["log"])
        if u.path == "/authorize":
            with lock:
                sc = state["scenario"]
                state["log"].append({"authorize_params": q})
            ru = q["redirect_uri"]
            # 本物の Google と同じく、アカウント選択画面を出して利用者のクリックで戻す
            # (戻りのナビゲーションの起点が IdP 側 = クロスサイトになる)
            rid = secrets.token_urlsafe(8)
            with lock:
                state.setdefault("pending", {})[rid] = q
            b = (f'<!doctype html><meta charset="utf-8"><title>fake google</title>'
                 f'<p>アカウントを選択</p><a id="choose" href="/approve?rid={rid}">taro@f-a-c.co.jp</a>').encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.end_headers()
            return self.wfile.write(b)
        if u.path == "/approve":
            with lock:
                q = state["pending"].pop(q["rid"])
                sc = state["scenario"]
            ru = q["redirect_uri"]
            if sc == "cancel":
                return self.redirect(ru + "?" + urllib.parse.urlencode({"error": "access_denied", "state": q["state"]}))
            code = secrets.token_urlsafe(16)
            with lock:
                state["codes"][code] = {"nonce": q.get("nonce"), "challenge": q.get("code_challenge"),
                                        "redirect_uri": ru, "client_id": q.get("client_id"), "scenario": sc}
            return self.redirect(ru + "?" + urllib.parse.urlencode({"code": code, "state": q["state"]}))
        self.send_json({"error": "not found"}, 404)

    def do_POST(self):
        u = urllib.parse.urlparse(self.path)
        n = int(self.headers.get("Content-Length", 0))
        form = dict(urllib.parse.parse_qsl(self.rfile.read(n).decode()))
        if u.path != "/token":
            return self.send_json({"error": "not found"}, 404)
        with lock:
            c = state["codes"].pop(form.get("code"), None)  # 1 回きり
        checks = {
            "code_known_single_use": c is not None,
            "pkce_ok": c is not None and b64url_sha256(form.get("code_verifier", "")) == c["challenge"],
            "client_secret_ok": form.get("client_secret") == CLIENT_SECRET,
            "client_id_ok": form.get("client_id") == CLIENT_ID,
            "redirect_uri_ok": c is not None and form.get("redirect_uri") == c["redirect_uri"],
            "grant_type_ok": form.get("grant_type") == "authorization_code",
        }
        with lock:
            state["log"].append({"token_checks": checks})
        if not all(checks.values()):
            return self.send_json({"error": "invalid_grant", "checks": checks}, 400)
        sc = c["scenario"]
        now = int(time.time())
        claims = {"iss": ISSUER, "aud": CLIENT_ID, "sub": "1234567890", "email": "taro@f-a-c.co.jp",
                  "email_verified": True, "hd": "f-a-c.co.jp", "nonce": c["nonce"], "iat": now, "exp": now + 600}
        if sc == "gmail":
            claims.update(hd="gmail.com", email="taro@gmail.com")
        elif sc == "unverified":
            claims["email_verified"] = False
        elif sc == "badnonce":
            claims["nonce"] = "not-the-nonce-" + "x" * 32
        elif sc == "expired":
            claims.update(iat=now - 7200, exp=now - 3600)
        elif sc == "wrongaud":
            claims["aud"] = "someone-else"
        tok = jwt.encode(claims, KEY, algorithm="RS256", headers={"kid": "test-key-1"})
        self.send_json({"access_token": "x", "token_type": "Bearer", "expires_in": 3600, "id_token": tok})


ThreadingHTTPServer(("127.0.0.1", 9300), H).serve_forever()
