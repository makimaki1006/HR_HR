# -*- coding: utf-8 -*-
"""
Phase 3 Step 5 Turso Upload スクリプト
==========================================
専用 upload script for Phase 3 Step 5 (7 tables)。

戦略:
    replace     : DROP -> CREATE (DDL/INDEX をローカルから動的取得) -> bulk INSERT
    incremental : designated_ward の 175 件 / 1,575 行のみを INSERT
                  (リモートに該当行がある場合は abort)

安全装置 (8 件):
    1. token mask (生 token をログ・stdout に出さない)
    2. host allowlist (TURSO_EXTERNAL_URL のホスト名検証)
    3. --upload には --yes フラグ必須
    4. --max-writes 超過時 abort
    5. retry 3 回 (exponential backoff: 2s/4s/8s)
    6. 連続 5 エラーで abort
    7. BEGIN/COMMIT (バッチ内 transaction)
    8. audit log JSON (data/generated/turso_upload_audit_log.json)

CLI:
    python scripts/upload_phase3_step5.py --dry-run
    python scripts/upload_phase3_step5.py --check-remote
    python scripts/upload_phase3_step5.py --upload --yes
    python scripts/upload_phase3_step5.py --verify

環境変数 (PowerShell で事前設定、os.getenv 経由のみ参照、.env 直接 open 禁止):
    $env:TURSO_EXTERNAL_URL = "https://...turso.io"
    $env:TURSO_EXTERNAL_TOKEN = "<auth token>"
"""
from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import requests

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass


# ─────────────────────────────────────────────
# 定数
# ─────────────────────────────────────────────
SCRIPT_DIR = Path(__file__).resolve().parent
DEPLOY_ROOT = SCRIPT_DIR.parent
LOCAL_DB = DEPLOY_ROOT / "data" / "hellowork.db"
AUDIT_LOG_PATH = DEPLOY_ROOT / "data" / "generated" / "turso_upload_audit_log.json"

# host allowlist (生 URL の hostname 検証)
ALLOWED_TURSO_HOSTS = {
    "country-statistics-makimaki1006.aws-ap-northeast-1.turso.io",
}

# テーブル定義 (戦略 + 期待ローカル行数)
TABLES: dict[str, dict[str, Any]] = {
    "municipality_occupation_population": {
        "strategy": "replace",
        "expect_local": 729_949,
    },
    "v2_municipality_target_thickness": {
        "strategy": "replace",
        "expect_local": 20_845,
    },
    "municipality_code_master": {
        "strategy": "replace",
        "expect_local": 1_917,
    },
    "commute_flow_summary": {
        "strategy": "replace",
        "expect_local": 27_879,
    },
    "v2_external_commute_od_with_codes": {
        "strategy": "replace",
        "expect_local": 86_762,
    },
    "v2_external_population": {
        "strategy": "incremental",
        "expect_local": 1_917,
        "incr_filter": "designated_ward",
        "incr_expect_rows": 175,
    },
    "v2_external_population_pyramid": {
        "strategy": "incremental",
        "expect_local": 17_235,
        "incr_filter": "designated_ward",
        "incr_expect_rows": 1_575,
    },
}

ALL_TABLES = list(TABLES.keys())

MAX_RETRIES = 3
MAX_CONSECUTIVE_ERRORS = 5


# ─────────────────────────────────────────────
# token mask
# ─────────────────────────────────────────────
def mask_token(t: str | None) -> str:
    """生 token をログ等に出さないためのマスク。"""
    if not t or len(t) < 8:
        return "***"
    return f"{t[:4]}...{t[-4:]}"


# ─────────────────────────────────────────────
# host allowlist
# ─────────────────────────────────────────────
def assert_allowed_host(url: str) -> None:
    h = urlparse(url).hostname or ""
    if h not in ALLOWED_TURSO_HOSTS:
        raise SystemExit(
            f"[abort] host '{h}' is not in allowlist {sorted(ALLOWED_TURSO_HOSTS)}"
        )


# ─────────────────────────────────────────────
# .env 直接 open は禁止 (ユーザー指示)。
# 環境変数はユーザーが PowerShell 等で事前に設定する想定:
#   $env:TURSO_EXTERNAL_URL = "..."
#   $env:TURSO_EXTERNAL_TOKEN = "..."
# os.getenv 経由のみで参照する。
# ─────────────────────────────────────────────
# audit log
# ─────────────────────────────────────────────
def _load_audit_log() -> dict:
    if AUDIT_LOG_PATH.exists():
        try:
            return json.loads(AUDIT_LOG_PATH.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            pass
    return {"events": []}


def _save_audit_log(log: dict) -> None:
    AUDIT_LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    AUDIT_LOG_PATH.write_text(
        json.dumps(log, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


def write_audit(action: str, table: str, rows: int, status: str, **extra: Any) -> None:
    log = _load_audit_log()
    log["events"].append(
        {
            "ts": datetime.now(timezone.utc).isoformat(),
            "action": action,
            "table": table,
            "rows": rows,
            "status": status,
            **extra,
        }
    )
    _save_audit_log(log)


# ─────────────────────────────────────────────
# エラートラッカ (連続 5 エラーで abort)
# ─────────────────────────────────────────────
class ErrorTracker:
    def __init__(self) -> None:
        self.consec = 0

    def hit(self, msg: str = "") -> None:
        self.consec += 1
        if self.consec >= MAX_CONSECUTIVE_ERRORS:
            raise SystemExit(
                f"[abort] {MAX_CONSECUTIVE_ERRORS} consecutive errors. last: {msg}"
            )

    def reset(self) -> None:
        self.consec = 0


# ─────────────────────────────────────────────
# Turso HTTP API ラッパ (libSQL /v2/pipeline)
# ─────────────────────────────────────────────
def _arg_for(value: Any) -> dict:
    """libSQL HTTP API 用の引数オブジェクト変換。

    upload_new_external_to_turso.py と同じロジック。
    """
    if value is None:
        return {"type": "null", "value": None}
    if isinstance(value, bool):
        return {"type": "integer", "value": "1" if value else "0"}
    if isinstance(value, int):
        return {"type": "integer", "value": str(value)}
    if isinstance(value, float):
        return {"type": "float", "value": value}
    return {"type": "text", "value": str(value)}


def _build_stmt(sql: str, params: list[Any] | None) -> dict:
    stmt: dict[str, Any] = {"sql": sql}
    if params:
        stmt["args"] = [_arg_for(v) for v in params]
    return stmt


def turso_pipeline(
    url: str,
    token: str,
    statements: list[tuple[str, list[Any] | None]],
    timeout: int = 60,
) -> dict:
    """複数 SQL を 1 リクエストで送信。BEGIN/COMMIT を含めても良い。"""
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }
    requests_list: list[dict] = []
    for sql, params in statements:
        requests_list.append({"type": "execute", "stmt": _build_stmt(sql, params)})
    requests_list.append({"type": "close"})

    resp = requests.post(
        f"{url}/v2/pipeline",
        headers=headers,
        json={"requests": requests_list},
        timeout=timeout,
    )
    resp.raise_for_status()
    data = resp.json()

    errors = [
        r for r in data.get("results", []) if r.get("type") == "error"
    ]
    if errors:
        raise RuntimeError(f"SQL errors (first 3): {errors[:3]}")
    return data


def with_retry(fn, *args, **kwargs):
    """retry MAX_RETRIES 回 (exponential backoff: 2s/4s/8s)。"""
    last_exc: Exception | None = None
    for attempt in range(MAX_RETRIES):
        try:
            return fn(*args, **kwargs)
        except (requests.HTTPError, requests.Timeout, requests.ConnectionError, RuntimeError) as e:
            last_exc = e
            wait = (2 ** attempt) * 2
            print(f"  [retry {attempt + 1}/{MAX_RETRIES}] wait {wait}s ({type(e).__name__})")
            time.sleep(wait)
    raise SystemExit(f"[abort] retry exhausted: {last_exc}")


def turso_pipeline_retry(url: str, token: str, statements):
    return with_retry(turso_pipeline, url, token, statements)


def remote_count(url: str, token: str, table: str) -> int | None:
    try:
        data = turso_pipeline_retry(url, token, [(f"SELECT COUNT(*) FROM {table}", None)])
        return int(data["results"][0]["response"]["result"]["rows"][0][0]["value"])
    except SystemExit:
        return None
    except Exception as e:
        print(f"  [remote_count] {table}: error {e}")
        return None


def remote_table_exists(url: str, token: str, table: str) -> bool:
    data = turso_pipeline_retry(
        url,
        token,
        [(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
            [table],
        )],
    )
    rows = data["results"][0]["response"]["result"].get("rows", [])
    return len(rows) > 0


# ─────────────────────────────────────────────
# ローカル DB ヘルパ
# ─────────────────────────────────────────────
def open_local_ro() -> sqlite3.Connection:
    if not LOCAL_DB.exists():
        raise SystemExit(f"[abort] local DB not found: {LOCAL_DB}")
    return sqlite3.connect(f"file:{LOCAL_DB}?mode=ro", uri=True)


def get_local_count(conn: sqlite3.Connection, table: str) -> int:
    return conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]


def get_create_table_sql(conn: sqlite3.Connection, table: str) -> str:
    row = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?", (table,)
    ).fetchone()
    if not row or not row[0]:
        raise SystemExit(f"[abort] CREATE TABLE not found for {table}")
    return row[0]


def get_create_index_sqls(conn: sqlite3.Connection, table: str) -> list[str]:
    rows = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name=? AND sql IS NOT NULL",
        (table,),
    ).fetchall()
    return [r[0] for r in rows]


def get_columns(conn: sqlite3.Connection, table: str) -> list[str]:
    rows = conn.execute(f"PRAGMA table_info({table})").fetchall()
    return [r[1] for r in rows]


def get_designated_ward_munis(conn: sqlite3.Connection) -> list[tuple[str, str]]:
    """(prefecture, municipality_name) のリスト。175 件想定。"""
    rows = conn.execute(
        "SELECT prefecture, municipality_name FROM municipality_code_master "
        "WHERE area_type='designated_ward'"
    ).fetchall()
    return [(r[0], r[1]) for r in rows]


def get_incremental_rows(
    conn: sqlite3.Connection, table: str
) -> tuple[list[str], list[tuple]]:
    """designated_ward の municipality に該当する行のみ抽出。"""
    cols = get_columns(conn, table)
    sql = (
        f"SELECT p.* FROM {table} p "
        f"JOIN municipality_code_master m "
        f"  ON p.prefecture = m.prefecture "
        f" AND p.municipality = m.municipality_name "
        f"WHERE m.area_type = 'designated_ward'"
    )
    rows = conn.execute(sql).fetchall()
    return cols, rows


def get_replace_rows(
    conn: sqlite3.Connection, table: str
) -> tuple[list[str], list[tuple]]:
    cols = get_columns(conn, table)
    rows = conn.execute(f"SELECT * FROM {table}").fetchall()
    return cols, rows


# ─────────────────────────────────────────────
# 戦略別 upload 実装
# ─────────────────────────────────────────────
def estimate_writes_for_table(
    conn: sqlite3.Connection, table: str, strategy: str
) -> int:
    if strategy == "replace":
        return get_local_count(conn, table)
    if strategy == "incremental":
        spec = TABLES[table]
        # incremental は designated_ward に限定された行数のみ書く
        return spec.get("incr_expect_rows", 0)
    return 0


def assert_no_existing_designated(
    url: str, token: str, table: str, designated_munis: list[tuple[str, str]]
) -> None:
    """incremental: リモートに designated_ward の行が既存なら abort。"""
    # 1 ステートメントで COUNT (prefecture+municipality の組み合わせ)
    if not designated_munis:
        return
    # IN 句: (prefecture||':'||municipality) で照合
    keys = [f"{p}::{m}" for p, m in designated_munis]
    placeholders = ",".join("?" * len(keys))
    sql = (
        f"SELECT COUNT(*) FROM {table} "
        f"WHERE (prefecture || '::' || municipality) IN ({placeholders})"
    )
    data = turso_pipeline_retry(url, token, [(sql, keys)])
    n = int(data["results"][0]["response"]["result"]["rows"][0][0]["value"])
    if n > 0:
        raise SystemExit(
            f"[abort] {table}: remote already has {n} designated_ward rows. "
            f"manual DELETE required before incremental upload."
        )


def upload_replace(
    url: str,
    token: str,
    conn: sqlite3.Connection,
    table: str,
    batch_size: int,
    tracker: ErrorTracker,
) -> int:
    """DROP -> CREATE -> CREATE INDEX -> bulk INSERT (BEGIN/COMMIT)。"""
    create_sql = get_create_table_sql(conn, table)
    index_sqls = get_create_index_sqls(conn, table)
    cols, rows = get_replace_rows(conn, table)

    # DDL
    setup: list[tuple[str, list | None]] = [
        (f"DROP TABLE IF EXISTS {table}", None),
        (create_sql, None),
    ]
    for idx_sql in index_sqls:
        setup.append((idx_sql, None))
    turso_pipeline_retry(url, token, setup)
    print(f"  [{table}] DDL applied (1 table + {len(index_sqls)} indexes)")

    return _bulk_insert(
        url, token, table, cols, rows, batch_size, tracker, replace_mode=True
    )


def upload_incremental(
    url: str,
    token: str,
    conn: sqlite3.Connection,
    table: str,
    batch_size: int,
    tracker: ErrorTracker,
) -> int:
    """designated_ward 行のみ INSERT (DDL は触らない)。"""
    designated = get_designated_ward_munis(conn)
    assert_no_existing_designated(url, token, table, designated)

    cols, rows = get_incremental_rows(conn, table)
    spec = TABLES[table]
    expected = spec["incr_expect_rows"]
    if len(rows) != expected:
        raise SystemExit(
            f"[abort] {table}: incremental rows {len(rows)} != expected {expected}"
        )
    print(f"  [{table}] incremental rows: {len(rows)} (designated_ward)")
    return _bulk_insert(
        url, token, table, cols, rows, batch_size, tracker, replace_mode=False
    )


def _bulk_insert(
    url: str,
    token: str,
    table: str,
    cols: list[str],
    rows: list[tuple],
    batch_size: int,
    tracker: ErrorTracker,
    replace_mode: bool,
) -> int:
    if not rows:
        return 0
    col_names = ", ".join(cols)
    placeholders = ", ".join(["?"] * len(cols))
    insert_sql = f"INSERT INTO {table} ({col_names}) VALUES ({placeholders})"

    total = 0
    t0 = time.time()
    for i in range(0, len(rows), batch_size):
        batch = rows[i : i + batch_size]
        stmts: list[tuple[str, list | None]] = [("BEGIN", None)]
        stmts += [(insert_sql, list(r)) for r in batch]
        stmts.append(("COMMIT", None))
        try:
            turso_pipeline_retry(url, token, stmts)
            tracker.reset()
            total += len(batch)
        except SystemExit as e:
            # 単発リトライ枯渇 -> ROLLBACK 試行 + tracker.hit
            try:
                turso_pipeline(url, token, [("ROLLBACK", None)])
            except Exception:
                pass
            tracker.hit(str(e))
            write_audit("upload_batch", table, len(batch), "error", error=str(e))
            continue
        if total % 5000 == 0 or total == len(rows):
            elapsed = time.time() - t0
            print(f"    [{table}] {total:,}/{len(rows):,} rows ({elapsed:.1f}s)")
    elapsed = time.time() - t0
    write_audit(
        "upload_table",
        table,
        total,
        "success" if total == len(rows) else "partial",
        elapsed_sec=round(elapsed, 2),
        replace_mode=replace_mode,
    )
    return total


# ─────────────────────────────────────────────
# モード別エントリ
# ─────────────────────────────────────────────
def filter_target_tables(args_tables: list[str] | None) -> list[str]:
    if not args_tables:
        return ALL_TABLES
    bad = [t for t in args_tables if t not in TABLES]
    if bad:
        raise SystemExit(f"[abort] unknown table(s): {bad}. valid: {ALL_TABLES}")
    return args_tables


def resolve_strategy(table: str, override: str) -> str:
    if override == "auto":
        return TABLES[table]["strategy"]
    return override


def run_dry_run(args, conn: sqlite3.Connection) -> None:
    print("=" * 70)
    print("[Phase 3 Step 5] --dry-run (no remote calls)")
    print("=" * 70)
    targets = filter_target_tables(args.tables)
    total_writes = 0
    replace_total = 0
    incremental_total = 0
    for table in targets:
        spec = TABLES[table]
        strategy = resolve_strategy(table, args.strategy)
        local_n = get_local_count(conn, table)
        expected = spec["expect_local"]
        match = "OK" if local_n == expected else f"MISMATCH (expect {expected:,})"
        writes = estimate_writes_for_table(conn, table, strategy)
        total_writes += writes
        if strategy == "replace":
            replace_total += writes
        elif strategy == "incremental":
            incremental_total += writes
        extra = ""
        if strategy == "incremental":
            extra = f" filter={spec.get('incr_filter')} expect={spec.get('incr_expect_rows'):,}"
        print(
            f"  [{table:42s}] strategy={strategy:11s} local={local_n:>8,} {match:20s} writes={writes:>8,}{extra}"
        )
    print("-" * 70)
    print(f"  TOTAL writes: {total_writes:,}")
    print(f"    replace:     {replace_total:,}")
    print(f"    incremental: {incremental_total:,}")
    print(f"  --max-writes: {args.max_writes:,}")
    if total_writes > args.max_writes:
        print(f"  WARNING: total writes exceeds --max-writes")
    else:
        print(f"  OK: within --max-writes budget")
    write_audit(
        "dry_run", "ALL", total_writes, "success",
        replace_total=replace_total, incremental_total=incremental_total,
    )


def run_check_remote(args, conn: sqlite3.Connection, url: str, token: str) -> None:
    print("=" * 70)
    print("[Phase 3 Step 5] --check-remote (READ-only)")
    print(f"  Turso host: {urlparse(url).hostname}")
    print(f"  Turso token: {mask_token(token)}")
    print("=" * 70)
    targets = filter_target_tables(args.tables)
    designated = get_designated_ward_munis(conn)
    for table in targets:
        spec = TABLES[table]
        strategy = resolve_strategy(table, args.strategy)
        local_n = get_local_count(conn, table)
        try:
            exists = remote_table_exists(url, token, table)
        except SystemExit as e:
            print(f"  [{table}] check failed: {e}")
            continue
        if exists:
            n = remote_count(url, token, table)
            remote_str = f"{n:,}" if n is not None else "?"
        else:
            remote_str = "(not exists)"
        diff_note = ""
        if strategy == "incremental" and exists:
            try:
                assert_no_existing_designated(url, token, table, designated)
                diff_note = "designated_ward=0 (OK to incremental)"
            except SystemExit as e:
                diff_note = f"designated_ward present -> would abort"
        print(
            f"  [{table:42s}] strategy={strategy:11s} local={local_n:>8,} remote={remote_str:>10s} {diff_note}"
        )


def run_upload(args, conn: sqlite3.Connection, url: str, token: str) -> None:
    print("=" * 70)
    print("[Phase 3 Step 5] --upload")
    print(f"  Turso host: {urlparse(url).hostname}")
    print(f"  Turso token: {mask_token(token)}")
    print("=" * 70)

    targets = filter_target_tables(args.tables)

    # 事前: max-writes ガード
    total_writes = sum(
        estimate_writes_for_table(conn, t, resolve_strategy(t, args.strategy))
        for t in targets
    )
    if total_writes > args.max_writes:
        raise SystemExit(
            f"[abort] estimated writes {total_writes:,} exceeds --max-writes {args.max_writes:,}"
        )
    print(f"  estimated total writes: {total_writes:,} (limit {args.max_writes:,})")

    tracker = ErrorTracker()
    total_uploaded = 0
    t0 = time.time()
    for table in targets:
        strategy = resolve_strategy(table, args.strategy)
        print(f"\n  ===== {table} (strategy={strategy}) =====")
        if strategy == "replace":
            n = upload_replace(url, token, conn, table, args.batch_size, tracker)
        elif strategy == "incremental":
            n = upload_incremental(url, token, conn, table, args.batch_size, tracker)
        else:
            raise SystemExit(f"[abort] unknown strategy: {strategy}")
        total_uploaded += n
        print(f"  [{table}] uploaded {n:,} rows")
    elapsed = time.time() - t0
    print(f"\n  TOTAL uploaded: {total_uploaded:,} rows in {elapsed:.1f}s")
    write_audit("upload_session", "ALL", total_uploaded, "success", elapsed_sec=round(elapsed, 2))


def run_verify(args, conn: sqlite3.Connection, url: str, token: str) -> None:
    print("=" * 70)
    print("[Phase 3 Step 5] --verify")
    print(f"  Turso host: {urlparse(url).hostname}")
    print(f"  Turso token: {mask_token(token)}")
    print("=" * 70)
    targets = filter_target_tables(args.tables)
    fail = 0
    for table in targets:
        strategy = resolve_strategy(table, args.strategy)
        local_n = get_local_count(conn, table)
        remote_n = remote_count(url, token, table)
        if strategy == "replace":
            expect_remote = local_n
        else:
            expect_remote = TABLES[table]["incr_expect_rows"]
        ok = remote_n == expect_remote
        if not ok:
            fail += 1
        print(
            f"  [{table:42s}] strategy={strategy:11s} local={local_n:>8,} "
            f"remote={remote_n!s:>10s} expect_remote={expect_remote:>8,} "
            f"{'OK' if ok else 'MISMATCH'}"
        )
    if fail:
        raise SystemExit(f"[abort] verify mismatch on {fail} table(s)")
    print("  ALL OK")


# ─────────────────────────────────────────────
# main
# ─────────────────────────────────────────────
def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Phase 3 Step 5 Turso upload (7 tables, replace + incremental)"
    )
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="local 読込 + 見積出力 (実通信なし)")
    mode.add_argument("--check-remote", action="store_true", help="Turso 既存状況確認 (READ-only)")
    mode.add_argument("--upload", action="store_true", help="本番 upload (DDL apply + bulk INSERT)")
    mode.add_argument("--verify", action="store_true", help="upload 後の整合性検証")

    p.add_argument("--tables", nargs="+", default=None, help="対象テーブル (default: 全 7)")
    p.add_argument(
        "--strategy",
        choices=["auto", "replace", "incremental"],
        default="auto",
        help="auto = TABLES 設定。明示すれば全テーブルに同じ戦略を適用",
    )
    p.add_argument("--batch-size", type=int, default=500)
    p.add_argument("--max-writes", type=int, default=850_000)
    p.add_argument("--yes", action="store_true", help="--upload 実行に必須")
    return p.parse_args()


def require_yes(args, action_desc: str) -> None:
    if not args.yes:
        raise SystemExit(f"[abort] {action_desc} requires --yes flag")


def main() -> None:
    args = parse_args()

    # local DB は常に必要
    conn = open_local_ro()

    if args.dry_run:
        run_dry_run(args, conn)
        return

    # --upload は --yes チェックを最優先 (token 不在より前に明示停止)
    if args.upload:
        require_yes(args, "--upload")

    # 以降のモードは Turso 接続必須
    url = os.getenv("TURSO_EXTERNAL_URL", "").strip()
    token = os.getenv("TURSO_EXTERNAL_TOKEN", "").strip()
    if not url:
        raise SystemExit("[abort] TURSO_EXTERNAL_URL not set")
    if not token:
        raise SystemExit("[abort] TURSO_EXTERNAL_TOKEN not set")
    if url.startswith("libsql://"):
        url = url.replace("libsql://", "https://", 1)
    assert_allowed_host(url)

    if args.check_remote:
        run_check_remote(args, conn, url, token)
        return

    if args.upload:
        run_upload(args, conn, url, token)
        return

    if args.verify:
        run_verify(args, conn, url, token)
        return


if __name__ == "__main__":
    main()
