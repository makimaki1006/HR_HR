# -*- coding: utf-8 -*-
"""
municipality_living_cost_proxy 専用 Turso upload スクリプト
==========================================================
upload_phase3_step5.py のロジックを継承 (token mask / host allowlist /
retry / audit log / BEGIN/COMMIT バッチ)。1 テーブルのみ replace 戦略。

CLI:
    python scripts/upload_living_cost_proxy.py --dry-run
    python scripts/upload_living_cost_proxy.py --check-remote
    python scripts/upload_living_cost_proxy.py --upload --yes
    python scripts/upload_living_cost_proxy.py --verify

env (PowerShell 事前設定、os.getenv 経由のみ):
    $env:TURSO_EXTERNAL_URL = "https://...turso.io"
    $env:TURSO_EXTERNAL_TOKEN = "<auth token>"
"""
from __future__ import annotations

import argparse
import os
import sqlite3
import sys
import time
from pathlib import Path
from urllib.parse import urlparse

# 既存 upload_phase3_step5 のヘルパを再利用
SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from upload_phase3_step5 import (  # noqa: E402
    ALLOWED_TURSO_HOSTS,
    ErrorTracker,
    assert_allowed_host,
    get_columns,
    get_create_index_sqls,
    get_create_table_sql,
    get_local_count,
    mask_token,
    open_local_ro,
    remote_count,
    remote_table_exists,
    turso_pipeline_retry,
    write_audit,
    _bulk_insert,
)

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass


TABLE = "municipality_living_cost_proxy"
EXPECT_LOCAL = 1_917
DEFAULT_BATCH = 500
DEFAULT_MAX_WRITES = 3_000


def run_dry_run(conn: sqlite3.Connection, max_writes: int) -> None:
    print("=" * 70)
    print(f"[living_cost_proxy] --dry-run (no remote calls)")
    print("=" * 70)
    n = get_local_count(conn, TABLE)
    match = "OK" if n == EXPECT_LOCAL else f"MISMATCH (expect {EXPECT_LOCAL:,})"
    print(f"  [{TABLE}] strategy=replace local={n:,} {match} writes={n:,}")
    print(f"  --max-writes: {max_writes:,}")
    if n > max_writes:
        print(f"  WARNING: writes exceeds --max-writes")
    else:
        print(f"  OK: within --max-writes budget")
    write_audit("dry_run", TABLE, n, "success")


def run_check_remote(conn: sqlite3.Connection, url: str, token: str) -> None:
    print("=" * 70)
    print(f"[living_cost_proxy] --check-remote (READ-only)")
    print(f"  Turso host : {urlparse(url).hostname}")
    print(f"  Turso token: {mask_token(token)}")
    print("=" * 70)
    local_n = get_local_count(conn, TABLE)
    exists = remote_table_exists(url, token, TABLE)
    if exists:
        n = remote_count(url, token, TABLE)
        remote_str = f"{n:,}" if n is not None else "?"
        print(f"  [{TABLE}] local={local_n:,} remote={remote_str} (exists)")
    else:
        print(f"  [{TABLE}] local={local_n:,} remote=(not exists) -> ready for replace")


def run_upload(conn: sqlite3.Connection, url: str, token: str,
               batch_size: int, max_writes: int) -> None:
    print("=" * 70)
    print(f"[living_cost_proxy] --upload")
    print(f"  Turso host : {urlparse(url).hostname}")
    print(f"  Turso token: {mask_token(token)}")
    print("=" * 70)

    local_n = get_local_count(conn, TABLE)
    if local_n > max_writes:
        raise SystemExit(
            f"[abort] local rows {local_n:,} exceeds --max-writes {max_writes:,}"
        )
    print(f"  estimated writes: {local_n:,} (limit {max_writes:,})")

    create_sql = get_create_table_sql(conn, TABLE)
    index_sqls = get_create_index_sqls(conn, TABLE)
    cols = get_columns(conn, TABLE)
    rows = conn.execute(f"SELECT * FROM {TABLE}").fetchall()

    setup: list[tuple[str, list | None]] = [
        (f"DROP TABLE IF EXISTS {TABLE}", None),
        (create_sql, None),
    ]
    for idx_sql in index_sqls:
        setup.append((idx_sql, None))
    turso_pipeline_retry(url, token, setup)
    print(f"  [{TABLE}] DDL applied (1 table + {len(index_sqls)} indexes)")

    tracker = ErrorTracker()
    t0 = time.time()
    n = _bulk_insert(url, token, TABLE, cols, rows, batch_size, tracker, replace_mode=True)
    elapsed = time.time() - t0
    print(f"\n  [{TABLE}] uploaded {n:,} rows in {elapsed:.1f}s")
    write_audit("upload_session", TABLE, n, "success",
                elapsed_sec=round(elapsed, 2))


def run_verify(conn: sqlite3.Connection, url: str, token: str) -> None:
    print("=" * 70)
    print(f"[living_cost_proxy] --verify")
    print(f"  Turso host : {urlparse(url).hostname}")
    print(f"  Turso token: {mask_token(token)}")
    print("=" * 70)
    local_total = get_local_count(conn, TABLE)
    remote_total = remote_count(url, token, TABLE)
    ok = remote_total == local_total
    print(f"  [{TABLE}] local={local_total:,} remote={remote_total!s:>8s} "
          f"{'OK' if ok else 'MISMATCH'}")
    if not ok:
        raise SystemExit(f"[abort] verify mismatch")
    print("  ALL OK")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Upload municipality_living_cost_proxy to Turso")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--check-remote", action="store_true")
    mode.add_argument("--upload", action="store_true")
    mode.add_argument("--verify", action="store_true")
    p.add_argument("--batch-size", type=int, default=DEFAULT_BATCH)
    p.add_argument("--max-writes", type=int, default=DEFAULT_MAX_WRITES)
    p.add_argument("--yes", action="store_true")
    return p.parse_args()


def main() -> None:
    args = parse_args()
    conn = open_local_ro()

    if args.dry_run:
        run_dry_run(conn, args.max_writes)
        return

    if args.upload and not args.yes:
        raise SystemExit("[abort] --upload requires --yes flag")

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
        run_check_remote(conn, url, token)
    elif args.upload:
        run_upload(conn, url, token, args.batch_size, args.max_writes)
    elif args.verify:
        run_verify(conn, url, token)


if __name__ == "__main__":
    main()
