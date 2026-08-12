# -*- coding: utf-8 -*-
"""E2E用: libSQL HTTP Pipeline API (/v2/pipeline) 互換スタブ。

本番 Turso の認証情報が無いため、ローカル SQLite をバックにして
src/db/turso_http.rs が期待する形式で応答する。
これにより Rust → HTTP → 実 SQL → JSON の全経路を実行できる。

リクエスト形式 (turso_http.rs:144-149):
  {"requests":[{"type":"execute","stmt":{"sql":..., "args":[{"type":"text","value":...}]}},
               {"type":"close"}]}
レスポンス形式 (turso_http.rs:175-191, 237-260):
  {"results":[{"type":"ok","response":{"result":{"cols":[{"name":...}],"rows":[[cell,...]]}}}]}
"""
import json
import os
import sqlite3
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DB = os.path.join(os.path.dirname(os.path.abspath(__file__)), "e2e_salesnow.db")
PORT = int(os.environ.get("STUB_PORT", "9401"))
LOG = os.path.join(os.path.dirname(os.path.abspath(__file__)), "e2e_stub_queries.log")

# 1 接続を全スレッドで共有し、ロックで直列化する。
# スレッドごとに接続を開くと書き込みで "database is locked" になる
# (本物の Turso はサーバ側で直列化するため、この差はスタブ固有)。
_lock = threading.Lock()
_conn = None


def conn():
    global _conn
    if _conn is None:
        _conn = sqlite3.connect(DB, check_same_thread=False, timeout=30)
        _conn.execute("PRAGMA journal_mode=WAL")
    return _conn


def to_cell(v):
    """Python の値を libSQL のセル表現へ。turso_cell_to_value と対になる。"""
    if v is None:
        return {"type": "null"}
    if isinstance(v, bool):
        return {"type": "integer", "value": str(int(v))}
    if isinstance(v, int):
        return {"type": "integer", "value": str(v)}
    if isinstance(v, float):
        return {"type": "float", "value": v}
    if isinstance(v, bytes):
        return {"type": "blob", "base64": ""}
    return {"type": "text", "value": str(v)}


def from_arg(a):
    t = a.get("type")
    if t == "null":
        return None
    if t == "integer":
        return int(a["value"])
    if t == "float":
        return float(a["value"])
    return a.get("value")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass  # 標準のアクセスログは抑制

    def do_POST(self):
        if not self.path.endswith("/v2/pipeline"):
            self.send_error(404)
            return
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        try:
            payload = json.loads(body)
        except Exception as e:
            self.send_error(400, str(e))
            return

        results = []
        for req in payload.get("requests", []):
            if req.get("type") != "execute":
                results.append({"type": "ok", "response": {"type": "close"}})
                continue
            stmt = req.get("stmt", {})
            sql = stmt.get("sql", "")
            args = [from_arg(a) for a in stmt.get("args", [])]
            with open(LOG, "a", encoding="utf-8") as f:
                f.write(json.dumps({"sql": sql, "args": args}, ensure_ascii=False) + "\n")
            try:
                with _lock:
                    cur = conn().execute(sql, args)
                    rows = cur.fetchall()
                    conn().commit()
                cols = [{"name": d[0], "decltype": None} for d in (cur.description or [])]
                results.append({
                    "type": "ok",
                    "response": {"type": "execute", "result": {
                        "cols": cols,
                        "rows": [[to_cell(v) for v in row] for row in rows],
                        "affected_row_count": 0,
                        "last_insert_rowid": None,
                    }},
                })
            except Exception as e:
                # 本物の Turso と同じくエラー型で返す (turso_http.rs:177-184)
                sys.stderr.write(f"[SQL ERROR] {e}\nSQL: {sql[:400]}\n")
                sys.stderr.flush()
                results.append({"type": "error", "error": {"message": str(e), "code": "SQLITE_ERROR"}})

        out = json.dumps({"baton": None, "base_url": None, "results": results},
                         ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)


if __name__ == "__main__":
    if not os.path.exists(DB):
        sys.exit(f"DB が無い: {DB} (先に e2e_build_db.py を実行)")
    if os.path.exists(LOG):
        os.remove(LOG)
    print(f"libSQL スタブ起動 port={PORT} db={DB}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
