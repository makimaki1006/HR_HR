# -*- coding: utf-8 -*-
"""
Turso V2 ←→ ローカル hellowork.db 同期検証スクリプト (READ-ONLY)
===============================================================
Phase 3 着手前に、ローカル `data/hellowork.db` と本番 Turso V2
(`country-statistics-makimaki1006.aws-ap-northeast-1.turso.io`) の
v2_external_* テーブルが同期されているかを検証する。

設計原則:
  - **READ-ONLY**: SELECT / PRAGMA のみ。INSERT/UPDATE/DELETE/DROP/CREATE は禁止。
  - **READ 上限**: 100 で abort (無料枠 300/月の 1/3、安全マージン)。
  - **トークン非露出**: 認証 token は標準出力にも生成レポートにも転記しない。
  - **WRITE 検出**: SQL 文字列を allowlist チェック → 違反時 immediate exit。

使い方:
    # 接続確認 + READ 試算のみ (Turso 接続せず)
    python scripts/verify_turso_v2_sync.py --dry-run

    # 本番実行 (READ 約 90、レポート出力)
    python scripts/verify_turso_v2_sync.py

    # 出力先指定
    python scripts/verify_turso_v2_sync.py --output docs/turso_v2_sync_report_2026-05-04.md

環境変数:
    TURSO_EXTERNAL_URL     例: libsql://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io
    TURSO_EXTERNAL_TOKEN   Bearer token

対象: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_VERIFY.md 参照
作成: 2026-05-04
"""
import argparse
import hashlib
import json
import os
import re
import sqlite3
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

try:
    import requests
except ImportError:
    print("ERROR: requests が必要です。`pip install requests`", file=sys.stderr)
    sys.exit(1)

# Windows コンソール (cp932) でも絵文字を出力できるように UTF-8 化
try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

# ──────────────────────────────────────────────
# 設定
# ──────────────────────────────────────────────
SCRIPT_DIR = Path(__file__).parent
DEFAULT_LOCAL_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
DEFAULT_OUTPUT = SCRIPT_DIR.parent / "docs" / "turso_v2_sync_report_{date}.md"

# 比較対象テーブル (upload_to_turso.py の TABLES + 拡張テーブル)
# Phase 3 実装で参照する v2_external_* と SalesNow を中心に
TARGET_TABLES = [
    # 基本 14 テーブル (upload_to_turso.py L25-46)
    "v2_external_population",
    "v2_external_migration",
    "v2_external_foreign_residents",
    "v2_external_daytime_population",
    "v2_external_population_pyramid",
    "v2_external_prefecture_stats",
    "v2_external_job_openings_ratio",
    "v2_external_labor_stats",
    "v2_external_establishments",
    "v2_external_turnover",
    "v2_external_household_spending",
    "v2_external_business_dynamics",
    "v2_external_climate",
    "v2_external_care_demand",
    # 時系列 6 テーブル
    "ts_turso_counts",
    "ts_turso_vacancy",
    "ts_turso_salary",
    "ts_turso_fulfillment",
    "ts_agg_workstyle",
    "ts_agg_tracking",
    # Phase 3 で追加投入予定の拡張テーブル
    "v2_external_industry_structure",
    "v2_external_land_price",
    "v2_external_minimum_wage",
    "v2_external_commute_od",
    "v2_external_education",
    "v2_external_education_facilities",
    "v2_external_households",
    "v2_external_household",
    "v2_external_internet_usage",
    "v2_external_car_ownership",
    "v2_external_geography",
    "v2_external_social_life",
    "v2_external_vital_statistics",
    "v2_external_labor_force",
    "v2_external_medical_welfare",
    "v2_external_boj_tankan",
    "v2_external_minimum_wage_history",
]

# READ 上限 (安全装置)
MAX_READS = 100

# 1 リクエストあたりのタイムアウト
HTTP_TIMEOUT = 30

# WRITE 系 SQL の検出パターン (大文字小文字無視)
FORBIDDEN_SQL_PATTERNS = [
    r"\bINSERT\b",
    r"\bUPDATE\b",
    r"\bDELETE\b",
    r"\bDROP\b",
    r"\bCREATE\b",
    r"\bALTER\b",
    r"\bTRUNCATE\b",
    r"\bREPLACE\b",
    r"\bATTACH\b",
    r"\bDETACH\b",
    r"\bVACUUM\b",
    r"\bREINDEX\b",
    r"\bGRANT\b",
    r"\bREVOKE\b",
]

FORBIDDEN_REGEX = re.compile("|".join(FORBIDDEN_SQL_PATTERNS), re.IGNORECASE)


# ──────────────────────────────────────────────
# 安全装置
# ──────────────────────────────────────────────
class ReadOnlyViolation(Exception):
    """WRITE 系 SQL 検出時に投げる"""
    pass


class ReadLimitExceeded(Exception):
    """READ 上限超過時に投げる"""
    pass


def assert_readonly(sql: str) -> None:
    """SQL に WRITE 系キーワードが含まれていないか確認。
    違反時は ReadOnlyViolation を raise → スクリプト終了。
    """
    if FORBIDDEN_REGEX.search(sql):
        raise ReadOnlyViolation(
            f"WRITE 系 SQL を検出: {sql[:100]}... 本スクリプトは READ-ONLY です。"
        )


# ──────────────────────────────────────────────
# Turso HTTP クライアント (read-only)
# ──────────────────────────────────────────────
class TursoReadOnlyClient:
    """Turso v2/pipeline API の SELECT 専用クライアント。
    READ カウンタと WRITE 検出を内蔵。
    """

    def __init__(self, url: str, token: str, max_reads: int = MAX_READS):
        # libsql:// → https:// 変換
        if url.startswith("libsql://"):
            url = url.replace("libsql://", "https://", 1)
        self.url = url.rstrip("/")
        self.token = token
        self.max_reads = max_reads
        self.read_count = 0
        # ホスト名のみ (token / URL 全体は転記しない)
        self.host = re.sub(r"^https?://", "", self.url).split("/")[0]

    def execute(self, sql: str) -> dict:
        """SELECT 文を実行。WRITE 系を検出したら即座に終了。"""
        assert_readonly(sql)
        if self.read_count >= self.max_reads:
            raise ReadLimitExceeded(
                f"READ 上限 {self.max_reads} に到達。"
                f"残りの検証はスキップします。"
            )

        headers = {
            "Authorization": f"Bearer {self.token}",
            "Content-Type": "application/json",
        }
        body = {
            "requests": [
                {"type": "execute", "stmt": {"sql": sql}},
                {"type": "close"},
            ]
        }
        resp = requests.post(
            f"{self.url}/v2/pipeline",
            headers=headers,
            json=body,
            timeout=HTTP_TIMEOUT,
        )
        self.read_count += 1

        if resp.status_code != 200:
            raise RuntimeError(
                f"Turso API error {resp.status_code}: {resp.text[:200]}"
            )
        data = resp.json()
        for r in data.get("results", []):
            if r.get("type") == "error":
                raise RuntimeError(f"SQL error: {r.get('error', {}).get('message', '?')}")
        return data

    def list_tables(self) -> list[str]:
        """テーブル一覧 (sqlite_master)"""
        data = self.execute(
            "SELECT name FROM sqlite_master WHERE type='table' "
            "AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
        results = data.get("results", [])
        if not results:
            return []
        rows = results[0].get("response", {}).get("result", {}).get("rows", [])
        return [row[0].get("value", "") for row in rows]

    def count_rows(self, table: str) -> int:
        """COUNT(*) (テーブル名は事前 allowlist で検証済みであることを前提)"""
        # テーブル名サニタイズ (英数 + アンダースコアのみ許可)
        if not re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", table):
            raise ReadOnlyViolation(f"不正なテーブル名: {table}")
        data = self.execute(f"SELECT COUNT(*) FROM {table}")
        results = data.get("results", [])
        if not results:
            return -1
        rows = results[0].get("response", {}).get("result", {}).get("rows", [])
        if not rows:
            return -1
        try:
            return int(rows[0][0].get("value", "-1"))
        except (ValueError, KeyError, IndexError):
            return -1

    def sample_rows_hash(self, table: str, limit: int = 5) -> str:
        """先頭 N 行を取得し SHA256 ハッシュ化 (代表値比較用)"""
        if not re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", table):
            raise ReadOnlyViolation(f"不正なテーブル名: {table}")
        data = self.execute(
            f"SELECT * FROM {table} ORDER BY rowid LIMIT {int(limit)}"
        )
        results = data.get("results", [])
        if not results:
            return "EMPTY"
        result = results[0].get("response", {}).get("result", {})
        rows = result.get("rows", [])
        cols = result.get("cols", [])
        normalized = json.dumps(
            {
                "cols": [c.get("name") for c in cols],
                "rows": [[v.get("value") for v in row] for row in rows],
            },
            sort_keys=True,
            ensure_ascii=False,
        )
        return hashlib.sha256(normalized.encode("utf-8")).hexdigest()


# ──────────────────────────────────────────────
# ローカル sqlite (read-only)
# ──────────────────────────────────────────────
class LocalReadOnlyClient:
    """ローカル sqlite3 の SELECT 専用クライアント"""

    def __init__(self, db_path: Path):
        if not db_path.exists():
            raise FileNotFoundError(f"ローカル DB が見つかりません: {db_path}")
        # URI モードで read-only オープン
        uri = f"file:{db_path.as_posix()}?mode=ro"
        self.conn = sqlite3.connect(uri, uri=True)
        self.conn.row_factory = sqlite3.Row

    def list_tables(self) -> list[str]:
        cur = self.conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table' "
            "AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
        return [r[0] for r in cur.fetchall()]

    def count_rows(self, table: str) -> int:
        if not re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", table):
            return -1
        try:
            cur = self.conn.execute(f"SELECT COUNT(*) FROM {table}")
            return int(cur.fetchone()[0])
        except sqlite3.OperationalError:
            return -1

    def sample_rows_hash(self, table: str, limit: int = 5) -> str:
        if not re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", table):
            return "INVALID"
        try:
            cur = self.conn.execute(
                f"SELECT * FROM {table} ORDER BY rowid LIMIT {int(limit)}"
            )
            cols = [d[0] for d in cur.description]
            rows = cur.fetchall()
        except sqlite3.OperationalError:
            return "MISSING"
        normalized = json.dumps(
            {
                "cols": cols,
                "rows": [list(r) for r in rows],
            },
            sort_keys=True,
            ensure_ascii=False,
            default=str,
        )
        return hashlib.sha256(normalized.encode("utf-8")).hexdigest()

    def close(self):
        self.conn.close()


# ──────────────────────────────────────────────
# 検証ロジック
# ──────────────────────────────────────────────
def compare_table(
    local: LocalReadOnlyClient,
    remote: TursoReadOnlyClient,
    table: str,
    local_tables: set,
    remote_tables: set,
) -> dict:
    """1 テーブルを比較 (3 READ: 存在 + COUNT + サンプルハッシュ)"""
    result = {"table": table}
    result["local_exists"] = table in local_tables
    result["remote_exists"] = table in remote_tables

    if not result["local_exists"] and not result["remote_exists"]:
        result["status"] = "BOTH_MISSING"
        return result
    if not result["local_exists"]:
        result["status"] = "LOCAL_MISSING"
        return result
    if not result["remote_exists"]:
        result["status"] = "REMOTE_MISSING"
        return result

    # 両方存在 → 行数とサンプルハッシュ比較 (各 1 READ)
    result["local_count"] = local.count_rows(table)
    try:
        result["remote_count"] = remote.count_rows(table)
    except ReadLimitExceeded:
        result["status"] = "READ_LIMIT"
        return result

    result["local_hash"] = local.sample_rows_hash(table, 5)
    try:
        result["remote_hash"] = remote.sample_rows_hash(table, 5)
    except ReadLimitExceeded:
        result["status"] = "READ_LIMIT"
        return result

    if result["local_count"] != result["remote_count"]:
        result["status"] = "COUNT_MISMATCH"
    elif result["local_hash"] != result["remote_hash"]:
        result["status"] = "SAMPLE_MISMATCH"
    else:
        result["status"] = "MATCH"
    return result


# ──────────────────────────────────────────────
# レポート出力
# ──────────────────────────────────────────────
STATUS_EMOJI = {
    "MATCH": "✅",
    "COUNT_MISMATCH": "❌",
    "SAMPLE_MISMATCH": "⚠️",
    "LOCAL_MISSING": "🔴",
    "REMOTE_MISSING": "🟡",
    "BOTH_MISSING": "⚪",
    "READ_LIMIT": "⏸️",
}


def render_markdown_report(
    results: list[dict],
    remote_host: str,
    local_db_path: Path,
    read_count: int,
    started_at: datetime,
    finished_at: datetime,
    extra_local: list[str],
    extra_remote: list[str],
) -> str:
    duration = (finished_at - started_at).total_seconds()
    summary_counts = {}
    for r in results:
        summary_counts[r["status"]] = summary_counts.get(r["status"], 0) + 1

    lines = [
        f"# Turso V2 同期検証レポート",
        "",
        f"- 実行日時 (UTC): {started_at.isoformat()} 〜 {finished_at.isoformat()}",
        f"- 所要時間: {duration:.1f} 秒",
        f"- ローカル DB: `{local_db_path}`",
        f"- リモート: `{remote_host}` (Turso V2)",
        f"- READ 消費: {read_count} (上限 {MAX_READS})",
        "",
        "## サマリ",
        "",
        "| ステータス | 件数 | 意味 |",
        "|-----------|-----:|------|",
    ]
    status_meaning = {
        "MATCH": "ローカル・リモート完全一致",
        "COUNT_MISMATCH": "行数が異なる",
        "SAMPLE_MISMATCH": "行数は同じだが先頭 5 行のハッシュが異なる",
        "LOCAL_MISSING": "ローカルに不在 (リモートのみ存在)",
        "REMOTE_MISSING": "リモートに不在 (ローカルのみ存在、要 upload)",
        "BOTH_MISSING": "両方に不在",
        "READ_LIMIT": "READ 上限到達で検証スキップ",
    }
    for status, emoji in STATUS_EMOJI.items():
        cnt = summary_counts.get(status, 0)
        lines.append(f"| {emoji} {status} | {cnt} | {status_meaning[status]} |")

    lines.extend([
        "",
        "## テーブル別結果",
        "",
        "| テーブル | 状態 | ローカル行数 | リモート行数 | ハッシュ一致 |",
        "|---------|------|-----------:|------------:|:-----------:|",
    ])

    for r in results:
        emoji = STATUS_EMOJI.get(r["status"], "❓")
        lc = r.get("local_count", "-")
        rc = r.get("remote_count", "-")
        h = "—"
        if r["status"] == "MATCH":
            h = "✅"
        elif r["status"] == "SAMPLE_MISMATCH":
            h = "❌"
        lines.append(
            f"| `{r['table']}` | {emoji} {r['status']} | {lc} | {rc} | {h} |"
        )

    if extra_remote:
        lines.extend([
            "",
            "## 追加発見: リモートのみに存在するテーブル",
            "",
            "(検証対象 TARGET_TABLES に未登録)",
            "",
        ])
        for t in extra_remote:
            lines.append(f"- `{t}`")

    if extra_local:
        lines.extend([
            "",
            "## 追加発見: ローカルのみに存在するテーブル",
            "",
            "(検証対象 TARGET_TABLES に未登録)",
            "",
        ])
        for t in extra_local:
            lines.append(f"- `{t}`")

    lines.extend([
        "",
        "## 推奨対応",
        "",
        "- **MATCH**: アクション不要。",
        "- **COUNT_MISMATCH / SAMPLE_MISMATCH**: ローカル → リモートを `upload_to_turso.py` で再アップロード推奨。",
        "- **REMOTE_MISSING**: ローカルに新規投入後、`upload_to_turso.py` で初回アップロード。",
        "- **LOCAL_MISSING**: リモートにのみ存在。Phase 3 で必要なら `download_db.sh` 等で同期。",
        "- **BOTH_MISSING**: Task A の投入手順書 (`SURVEY_MARKET_INTELLIGENCE_PHASE3_TABLE_INGEST.md`) で投入。",
        "- **READ_LIMIT**: 翌月 (READ クォータリセット後) または上限緩和後に再実行。",
        "",
        "## 安全装置の動作確認",
        "",
        f"- WRITE 系 SQL 検出: 0 件 (本実行で WRITE 系クエリ未発行)",
        f"- READ 上限到達: {'あり' if any(r['status'] == 'READ_LIMIT' for r in results) else 'なし'}",
        f"- 認証 token 露出: 本レポートに転記なし",
        "",
        "---",
        "",
        f"生成: `scripts/verify_turso_v2_sync.py` ({finished_at.strftime('%Y-%m-%d')})",
    ])
    return "\n".join(lines)


# ──────────────────────────────────────────────
# main
# ──────────────────────────────────────────────
def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    parser.add_argument(
        "--db", type=Path, default=DEFAULT_LOCAL_DB,
        help=f"ローカル sqlite DB (default: {DEFAULT_LOCAL_DB})"
    )
    parser.add_argument(
        "--output", type=Path, default=None,
        help=f"レポート出力先 (default: docs/turso_v2_sync_report_YYYY-MM-DD.md)"
    )
    parser.add_argument(
        "--dry-run", action="store_true",
        help="Turso 接続せず、READ 試算と対象テーブル一覧のみ出力"
    )
    parser.add_argument(
        "--max-reads", type=int, default=MAX_READS,
        help=f"READ 上限 (default: {MAX_READS})"
    )
    args = parser.parse_args()

    print("=" * 70)
    print("Turso V2 同期検証 (READ-ONLY)")
    print("=" * 70)

    # Dry-run
    if args.dry_run:
        print("\n[DRY-RUN] Turso 接続なし")
        print(f"  対象テーブル: {len(TARGET_TABLES)} 件")
        print(f"  READ 試算: {len(TARGET_TABLES)} (テーブル一覧) "
              f"+ {len(TARGET_TABLES) * 2} (各テーブル COUNT + sample) "
              f"+ 1 (リモートテーブル一覧) = 約 {len(TARGET_TABLES) * 2 + 2}")
        print(f"  READ 上限: {args.max_reads}")
        if not args.db.exists():
            print(f"  ⚠️  ローカル DB 不在: {args.db}")
        else:
            print(f"  ローカル DB: {args.db} (size={args.db.stat().st_size:,} B)")
        url = os.environ.get("TURSO_EXTERNAL_URL", "")
        token = os.environ.get("TURSO_EXTERNAL_TOKEN", "")
        print(f"  TURSO_EXTERNAL_URL  : {'設定済' if url else '未設定'}")
        print(f"  TURSO_EXTERNAL_TOKEN: {'設定済' if token else '未設定'}")
        return 0

    # 環境変数確認
    url = os.environ.get("TURSO_EXTERNAL_URL", "").strip()
    token = os.environ.get("TURSO_EXTERNAL_TOKEN", "").strip()
    if not url or not token:
        print("ERROR: 環境変数 TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN を設定してください。", file=sys.stderr)
        return 1

    # ローカル DB
    print(f"\nローカル DB: {args.db}")
    local = LocalReadOnlyClient(args.db)
    local_tables = set(local.list_tables())
    print(f"  検出テーブル数: {len(local_tables)}")

    # Turso 接続
    print(f"\nTurso 接続: {url[:40]}...")
    remote = TursoReadOnlyClient(url, token, max_reads=args.max_reads)
    started_at = datetime.now(timezone.utc)
    try:
        remote_tables_list = remote.list_tables()
    except Exception as e:
        print(f"ERROR: Turso 接続失敗: {e}", file=sys.stderr)
        return 1
    remote_tables = set(remote_tables_list)
    print(f"  検出テーブル数: {len(remote_tables)}")

    # 検証対象テーブルの差分も発見的に列挙
    extra_local = sorted(local_tables - set(TARGET_TABLES) - remote_tables)
    extra_remote = sorted(remote_tables - set(TARGET_TABLES) - local_tables)

    # メイン比較
    print(f"\n{len(TARGET_TABLES)} テーブルを比較中...")
    results = []
    for i, table in enumerate(TARGET_TABLES, 1):
        try:
            r = compare_table(local, remote, table, local_tables, remote_tables)
        except ReadLimitExceeded:
            r = {"table": table, "status": "READ_LIMIT"}
            results.append(r)
            # 残りも READ_LIMIT として記録
            for remaining in TARGET_TABLES[i:]:
                results.append({"table": remaining, "status": "READ_LIMIT"})
            print(f"  [{i}/{len(TARGET_TABLES)}] {table}: ⏸️ READ 上限到達、残りスキップ")
            break
        except ReadOnlyViolation as e:
            print(f"FATAL: READ-ONLY 違反検出: {e}", file=sys.stderr)
            return 1
        except Exception as e:
            r = {"table": table, "status": "ERROR", "error": str(e)[:200]}
        results.append(r)
        emoji = STATUS_EMOJI.get(r["status"], "❓")
        print(f"  [{i}/{len(TARGET_TABLES)}] {table}: {emoji} {r['status']}")

    finished_at = datetime.now(timezone.utc)
    print(f"\nREAD 消費: {remote.read_count} / {args.max_reads}")

    # レポート出力
    output_path = args.output or Path(
        str(DEFAULT_OUTPUT).replace("{date}", finished_at.strftime("%Y-%m-%d"))
    )
    output_path.parent.mkdir(parents=True, exist_ok=True)
    md = render_markdown_report(
        results=results,
        remote_host=remote.host,
        local_db_path=args.db,
        read_count=remote.read_count,
        started_at=started_at,
        finished_at=finished_at,
        extra_local=extra_local,
        extra_remote=extra_remote,
    )
    output_path.write_text(md, encoding="utf-8")
    print(f"\nレポート出力: {output_path}")

    # サマリ
    summary = {}
    for r in results:
        summary[r["status"]] = summary.get(r["status"], 0) + 1
    print("\nサマリ:")
    for status, cnt in sorted(summary.items()):
        emoji = STATUS_EMOJI.get(status, "❓")
        print(f"  {emoji} {status}: {cnt}")

    local.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
