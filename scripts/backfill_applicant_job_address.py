"""応募 (0-421) の「応募先の勤務地の住所（地図用）」(`oubosaki_kinmuchi_juusho`) を入れる。

- 既定は dry-run (読むだけ。件数と例を出す)。`--apply` で HubSpot に書く (100 件ずつ batch update)。
- 入れる値 (`pick`): 関連する求人票の「求人の住所（地図用）」(`hrhr_job_address`。計算式の項目) と、
  応募の「応募先勤務地」(`oubosaki_kinmuchi`) のうち、文字数の多いほう (同じなら求人票)。
  求人票が複数あるときは、住所の入っている最初の求人票を使う。
  応募の取り込み側 (専用 PC) でも同じ決まりで入れる想定。
- 値が今と同じ応募には書かない。どちらも空の応募には書かない。
- 鍵は環境変数 HUBSPOT_ACCESS_TOKEN (値は出力しない)。

使い方:
    python3 scripts/backfill_applicant_job_address.py            # dry-run
    python3 scripts/backfill_applicant_job_address.py --apply    # 書く
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request

API = "https://api.hubapi.com"
PROP = "oubosaki_kinmuchi_juusho"


def pick(listing_address: str | None, applicant_kinmuchi: str | None) -> str | None:
    """応募に入れる応募先の住所。どちらも空なら None"""
    a = (listing_address or "").strip()
    b = (applicant_kinmuchi or "").strip()
    if not a and not b:
        return None
    return a if len(a) >= len(b) else b


def call(token: str, method: str, path: str, body: dict | None = None) -> dict:
    data = json.dumps(body).encode() if body is not None else None
    for attempt in range(6):
        req = urllib.request.Request(
            API + path,
            data=data,
            method=method,
            headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=60) as res:
                return json.load(res)
        except urllib.error.HTTPError as e:
            if e.code == 429 or e.code >= 500:
                time.sleep(2 + attempt * 2)
                continue
            raise
    raise RuntimeError(f"{method} {path} が続けて失敗しました")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true")
    args = ap.parse_args()
    token = os.environ.get("HUBSPOT_ACCESS_TOKEN", "").strip()
    if not token:
        print("HUBSPOT_ACCESS_TOKEN がありません", file=sys.stderr)
        return 2

    # 1. 応募を全部読む (id 順)
    applicants: dict[str, dict] = {}
    after = "0"
    while True:
        d = call(token, "POST", "/crm/v3/objects/0-421/search", {
            "limit": 200,
            "filterGroups": [{"filters": [{"propertyName": "hs_object_id", "operator": "GT", "value": after}]}],
            "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
            "properties": ["oubosaki_kinmuchi", PROP],
        })
        rows = d.get("results", [])
        if not rows:
            break
        for r in rows:
            applicants[r["id"]] = r["properties"]
        after = rows[-1]["id"]
        time.sleep(0.35)

    # 2. 応募 → 求人票の関連 (100 件ずつ)
    ids = list(applicants)
    links: dict[str, list[str]] = {}
    for i in range(0, len(ids), 100):
        d = call(token, "POST", "/crm/v4/associations/0-421/0-420/batch/read",
                 {"inputs": [{"id": x} for x in ids[i:i + 100]]})
        for r in d.get("results", []):
            links[str(r["from"]["id"])] = [str(t["toObjectId"]) for t in r.get("to", [])]
        time.sleep(0.2)

    # 3. 求人票の住所 (100 件ずつ)
    listing_ids = sorted({x for v in links.values() for x in v})
    listing_address: dict[str, str] = {}
    for i in range(0, len(listing_ids), 100):
        d = call(token, "POST", "/crm/v3/objects/0-420/batch/read", {
            "properties": ["hrhr_job_address"],
            "inputs": [{"id": x} for x in listing_ids[i:i + 100]],
        })
        for r in d.get("results", []):
            v = (r["properties"].get("hrhr_job_address") or "").strip()
            if v:
                listing_address[r["id"]] = v
        time.sleep(0.2)

    updates: list[dict] = []
    same = empty = from_listing = from_applicant = 0
    for aid, p in applicants.items():
        listing = next((listing_address[x] for x in links.get(aid, []) if x in listing_address), None)
        value = pick(listing, p.get("oubosaki_kinmuchi"))
        if value is None:
            empty += 1
            continue
        if (p.get(PROP) or "") == value:
            same += 1
            continue
        if listing and value == listing.strip():
            from_listing += 1
        else:
            from_applicant += 1
        updates.append({"id": aid, "properties": {PROP: value}})

    print(f"応募 {len(applicants)} 件 / 求人票の関連あり {sum(1 for v in links.values() if v)} 件 / "
          f"住所が無い {empty} 件 / 今と同じ {same} 件 / 書く {len(updates)} 件 "
          f"(求人票から {from_listing} 件・応募先勤務地から {from_applicant} 件)")
    if not args.apply:
        print("dry-run です。書くときは --apply を付けてください。")
        return 0
    done = 0
    for i in range(0, len(updates), 100):
        call(token, "POST", "/crm/v3/objects/0-421/batch/update", {"inputs": updates[i:i + 100]})
        done += len(updates[i:i + 100])
        time.sleep(0.5)
    print(f"書き込み {done} 件")
    return 0


if __name__ == "__main__":
    sys.exit(main())
