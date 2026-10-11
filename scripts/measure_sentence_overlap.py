#!/usr/bin/env python3
"""読み取り専用の文一致率実測。本文・名称・識別値は出力しない。標準ライブラリのみ。"""
import argparse
import itertools
import json
import math
import os
from pathlib import Path
import re
import time
import unicodedata
import urllib.error
import urllib.parse
import urllib.request


def sentence_text(text):
    columns = set(json.loads((Path(__file__).resolve().parents[1] / "frontend/src/generated/jobgen/columns.json").read_text()))
    lines = []
    for line in unicodedata.normalize("NFKC", text).splitlines():
        key, separator, value = line.strip().partition(":")
        lines.append(value.strip() if separator and key.strip() in columns else line.strip())
    return "\n".join(lines).strip()


def sentences(text, minimum=0):
    text = sentence_text(text)
    return [part.strip() for part in re.split(r"[。!?\n\r]", text)
            if part.strip() and len(part.strip()) >= minimum]


def overlap(generated, original, minimum=0):
    generated = sentence_text(generated)
    original = sentence_text(original)
    if not generated or not original:
        return None, False
    parts = sentences(generated, minimum)
    reference = set(sentences(original, minimum))
    return (sum(part in reference for part in parts) / len(parts) if parts else None,
            generated in original)


def token():
    value = os.environ.get("HUBSPOT_ACCESS_TOKEN", "").strip()
    if value:
        return value
    path = Path("/Users/s_fujimaki/Downloads/env")
    if path.exists():
        for line in path.read_text().splitlines():
            line = line.strip().removeprefix("export ")
            if line.startswith("HUBSPOT_ACCESS_TOKEN="):
                value = line.split("=", 1)[1].strip().strip("\"'")
                if value:
                    return value
    raise RuntimeError("読み取り用の認証情報が未取得です")


class Reader:
    def __init__(self, access_token):
        self.access_token = access_token
        self.last = 0.0
        self.calls = 0
        self.association_cache = {}

    def request(self, path, body=None):
        # 許可した読み取り窓口以外へはリクエストを作れない。
        if not (body is not None and (path.endswith("/batch/read") or path == "/crm/v3/objects/0-420/search")
                or body is None and "/associations/" in path):
            raise RuntimeError("読み取り以外の操作は禁止です")
        for attempt in range(5):
            time.sleep(max(0, self.last + 1.01 - time.monotonic()))
            self.last = time.monotonic()
            self.calls += 1
            if self.calls % 50 == 0:
                print(f"読み取り回数：{self.calls}回", flush=True)
            request = urllib.request.Request("https://api.hubapi.com" + path,
                data=json.dumps(body).encode() if body is not None else None,
                headers={"Authorization": "Bearer " + self.access_token, "Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    return json.load(response)
            except urllib.error.HTTPError as error:
                if error.code == 429 or error.code >= 500:
                    try:
                        pause = float(error.headers.get("Retry-After", 2 ** attempt))
                    except ValueError:
                        pause = 2 ** attempt
                    time.sleep(max(1, pause))
                    continue
                raise RuntimeError(f"読み取りに失敗しました（応答番号{error.code}）") from None
            except (urllib.error.URLError, TimeoutError):
                if attempt == 4:
                    raise RuntimeError("読み取りに失敗しました（通信）") from None
        raise RuntimeError("読み取りの再試行上限に達しました")

    def batch(self, ids):
        output = {}
        unique = sorted(set(ids))
        for index in range(0, len(unique), 100):
            chunk = unique[index:index + 100]
            data = self.request("/crm/v3/objects/0-420/batch/read", {
                "inputs": [{"id": key} for key in chunk],
                "properties": ["id_hrhakkaa", "id_airwork", "hrh_kyuujinhyou_honbun", "shigotonaiyou", "baitai_genjoukyou_airwork", "kyuujin_status"]})
            if data.get("errors"):
                raise RuntimeError("本文の一部を取得できませんでした")
            records = {str(row["id"]): row.get("properties", {}) for row in data["results"]}
            if set(records) != set(chunk):
                raise RuntimeError("本文の一部を取得できませんでした")
            output.update(records)
        return output

    def all_associations(self, source, key, target):
        output, after, seen = set(), None, set()
        while True:
            path = f"/crm/v4/objects/{source}/{key}/associations/{target}?limit=500"
            if after is not None:
                path += "&after=" + urllib.parse.quote(str(after), safe="")
            data = self.request(path)
            output.update(str(row["toObjectId"]) for row in data["results"])
            after = data.get("paging", {}).get("next", {}).get("after")
            if after is None:
                return output
            if str(after) in seen:
                raise RuntimeError("関連のページを最後まで取得できませんでした")
            seen.add(str(after))

    def associations(self, source, ids, target):
        unique = sorted(set(ids))
        output = {key: self.association_cache[source, key, target] for key in unique if (source, key, target) in self.association_cache}
        pending = [key for key in unique if key not in output]
        for index in range(0, len(pending), 100):
            chunk = pending[index:index + 100]
            data = self.request(f"/crm/v4/associations/{source}/{target}/batch/read", {"inputs": [{"id": key} for key in chunk]})
            for record in data["results"]:
                key = str(record["from"]["id"])
                if key not in chunk:
                    raise RuntimeError("関連の応答を確認できませんでした")
                output[key] = (self.all_associations(source, key, target) if record.get("paging") else
                               {str(item["toObjectId"]) for item in record["to"]})
            # 関連なしの入力がbatch応答から省かれる場合も、個別の全ページ読取で確認する。
            for key in set(chunk) - output.keys():
                output[key] = self.all_associations(source, key, target)
            for key in chunk:
                self.association_cache[source, key, target] = output[key]
        return output

    def companies(self, maximum):
        found, after, seen = set(), None, set()
        while len(found) < maximum:
            body = {"filterGroups": [{"filters": [
                        {"propertyName": "id_airwork", "operator": "HAS_PROPERTY"},
                        {"propertyName": "id_hrhakkaa", "operator": "NOT_HAS_PROPERTY"},
                        {"propertyName": "kyuujin_status", "operator": "EQ", "value": "公開中"}]}],
                    "properties": ["id_airwork"], "limit": 100, "sorts": ["hs_object_id"]}
            if after is not None:
                body["after"] = after
            data = self.request("/crm/v3/objects/0-420/search", body)
            seeds = [str(record["id"]) for record in data["results"]]
            jobs = self.associations("0-420", seeds, "deals")
            deals = self.associations("deals", set().union(*jobs.values()) if jobs else set(), "companies")
            for job in seeds:
                linked = set().union(*(deals.get(key, set()) for key in jobs[job]))
                # 掲載中のAirWorkから、取引先が一意に決まる候補を集める。
                if len(linked) == 1:
                    found.update(linked)
                if len(found) >= maximum:
                    break
            print(f"確認した取引先候補：{len(found)}件", flush=True)
            after = data.get("paging", {}).get("next", {}).get("after")
            if after is None:
                break
            if str(after) in seen:
                raise RuntimeError("求人のページを最後まで取得できませんでした")
            seen.add(str(after))
        return sorted(found)


def publication(raw, normalized):
    raw = (raw or "").strip()
    normalized = (normalized or "").strip()
    if normalized == "公開中":
        return "掲載中"
    if normalized in {"公開前", "公開終了"}:
        return None
    if raw in {"公開", "掲載中", "公開中"}:
        return "掲載中"
    if raw in {"非公開", "公開終了", "掲載終了", "公開開始前", "未掲載", "掲載停止", "停止中"}:
        return None
    return "掲載中か不明"


def samples(records):
    hrh = [r.get("hrh_kyuujinhyou_honbun") or "" for r in records if r.get("id_hrhakkaa")]
    air = [r.get("shigotonaiyou") or "" for r in records if r.get("id_airwork")
           and publication((r.get("baitai_genjoukyou_airwork") or "").strip(),
                           (r.get("kyuujin_status") or "").strip() if not r.get("id_hrhakkaa") else None) == "掲載中"]
    return hrh, air


def summary(values, whole, missing):
    values = sorted(values)
    def percentile(p):
        if not values:
            return None
        position = (len(values) - 1) * p
        lower, upper = math.floor(position), math.ceil(position)
        return values[lower] + (values[upper] - values[lower]) * (position - lower)
    bins = [0] * 10
    for value in values:
        bins[min(9, math.floor(value * 10))] += 1
    return {"count": len(values), "unmeasurable_count": missing, "whole_match_count": whole,
            "p10": percentile(.1), "p25": percentile(.25), "p50": percentile(.5),
            "p75": percentile(.75), "p90": percentile(.9), "p95": percentile(.95), "histogram": bins}


def measure(reader, maximum):
    companies = reader.companies(maximum)
    company_deals = reader.associations("companies", companies, "deals")
    all_deals = set().union(*company_deals.values()) if company_deals else set()
    deal_jobs = reader.associations("deals", all_deals, "0-420")
    all_jobs = set().union(*deal_jobs.values()) if deal_jobs else set()
    records = reader.batch(all_jobs)
    buckets = {(kind, minimum): [[], 0, 0] for kind in ["hrh_airwork", "airwork_airwork"] for minimum in [0, 8]}
    eligible = hrh_count = air_count = 0
    for company in companies:
        ids = set().union(*(deal_jobs[key] for key in company_deals[company]))
        hrh, air = samples([records[key] for key in sorted(ids)])
        if not hrh or not air:
            continue
        eligible += 1
        hrh_count += len(hrh)
        air_count += len(air)
        for kind, pairs in [("hrh_airwork", itertools.product(hrh, air)),
                            ("airwork_airwork", itertools.permutations(air, 2))]:
            for first, second in pairs:
                for minimum in [0, 8]:
                    ratio, whole = overlap(first, second, minimum)
                    bucket = buckets[kind, minimum]
                    bucket[1] += whole
                    if ratio is None:
                        bucket[2] += 1
                    else:
                        bucket[0].append(ratio)
    return {"candidate_company_count": len(companies), "company_count": eligible,
            "hrh_job_count": hrh_count, "airwork_job_count": air_count,
            "measurements": {f"{kind}_{minimum}": summary(*bucket) for (kind, minimum), bucket in buckets.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--max-companies", type=int, default=200)
    parser.add_argument("--output", type=Path, default=Path("/Users/s_fujimaki/orca/workspaces/HR_HR/codex-logs/rikurozi-overlap"))
    args = parser.parse_args()
    if not 1 <= args.max_companies <= 200:
        parser.error("対象の上限は1〜200です")
    try:
        result = measure(Reader(token()), args.max_companies)
    except RuntimeError as error:
        print(str(error), flush=True)
        return 1
    except (ValueError, KeyError):
        # リクエスト・本文・認証情報や例外の詳細はログにしない。
        print("実測を完了できませんでした。認証・読み取り権限・応答の形式を確認してください。", flush=True)
        return 1
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "summary.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(f"実測完了：取引先{result['company_count']}件。本文や名称は保存していません。", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
