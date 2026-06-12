"""
fetch_estat_occupation_middle.py
================================
e-Stat 国勢調査 R2 統計表 0003464372 (都道府県 × 職業中分類 × 男女 × 年齢階級) 取得。

職種カルテの「都道府県別年齢分布」用データ。
"""
from __future__ import annotations
import argparse
import csv
import json
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

ESTAT_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json"
STATS_DATA_ID = "0003464372"
LIMIT_PER_REQ = 100000  # e-Stat API の 1 リクエスト上限

OUTPUT_DIR = Path("data/generated")
CSV_OUT = OUTPUT_DIR / "estat_occupation_middle_pref.csv"
RAW_DIR = OUTPUT_DIR / "estat_occupation_middle_raw"


def fetch_meta(app_id: str) -> dict:
    url = f"{ESTAT_BASE}/getMetaInfo?appId={app_id}&statsDataId={STATS_DATA_ID}"
    return json.loads(urllib.request.urlopen(url, timeout=60).read().decode("utf-8"))


def build_code_map(meta: dict) -> dict:
    out = {}
    objs = meta["GET_META_INFO"]["METADATA_INF"]["CLASS_INF"]["CLASS_OBJ"]
    if not isinstance(objs, list):
        objs = [objs]
    for o in objs:
        oid = o["@id"]
        classes = o.get("CLASS", [])
        if not isinstance(classes, list):
            classes = [classes]
        out[oid] = {c["@code"]: c["@name"] for c in classes}
        # parent code 情報も取得（中分類が大分類に属することを判別するため）
        out[oid + "__parent"] = {c["@code"]: c.get("@parentCode", "") for c in classes}
    return out


def fetch_page(app_id: str, start: int) -> dict:
    q = urllib.parse.urlencode({
        "appId": app_id,
        "statsDataId": STATS_DATA_ID,
        "limit": LIMIT_PER_REQ,
        "startPosition": start,
    })
    url = f"{ESTAT_BASE}/getStatsData?{q}"
    return json.loads(urllib.request.urlopen(url, timeout=120).read().decode("utf-8"))


def fetch_all(app_id: str) -> list[dict]:
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    rows = []
    start = 1
    page = 0
    while True:
        page += 1
        print(f"[page {page}] fetching from {start} ...", flush=True)
        d = fetch_page(app_id, start)
        (RAW_DIR / f"page_{page:03d}.json").write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
        sd = d["GET_STATS_DATA"]["STATISTICAL_DATA"]
        rs = sd["RESULT_INF"]
        total = rs.get("TOTAL_NUMBER", 0)
        to_n = rs.get("TO_NUMBER", 0)
        values = sd["DATA_INF"]["VALUE"]
        if not isinstance(values, list):
            values = [values]
        rows.extend(values)
        print(f"  got {len(values)}, total received={len(rows)}/{total}", flush=True)
        next_key = rs.get("NEXT_KEY")
        if not next_key:
            break
        start = int(next_key)
        time.sleep(1)
    return rows


def write_csv(rows: list[dict], codes: dict) -> int:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    written = 0
    with open(CSV_OUT, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow([
            "pref_code", "prefecture", "occupation_code", "occupation_middle",
            "occupation_major_code", "age_class", "gender", "population",
        ])
        gender_map = {"0": "total", "1": "male", "2": "female"}
        for v in rows:
            area = v.get("@area", "")
            cat03 = v.get("@cat03", "")
            cat02 = v.get("@cat02", "")
            cat01 = v.get("@cat01", "")
            # 全国除外、年齢総数除外、職業総数除外
            if area == "00000":
                continue
            if cat02 == "00":
                continue
            if cat03 == "0":
                continue
            pref_name = codes["area"].get(area, "")
            occ_name = codes["cat03"].get(cat03, "")
            occ_parent = codes["cat03__parent"].get(cat03, "")
            age_name = codes["cat02"].get(cat02, "")
            # 「（再掲）」「総数」「不詳」はCSV生成時点でスキップ
            if "（再掲）" in age_name or age_name in ("総数", "不詳"):
                continue
            gender = gender_map.get(cat01, cat01)
            raw_val = v.get("$", "")
            # 「-」「***」「X」等は欠損
            try:
                pop = int(raw_val)
            except (ValueError, TypeError):
                pop = None
            w.writerow([
                area[:2], pref_name, cat03, occ_name, occ_parent,
                age_name, gender, pop if pop is not None else "",
            ])
            written += 1
    return written


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--app-id", required=True)
    ap.add_argument("--meta-only", action="store_true")
    args = ap.parse_args()

    print("Fetching meta...")
    meta = fetch_meta(args.app_id)
    codes = build_code_map(meta)
    print(f"  prefectures: {len(codes['area'])}")
    print(f"  age classes: {len(codes['cat02'])}")
    print(f"  occupations: {len(codes['cat03'])}")

    if args.meta_only:
        return 0

    rows = fetch_all(args.app_id)
    print(f"Total rows fetched: {len(rows)}")

    written = write_csv(rows, codes)
    print(f"CSV written: {CSV_OUT} ({written} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
