# -*- coding: utf-8 -*-
"""
Phase 3 Step 5 SQL Window Function 互換性検証 (READ-ONLY)
==========================================================
Turso libSQL での RANK() OVER / COUNT(*) OVER / PARTITION BY の互換性確認。

- READ-ONLY (SELECT/PRAGMA/WITH のみ)
- READ 上限 10 (Turso 月間クォータ余裕確保)
- token / URL は os.getenv() で取得、マスク表示のみ
- 出力: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_SQL_WINDOW_COMPAT.md

使い方:
    python scripts/verify_turso_window_function.py --local
    python scripts/verify_turso_window_function.py --remote
    python scripts/verify_turso_window_function.py --both
"""
import argparse
import json
import os
import re
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from verify_turso_v2_sync import TursoReadOnlyClient, ReadLimitExceeded

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
LOCAL_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
OUTPUT_PATH = SCRIPT_DIR.parent / "docs" / "SURVEY_MARKET_INTELLIGENCE_PHASE3_SQL_WINDOW_COMPAT.md"

MAX_READS = 10

# READ-only allowlist (CTE 含む)
ALLOWED_SQL_PREFIXES = ("SELECT", "PRAGMA", "WITH")


def assert_read_only(sql: str) -> None:
    s = sql.strip().upper()
    if not any(s.startswith(p) for p in ALLOWED_SQL_PREFIXES):
        raise SystemExit(f"[abort] non-read-only SQL: {sql[:80]}")


def mask_token(token: str) -> str:
    if not token:
        return "(unset)"
    if len(token) < 16:
        return "***"
    return f"{token[:6]}...{token[-4:]} (len={len(token)})"


def mask_url(url: str) -> str:
    if not url:
        return "(unset)"
    # libsql://host.turso.io → libsql://***.turso.io 風にマスクは不要、host名のみ表示
    m = re.match(r"^(libsql://|https://)([^/]+)", url)
    if m:
        host = m.group(2)
        # サブドメイン部分を一部マスク
        return f"{m.group(1)}{host}"
    return url


# ──────────────────────────────────────────────
# 検証 SQL
# ──────────────────────────────────────────────
QUERIES = {
    "Q1_RANK_PARTITION": """
WITH tgt AS (
  SELECT v.municipality_code, mcm.municipality_name,
         mcm.parent_code, parent.municipality_name AS parent_name,
         v.thickness_index, v.occupation_code
  FROM v2_municipality_target_thickness v
  JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
  LEFT JOIN municipality_code_master parent ON mcm.parent_code = parent.municipality_code
  WHERE mcm.area_type = 'designated_ward'
    AND v.occupation_code = '08_生産工程'
    AND mcm.parent_code = '14100'
)
SELECT municipality_code, municipality_name,
       RANK() OVER (PARTITION BY parent_code ORDER BY thickness_index DESC) AS parent_rank
FROM tgt
ORDER BY parent_rank
""".strip(),

    "Q2_COUNT_OVER_PARTITION": """
SELECT v.municipality_code, mcm.municipality_name,
       mcm.parent_code,
       COUNT(*) OVER (PARTITION BY mcm.parent_code) AS parent_total
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type = 'designated_ward'
  AND v.occupation_code = '08_生産工程'
  AND mcm.parent_code = '14100'
LIMIT 18
""".strip(),

    "Q3_RANK_AND_COUNT": """
SELECT v.municipality_code, mcm.municipality_name,
       mcm.parent_code, parent.municipality_name AS parent_name,
       v.thickness_index, v.occupation_code,
       RANK() OVER (PARTITION BY mcm.parent_code ORDER BY v.thickness_index DESC) AS parent_rank,
       COUNT(*) OVER (PARTITION BY mcm.parent_code) AS parent_total
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
LEFT JOIN municipality_code_master parent ON mcm.parent_code = parent.municipality_code
WHERE mcm.area_type = 'designated_ward'
  AND v.occupation_code = '08_生産工程'
ORDER BY mcm.parent_code, parent_rank
LIMIT 30
""".strip(),
}


# ──────────────────────────────────────────────
# ローカル実行
# ──────────────────────────────────────────────
def run_local(db_path: Path) -> dict:
    if not db_path.exists():
        return {"error": f"DB not found: {db_path}"}
    uri = f"file:{db_path.as_posix()}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    out = {}
    try:
        cur = conn.execute("SELECT sqlite_version()")
        out["sqlite_version"] = cur.fetchone()[0]
        for name, sql in QUERIES.items():
            assert_read_only(sql)
            try:
                cur = conn.execute(sql)
                cols = [d[0] for d in cur.description]
                rows = [list(r) for r in cur.fetchall()]
                out[name] = {"ok": True, "cols": cols, "rows": rows}
            except sqlite3.Error as e:
                out[name] = {"ok": False, "error": str(e)}
    finally:
        conn.close()
    return out


# ──────────────────────────────────────────────
# Turso 実行
# ──────────────────────────────────────────────
def run_remote(url: str, token: str) -> dict:
    client = TursoReadOnlyClient(url, token, max_reads=MAX_READS)
    out = {"host": client.host}
    try:
        data = client.execute("SELECT sqlite_version()")
        rows = data.get("results", [])[0].get("response", {}).get("result", {}).get("rows", [])
        out["sqlite_version"] = rows[0][0].get("value") if rows else "?"
    except Exception as e:
        out["sqlite_version"] = f"error: {e}"

    for name, sql in QUERIES.items():
        assert_read_only(sql)
        try:
            data = client.execute(sql)
            result = data.get("results", [])[0].get("response", {}).get("result", {})
            cols = [c.get("name") for c in result.get("cols", [])]
            rows = [[v.get("value") for v in row] for row in result.get("rows", [])]
            out[name] = {"ok": True, "cols": cols, "rows": rows}
        except ReadLimitExceeded as e:
            out[name] = {"ok": False, "error": f"READ_LIMIT: {e}"}
            break
        except Exception as e:
            out[name] = {"ok": False, "error": str(e)[:300]}
    out["read_count"] = client.read_count
    return out


# ──────────────────────────────────────────────
# 比較
# ──────────────────────────────────────────────
def normalize_value(v):
    """libSQL は数値も文字列で返すことがある → 比較用に正規化"""
    if v is None:
        return None
    if isinstance(v, (int, float)):
        return str(v)
    return str(v)


def compare_results(local_res: dict, remote_res: dict) -> dict:
    """ローカルとリモートの行データを完全比較"""
    diffs = {}
    for name in QUERIES.keys():
        l = local_res.get(name)
        r = remote_res.get(name)
        d = {"name": name}
        if not l or not l.get("ok"):
            d["status"] = "LOCAL_FAIL"
            d["detail"] = l.get("error") if l else "missing"
            diffs[name] = d
            continue
        if not r or not r.get("ok"):
            d["status"] = "REMOTE_FAIL"
            d["detail"] = r.get("error") if r else "missing"
            diffs[name] = d
            continue
        l_rows = [[normalize_value(v) for v in row] for row in l["rows"]]
        r_rows = [[normalize_value(v) for v in row] for row in r["rows"]]
        if l_rows == r_rows:
            d["status"] = "MATCH"
            d["row_count"] = len(l_rows)
        else:
            d["status"] = "MISMATCH"
            d["local_count"] = len(l_rows)
            d["remote_count"] = len(r_rows)
            # 最初の差分行を抽出
            sample_diffs = []
            for i, (lr, rr) in enumerate(zip(l_rows, r_rows)):
                if lr != rr:
                    sample_diffs.append({"idx": i, "local": lr, "remote": rr})
                    if len(sample_diffs) >= 3:
                        break
            d["sample_diffs"] = sample_diffs
        diffs[name] = d
    return diffs


# ──────────────────────────────────────────────
# レポート生成
# ──────────────────────────────────────────────
def render_table(cols, rows, limit=10):
    if not rows:
        return "(0 rows)"
    rows = rows[:limit]
    out = ["| " + " | ".join(cols) + " |", "|" + "|".join(["---"] * len(cols)) + "|"]
    for r in rows:
        out.append("| " + " | ".join("" if v is None else str(v) for v in r) + " |")
    return "\n".join(out)


def render_report(local_res, remote_res, diffs, started_at, finished_at, mode):
    lines = [
        "# Phase 3 Step 5 SQL Window Function 互換性検証",
        "",
        f"- 実行日時 (UTC): {started_at.isoformat()} 〜 {finished_at.isoformat()}",
        f"- 実行モード: `{mode}`",
        f"- 検証者: Worker P0 (READ-ONLY)",
        "",
    ]

    # 0. 結論
    lines.extend(["## 0. 結論", ""])
    if mode == "local":
        all_ok = all(local_res.get(q, {}).get("ok") for q in QUERIES)
        if all_ok:
            lines.append("**ローカル PASS** (3/3)。Turso 実行は環境変数未設定でスキップ。Turso 結果は未取得 (ユーザー手動実行待ち)。")
        else:
            lines.append("**ローカル FAIL**。下記詳細参照。")
    elif diffs:
        match_count = sum(1 for d in diffs.values() if d["status"] == "MATCH")
        total = len(diffs)
        if match_count == total:
            lines.append(f"**PASS** ({match_count}/{total} 一致)。SQL Window Function は Turso libSQL で動作する。Phase 3 本実装で `RANK() OVER` / `COUNT(*) OVER` / `PARTITION BY` を採用可能。")
        else:
            lines.append(f"**FAIL** ({match_count}/{total} 一致のみ)。Rust fallback 採用を推奨。")
    lines.append("")

    # 1. SQLite バージョン
    lines.extend(["## 1. SQLite バージョン", ""])
    lines.append(f"- ローカル: `{local_res.get('sqlite_version', 'n/a')}`")
    if remote_res:
        lines.append(f"- Turso: `{remote_res.get('sqlite_version', 'n/a')}`")
    else:
        lines.append("- Turso: 未取得")
    lines.append("")
    lines.append("Window Function は SQLite 3.25+ で利用可能 (RANK/PARTITION BY 含む)。")
    lines.append("")

    # 2. 検証 SQL
    lines.extend(["## 2. 検証 SQL (3 種)", ""])
    for name, sql in QUERIES.items():
        lines.append(f"### {name}")
        lines.append("")
        lines.append("```sql")
        lines.append(sql)
        lines.append("```")
        lines.append("")

    # 3. ローカル結果
    lines.extend(["## 3. ローカル実行結果", ""])
    for name in QUERIES:
        r = local_res.get(name, {})
        lines.append(f"### {name}")
        lines.append("")
        if r.get("ok"):
            lines.append(f"- 行数: {len(r['rows'])}")
            lines.append("")
            lines.append(render_table(r["cols"], r["rows"], limit=10))
        else:
            lines.append(f"- ❌ ERROR: `{r.get('error', 'n/a')}`")
        lines.append("")

    # 4. Turso 結果
    lines.extend(["## 4. Turso 実行結果", ""])
    if not remote_res:
        lines.append("(Turso 未実行 — env 未設定または `--local` モード)")
        lines.append("")
    else:
        lines.append(f"- ホスト: `{remote_res.get('host', 'n/a')}` (token はマスク済)")
        lines.append(f"- READ 消費: {remote_res.get('read_count', 'n/a')} / {MAX_READS}")
        lines.append("")
        for name in QUERIES:
            r = remote_res.get(name, {})
            lines.append(f"### {name}")
            lines.append("")
            if r.get("ok"):
                lines.append(f"- 行数: {len(r['rows'])}")
                lines.append("")
                lines.append(render_table(r["cols"], r["rows"], limit=10))
            else:
                lines.append(f"- ❌ ERROR: `{r.get('error', 'n/a')}`")
            lines.append("")

    # 5. 差分比較
    lines.extend(["## 5. 差分比較 (ローカル vs Turso)", ""])
    if not diffs:
        lines.append("(比較は `--both` モードでのみ実行)")
        lines.append("")
    else:
        lines.append("| Query | 状態 | ローカル行数 | Turso行数 | 一致 |")
        lines.append("|-------|------|----:|----:|:---:|")
        for name, d in diffs.items():
            status = d["status"]
            lc = d.get("row_count") or d.get("local_count", "—")
            rc = d.get("row_count") or d.get("remote_count", "—")
            ok = "✅" if status == "MATCH" else "❌"
            lines.append(f"| {name} | {status} | {lc} | {rc} | {ok} |")
        lines.append("")
        for name, d in diffs.items():
            if d["status"] == "MISMATCH" and d.get("sample_diffs"):
                lines.append(f"### {name} 差分サンプル")
                lines.append("")
                for sd in d["sample_diffs"]:
                    lines.append(f"- 行 {sd['idx']}:")
                    lines.append(f"  - ローカル: `{sd['local']}`")
                    lines.append(f"  - Turso  : `{sd['remote']}`")
                lines.append("")

    # 6. PASS 判定
    lines.extend(["## 6. PASS 判定", ""])
    if not diffs:
        lines.append("- ローカルのみ実行。Turso 互換性は手動再実行 (`--both`) で確認要。")
    else:
        match_count = sum(1 for d in diffs.values() if d["status"] == "MATCH")
        total = len(diffs)
        if match_count == total:
            lines.append(f"✅ **PASS** ({match_count}/{total})")
            lines.append("")
            lines.append("Turso libSQL は SQL Window Function (`RANK() OVER`, `COUNT(*) OVER`, `PARTITION BY`) を完全サポート。")
        else:
            lines.append(f"❌ **FAIL** ({match_count}/{total})")
    lines.append("")

    # 7. FAIL 時の Rust fallback
    lines.extend(["## 7. FAIL 時の Rust fallback 案", ""])
    lines.append("Window Function が Turso 側で動かない場合の代替策 (Rust Integration Plan §0.5):")
    lines.append("")
    lines.append("1. **Rust 側でランク計算**: `SELECT municipality_code, parent_code, thickness_index FROM ...` のみ Turso から取得し、Rust の `Vec` を `parent_code` でグループ化 → `sort_by` → enumerate で rank 付与。")
    lines.append("2. **2 ステップクエリ**: 親グループの総数を別 `COUNT(*) GROUP BY parent_code` で取得し、Rust 側で Map<parent_code, total> を保持。")
    lines.append("3. **事前計算**: `build_municipality_target_thickness.py` の段階で rank/total カラムを事前計算しテーブルに格納。Phase 3 のクエリ複雑度を下げる。")
    lines.append("")

    lines.extend(["---", "", f"生成: `scripts/verify_turso_window_function.py` ({finished_at.strftime('%Y-%m-%d %H:%M UTC')})"])
    return "\n".join(lines)


# ──────────────────────────────────────────────
# main
# ──────────────────────────────────────────────
def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1] if __doc__ else "")
    g = parser.add_mutually_exclusive_group(required=True)
    g.add_argument("--local", action="store_true", help="ローカル DB のみで実行")
    g.add_argument("--remote", action="store_true", help="Turso V2 のみで実行")
    g.add_argument("--both", action="store_true", help="両方実行 + 比較")
    args = parser.parse_args()

    print("=" * 70)
    print("Phase 3 Step 5: SQL Window Function 互換性検証 (READ-ONLY)")
    print("=" * 70)

    started_at = datetime.now(timezone.utc)
    local_res = {}
    remote_res = {}
    diffs = {}
    mode = "local" if args.local else ("remote" if args.remote else "both")

    # ローカル
    if args.local or args.both:
        print(f"\n[Local] DB: {LOCAL_DB}")
        local_res = run_local(LOCAL_DB)
        print(f"  SQLite version: {local_res.get('sqlite_version', 'n/a')}")
        for name in QUERIES:
            r = local_res.get(name, {})
            if r.get("ok"):
                print(f"  {name}: ✅ {len(r['rows'])} rows")
            else:
                print(f"  {name}: ❌ {r.get('error', 'n/a')}")

    # リモート
    if args.remote or args.both:
        url = os.getenv("TURSO_EXTERNAL_URL", "").strip()
        token = os.getenv("TURSO_EXTERNAL_TOKEN", "").strip()
        if not url or not token:
            print("\n[Remote] ERROR: TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN 未設定")
            if args.remote:
                return 1
            print("[Remote] スキップ (--both → ローカルのみで継続)")
            mode = "local (remote skipped: env unset)"
        else:
            print(f"\n[Remote] URL: {mask_url(url)}")
            print(f"[Remote] Token: {mask_token(token)}")
            try:
                remote_res = run_remote(url, token)
                print(f"  SQLite version: {remote_res.get('sqlite_version', 'n/a')}")
                print(f"  READ 消費: {remote_res.get('read_count', 'n/a')} / {MAX_READS}")
                for name in QUERIES:
                    r = remote_res.get(name, {})
                    if r.get("ok"):
                        print(f"  {name}: ✅ {len(r['rows'])} rows")
                    else:
                        print(f"  {name}: ❌ {r.get('error', 'n/a')}")
            except Exception as e:
                print(f"  ❌ Turso 実行失敗: {e}")
                if args.remote:
                    return 1

    # 比較
    if args.both and remote_res:
        diffs = compare_results(local_res, remote_res)
        print("\n[比較]")
        for name, d in diffs.items():
            ok = "✅" if d["status"] == "MATCH" else "❌"
            print(f"  {ok} {name}: {d['status']}")

    finished_at = datetime.now(timezone.utc)

    # レポート出力
    md = render_report(local_res, remote_res, diffs, started_at, finished_at, mode)
    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(md, encoding="utf-8")
    print(f"\nレポート出力: {OUTPUT_PATH}")

    # 終了コード判定
    if args.local:
        ok = all(local_res.get(q, {}).get("ok") for q in QUERIES)
        return 0 if ok else 2
    elif args.remote:
        ok = all(remote_res.get(q, {}).get("ok") for q in QUERIES)
        return 0 if ok else 2
    elif args.both:
        if not remote_res:
            return 0  # ローカル実行済、env 未設定で remote skip
        ok = all(d["status"] == "MATCH" for d in diffs.values())
        return 0 if ok else 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
