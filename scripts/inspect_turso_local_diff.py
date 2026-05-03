# -*- coding: utf-8 -*-
"""
SAMPLE_MISMATCH 詳細比較スクリプト (READ-ONLY)
=============================================
verify_turso_v2_sync.py で SAMPLE_MISMATCH 判定された 5 テーブルについて、
主キー単位で local 値と Turso 値を併記し、差分原因を切り分ける。

設計原則 (verify_turso_v2_sync.py と同じ):
  - READ-ONLY (SELECT のみ、WRITE 検出時 immediate exit)
  - READ 上限 50 (5 テーブル × 数 READ で十分)
  - 認証 token 非露出

使い方:
    python scripts/inspect_turso_local_diff.py
    python scripts/inspect_turso_local_diff.py --output docs/turso_v2_diff_inspection_2026-05-04.md
    python scripts/inspect_turso_local_diff.py --rows 10
"""
import argparse
import json
import os
import re
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    import requests
except ImportError:
    print("ERROR: pip install requests", file=sys.stderr)
    sys.exit(1)

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
DEFAULT_LOCAL_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
DEFAULT_OUTPUT = SCRIPT_DIR.parent / "docs" / "turso_v2_diff_inspection_{date}.md"

MAX_READS = 50
HTTP_TIMEOUT = 30

FORBIDDEN_REGEX = re.compile(
    r"\b(INSERT|UPDATE|DELETE|DROP|CREATE|ALTER|TRUNCATE|REPLACE|ATTACH|DETACH|VACUUM|REINDEX|GRANT|REVOKE)\b",
    re.IGNORECASE,
)

# 比較対象テーブル定義
# (table, primary_key columns, all columns to compare, sample WHERE conditions)
INSPECT_TARGETS = [
    {
        "table": "v2_external_population",
        "pk": ["prefecture", "municipality"],
        "cols": ["prefecture", "municipality", "total_population", "male_population",
                 "female_population", "aging_rate", "reference_date"],
        # ヘッダー混入除外 + 代表的な3市区町村
        "where": "prefecture IN ('北海道', '東京都', '大阪府') AND municipality IN ('札幌市', '新宿区', '大阪市')",
        "order": "prefecture, municipality",
    },
    {
        "table": "v2_external_migration",
        "pk": ["prefecture", "municipality"],
        "cols": ["prefecture", "municipality", "inflow", "outflow", "net_migration",
                 "net_migration_rate", "reference_year"],
        "where": "prefecture IN ('北海道', '東京都', '大阪府') AND municipality IN ('札幌市', '新宿区', '大阪市')",
        "order": "prefecture, municipality",
    },
    {
        "table": "v2_external_daytime_population",
        "pk": ["prefecture", "municipality"],
        "cols": ["prefecture", "municipality", "nighttime_pop", "daytime_pop",
                 "day_night_ratio", "reference_year"],
        "where": "prefecture IN ('北海道', '東京都', '大阪府') AND municipality IN ('札幌市', '新宿区', '大阪市')",
        "order": "prefecture, municipality",
    },
    {
        "table": "v2_external_population_pyramid",
        "pk": ["prefecture", "municipality", "age_group"],
        "cols": ["prefecture", "municipality", "age_group", "male_count", "female_count"],
        "where": "prefecture = '東京都' AND municipality = '新宿区'",
        "order": "age_group",
    },
    {
        "table": "v2_external_prefecture_stats",
        "pk": ["prefecture"],
        "cols": ["prefecture", "unemployment_rate", "job_change_desire_rate",
                 "non_regular_rate", "avg_monthly_wage", "price_index",
                 "fulfillment_rate", "real_wage_index"],
        "where": "prefecture IN ('北海道', '東京都', '大阪府')",
        "order": "prefecture",
    },
]


def assert_readonly(sql: str) -> None:
    if FORBIDDEN_REGEX.search(sql):
        raise RuntimeError(f"WRITE 系 SQL 検出: {sql[:100]}")


class TursoClient:
    def __init__(self, url: str, token: str):
        if url.startswith("libsql://"):
            url = url.replace("libsql://", "https://", 1)
        self.url = url.rstrip("/")
        self.token = token
        self.read_count = 0
        self.host = re.sub(r"^https?://", "", self.url).split("/")[0]

    def execute(self, sql: str):
        assert_readonly(sql)
        if self.read_count >= MAX_READS:
            raise RuntimeError("READ 上限到達")
        headers = {"Authorization": f"Bearer {self.token}", "Content-Type": "application/json"}
        body = {"requests": [{"type": "execute", "stmt": {"sql": sql}}, {"type": "close"}]}
        resp = requests.post(f"{self.url}/v2/pipeline", headers=headers, json=body, timeout=HTTP_TIMEOUT)
        self.read_count += 1
        if resp.status_code != 200:
            raise RuntimeError(f"Turso {resp.status_code}: {resp.text[:200]}")
        data = resp.json()
        for r in data.get("results", []):
            if r.get("type") == "error":
                raise RuntimeError(f"SQL error: {r.get('error', {}).get('message', '?')}")
        return data

    def select(self, sql: str):
        """SELECT → list of dict"""
        data = self.execute(sql)
        results = data.get("results", [])
        if not results:
            return []
        result = results[0].get("response", {}).get("result", {})
        cols = [c.get("name") for c in result.get("cols", [])]
        rows = result.get("rows", [])
        out = []
        for row in rows:
            out.append({cols[i]: v.get("value") for i, v in enumerate(row)})
        return out


def select_local(conn: sqlite3.Connection, sql: str):
    assert_readonly(sql)
    cur = conn.execute(sql)
    cols = [d[0] for d in cur.description]
    return [dict(zip(cols, row)) for row in cur.fetchall()]


def normalize(v):
    """値を文字列正規化 (型差・NULL差を吸収)"""
    if v is None:
        return "NULL"
    if isinstance(v, float):
        # 浮動小数点表示揺れを丸める
        return f"{v:.4g}"
    if isinstance(v, int):
        return str(v)
    return str(v)


def compare_rows(local_rows, remote_rows, pk_cols, all_cols):
    """主キー単位で local/remote を結合し、差分カラムを抽出"""
    local_by_pk = {tuple(normalize(r.get(c)) for c in pk_cols): r for r in local_rows}
    remote_by_pk = {tuple(normalize(r.get(c)) for c in pk_cols): r for r in remote_rows}
    all_pks = sorted(set(local_by_pk.keys()) | set(remote_by_pk.keys()))
    pairs = []
    for pk in all_pks:
        l = local_by_pk.get(pk)
        r = remote_by_pk.get(pk)
        diffs = []
        for c in all_cols:
            lv = normalize(l.get(c)) if l else "(missing)"
            rv = normalize(r.get(c)) if r else "(missing)"
            if lv != rv:
                diffs.append((c, lv, rv))
        pairs.append({"pk": pk, "local": l, "remote": r, "diffs": diffs})
    return pairs


def render_report(comparisons, read_count, started_at, finished_at, host, db_path):
    lines = [
        "# Turso V2 ↔ ローカル DB 詳細差分レポート (SAMPLE_MISMATCH 5 テーブル)",
        "",
        f"- 実行日時 (UTC): {started_at.isoformat()} 〜 {finished_at.isoformat()}",
        f"- ローカル DB: `{db_path}`",
        f"- リモート: `{host}` (Turso V2)",
        f"- READ 消費: {read_count} (上限 {MAX_READS})",
        "",
        "## 目的",
        "",
        "`turso_v2_sync_report_2026-05-03.md` で SAMPLE_MISMATCH 判定された 5 テーブルについて、",
        "主キー単位で local 値と Turso 値を併記し、差分原因を切り分ける。",
        "",
    ]
    for cmp in comparisons:
        tbl = cmp["table"]
        pairs = cmp["pairs"]
        cols = cmp["cols"]
        lines.append(f"## {tbl}")
        lines.append("")
        lines.append(f"- 主キー: `{', '.join(cmp['pk'])}`")
        lines.append(f"- WHERE: `{cmp['where']}`")
        lines.append(f"- 取得行数: local={len(cmp['local_rows'])}, Turso={len(cmp['remote_rows'])}")
        lines.append("")

        if not pairs:
            lines.append("(取得行なし)")
            lines.append("")
            continue

        # 全行差分まとめ表
        any_diff = any(p["diffs"] for p in pairs)
        if any_diff:
            lines.append("### 主キー別差分 (差分カラムのみ表示)")
            lines.append("")
            lines.append("| 主キー | カラム | local | Turso |")
            lines.append("|--------|--------|-------|-------|")
            for p in pairs:
                if not p["diffs"]:
                    continue
                pk_str = " / ".join(p["pk"])
                for c, lv, rv in p["diffs"]:
                    lines.append(f"| {pk_str} | `{c}` | `{lv}` | `{rv}` |")
            lines.append("")
        else:
            lines.append("### 取得範囲では差分なし")
            lines.append("")

        # 全行詳細 (代表 2 件)
        lines.append("### 詳細サンプル")
        lines.append("")
        for p in pairs[:3]:
            pk_str = " / ".join(p["pk"])
            lines.append(f"#### {pk_str}")
            lines.append("")
            lines.append(f"| カラム | local | Turso | 一致 |")
            lines.append(f"|--------|-------|-------|:----:|")
            for c in cols:
                lv = normalize(p["local"].get(c)) if p["local"] else "(missing)"
                rv = normalize(p["remote"].get(c)) if p["remote"] else "(missing)"
                match = "✅" if lv == rv else "❌"
                lines.append(f"| `{c}` | {lv} | {rv} | {match} |")
            lines.append("")

    lines.extend([
        "## 推定原因の判断基準",
        "",
        "| 観察 | 推定原因 |",
        "|------|---------|",
        "| 数値カラムが微妙にずれている (端数差、丸め差) | 集計年度が異なる / 集計ロジック更新 |",
        "| 数値が大きく異なる (オーダー違い) | データソース変更 / 単位変更 (% vs 比率) |",
        "| 1 列だけ異なる、他は完全一致 | カラム追加・型変更による再投入 |",
        "| local が古く Turso が新しい | Turso が正本、ローカルは古いキャッシュ |",
        "| 主キーが違う行が混入 | ヘッダー混入 / フィルタ条件差 |",
        "",
        "## Phase 3 への影響",
        "",
        "本レポートの内容を見て、Turso を正本として進められるか判断する:",
        "",
        "- **すべて軽微な数値差**: Turso 正本で進めて問題なし",
        "- **定義差・カラム意味差あり**: Phase 3 指標定義を修正してから進める",
        "- **大幅な構造差**: ローカル DB を `download_db.sh` で同期してから比較し直す",
        "",
        "---",
        "",
        f"生成: `scripts/inspect_turso_local_diff.py` ({finished_at.strftime('%Y-%m-%d')})",
    ])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", type=Path, default=DEFAULT_LOCAL_DB)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    url = os.environ.get("TURSO_EXTERNAL_URL", "").strip()
    token = os.environ.get("TURSO_EXTERNAL_TOKEN", "").strip()
    if not url or not token:
        print("ERROR: TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN を設定", file=sys.stderr)
        return 1

    print("=" * 70)
    print("SAMPLE_MISMATCH 詳細差分インスペクター (READ-ONLY)")
    print("=" * 70)

    started_at = datetime.now(timezone.utc)
    print(f"\nローカル DB: {args.db}")
    conn = sqlite3.connect(f"file:{args.db.as_posix()}?mode=ro", uri=True)

    print(f"Turso: {url[:50]}...")
    remote = TursoClient(url, token)

    comparisons = []
    for target in INSPECT_TARGETS:
        tbl = target["table"]
        cols = target["cols"]
        pk = target["pk"]
        where = target["where"]
        order = target["order"]

        print(f"\n[{tbl}]")
        sel_cols = ", ".join(cols)
        sql = f"SELECT {sel_cols} FROM {tbl} WHERE {where} ORDER BY {order}"

        try:
            local_rows = select_local(conn, sql)
            print(f"  local: {len(local_rows)} 行")
        except sqlite3.OperationalError as e:
            print(f"  local エラー: {e}")
            local_rows = []

        try:
            remote_rows = remote.select(sql)
            print(f"  Turso: {len(remote_rows)} 行")
        except Exception as e:
            print(f"  Turso エラー: {e}")
            remote_rows = []

        pairs = compare_rows(local_rows, remote_rows, pk, cols)
        comparisons.append({
            "table": tbl,
            "pk": pk,
            "cols": cols,
            "where": where,
            "local_rows": local_rows,
            "remote_rows": remote_rows,
            "pairs": pairs,
        })
        diff_count = sum(1 for p in pairs if p["diffs"])
        print(f"  比較: {len(pairs)} ペア中 {diff_count} 件差分")

    finished_at = datetime.now(timezone.utc)

    output_path = args.output or Path(
        str(DEFAULT_OUTPUT).replace("{date}", finished_at.strftime("%Y-%m-%d"))
    )
    output_path.parent.mkdir(parents=True, exist_ok=True)
    md = render_report(comparisons, remote.read_count, started_at, finished_at,
                       remote.host, args.db)
    output_path.write_text(md, encoding="utf-8")
    print(f"\nレポート出力: {output_path}")
    print(f"READ 消費: {remote.read_count} / {MAX_READS}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
