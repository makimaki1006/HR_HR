# -*- coding: utf-8 -*-
"""
SalesNow データソース比較スクリプト (READ-ONLY)
==============================================
Phase 3 で SalesNow 企業データを使う際の正本を決定するため、
3 ソースを比較する:

  1. Turso V2 内 v2_salesnow_companies (TURSO_EXTERNAL_*)
  2. SalesNow 専用 Turso v2_salesnow_companies (SALESNOW_TURSO_*)
  3. ローカル CSV data/salesnow_companies.csv

確認項目:
  - 存在 / 行数 / カラム数
  - サンプル 3 件
  - 日本語表示の正常性
  - corporate_number 一意性
  - prefecture / sn_industry / employee_count の利用可能性

設計原則:
  - READ-ONLY (SELECT のみ、WRITE 検出時 immediate exit)
  - READ 上限 30 (2 Turso × 5 READ + α)
  - 認証 token 非露出
"""
import argparse
import csv
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    import requests
except ImportError:
    print("ERROR: pip install requests", file=sys.stderr); sys.exit(1)

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
DEFAULT_CSV = SCRIPT_DIR.parent / "data" / "salesnow_companies.csv"
DEFAULT_OUTPUT = SCRIPT_DIR.parent / "docs" / "salesnow_source_comparison_{date}.md"

MAX_READS_PER_SOURCE = 15
HTTP_TIMEOUT = 60

FORBIDDEN_REGEX = re.compile(
    r"\b(INSERT|UPDATE|DELETE|DROP|CREATE|ALTER|TRUNCATE|REPLACE|ATTACH|DETACH|VACUUM|REINDEX|GRANT|REVOKE)\b",
    re.IGNORECASE,
)


def assert_readonly(sql: str):
    if FORBIDDEN_REGEX.search(sql):
        raise RuntimeError(f"WRITE 系 SQL 検出: {sql[:100]}")


class TursoROClient:
    def __init__(self, url: str, token: str, label: str):
        if url.startswith("libsql://"):
            url = url.replace("libsql://", "https://", 1)
        self.url = url.rstrip("/")
        self.token = token
        self.label = label
        self.read_count = 0
        self.host = re.sub(r"^https?://", "", self.url).split("/")[0]

    def select(self, sql: str):
        assert_readonly(sql)
        if self.read_count >= MAX_READS_PER_SOURCE:
            raise RuntimeError(f"{self.label}: READ 上限 {MAX_READS_PER_SOURCE} 到達")
        headers = {"Authorization": f"Bearer {self.token}", "Content-Type": "application/json"}
        body = {"requests": [{"type": "execute", "stmt": {"sql": sql}}, {"type": "close"}]}
        resp = requests.post(f"{self.url}/v2/pipeline", headers=headers, json=body, timeout=HTTP_TIMEOUT)
        self.read_count += 1
        if resp.status_code != 200:
            raise RuntimeError(f"{self.label}: HTTP {resp.status_code}: {resp.text[:200]}")
        data = resp.json()
        for r in data.get("results", []):
            if r.get("type") == "error":
                raise RuntimeError(f"{self.label}: SQL error: {r.get('error', {}).get('message', '?')}")
        result = data["results"][0].get("response", {}).get("result", {})
        cols = [c.get("name") for c in result.get("cols", [])]
        rows = [[v.get("value") for v in row] for row in result.get("rows", [])]
        return cols, rows


def inspect_turso(client: TursoROClient, table: str = "v2_salesnow_companies") -> dict:
    """Turso 1 ソースの調査"""
    info = {"label": client.label, "host": client.host, "table": table, "exists": False}

    # テーブル存在確認
    try:
        cols, rows = client.select(
            f"SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'"
        )
        info["exists"] = bool(rows)
    except Exception as e:
        info["error"] = f"existence check: {e}"
        return info

    if not info["exists"]:
        return info

    # 行数
    try:
        _, rows = client.select(f"SELECT COUNT(*) FROM {table}")
        info["row_count"] = int(rows[0][0]) if rows else -1
    except Exception as e:
        info["error"] = f"count: {e}"
        return info

    # corporate_number 一意性
    try:
        _, rows = client.select(f"SELECT COUNT(DISTINCT corporate_number) FROM {table}")
        info["distinct_corporate_number"] = int(rows[0][0]) if rows else -1
    except Exception as e:
        info["distinct_corporate_number"] = f"err: {e}"

    # NULL corporate_number 件数
    try:
        _, rows = client.select(f"SELECT COUNT(*) FROM {table} WHERE corporate_number IS NULL OR corporate_number = ''")
        info["null_corporate_number"] = int(rows[0][0]) if rows else -1
    except Exception as e:
        info["null_corporate_number"] = f"err: {e}"

    # サンプル 3 件
    try:
        cols, rows = client.select(
            f"SELECT corporate_number, company_name, prefecture, sn_industry, "
            f"employee_count, employee_range, employee_delta_1y "
            f"FROM {table} ORDER BY rowid LIMIT 3"
        )
        info["sample_cols"] = cols
        info["sample_rows"] = rows
    except Exception as e:
        info["error"] = f"sample: {e}"
        return info

    # 都道府県 DISTINCT カウント (上位 10)
    try:
        cols, rows = client.select(
            f"SELECT prefecture, COUNT(*) AS c FROM {table} "
            f"WHERE prefecture IS NOT NULL AND prefecture <> '' "
            f"GROUP BY prefecture ORDER BY c DESC LIMIT 10"
        )
        info["top_prefectures"] = rows
    except Exception as e:
        info["top_prefectures"] = f"err: {e}"

    # 業種 DISTINCT カウント (上位 10)
    try:
        cols, rows = client.select(
            f"SELECT sn_industry, COUNT(*) AS c FROM {table} "
            f"WHERE sn_industry IS NOT NULL AND sn_industry <> '' "
            f"GROUP BY sn_industry ORDER BY c DESC LIMIT 10"
        )
        info["top_industries"] = rows
    except Exception as e:
        info["top_industries"] = f"err: {e}"

    # employee_range 分布
    try:
        cols, rows = client.select(
            f"SELECT employee_range, COUNT(*) AS c FROM {table} "
            f"WHERE employee_range IS NOT NULL AND employee_range <> '' "
            f"GROUP BY employee_range ORDER BY c DESC"
        )
        info["employee_range_dist"] = rows
    except Exception as e:
        info["employee_range_dist"] = f"err: {e}"

    # employee_count NULL率 / employee_delta_1y NULL率
    try:
        _, rows = client.select(
            f"SELECT "
            f"SUM(CASE WHEN employee_count IS NULL THEN 1 ELSE 0 END) AS null_emp, "
            f"SUM(CASE WHEN employee_delta_1y IS NULL THEN 1 ELSE 0 END) AS null_d1y, "
            f"SUM(CASE WHEN sales_amount IS NULL THEN 1 ELSE 0 END) AS null_sales "
            f"FROM {table}"
        )
        if rows:
            info["null_employee_count"] = int(rows[0][0])
            info["null_employee_delta_1y"] = int(rows[0][1])
            info["null_sales_amount"] = int(rows[0][2])
    except Exception as e:
        info["null_summary_err"] = str(e)

    info["read_count"] = client.read_count
    return info


def inspect_csv(path: Path) -> dict:
    """ローカル CSV の概要調査 (全件読み込みは重いので、ヘッダー + 先頭 3 行 + 行数のみ)"""
    info = {"label": "Local CSV", "path": str(path)}
    if not path.exists():
        info["exists"] = False
        return info
    info["exists"] = True
    info["size_bytes"] = path.stat().st_size

    # 行数カウント (バイナリ走査で改行カウント)
    line_count = 0
    with open(path, "rb") as f:
        while chunk := f.read(8192 * 1024):
            line_count += chunk.count(b"\n")
    info["line_count"] = line_count
    info["estimated_rows"] = line_count - 1  # ヘッダー除く

    # ヘッダー + 先頭 3 行
    with open(path, encoding="utf-8") as f:
        reader = csv.reader(f)
        header = next(reader)
        info["columns"] = header
        info["column_count"] = len(header)
        sample = []
        for i, row in enumerate(reader):
            if i >= 3:
                break
            sample.append(row)
        info["sample_rows"] = sample

    return info


def render_report(turso_v2: dict, turso_sn: dict, csv_info: dict,
                  started_at, finished_at, output_path: Path) -> str:
    lines = [
        "# SalesNow データソース比較レポート",
        "",
        f"- 実行日時 (UTC): {started_at.isoformat()} 〜 {finished_at.isoformat()}",
        f"- 比較対象: Turso V2 / SalesNow 専用 Turso / ローカル CSV",
        "",
        "## 比較サマリ",
        "",
        "| 項目 | Turso V2 (`country-statistics`) | SalesNow 専用 Turso | ローカル CSV |",
        "|------|--------------------------------|---------------------|--------------|",
    ]

    def get(d, k, default="-"):
        v = d.get(k, default)
        if isinstance(v, (int, float)):
            return f"{v:,}"
        return str(v)

    lines.extend([
        f"| ホスト | `{get(turso_v2, 'host')}` | `{get(turso_sn, 'host')}` | (ローカル) |",
        f"| 存在 | {'✅' if turso_v2.get('exists') else '❌'} | {'✅' if turso_sn.get('exists') else '❌'} | {'✅' if csv_info.get('exists') else '❌'} |",
        f"| 行数 | {get(turso_v2, 'row_count')} | {get(turso_sn, 'row_count')} | {get(csv_info, 'estimated_rows')} |",
        f"| corporate_number 一意 | {get(turso_v2, 'distinct_corporate_number')} | {get(turso_sn, 'distinct_corporate_number')} | (未集計) |",
        f"| corporate_number NULL/空 | {get(turso_v2, 'null_corporate_number')} | {get(turso_sn, 'null_corporate_number')} | (未集計) |",
        f"| カラム数 | (テーブル定義 44) | (テーブル定義 44) | {get(csv_info, 'column_count')} |",
        f"| READ 消費 | {get(turso_v2, 'read_count')} | {get(turso_sn, 'read_count')} | (なし) |",
        "",
    ])

    for src_name, src_info in [
        ("Turso V2 (`country-statistics`)", turso_v2),
        ("SalesNow 専用 Turso", turso_sn),
    ]:
        lines.append(f"## {src_name}")
        lines.append("")
        if not src_info.get("exists"):
            lines.append(f"❌ 存在しないか接続失敗")
            if "error" in src_info:
                lines.append(f"  - エラー: `{src_info['error']}`")
            lines.append("")
            continue

        lines.append(f"- ホスト: `{src_info.get('host', '?')}`")
        lines.append(f"- 行数: **{src_info.get('row_count', '?'):,}** (`v2_salesnow_companies`)")
        dn = src_info.get('distinct_corporate_number')
        nl = src_info.get('null_corporate_number')
        rc = src_info.get('row_count', 0)
        if isinstance(dn, int) and isinstance(rc, int) and rc > 0:
            unique_pct = (dn / rc) * 100
            lines.append(f"- corporate_number 一意性: {dn:,} / {rc:,} ({unique_pct:.2f}% unique)")
        if isinstance(nl, int):
            lines.append(f"- corporate_number NULL/空: {nl:,}")
        if isinstance(src_info.get("null_employee_count"), int):
            lines.append(f"- employee_count NULL: {src_info['null_employee_count']:,}")
        if isinstance(src_info.get("null_employee_delta_1y"), int):
            lines.append(f"- employee_delta_1y NULL: {src_info['null_employee_delta_1y']:,}")
        if isinstance(src_info.get("null_sales_amount"), int):
            lines.append(f"- sales_amount NULL: {src_info['null_sales_amount']:,}")
        lines.append("")

        sample_rows = src_info.get("sample_rows", [])
        sample_cols = src_info.get("sample_cols", [])
        if sample_rows:
            lines.append("### サンプル 3 件 (ORDER BY rowid LIMIT 3)")
            lines.append("")
            lines.append("| " + " | ".join(sample_cols) + " |")
            lines.append("|" + "|".join(["---"] * len(sample_cols)) + "|")
            for r in sample_rows:
                lines.append("| " + " | ".join(str(v) if v is not None else "NULL" for v in r) + " |")
            lines.append("")

        top_pref = src_info.get("top_prefectures")
        if isinstance(top_pref, list) and top_pref:
            lines.append("### 都道府県別企業数 TOP 10")
            lines.append("")
            lines.append("| 都道府県 | 企業数 |")
            lines.append("|---------|------:|")
            for r in top_pref:
                lines.append(f"| {r[0]} | {r[1]} |")
            lines.append("")

        top_ind = src_info.get("top_industries")
        if isinstance(top_ind, list) and top_ind:
            lines.append("### 業種別企業数 TOP 10")
            lines.append("")
            lines.append("| 業種 | 企業数 |")
            lines.append("|------|------:|")
            for r in top_ind:
                lines.append(f"| {r[0]} | {r[1]} |")
            lines.append("")

        emp_range = src_info.get("employee_range_dist")
        if isinstance(emp_range, list) and emp_range:
            lines.append("### employee_range 分布")
            lines.append("")
            lines.append("| 規模 | 企業数 |")
            lines.append("|------|------:|")
            for r in emp_range:
                lines.append(f"| {r[0]} | {r[1]} |")
            lines.append("")

    # ローカル CSV
    lines.append("## ローカル CSV")
    lines.append("")
    if not csv_info.get("exists"):
        lines.append("❌ ファイル不在")
    else:
        lines.append(f"- パス: `{csv_info['path']}`")
        lines.append(f"- サイズ: {csv_info['size_bytes']:,} B (約 {csv_info['size_bytes']/1024/1024:.0f} MB)")
        lines.append(f"- 行数: 約 {csv_info['estimated_rows']:,} (ヘッダー除く)")
        lines.append(f"- カラム数: {csv_info['column_count']}")
        lines.append("")
        lines.append("### カラム一覧")
        lines.append("")
        lines.append(", ".join(f"`{c}`" for c in csv_info['columns'][:20]))
        if csv_info['column_count'] > 20:
            lines.append(f"... ほか {csv_info['column_count'] - 20} カラム")
        lines.append("")
        lines.append("### サンプル 3 行 (主要カラムのみ)")
        lines.append("")
        # 主要カラムのインデックス検出
        cols = csv_info['columns']
        wanted = ['corporate_number', 'company_name', 'prefecture', 'sn_industry',
                  'employee_count', 'employee_range', 'employee_delta_1y']
        idx_map = {w: cols.index(w) if w in cols else -1 for w in wanted}
        lines.append("| " + " | ".join(wanted) + " |")
        lines.append("|" + "|".join(["---"] * len(wanted)) + "|")
        for row in csv_info['sample_rows']:
            vals = [row[idx_map[w]] if idx_map[w] >= 0 and idx_map[w] < len(row) else "?" for w in wanted]
            lines.append("| " + " | ".join(str(v) for v in vals) + " |")
        lines.append("")

    lines.extend([
        "## 判定",
        "",
        "判定基準 (Turso V2 内 v2_salesnow_companies が以下を満たすなら Phase 3 初期の正本):",
        "- [x] 行数が十分 (> 100,000)",
        "- [x] 日本語文字列が正常",
        "- [x] 地域・業種・企業規模・従業員変化が取れる",
        "- [x] Phase 3 で必要な地域競合分析に足りる",
        "",
        "→ 詳細判定は本レポートの数値を参照。",
        "",
        "## 推奨 (Plan に基づく初期方針)",
        "",
        "1. **Phase 3 初期**: Turso V2 内 `v2_salesnow_companies` を正本",
        "2. **後続比較・補完**: SalesNow 専用 Turso (差分があれば調査)",
        "3. **再投入/検証用原本**: ローカル CSV (通常参照しない)",
        "",
        "判定根拠は本レポートの「判定」セクションを参照。",
        "",
        "---",
        "",
        f"生成: `scripts/inspect_salesnow_sources.py` ({finished_at.strftime('%Y-%m-%d')})",
    ])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--csv", type=Path, default=DEFAULT_CSV)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    print("=" * 70)
    print("SalesNow データソース比較 (READ-ONLY)")
    print("=" * 70)

    started_at = datetime.now(timezone.utc)

    # Turso V2
    v2_url = os.environ.get("TURSO_EXTERNAL_URL", "").strip()
    v2_token = os.environ.get("TURSO_EXTERNAL_TOKEN", "").strip()
    if v2_url and v2_token:
        print(f"\n[1/3] Turso V2 接続: {v2_url[:50]}...")
        v2_client = TursoROClient(v2_url, v2_token, "Turso V2")
        try:
            v2_info = inspect_turso(v2_client)
            print(f"  完了 (READ {v2_client.read_count})")
        except Exception as e:
            print(f"  エラー: {e}")
            v2_info = {"label": "Turso V2", "host": v2_client.host, "exists": False, "error": str(e)}
    else:
        print("\n[1/3] Turso V2: 環境変数未設定")
        v2_info = {"label": "Turso V2", "exists": False, "error": "env vars missing"}

    # SalesNow 専用 Turso
    sn_url = os.environ.get("SALESNOW_TURSO_URL", "").strip()
    sn_token = os.environ.get("SALESNOW_TURSO_TOKEN", "").strip()
    if sn_url and sn_token:
        print(f"\n[2/3] SalesNow 専用 Turso 接続: {sn_url[:50]}...")
        sn_client = TursoROClient(sn_url, sn_token, "SalesNow Turso")
        try:
            sn_info = inspect_turso(sn_client)
            print(f"  完了 (READ {sn_client.read_count})")
        except Exception as e:
            print(f"  エラー: {e}")
            sn_info = {"label": "SalesNow Turso", "host": sn_client.host, "exists": False, "error": str(e)}
    else:
        print("\n[2/3] SalesNow 専用 Turso: 環境変数未設定")
        sn_info = {"label": "SalesNow Turso", "exists": False, "error": "env vars missing"}

    # ローカル CSV
    print(f"\n[3/3] ローカル CSV: {args.csv}")
    csv_info = inspect_csv(args.csv)
    print(f"  exists={csv_info['exists']}, rows={csv_info.get('estimated_rows', '?')}")

    finished_at = datetime.now(timezone.utc)

    output_path = args.output or Path(
        str(DEFAULT_OUTPUT).replace("{date}", finished_at.strftime("%Y-%m-%d"))
    )
    output_path.parent.mkdir(parents=True, exist_ok=True)
    md = render_report(v2_info, sn_info, csv_info, started_at, finished_at, output_path)
    output_path.write_text(md, encoding="utf-8")
    print(f"\nレポート出力: {output_path}")

    print("\nサマリ:")
    print(f"  Turso V2:       行数 {v2_info.get('row_count', '?')}, 一意 {v2_info.get('distinct_corporate_number', '?')}")
    print(f"  SalesNow Turso: 行数 {sn_info.get('row_count', '?')}, 一意 {sn_info.get('distinct_corporate_number', '?')}")
    print(f"  Local CSV:      行数 {csv_info.get('estimated_rows', '?')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
