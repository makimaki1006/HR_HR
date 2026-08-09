# -*- coding: utf-8 -*-
"""応募者ジャーニー診断のスモーク実行 (開発用)。

ゲート・プロンプト改修の検証を、フル実行 (6ペルソナ×19工程 ≈ Gemini 85回) ではなく
「diagnose → keywords → ペルソナ1体の detail/note/posting」(≈ 12〜18回) で行うための
省コスト実行ルート。フル実行は最終証明の1回だけにする (2026-08-09 運用合意)。

使い方:
  python scripts/journey_smoke.py --token TOKEN \
      --client-html 顧客求人.html --competitor-csv 競合.csv [--review-csv 口コミ.csv]
      [--base http://127.0.0.1:9216] [--persona persona_1] [--out スモーク出力dir]

終了コード: 全工程ゲート通過で0、それ以外は1。
"""
import argparse
import base64
import json
import pathlib
import sys
import urllib.request


def post(base, path, payload, token, timeout=600):
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        base + path, data=body,
        headers={"Content-Type": "application/json", "X-Api-Token": token}, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode("utf-8"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--token", required=True)
    ap.add_argument("--client-html", required=True)
    ap.add_argument("--competitor-csv", required=True)
    ap.add_argument("--review-csv", default=None)
    ap.add_argument("--base", default="http://127.0.0.1:9216")
    ap.add_argument("--persona", default="persona_1")
    ap.add_argument("--out", default=None, help="レスポンスJSONの保存先ディレクトリ (省略時は保存しない)")
    ap.add_argument("--captured-at", default="2026-01-01")
    args = ap.parse_args()

    out_dir = pathlib.Path(args.out) if args.out else None
    if out_dir:
        out_dir.mkdir(parents=True, exist_ok=True)

    ok = ng = 0
    total_llm = 0

    def check(name, resp, save_name=None):
        nonlocal ok, ng, total_llm
        passed = bool((resp.get("quality_gate") or {}).get("passed"))
        calls = resp.get("llm_calls") or 0
        total_llm += calls
        ok += passed
        ng += not passed
        detail = "" if passed else " " + str(
            resp.get("message") or ((resp.get("quality_gate") or {}).get("issues") or [])[:2])[:160]
        print(f"{'PASS' if passed else 'FAIL'} {name} (LLM {calls}回){detail}", flush=True)
        if out_dir and save_name:
            (out_dir / save_name).write_text(
                json.dumps(resp, ensure_ascii=False, indent=1), encoding="utf-8")
        return passed

    payload = {
        "client_job": pathlib.Path(args.client_html).read_text(encoding="utf-8", errors="replace"),
        "client_kind": "html",
        "competitor_csv_base64": base64.b64encode(pathlib.Path(args.competitor_csv).read_bytes()).decode(),
        "competitor_filename": pathlib.Path(args.competitor_csv).name,
        "competitor_captured_at": args.captured_at,
        "customer_statements": "",
        "customer_statement_speaker": "顧客担当者",
        "customer_statement_date": args.captured_at,
        "employer_note": "",
    }
    if args.review_csv:
        payload["review_csv_base64"] = base64.b64encode(pathlib.Path(args.review_csv).read_bytes()).decode()
        payload["review_filename"] = pathlib.Path(args.review_csv).name
        payload["review_captured_at"] = args.captured_at

    d = post(args.base, "/api/jobgen/journey-diagnose", payload, args.token)
    if not check("diagnose", d, "smoke_diagnose.json"):
        print("\ndiagnose不合格のため中断")
        return 1
    case = d["case_id"]
    ids = [p["id"] for p in (d.get("result") or {}).get("personas") or []]
    pid = args.persona if args.persona in ids else ids[0]
    print(f"personas={len(ids)} 対象={pid}", flush=True)

    k = post(args.base, "/api/jobgen/journey-keywords", {"case_id": case, "persona_ids": ids}, args.token)
    print(f"keywords: {k.get('measurement_status')}", flush=True)

    det = post(args.base, "/api/jobgen/journey-persona-detail",
               {"case_id": case, "persona_id": pid}, args.token)
    if not check(f"detail_{pid}", det, "smoke_detail.json"):
        print("\ndetail不合格のため中断")
        return 1

    note = post(args.base, "/api/jobgen/journey-note-draft",
                {"case_id": case, "persona_ids": [pid]}, args.token)
    check(f"note_{pid}", note, "smoke_note.json")

    posting = post(args.base, "/api/jobgen/journey-posting-draft",
                   {"case_id": case, "persona_ids": [pid]}, args.token)
    check(f"posting_{pid}", posting, "smoke_posting.json")

    print(f"\n{ok} PASS / {ng} FAIL ／ LLM合計 {total_llm}回")
    return 1 if ng else 0


if __name__ == "__main__":
    sys.exit(main())
