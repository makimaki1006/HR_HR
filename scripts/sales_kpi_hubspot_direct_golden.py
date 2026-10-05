# -*- coding: utf-8 -*-
"""営業KPI HubSpot 直読み(Rust)の「旧新一致」テスト用の golden を作る(2026-10-05)。

Python 版 `sync_daily.py`(Hubspot リポジトリ)の **実物の関数** を、偽の HubSpot 応答で動かし、
(1) シートに書かれるはずの行 (2) HubSpot に投げた検索の本文 を記録する。
Rust 側のテスト(`src/handlers/sales_kpi/tests/hubspot_direct.rs`)は同じ偽応答
(`scenario.json`)から組んだ行・検索本文が、ここで記録した値と一致することを確かめる。

本物の HubSpot・Sheets・Zoom には一切つなぐ時間がない(すべて差し替える)。

使い方(Hubspot リポジトリの該当ブランチのファイルを渡す):
  git -C C:/dev/hs_negotype show origin/feat/sales-kpi-shoudanzokusei:scripts/sales_kpi/sync_daily.py > sync_daily_ref.py
  python scripts/sales_kpi_hubspot_direct_golden.py sync_daily_ref.py

出力: tests/fixtures/sales_kpi/hubspot_direct/golden.json
TZ: ワークフローは TZ=Asia/Tokyo。日付だけの値(`2026-09-02`)は JST の 0 時として読まれる。
"""
import importlib.util
import json
import os
import sys
import types
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

REF = Path(sys.argv[1]).resolve()
OUT_DIR = Path(__file__).resolve().parents[1] / "tests/fixtures/sales_kpi/hubspot_direct"
SCN = json.loads((OUT_DIR / "scenario.json").read_text(encoding="utf-8"))
JST = timezone(timedelta(hours=9))

# 本物の資格情報は使わない。ダミーのトークンだけ(通信しない)。
os.environ["HUBSPOT_TOKEN"] = "dummy-not-used"
for name in ("fetch_zoom",):
    m = types.ModuleType(name)
    m.fetch_outbound_by_user = lambda *a, **k: {}
    m.fetch_zoom_users = lambda *a, **k: {}
    sys.modules[name] = m
for name in ("google", "google.oauth2", "google.oauth2.credentials", "google.oauth2.service_account",
             "googleapiclient", "googleapiclient.discovery", "googleapiclient.errors"):
    if name not in sys.modules:
        try:
            __import__(name)
        except Exception:
            sys.modules[name] = mock.MagicMock()

spec = importlib.util.spec_from_file_location("sync_daily_ref", REF)
sd = importlib.util.module_from_spec(spec)
# ROOT = parents[2] を読むので、場所はどこでもよい(.env は読まれても害は無い)
spec.loader.exec_module(sd)

calls = []  # 検索の本文(順番どおり)
written = {}
meta_written = []


def page(items, body):
    """after をオフセットにした素朴なページング(200 件/ページ)。"""
    start = int(body.get("after") or 0)
    lim = body.get("limit", 200)
    chunk = items[start:start + lim]
    out = {"results": chunk}
    if start + lim < len(items):
        out["paging"] = {"next": {"after": str(start + lim)}}
    return out


def fake_hs_post(url, body):
    assert url.endswith("/crm/v3/objects/deals/search"), url
    calls.append(json.loads(json.dumps(body)))
    fg = body["filterGroups"]
    names = {f["propertyName"] for g in fg for f in g["filters"]}
    if "scheduled_business_meeting_date" in names:
        return page(SCN["shodan"], body)
    if any(n.startswith("hs_v2_date_entered_") for n in names):
        return page(SCN["apo"], body)
    if "dealstage" in names and any(f.get("value") == sd.ST_C for g in fg for f in g["filters"]):
        return page(SCN["cyomi"], body)
    if "ketteishamei" in names:
        return page(SCN["kettei"], body)
    raise AssertionError(f"想定外の検索 {body}")


def owners_get(url, timeout=None):
    from urllib.parse import parse_qs, urlparse
    q = parse_qs(urlparse(url).query)
    path = urlparse(url).path
    r = mock.Mock()
    r.raise_for_status = lambda: None
    if path.endswith("/pipelines/deals/default"):
        r.json = lambda: {"id": "default", "stages": SCN["stages"]}
        return r
    assert path.endswith("/crm/v3/owners"), url
    key = "owners_archived" if q["archived"][0] == "true" else "owners_active"
    after = q.get("after", [None])[0]
    pg = next(p for p in SCN[key] if p["after"] == after)
    body = {"results": pg["results"]}
    if pg["next_after"]:
        body["paging"] = {"next": {"after": pg["next_after"]}}
    r.json = lambda: body
    return r


count_jobs = []


def fake_count_many(jobs):
    count_jobs.append([[list(k), f] for k, f in jobs])
    out = {}
    for key, filters in jobs:
        k = key[0]
        if k == "__all__":
            out[key] = SCN["counts"]["all"]
        elif k.startswith(("ketteisha", "kessaisha")):
            out[key] = SCN["counts"]["has:" + k]
        else:
            out[key] = SCN["counts"]["stage:" + k]
    return out


def fake_write_sheet(svc, name, header, rows, titles):
    written[name] = {"header": header, "rows": [[str(c) for c in r] for r in rows]}


def fake_upsert(svc, name, header, rows, key_cols, titles):
    written["upsert:" + name] = {"header": header, "rows": [[str(c) for c in r] for r in rows]}


def run(today):
    calls.clear()
    count_jobs.clear()
    written.clear()
    sd._owners_cache.clear()
    excl = {k: set(v) for k, v in SCN["exclusions"].items()}
    with mock.patch.object(sd, "hs_post", fake_hs_post), \
         mock.patch.object(sd.HS, "get", owners_get), \
         mock.patch.object(sd, "count_many", fake_count_many), \
         mock.patch.object(sd, "write_sheet", fake_write_sheet), \
         mock.patch.object(sd, "upsert_sheet", fake_upsert), \
         mock.patch.object(sd, "read_roster", lambda svc: SCN["roster"]), \
         mock.patch.object(sd, "read_exclusions", lambda svc, titles: excl), \
         mock.patch.object(sd, "kaden_by_owner_rows", lambda *a, **k: ([], 0)), \
         mock.patch.object(sd, "report_kaden_gap", lambda *a, **k: 0), \
         mock.patch.object(sd, "sync_list_stock", lambda *a, **k: []), \
         mock.patch.object(sd, "write_meta", lambda svc, titles, rows: meta_written.append(rows)):
        sd.sync_shodan(None, [], today, False)
    return {
        "today": f"{today:%Y-%m-%d}",
        "searches": json.loads(json.dumps(calls)),
        "count_jobs": json.loads(json.dumps(count_jobs)),
        "written": json.loads(json.dumps(written)),
    }


def day(s):
    return datetime.strptime(s, "%Y-%m-%d").replace(tzinfo=JST)


primary = day(SCN["now_jst"][:10])
golden = {"primary": run(primary), "dates": []}
for s in SCN["extra_dates"]:
    g = run(day(s))
    golden["dates"].append({"today": g["today"], "searches": g["searches"]})
# 日時の読み方そのもの(jst_text)の対応表
samples = [None, "", "null", "1788224400000", "1788224400000.7", "2026-09-30T15:00:00Z",
           "2026-09-30T15:00:00.000Z", "2026-09-30T15:00:00+09:00", "2026-09-30", "garbage",
           "2026-09-30T14:59:59.999Z", "0", "-1000", "2026-09-30 15:00:00"]
golden["jst_text"] = [[s, sd.jst_text(s)] for s in samples]
meta = [r for rows in meta_written for r in rows]
golden["meta_keys"] = [r[0] for r in meta]
(OUT_DIR / "golden.json").write_text(json.dumps(golden, ensure_ascii=False, indent=1), encoding="utf-8")
print("golden を書きました", OUT_DIR / "golden.json")
print({k: len(v["rows"]) for k, v in golden["primary"]["written"].items()})
