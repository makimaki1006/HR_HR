# -*- coding: utf-8 -*-
"""外部統計 Turso のごく小さなスタブ。手元で図を目視するためだけに使う。

■ なぜ要るのか
  職種詳細の「掲示時給と最低賃金の開き」（ダンベル図）は、最低賃金を
  Turso から引く。手元では turso_db が None になるため図そのものが出ず、
  **本番にデプロイするまで誰も見られない**状態だった。
  実際、掲示時給が最低賃金を下回る県を 0 に丸めていた不具合は、
  データを SQL で数えて初めて見つかっている（図では消えていた）。

■ 使い方
    python scripts/turso_stub_for_local_check.py &
    TURSO_EXTERNAL_URL=http://127.0.0.1:9401 TURSO_EXTERNAL_TOKEN=stub       PORT=9324 AUTH_PASSWORD=... ./rust_dashboard
    → /tab/indeed/title?name=警備員 で 9 県のオレンジ帯が出る

■ 本物の Turso には触らない
  読み取り枠を使わないためにこれを用意している。
  最低賃金は公表値（2025 年度・地域別最低賃金）を直書きしている。
  金額の正しさを検証する用途には使わないこと。図の見え方の確認用。
"""
import json, re
from http.server import BaseHTTPRequestHandler, HTTPServer

MIN_WAGE = {
    "北海道":1075,"青森県":1017,"岩手県":1013,"宮城県":1038,"秋田県":1031,"山形県":1023,"福島県":1023,
    "茨城県":1074,"栃木県":1068,"群馬県":1063,"埼玉県":1141,"千葉県":1140,"東京都":1226,"神奈川県":1225,
    "新潟県":1050,"富山県":1062,"石川県":1054,"福井県":1053,"山梨県":1057,"長野県":1064,"岐阜県":1065,
    "静岡県":1097,"愛知県":1140,"三重県":1087,"滋賀県":1080,"京都府":1122,"大阪府":1177,"兵庫県":1116,
    "奈良県":1051,"和歌山県":1035,"鳥取県":1030,"島根県":1033,"岡山県":1047,"広島県":1085,"山口県":1043,
    "徳島県":1046,"香川県":1036,"愛媛県":1033,"高知県":1023,"福岡県":1057,"佐賀県":1030,"長崎県":1024,
    "熊本県":1034,"大分県":1024,"宮崎県":1023,"鹿児島県":1026,"沖縄県":1023,
}
FY = 2025

def cell(v):
    if v is None: return {"type":"null"}
    if isinstance(v,int): return {"type":"integer","value":str(v)}
    if isinstance(v,float): return {"type":"float","value":v}
    return {"type":"text","value":str(v)}

def result_for(sql):
    s = " ".join(sql.split())
    if re.search(r"v2_external_minimum_wage", s, re.I):
        cols = [{"name":"prefecture"},{"name":"hourly_min_wage"},{"name":"fiscal_year"}]
        rows = [[cell(p), cell(float(w)), cell(FY)] for p, w in MIN_WAGE.items()]
        return {"cols":cols,"rows":rows}
    if re.match(r"(?i)^select\s+1\b", s):
        return {"cols":[{"name":"1"}],"rows":[[cell(1)]]}
    # 知らないテーブルは空で返す（落とさない）
    return {"cols":[],"rows":[]}

class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length","0"))
        body = json.loads(self.rfile.read(n) or b"{}")
        out = []
        for req in body.get("requests", []):
            if req.get("type") == "execute":
                sql = (req.get("stmt") or {}).get("sql","")
                out.append({"type":"ok","response":{"type":"execute","result":result_for(sql)}})
            else:
                out.append({"type":"ok","response":{"type":"close"}})
        data = json.dumps({"baton":None,"base_url":None,"results":out}).encode()
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length",str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def log_message(self, *a): pass

if __name__ == "__main__":
    HTTPServer(("127.0.0.1", 9401), H).serve_forever()
