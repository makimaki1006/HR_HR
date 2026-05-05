"""
fetch_estat_15_1.py
====================

e-Stat 国勢調査 R2 表 15-1 (statdisp_id=0003454508) 取得スクリプト

実装ステータス: 確認モード (--metadata-only / --sample-only / --dry-run)
- [OK] メタデータ取得 (1 API call)
- [OK] サンプル取得 (1 ページ、1000 行)
- [OK] Dry-run (API なし、env 確認)
- [TODO] --fetch / --merge / --validate は skeleton (本格 fetch は appId 確定後の別タスク)

設計書: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_ESTAT_15_1_PLAN.md (Worker A4)

CLI:
  python scripts/fetch_estat_15_1.py --dry-run                        # 接続なし
  $env:ESTAT_APP_ID = "your-app-id"; \
  python scripts/fetch_estat_15_1.py --metadata-only                  # 1 リクエスト
  python scripts/fetch_estat_15_1.py --sample-only --limit 1000       # 1 ページ取得
  python scripts/fetch_estat_15_1.py --fetch                          # NotImplementedError (本格 fetch)
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import requests

# UTF-8 logging (cp932 対応)
try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

# --------------------------------------------------------------------------- #
# 定数
# --------------------------------------------------------------------------- #

ESTAT_API_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json"
DEFAULT_STATS_DATA_ID = "0003454508"  # 統計表 ID (statdisp_id)。API 用 ID と異なる場合は --metadata-only で確定

OUTPUT_DIR = Path("data/generated")
TEMP_DIR = OUTPUT_DIR / "temp"
META_FILE = OUTPUT_DIR / "estat_15_1_metadata.json"
SAMPLE_FILE = TEMP_DIR / "estat_15_1_sample.json"
PROGRESS_FILE = OUTPUT_DIR / "estat_15_1_progress.json"


# --------------------------------------------------------------------------- #
# appId 管理
# --------------------------------------------------------------------------- #

def get_app_id(cli_arg: str | None = None) -> str:
    """
    appId 取得。

    優先順位:
      1. CLI 引数 --app-id
      2. 環境変数 ESTAT_APP_ID

    .env ファイルは直接 open しない (python-dotenv も使用禁止)。
    """
    app_id = cli_arg or os.environ.get("ESTAT_APP_ID")
    if not app_id:
        raise SystemExit(
            "ERROR: appId not provided.\n"
            "  Set $env:ESTAT_APP_ID='your-app-id' (PowerShell) before running, "
            "or pass --app-id."
        )
    return app_id


def mask_app_id(app_id: str) -> str:
    """appId をマスク表示 (先頭 3 + 末尾 2 を残す、間はアスタリスク)。"""
    if not app_id or len(app_id) < 6:
        return "***"
    return f"{app_id[:3]}{'*' * (len(app_id) - 5)}{app_id[-2:]}"


# --------------------------------------------------------------------------- #
# API 呼び出し (READ-only GET)
# --------------------------------------------------------------------------- #

def get_meta(stats_data_id: str, app_id: str) -> dict[str, Any]:
    """getMetaInfo: 軸メタデータ (cat01/cat02/cat03/area) を取得。"""
    url = f"{ESTAT_API_BASE}/getMetaInfo"
    params = {"appId": app_id, "statsDataId": stats_data_id}
    resp = requests.get(url, params=params, timeout=30)
    resp.raise_for_status()
    return resp.json()


def get_stats_data(
    stats_data_id: str,
    app_id: str,
    start_position: int = 1,
    limit: int = 1000,
) -> dict[str, Any]:
    """getStatsData: データ本体取得 (1 ページ)。"""
    url = f"{ESTAT_API_BASE}/getStatsData"
    params = {
        "appId": app_id,
        "statsDataId": stats_data_id,
        "startPosition": start_position,
        "limit": limit,
        "metaGetFlg": "Y",
        "cntGetFlg": "N",
        "replaceSpChars": "2",
    }
    resp = requests.get(url, params=params, timeout=60)
    resp.raise_for_status()
    return resp.json()


# --------------------------------------------------------------------------- #
# モード実装
# --------------------------------------------------------------------------- #

def ensure_dirs() -> None:
    """出力ディレクトリを作成 (idempotent)。"""
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    TEMP_DIR.mkdir(parents=True, exist_ok=True)


def run_dry_run(app_id_arg: str | None) -> int:
    """API 接続なしで env + 出力ディレクトリを確認。"""
    print("[mode] --dry-run")
    print(f"[time] {datetime.now(timezone.utc).isoformat()}")

    # appId チェック (未設定なら SystemExit)
    app_id = get_app_id(app_id_arg)
    print(f"[appId] {mask_app_id(app_id)} (length={len(app_id)})")

    # 出力ディレクトリ作成
    ensure_dirs()
    print(f"[dir] OUTPUT_DIR={OUTPUT_DIR.resolve()} exists={OUTPUT_DIR.exists()}")
    print(f"[dir] TEMP_DIR={TEMP_DIR.resolve()} exists={TEMP_DIR.exists()}")

    # 想定出力先
    print(f"[plan] META_FILE   = {META_FILE}")
    print(f"[plan] SAMPLE_FILE = {SAMPLE_FILE}")
    print(f"[plan] PROGRESS    = {PROGRESS_FILE}")

    print("[OK] dry-run completed (no HTTP request issued).")
    return 0


def run_metadata_only(stats_data_id: str, app_id_arg: str | None) -> int:
    """getMetaInfo を 1 回呼び出し、軸メタデータを保存。"""
    print("[mode] --metadata-only")
    app_id = get_app_id(app_id_arg)
    print(f"[appId] {mask_app_id(app_id)}")
    print(f"[stats_data_id] {stats_data_id}")

    ensure_dirs()

    print(f"[GET] {ESTAT_API_BASE}/getMetaInfo")
    try:
        data = get_meta(stats_data_id, app_id)
    except requests.HTTPError as exc:
        print(f"[ERROR] HTTP {exc.response.status_code}: {exc.response.text[:300]}")
        return 1

    # API ステータス確認
    result = data.get("GET_META_INFO", {}).get("RESULT", {})
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    META_FILE.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[saved] {META_FILE} ({META_FILE.stat().st_size} bytes)")

    # 軸サマリ
    class_inf = (
        data.get("GET_META_INFO", {})
        .get("METADATA_INF", {})
        .get("CLASS_INF", {})
        .get("CLASS_OBJ", [])
    )
    if isinstance(class_inf, dict):
        class_inf = [class_inf]
    print(f"[axes] count={len(class_inf)}")
    for axis in class_inf:
        axis_id = axis.get("@id")
        axis_name = axis.get("@name")
        classes = axis.get("CLASS", [])
        if isinstance(classes, dict):
            classes = [classes]
        print(f"  - {axis_id} ({axis_name}): {len(classes)} codes")

    if status not in (0, "0"):
        print("[WARN] non-zero STATUS. statsDataId may need confirmation.")
        return 2
    return 0


def run_sample_only(stats_data_id: str, app_id_arg: str | None, limit: int) -> int:
    """getStatsData を 1 ページのみ呼び出してサンプル保存。"""
    print("[mode] --sample-only")
    app_id = get_app_id(app_id_arg)
    print(f"[appId] {mask_app_id(app_id)}")
    print(f"[stats_data_id] {stats_data_id}")
    print(f"[limit] {limit}")

    ensure_dirs()

    print(f"[GET] {ESTAT_API_BASE}/getStatsData (startPosition=1)")
    try:
        data = get_stats_data(stats_data_id, app_id, start_position=1, limit=limit)
    except requests.HTTPError as exc:
        print(f"[ERROR] HTTP {exc.response.status_code}: {exc.response.text[:300]}")
        return 1

    result = (
        data.get("GET_STATS_DATA", {})
        .get("RESULT", {})
    )
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    SAMPLE_FILE.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[saved] {SAMPLE_FILE} ({SAMPLE_FILE.stat().st_size} bytes)")

    # サンプル 5 行表示
    values = (
        data.get("GET_STATS_DATA", {})
        .get("STATISTICAL_DATA", {})
        .get("DATA_INF", {})
        .get("VALUE", [])
    )
    if isinstance(values, dict):
        values = [values]
    total_rows = len(values)
    print(f"[rows] total in this page: {total_rows}")
    for i, row in enumerate(values[:5]):
        print(f"  [{i}] {row}")

    # population 値の型確認
    if values:
        sample_val = values[0].get("$")
        print(f"[type] population value sample: {sample_val!r} (type={type(sample_val).__name__})")

    # 総件数
    result_inf = (
        data.get("GET_STATS_DATA", {})
        .get("STATISTICAL_DATA", {})
        .get("RESULT_INF", {})
    )
    print(f"[result_inf] {result_inf}")

    if status not in (0, "0"):
        return 2
    return 0


# --------------------------------------------------------------------------- #
# スケルトン (本タスクでは未実装)
# --------------------------------------------------------------------------- #

def fetch_all_pages(*args: Any, **kwargs: Any) -> None:
    raise NotImplementedError(
        "本格 fetch は別タスク。--metadata-only / --sample-only で確認後、"
        "ユーザー判断で本格 fetch を実施。"
    )


def merge_pages(*args: Any, **kwargs: Any) -> None:
    raise NotImplementedError(
        "--merge は別タスク。本格 fetch 完了後に実装。"
    )


def validate_clean_csv(*args: Any, **kwargs: Any) -> None:
    raise NotImplementedError(
        "--validate は別タスク。--merge 完了後に実装。"
    )


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="e-Stat 15-1 (sid=0003454508) fetch script",
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument(
        "--metadata-only", action="store_true",
        help="Fetch axis metadata (1 API call), no data rows",
    )
    mode.add_argument(
        "--sample-only", action="store_true",
        help="Fetch first page (1000 rows) for structure check",
    )
    mode.add_argument(
        "--dry-run", action="store_true",
        help="Verify CLI args + env without API call",
    )
    mode.add_argument(
        "--fetch", action="store_true",
        help="(SKELETON) Full paginated fetch",
    )
    mode.add_argument(
        "--merge", action="store_true",
        help="(SKELETON) Merge per-page CSVs",
    )
    mode.add_argument(
        "--validate", action="store_true",
        help="(SKELETON) Validate merged CSV",
    )

    parser.add_argument(
        "--app-id", default=None,
        help="e-Stat appId (overrides ESTAT_APP_ID env var)",
    )
    parser.add_argument(
        "--stats-data-id", default=DEFAULT_STATS_DATA_ID,
        help=f"e-Stat statsDataId (default: {DEFAULT_STATS_DATA_ID})",
    )
    parser.add_argument(
        "--from-page", type=int, default=None,
        help="(--fetch only) Resume from page N",
    )
    parser.add_argument(
        "--limit", type=int, default=100000,
        help="Rows per page (--fetch default 100000, --sample-only default 1000)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.dry_run:
        return run_dry_run(args.app_id)

    if args.metadata_only:
        return run_metadata_only(args.stats_data_id, args.app_id)

    if args.sample_only:
        # sample のデフォルト limit は 1000 (CLI で指定されない場合)
        sample_limit = 1000 if args.limit == 100000 else args.limit
        return run_sample_only(args.stats_data_id, args.app_id, sample_limit)

    if args.fetch:
        fetch_all_pages(
            stats_data_id=args.stats_data_id,
            app_id=get_app_id(args.app_id),
            from_page=args.from_page,
            limit=args.limit,
        )
        return 0  # unreachable (NotImplementedError)

    if args.merge:
        merge_pages()
        return 0

    if args.validate:
        validate_clean_csv()
        return 0

    parser.error("no mode selected")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
