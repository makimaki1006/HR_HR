"""アポのランク (talk_apo_rank) ごとに、商談の結果を数える。HubSpot は読み取りだけ。

使い方: python3 -I scripts/apo_rank_outcome.py ENVファイル [基準日 YYYY-MM-DD]
  ENVファイルに HUBSPOT_ACCESS_TOKEN=... の行があること。鍵は表示しない。
  出力は集計値だけ (取引 ID・会社名などは出さない)。

結果の定義 (docs/sales_call_analysis/apo-rank-vs-outcome-2026-11.html に同じことを書いた):
  成約     : リクロジ受注管理_商談の「成約」か請負契約_商談の「成約」に入った、
             または受注のパイプライン (納品管理・計上 5 本、apo_breakdown.rs の WON_PIPELINES と同じ) の
             どれかのステージに入った・今そこにいる取引
  商談実施 : 営業KPI の JISSHI と同じステージ・パイプライン (sales_kpi/mod.rs) に入った、BPO の商談実施処理、
             商談済リードのステージに入った、または成約
  価値合意 : Cヨミ (決定者との価値合意) 以上、商談済リードの C 以上、または成約
  対象     : 商談予定日 (scheduled_business_meeting_date、無ければ bpo_23) が基準日の 7 日前以前で、
             アポ日確定 (talk_apo_rank_entered_at) の前日より後のもの
"""
import datetime
import json
import sys
import time
import urllib.error
import urllib.request

API = "https://api.hubapi.com"
WON_PIPES = ["21596025", "743961896", "22417753", "741140824", "743705507"]
WON_STAGES = ["52017683", "1082014109"]
JISSHI = ["52035887", "52035888", "52035889", "52035890", "52035891", "52017683", "1325086466",
          "155012220", "155012221", "155012222", "155012223", "155012224"]
JISSHI_PIPES = {"62583420", "22417753", "21596025", "913508269"}
VALUE = ["52035889", "52035890", "52035891", "52017683", "155012222", "155012223", "155012224"]
RANKS = ["S", "A", "B", "C", "no_call", "unavailable"]
WAIT_DAYS = 7


def token(path):
    for line in open(path):
        if line.startswith("HUBSPOT_ACCESS_TOKEN="):
            return line.split("=", 1)[1].strip().strip('"').strip("'").replace("\r", "")
    raise SystemExit("HUBSPOT_ACCESS_TOKEN がありません")


def req(tok, method, path, body=None):
    for i in range(6):
        r = urllib.request.Request(API + path, method=method,
                                   data=json.dumps(body).encode() if body is not None else None,
                                   headers={"Authorization": "Bearer " + tok, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(r, timeout=60) as f:
                return json.load(f)
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 502, 503, 504):
                time.sleep(2 + 2 * i)
                continue
            raise SystemExit("HTTP %d %s" % (e.code, path))
    raise SystemExit("再試行の上限")


def dt(v):
    if not v:
        return None
    try:
        if "T" in v:
            return datetime.datetime.fromisoformat(v.replace("Z", "+00:00"))
        return datetime.datetime.fromisoformat(v + "T00:00:00+00:00")
    except ValueError:
        return None


def main():
    tok = token(sys.argv[1])
    base = (datetime.datetime.fromisoformat(sys.argv[2] + "T00:00:00+09:00") if len(sys.argv) > 2
            else datetime.datetime.now(datetime.timezone.utc))
    pipes = {p["id"]: p for p in req(tok, "GET", "/crm/v3/pipelines/deals")["results"]}
    won_pipe_stages = [s["id"] for pid in WON_PIPES for s in pipes[pid]["stages"]]
    stages = sorted(set(won_pipe_stages + WON_STAGES + JISSHI + VALUE))
    props = ["talk_apo_rank", "talk_apo_rank_entered_at", "pipeline", "dealstage",
             "scheduled_business_meeting_date", "bpo_23"] + ["hs_v2_date_entered_" + s for s in stages]
    deals, after = [], None
    while True:
        body = {"filterGroups": [{"filters": [{"propertyName": "talk_apo_rank", "operator": "HAS_PROPERTY"}]}],
                "properties": props, "limit": 200,
                "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}]}
        if after:
            body["after"] = after
        j = req(tok, "POST", "/crm/v3/objects/deals/search", body)
        deals += [r["properties"] for r in j["results"]]
        after = j.get("paging", {}).get("next", {}).get("after")
        time.sleep(0.4)
        if not after:
            break

    def entered(p, ss):
        return any(p.get("hs_v2_date_entered_" + s) for s in ss)

    cut = base - datetime.timedelta(days=WAIT_DAYS)
    out = {"base": base.isoformat(), "total": len(deals), "all": {}, "target": {}, "excluded": {}}
    ents = [dt(p.get("talk_apo_rank_entered_at")) for p in deals]
    ents = [e for e in ents if e]
    out["apo_entered_min"] = min(ents).isoformat() if ents else None
    out["apo_entered_max"] = max(ents).isoformat() if ents else None
    for p in deals:
        won = entered(p, WON_STAGES + won_pipe_stages) or p.get("pipeline") in WON_PIPES
        done = won or entered(p, JISSHI) or p.get("pipeline") in JISSHI_PIPES
        val = won or entered(p, VALUE)
        e = dt(p.get("talk_apo_rank_entered_at"))
        m = dt(p.get("scheduled_business_meeting_date")) or dt(p.get("bpo_23"))
        r = p.get("talk_apo_rank")
        res = {"n": 1, "done": int(done), "value": int(val), "won": int(won)}
        if not m:
            why = "no_meeting_date"
        elif e and m < e - datetime.timedelta(days=1):
            why = "meeting_before_apo"
        elif m > cut:
            why = "too_young"
        else:
            why = None
        for bucket in (["all"] + (["target"] if why is None else [])):
            acc = out[bucket].setdefault(r, {"n": 0, "done": 0, "value": 0, "won": 0})
            for k, v in res.items():
                acc[k] += v
        if why:
            out["excluded"].setdefault(why, {}).setdefault(r, 0)
            out["excluded"][why][r] += 1
    out["cut"] = cut.isoformat()
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
