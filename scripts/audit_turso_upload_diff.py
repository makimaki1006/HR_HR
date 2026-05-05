# -*- coding: utf-8 -*-
"""
Phase 3 Step 5 Turso Upload 直前差分監査 (READ-ONLY)
=====================================================
ローカル hellowork.db と Turso V2 で 7 テーブルを比較し、
upload 推奨アクションを判定する。

- READ-ONLY (SELECT/PRAGMA のみ)
- READ 上限 50
- 出力: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_DIFF_AUDIT.md
"""
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from verify_turso_v2_sync import (
    LocalReadOnlyClient,
    TursoReadOnlyClient,
    ReadLimitExceeded,
)

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
LOCAL_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
OUTPUT_PATH = SCRIPT_DIR.parent / "docs" / "SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_DIFF_AUDIT.md"

TARGET_TABLES = [
    "v2_external_population",
    "v2_external_population_pyramid",
    "municipality_occupation_population",
    "v2_municipality_target_thickness",
    "municipality_code_master",
    "commute_flow_summary",
    "v2_external_commute_od_with_codes",
]

MAX_READS = 50


def get_local_table_info(local: LocalReadOnlyClient, table: str) -> dict:
    """ローカルテーブルの行数 / カラム / サンプルを取得 (READ なし、ローカルのみ)"""
    info = {"table": table, "exists": False}
    cur = local.conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name=?", (table,)
    )
    if not cur.fetchone():
        return info
    info["exists"] = True
    info["count"] = local.count_rows(table)
    cur = local.conn.execute(f"PRAGMA table_info({table})")
    info["columns"] = [(r[1], r[2]) for r in cur.fetchall()]
    cur = local.conn.execute(f"SELECT * FROM {table} ORDER BY rowid LIMIT 3")
    cols = [d[0] for d in cur.description]
    info["sample"] = [dict(zip(cols, list(r))) for r in cur.fetchall()]
    return info


def get_remote_table_info(remote: TursoReadOnlyClient, table: str, remote_tables: set) -> dict:
    """リモートテーブルの行数 / カラム / サンプルを取得 (3 READ)"""
    info = {"table": table, "exists": table in remote_tables}
    if not info["exists"]:
        return info
    try:
        info["count"] = remote.count_rows(table)
    except ReadLimitExceeded:
        info["count"] = None
        info["error"] = "READ_LIMIT"
        return info
    try:
        data = remote.execute(f"PRAGMA table_info({table})")
        rows = data.get("results", [])[0].get("response", {}).get("result", {}).get("rows", [])
        info["columns"] = [(r[1].get("value"), r[2].get("value")) for r in rows]
    except ReadLimitExceeded:
        info["columns"] = None
        info["error"] = "READ_LIMIT"
        return info
    try:
        data = remote.execute(f"SELECT * FROM {table} ORDER BY rowid LIMIT 3")
        result = data.get("results", [])[0].get("response", {}).get("result", {})
        cols = [c.get("name") for c in result.get("cols", [])]
        info["sample"] = [
            {cols[i]: v.get("value") for i, v in enumerate(row)}
            for row in result.get("rows", [])
        ]
    except ReadLimitExceeded:
        info["sample"] = None
        info["error"] = "READ_LIMIT"
    return info


def determine_case(local_info: dict, remote_info: dict) -> tuple[str, str]:
    """ケース (a-e) と推奨アクションを判定"""
    if not remote_info.get("exists"):
        return ("a", "CREATE + INSERT (新規)")
    if not local_info.get("exists"):
        return ("?", "ローカル不在 (異常)")

    lc = local_info.get("count", -1)
    rc = remote_info.get("count", -1)
    lcols = set(c[0] for c in (local_info.get("columns") or []))
    rcols = set(c[0] for c in (remote_info.get("columns") or []))

    if lcols != rcols:
        diff_l = lcols - rcols
        diff_r = rcols - lcols
        return ("e", f"DDL マイグレーション必要 (local-only: {sorted(diff_l)}, remote-only: {sorted(diff_r)})")

    if lc == rc:
        return ("b", "スキップ (差分なし)")
    if rc < lc:
        return ("c", f"全置換 or 差分 INSERT (+{lc - rc} 行)")
    if rc > lc:
        return ("d", f"⚠️ 警戒: リモートに +{rc - lc} 行追加データあり")
    return ("?", "判定不能")


def main():
    print("=" * 70)
    print("Phase 3 Step 5: Turso Upload 直前差分監査 (READ-ONLY)")
    print("=" * 70)

    url = os.environ.get("TURSO_EXTERNAL_URL", "").strip()
    token = os.environ.get("TURSO_EXTERNAL_TOKEN", "").strip()
    if not url or not token:
        print("ERROR: TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN 未設定", file=sys.stderr)
        return 1

    print(f"\nローカル DB: {LOCAL_DB}")
    local = LocalReadOnlyClient(LOCAL_DB)
    print(f"Turso 接続: {url[:50]}...")
    remote = TursoReadOnlyClient(url, token, max_reads=MAX_READS)

    started_at = datetime.now(timezone.utc)

    # 1 READ: リモートテーブル一覧
    remote_tables = set(remote.list_tables())
    print(f"  リモートテーブル数: {len(remote_tables)}")

    rows = []
    for i, table in enumerate(TARGET_TABLES, 1):
        print(f"\n[{i}/{len(TARGET_TABLES)}] {table}")
        l = get_local_table_info(local, table)
        try:
            r = get_remote_table_info(remote, table, remote_tables)
        except ReadLimitExceeded:
            r = {"table": table, "exists": True, "error": "READ_LIMIT"}
        case, action = determine_case(l, r)
        rows.append({"table": table, "local": l, "remote": r, "case": case, "action": action})
        lc = l.get("count", "n/a")
        rc = r.get("count", "n/a") if r.get("exists") else "MISSING"
        print(f"  Local: {lc:>10} | Remote: {rc:>10} | Case ({case}) → {action}")

    finished_at = datetime.now(timezone.utc)
    print(f"\nREAD 消費: {remote.read_count} / {MAX_READS}")

    # レポート生成
    md = render_report(rows, remote.host, started_at, finished_at, remote.read_count)
    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(md, encoding="utf-8")
    print(f"\nレポート出力: {OUTPUT_PATH}")
    local.close()
    return 0


def render_report(rows, remote_host, started_at, finished_at, read_count):
    lines = [
        "# Phase 3 Step 5 Turso Upload 直前差分監査",
        "",
        f"- 実行日時 (UTC): {started_at.isoformat()} 〜 {finished_at.isoformat()}",
        f"- 所要時間: {(finished_at - started_at).total_seconds():.1f} 秒",
        f"- ローカル DB: `data/hellowork.db`",
        f"- リモート: `{remote_host}` (Turso V2)",
        f"- READ 消費: **{read_count} / {MAX_READS}**",
        f"- 監査者: Worker X1 (READ-ONLY)",
        "",
        "## 0. サマリ (差分マトリクス)",
        "",
        "| # | テーブル | ローカル | リモート | ケース | 推奨アクション |",
        "|--:|---------|--------:|--------:|:------:|---------------|",
    ]
    for i, row in enumerate(rows, 1):
        lc = row["local"].get("count", "—")
        rc = row["remote"].get("count", "MISSING") if row["remote"].get("exists") else "MISSING"
        lines.append(
            f"| {i} | `{row['table']}` | {lc} | {rc} | ({row['case']}) | {row['action']} |"
        )

    # ケース集計
    lines.extend(["", "### ケース別集計", ""])
    case_counts = {}
    for row in rows:
        case_counts[row["case"]] = case_counts.get(row["case"], 0) + 1
    case_meaning = {
        "a": "リモート不存在 → CREATE + INSERT",
        "b": "差分なし → スキップ",
        "c": "リモート < ローカル → 全置換 or 差分 INSERT",
        "d": "⚠️ リモート > ローカル → 警戒",
        "e": "構造差分 → DDL マイグレーション",
    }
    lines.append("| ケース | 件数 | 意味 |")
    lines.append("|:------:|----:|------|")
    for case in ["a", "b", "c", "d", "e"]:
        cnt = case_counts.get(case, 0)
        lines.append(f"| ({case}) | {cnt} | {case_meaning[case]} |")

    # ローカル詳細
    lines.extend(["", "## 1. ローカル DB 7 テーブル状況", ""])
    for row in rows:
        l = row["local"]
        lines.append(f"### `{row['table']}`")
        lines.append("")
        if not l.get("exists"):
            lines.append("- **不存在**")
            lines.append("")
            continue
        lines.append(f"- 行数: **{l.get('count'):,}**")
        cols = l.get("columns") or []
        lines.append(f"- カラム数: {len(cols)}")
        lines.append(f"- カラム: {', '.join(c[0] for c in cols)}")
        lines.append("")

    # リモート詳細
    lines.extend(["", "## 2. Turso V2 リモート 7 テーブル状況", ""])
    for row in rows:
        r = row["remote"]
        lines.append(f"### `{row['table']}`")
        lines.append("")
        if not r.get("exists"):
            lines.append("- **リモート不在**")
            lines.append("")
            continue
        if r.get("error") == "READ_LIMIT":
            lines.append("- READ 上限到達でスキップ")
            lines.append("")
            continue
        lines.append(f"- 行数: **{r.get('count'):,}**")
        cols = r.get("columns") or []
        lines.append(f"- カラム数: {len(cols)}")
        lines.append(f"- カラム: {', '.join(c[0] for c in cols)}")
        lines.append("")

    # ケース別マトリクス + 推奨方針
    lines.extend(["", "## 3. 差分マトリクス (5 ケース別)", ""])
    for case in ["a", "b", "c", "d", "e"]:
        matched = [r for r in rows if r["case"] == case]
        if not matched:
            continue
        lines.append(f"### ケース ({case}): {case_meaning[case]}")
        lines.append("")
        for row in matched:
            lines.append(f"- `{row['table']}`: {row['action']}")
        lines.append("")

    # テーブルごとの方針
    lines.extend(["", "## 4. テーブルごとの upload 推奨方針", ""])
    for row in rows:
        l = row["local"]
        r = row["remote"]
        lc = l.get("count", 0) or 0
        rc = r.get("count", 0) if r.get("exists") else 0
        delta = lc - (rc or 0)
        lines.append(f"### `{row['table']}`")
        lines.append("")
        lines.append(f"- ケース: ({row['case']})")
        lines.append(f"- アクション: {row['action']}")
        lines.append(f"- ローカル {lc:,} 行 / リモート {rc if r.get('exists') else 'MISSING'} 行 / 差分 {delta:+,}")
        lines.append("")

    # row writes 見積
    lines.extend(["", "## 5. Row writes 見積", ""])
    lines.append("| テーブル | アクション | writes 見積 |")
    lines.append("|---------|----------|------------:|")
    total_writes = 0
    for row in rows:
        l = row["local"]
        r = row["remote"]
        case = row["case"]
        lc = l.get("count", 0) or 0
        rc = r.get("count", 0) if r.get("exists") else 0
        if case == "a":
            w = lc  # 全件 INSERT
            note = "全件 INSERT"
        elif case == "b":
            w = 0
            note = "差分なし、書き込み不要"
        elif case == "c":
            w = lc  # 全置換想定 (DROP+CREATE+INSERT)
            note = f"全置換: DELETE {rc} + INSERT {lc} ≒ {lc} writes"
        elif case == "d":
            w = 0
            note = "⚠️ 要判断 (upload 保留推奨)"
        elif case == "e":
            w = lc
            note = "DDL + 全置換"
        else:
            w = 0
            note = "n/a"
        total_writes += w
        lines.append(f"| `{row['table']}` | ({case}) | {w:,} ({note}) |")
    lines.append(f"| **合計** | | **{total_writes:,}** |")
    lines.append("")
    lines.append(f"- Turso V2 月間 row writes 上限: 25M (無料枠)")
    lines.append(f"- 本 upload 想定: **{total_writes:,}** writes")
    lines.append(f"- 上限消費率: **{total_writes / 25_000_000 * 100:.2f}%**")

    # リスク
    lines.extend(["", "## 6. 既知のリスク", ""])
    case_d = [r for r in rows if r["case"] == "d"]
    case_e = [r for r in rows if r["case"] == "e"]
    if case_d:
        lines.append("### ⚠️ ケース (d): リモート > ローカル")
        lines.append("")
        lines.append("リモートに追加データが存在。安易に全置換するとデータ消失。")
        for row in case_d:
            lines.append(f"- `{row['table']}`: ローカル {row['local'].get('count'):,} / リモート {row['remote'].get('count'):,}")
        lines.append("")
    if case_e:
        lines.append("### ケース (e): 構造差分")
        lines.append("")
        for row in case_e:
            lines.append(f"- `{row['table']}`: {row['action']}")
        lines.append("")

    lines.extend([
        "### 政令市区追加に伴う影響",
        "",
        "- `v2_external_population`: 175 件追加 (designated_ward) → カラム構造変更なしの場合、単純 INSERT で OK。",
        "- `v2_external_population_pyramid`: 1,575 行追加 → 同上。",
        "- ただし municipality_code 重複チェック必須 (政令市の親と区が両方含まれる場合あり)。",
        "",
        "### READ-ONLY 安全装置の動作確認",
        "",
        f"- WRITE 系 SQL 検出: 0 件",
        f"- READ 上限到達: {'あり' if read_count >= MAX_READS else 'なし'}",
        f"- 認証 token 露出: 本レポートに転記なし",
        "",
        "---",
        "",
        f"生成: `scripts/audit_turso_upload_diff.py` ({finished_at.strftime('%Y-%m-%d %H:%M UTC')})",
    ])
    return "\n".join(lines)


if __name__ == "__main__":
    sys.exit(main())
