"""求人票 (0-420) の「勤務地の住所（番地まで）」(`kinmuchi_juusho`) を AirWork の本文から入れる。

- 既定は dry-run (読むだけ。件数と例を出す)。`--apply` で HubSpot に書く (100 件ずつ batch update)。
- 取り出し方 (`extract`) は取り込み側 (専用 PC の AirWork / HRハッカー同期) でも同じ決まりで使う想定。
  - AirWork の本文 (`aw_kyuujinhyou_honbun`) の「勤務地：」の行の値。行が空なら取らない
    (次の行の「アクセス：」を拾わない)。
  - 先頭の郵便番号 (〒123-4567) を外す。
  - 空白で区切られた後ろの建物名などは、番地 (数字) を含む最初の区切りまでで切る。数字が無ければ空白を詰めてそのまま。
- 値が今と同じ求人には書かない。
- 鍵は環境変数 HUBSPOT_ACCESS_TOKEN (値は出力しない)。

使い方:
    python3 scripts/backfill_job_address_detail.py            # dry-run
    python3 scripts/backfill_job_address_detail.py --apply    # 書く
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

API = "https://api.hubapi.com"
PROP = "kinmuchi_juusho"
BODY = "aw_kyuujinhyou_honbun"
LINE = re.compile(r"(?m)^勤務地[：:][ \t　]*(\S.*?)[ \t　]*$")
POSTAL = re.compile(r"^〒?\s*[0-9０-９]{3}[-－ー‐][0-9０-９]{4}\s*")
DIGIT = re.compile(r"[0-9０-９]")


def extract(body: str) -> str | None:
    """本文から勤務地の住所を取り出す。取れなければ None"""
    m = LINE.search(body or "")
    if not m:
        return None
    value = POSTAL.sub("", m.group(1).strip()).strip()
    if not value or re.match(r"^(アクセス|勤務時間|給与)[：:]", value):
        return None
    tokens = value.split()
    kept: list[str] = []
    for token in tokens:
        kept.append(token)
        if DIGIT.search(token):
            break
    return "".join(kept) or None


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

    updates: list[dict] = []
    seen = same = none = 0
    after = "0"
    while True:
        d = call(token, "POST", "/crm/v3/objects/0-420/search", {
            "limit": 200,
            "filterGroups": [{"filters": [
                {"propertyName": BODY, "operator": "HAS_PROPERTY"},
                {"propertyName": "hs_object_id", "operator": "GT", "value": after},
            ]}],
            "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
            "properties": [BODY, PROP],
        })
        rows = d.get("results", [])
        if not rows:
            break
        for r in rows:
            seen += 1
            p = r["properties"]
            value = extract(p.get(BODY) or "")
            if value is None:
                none += 1
                continue
            if (p.get(PROP) or "") == value:
                same += 1
                continue
            updates.append({"id": r["id"], "properties": {PROP: value}})
        after = rows[-1]["id"]
        time.sleep(0.35)

    print(f"本文あり {seen} 件 / 取り出せない {none} 件 / 今と同じ {same} 件 / 書く {len(updates)} 件")
    for u in updates[:5]:
        print("  例:", u["id"], u["properties"][PROP])
    if not args.apply:
        print("dry-run です。書くときは --apply を付けてください。")
        return 0
    done = 0
    for i in range(0, len(updates), 100):
        call(token, "POST", "/crm/v3/objects/0-420/batch/update", {"inputs": updates[i:i + 100]})
        done += len(updates[i:i + 100])
        time.sleep(0.5)
    print(f"書き込み {done} 件")
    return 0


if __name__ == "__main__":
    sys.exit(main())
