"""
fetch_estat_15_1.py
====================

e-Stat 国勢調査 R2 表 15-1 (statdisp_id=0003454508) 取得スクリプト

実装ステータス:
- [OK] --dry-run / --metadata-only / --sample-only (Worker A5)
- [OK] --fetch / --merge / --validate (Worker A6)

設計書: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_ESTAT_15_1_PLAN.md (Worker A4)

CLI:
  python scripts/fetch_estat_15_1.py --dry-run
  $env:ESTAT_APP_ID = "your-app-id"
  python scripts/fetch_estat_15_1.py --metadata-only
  python scripts/fetch_estat_15_1.py --sample-only --limit 1000
  python scripts/fetch_estat_15_1.py --fetch                          # 本格 paginated fetch
  python scripts/fetch_estat_15_1.py --fetch --from-page 5            # resume
  python scripts/fetch_estat_15_1.py --merge                          # ローカル JSON のみ
  python scripts/fetch_estat_15_1.py --validate                       # CSV 整合性検証
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import sqlite3
import sys
import time
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
DEFAULT_STATS_DATA_ID = "0003454508"

OUTPUT_DIR = Path("data/generated")
TEMP_DIR = OUTPUT_DIR / "temp"
META_FILE = OUTPUT_DIR / "estat_15_1_metadata.json"
SAMPLE_FILE = TEMP_DIR / "estat_15_1_sample.json"
PROGRESS_FILE = OUTPUT_DIR / "estat_15_1_progress.json"
MERGED_CSV = OUTPUT_DIR / "estat_15_1_merged.csv"

MASTER_DB_PATH = Path("data/hellowork.db")

# 除外ルール (Worker A4 計画 §6 + 文字列ベース改訂)
EXCLUDE_AXIS_VALUES: dict[str, set[str]] = {
    "cat01": {"00000"},                   # 男女総数 (コード値)
    "cat02": {"00000", "9999"},           # 年齢: 総数 / 不詳 (コード値)
    "cat03": {"00000", "999", "0"},       # 職業: 総数 / 分類不能 ("0" は分類不能の職業)
    "area": set(),                         # area は別ロジック
}

# axis 名前ベース除外 (contains 判定)
EXCLUDE_NAME_PATTERNS: dict[str, list[str]] = {
    "cat01": ["総数"],
    "cat02": ["総数", "再掲"],            # "（再掲）..." を含む集約も除外
    "cat03": ["総数", "分類不能"],
}

# axis 名前ベース除外 (exact 一致のみ)
# "95歳以上" は最終 5 歳階級として残す。"65/75/85歳以上" は再掲集約のため除外。
EXCLUDE_NAME_EXACT: dict[str, set[str]] = {
    "cat02": {"65歳以上", "75歳以上", "85歳以上"},
}

OUTPUT_CSV_COLUMNS = [
    "municipality_code",
    "prefecture",
    "municipality_name",
    "gender",
    "age_class",
    "occupation_code",
    "occupation_name",
    "population",
    "source_name",
    "source_year",
    "fetched_at",
]

SOURCE_NAME = "census_15_1"
SOURCE_YEAR = 2020

# fetch パラメータ
SLEEP_SEC = 1.0
MAX_RETRIES = 5
DEFAULT_PAGE_SIZE = 100000


# --------------------------------------------------------------------------- #
# appId 管理
# --------------------------------------------------------------------------- #

def get_app_id(cli_arg: str | None = None) -> str:
    """appId 取得。CLI 引数 > 環境変数 ESTAT_APP_ID。.env 直読禁止。"""
    app_id = cli_arg or os.environ.get("ESTAT_APP_ID")
    if not app_id:
        raise SystemExit(
            "ERROR: appId not provided.\n"
            "  Set $env:ESTAT_APP_ID='your-app-id' (PowerShell) before running, "
            "or pass --app-id."
        )
    return app_id


def mask_app_id(app_id: str) -> str:
    if not app_id or len(app_id) < 6:
        return "***"
    return f"{app_id[:3]}{'*' * (len(app_id) - 5)}{app_id[-2:]}"


# --------------------------------------------------------------------------- #
# API 呼び出し (READ-only GET)
# --------------------------------------------------------------------------- #

def get_meta(stats_data_id: str, app_id: str) -> dict[str, Any]:
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
# ヘルパー
# --------------------------------------------------------------------------- #

def ensure_dirs() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    TEMP_DIR.mkdir(parents=True, exist_ok=True)


def _load_progress(path: Path, stats_data_id: str = DEFAULT_STATS_DATA_ID) -> dict[str, Any]:
    if path.exists():
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    return {
        "stats_data_id": stats_data_id,
        "next_position": 1,
        "completed_pages": 0,
        "total_estimated": 1762605,
        "started_at": datetime.now(timezone.utc).isoformat(),
        "last_fetched_at": None,
    }


def _save_progress(progress: dict[str, Any], path: Path) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump(progress, f, ensure_ascii=False, indent=2)


def _load_master_codes() -> set[str]:
    """master DB から市区町村コード集合を読み込む (read-only)。"""
    if not MASTER_DB_PATH.exists():
        print(f"[warn] master DB not found: {MASTER_DB_PATH}")
        return set()
    try:
        conn = sqlite3.connect(f"file:{MASTER_DB_PATH}?mode=ro", uri=True)
        try:
            rows = conn.execute(
                "SELECT DISTINCT municipality_code FROM municipality_code_master"
            ).fetchall()
        finally:
            conn.close()
        return {str(r[0]).zfill(5) for r in rows if r[0] is not None}
    except sqlite3.Error as e:
        print(f"[warn] master DB read failed: {e}")
        return set()


# --------------------------------------------------------------------------- #
# モード: dry-run / metadata-only / sample-only (既存維持)
# --------------------------------------------------------------------------- #

def run_dry_run(app_id_arg: str | None) -> int:
    print("[mode] --dry-run")
    print(f"[time] {datetime.now(timezone.utc).isoformat()}")
    app_id = get_app_id(app_id_arg)
    print(f"[appId] {mask_app_id(app_id)} (length={len(app_id)})")
    ensure_dirs()
    print(f"[dir] OUTPUT_DIR={OUTPUT_DIR.resolve()} exists={OUTPUT_DIR.exists()}")
    print(f"[dir] TEMP_DIR={TEMP_DIR.resolve()} exists={TEMP_DIR.exists()}")
    print(f"[plan] META_FILE   = {META_FILE}")
    print(f"[plan] SAMPLE_FILE = {SAMPLE_FILE}")
    print(f"[plan] PROGRESS    = {PROGRESS_FILE}")
    print("[OK] dry-run completed (no HTTP request issued).")
    return 0


def run_metadata_only(stats_data_id: str, app_id_arg: str | None) -> int:
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

    result = data.get("GET_META_INFO", {}).get("RESULT", {})
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    META_FILE.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[saved] {META_FILE} ({META_FILE.stat().st_size} bytes)")

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

    result = data.get("GET_STATS_DATA", {}).get("RESULT", {})
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    SAMPLE_FILE.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[saved] {SAMPLE_FILE} ({SAMPLE_FILE.stat().st_size} bytes)")

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

    if values:
        sample_val = values[0].get("$")
        print(f"[type] population value sample: {sample_val!r} (type={type(sample_val).__name__})")

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
# モード: --fetch (本実装)
# --------------------------------------------------------------------------- #

def fetch_all_pages(
    stats_data_id: str,
    app_id: str,
    page_size: int = DEFAULT_PAGE_SIZE,
    from_page: int | None = None,
    progress_path: Path = PROGRESS_FILE,
    output_dir: Path = TEMP_DIR,
) -> dict[str, Any]:
    """ページング fetch。中断耐性 (progress.json 経由 resume)。"""
    output_dir.mkdir(parents=True, exist_ok=True)
    progress = _load_progress(progress_path, stats_data_id)
    progress["stats_data_id"] = stats_data_id

    if from_page is not None:
        if from_page < 1:
            raise SystemExit(f"--from-page must be >= 1 (got {from_page})")
        progress["next_position"] = (from_page - 1) * page_size + 1
        progress["completed_pages"] = from_page - 1
        print(f"[resume] from_page={from_page} -> next_position={progress['next_position']}")

    while True:
        page_num = (progress["next_position"] - 1) // page_size + 1
        page_path = output_dir / f"estat_15_1_page_{page_num:03d}.json"

        # 既存ページはスキップ (idempotent)
        if page_path.exists():
            print(f"[skip] page {page_num} already exists: {page_path}")
            progress["next_position"] += page_size
            progress["completed_pages"] = max(progress["completed_pages"], page_num)
            _save_progress(progress, progress_path)
            continue

        # API request with exponential backoff
        resp_json: dict[str, Any] | None = None
        last_err: Exception | None = None
        for retry in range(MAX_RETRIES):
            try:
                resp_json = get_stats_data(
                    stats_data_id, app_id,
                    start_position=progress["next_position"],
                    limit=page_size,
                )
                break
            except (requests.HTTPError, requests.Timeout, requests.ConnectionError) as e:
                last_err = e
                wait = min((2 ** retry) * 2, 60)  # 2,4,8,16,32 (cap 60)
                print(f"[retry {retry+1}/{MAX_RETRIES}] {e}, sleeping {wait}s")
                time.sleep(wait)
        if resp_json is None:
            print(f"[error] page {page_num} failed after {MAX_RETRIES} retries: {last_err}")
            raise SystemExit(1)

        # API ステータス検査
        result = resp_json.get("GET_STATS_DATA", {}).get("RESULT", {})
        result_status = result.get("STATUS")
        if result_status not in (0, "0"):
            err_msg = result.get("ERROR_MSG", "unknown")
            raise SystemExit(f"[error] API returned status {result_status}: {err_msg}")

        data_inf = (
            resp_json.get("GET_STATS_DATA", {})
            .get("STATISTICAL_DATA", {})
            .get("DATA_INF", {})
        )
        values = data_inf.get("VALUE", [])
        if isinstance(values, dict):
            values = [values]

        # 完走判定
        if len(values) == 0:
            print(f"[done] no more data at page {page_num}")
            break

        # 保存
        with open(page_path, "w", encoding="utf-8") as f:
            json.dump(resp_json, f, ensure_ascii=False)
        print(f"[saved] page {page_num} ({len(values)} cells) -> {page_path}")

        # progress 更新
        progress["next_position"] += len(values)
        progress["completed_pages"] = page_num
        progress["last_fetched_at"] = datetime.now(timezone.utc).isoformat()
        _save_progress(progress, progress_path)

        # NEXT_KEY が無ければ完走
        result_inf = (
            resp_json.get("GET_STATS_DATA", {})
            .get("STATISTICAL_DATA", {})
            .get("RESULT_INF", {})
        )
        next_key = result_inf.get("NEXT_KEY")
        if not next_key:
            print(f"[done] reached end at page {page_num} (no NEXT_KEY)")
            break

        # 次ポインタ調整 (NEXT_KEY が示す位置を優先)
        try:
            progress["next_position"] = int(next_key)
            _save_progress(progress, progress_path)
        except (TypeError, ValueError):
            pass

        time.sleep(SLEEP_SEC)

    return progress


# --------------------------------------------------------------------------- #
# モード: --merge (本実装)
# --------------------------------------------------------------------------- #

def load_axis_metadata(metadata_path: Path) -> dict[str, dict[str, str]]:
    """メタデータ JSON から軸コード → 名前辞書を構築。"""
    with open(metadata_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    class_inf = (
        data.get("GET_META_INFO", {})
        .get("METADATA_INF", {})
        .get("CLASS_INF", {})
        .get("CLASS_OBJ", [])
    )
    if isinstance(class_inf, dict):
        class_inf = [class_inf]

    axis_map: dict[str, dict[str, str]] = {}
    for axis in class_inf:
        axis_id = axis.get("@id", "")
        classes = axis.get("CLASS", [])
        if isinstance(classes, dict):
            classes = [classes]
        code_map = {}
        for cls in classes:
            code = cls.get("@code", "")
            name = cls.get("@name", "")
            code_map[code] = name
        axis_map[axis_id] = code_map

    return axis_map


def is_excluded(
    record: dict[str, Any],
    axis_map: dict[str, dict[str, str]] | None = None,
) -> bool:
    """API レスポンスの 1 セルを除外判定。True なら除外。

    判定は次の順:
      a. axis コード値除外 (EXCLUDE_AXIS_VALUES)
      b. axis 名前 contains 除外 (EXCLUDE_NAME_PATTERNS、axis_map 必須)
      c. axis 名前 exact 除外 (EXCLUDE_NAME_EXACT、axis_map 必須)
      d. area 形式判定 (00000、xx000、5 桁非数字)
    """
    cat01 = str(record.get("@cat01", ""))
    cat02 = str(record.get("@cat02", ""))
    cat03 = str(record.get("@cat03", ""))
    area = str(record.get("@area", ""))

    # (a) コード値除外
    if cat01 in EXCLUDE_AXIS_VALUES["cat01"]:
        return True
    if cat02 in EXCLUDE_AXIS_VALUES["cat02"]:
        return True
    if cat03 in EXCLUDE_AXIS_VALUES["cat03"]:
        return True

    # (b)/(c) 名前ベース除外 (axis_map 提供時のみ)
    if axis_map is not None:
        cat01_name = axis_map.get("cat01", {}).get(cat01, "")
        cat02_name = axis_map.get("cat02", {}).get(cat02, "")
        cat03_name = axis_map.get("cat03", {}).get(cat03, "")

        for pat in EXCLUDE_NAME_PATTERNS.get("cat01", []):
            if pat in cat01_name:
                return True
        for pat in EXCLUDE_NAME_PATTERNS.get("cat02", []):
            if pat in cat02_name:
                return True
        if cat02_name in EXCLUDE_NAME_EXACT.get("cat02", set()):
            return True
        for pat in EXCLUDE_NAME_PATTERNS.get("cat03", []):
            if pat in cat03_name:
                return True

    # (d) area
    if area == "00000":
        return True
    if len(area) == 5 and area[2:5] == "000":
        return True
    if len(area) != 5 or not area.isdigit():
        return True
    return False


def _normalize_gender(cat01_code: str, cat01_name: str) -> str:
    """男女コード/名前から 'male' / 'female' 正規化。"""
    name = cat01_name or ""
    if "男" in name:
        return "male"
    if "女" in name:
        return "female"
    # フォールバック (コード数字判定)
    if cat01_code in ("01", "1"):
        return "male"
    if cat01_code in ("02", "2"):
        return "female"
    return cat01_name or cat01_code


def _normalize_age_class(cat02_code: str, cat02_name: str) -> str:
    """年齢階級ラベル正規化。例: '15～19歳' -> '15-19'。"""
    name = cat02_name or ""
    # 全角チルダ／ハイフン正規化
    for sep in ("～", "〜", "-", "－", "ー"):
        if sep in name:
            parts = name.split(sep, 1)
            left = parts[0].replace("歳", "").strip()
            right = parts[1].replace("歳", "").replace("以上", "").strip()
            if left and right:
                return f"{left}-{right}"
            if left:
                return f"{left}+"
    # "65歳以上" 系
    if "以上" in name:
        digits = "".join(ch for ch in name if ch.isdigit())
        if digits:
            return f"{digits}+"
    return name or cat02_code


def _prefecture_from_area(area_code: str, axis_map: dict[str, dict[str, str]]) -> str:
    """area code 上 2 桁から都道府県名を取得。"""
    if len(area_code) < 2:
        return ""
    pref_code = area_code[:2] + "000"  # 例: 13000
    area_axis = axis_map.get("area", {})
    return area_axis.get(pref_code, "")


def merge_pages(
    pages_dir: Path = TEMP_DIR,
    output_csv: Path = MERGED_CSV,
    metadata_path: Path = META_FILE,
) -> dict[str, Any]:
    """複数 page JSON を 1 CSV にマージ + 除外フィルタ適用。"""
    if not metadata_path.exists():
        raise SystemExit(f"metadata file not found: {metadata_path}. Run --metadata-only first.")

    page_files = sorted(pages_dir.glob("estat_15_1_page_*.json"))
    if not page_files:
        raise SystemExit(f"no page JSONs found in {pages_dir}. Run --fetch first.")

    print(f"[merge] {len(page_files)} page files found")
    axis_map = load_axis_metadata(metadata_path)
    print(f"[merge] axis codes loaded: " + ", ".join(
        f"{k}={len(v)}" for k, v in axis_map.items()
    ))

    output_csv.parent.mkdir(parents=True, exist_ok=True)
    fetched_at = datetime.now(timezone.utc).isoformat()

    raw_rows = 0
    excluded_rows = 0
    written_rows = 0

    cat01_axis = axis_map.get("cat01", {})
    cat02_axis = axis_map.get("cat02", {})
    cat03_axis = axis_map.get("cat03", {})
    area_axis = axis_map.get("area", {})

    with open(output_csv, "w", encoding="utf-8", newline="") as f_out:
        writer = csv.DictWriter(f_out, fieldnames=OUTPUT_CSV_COLUMNS)
        writer.writeheader()

        for page_path in page_files:
            with open(page_path, "r", encoding="utf-8") as f_in:
                data = json.load(f_in)
            values = (
                data.get("GET_STATS_DATA", {})
                .get("STATISTICAL_DATA", {})
                .get("DATA_INF", {})
                .get("VALUE", [])
            )
            if isinstance(values, dict):
                values = [values]

            for rec in values:
                raw_rows += 1
                if is_excluded(rec, axis_map):
                    excluded_rows += 1
                    continue

                cat01_code = str(rec.get("@cat01", ""))
                cat02_code = str(rec.get("@cat02", ""))
                cat03_code = str(rec.get("@cat03", ""))
                area_code = str(rec.get("@area", "")).zfill(5)
                pop_raw = rec.get("$", "")

                # 数値変換 (欠損記号 '-' '*' 'X' 等は 0 にフォールバック)
                try:
                    population = int(pop_raw)
                except (TypeError, ValueError):
                    try:
                        population = int(float(pop_raw))
                    except (TypeError, ValueError):
                        population = 0

                writer.writerow({
                    "municipality_code": area_code,
                    "prefecture": _prefecture_from_area(area_code, axis_map),
                    "municipality_name": area_axis.get(area_code, ""),
                    "gender": _normalize_gender(cat01_code, cat01_axis.get(cat01_code, "")),
                    "age_class": _normalize_age_class(cat02_code, cat02_axis.get(cat02_code, "")),
                    "occupation_code": cat03_code,
                    "occupation_name": cat03_axis.get(cat03_code, ""),
                    "population": population,
                    "source_name": SOURCE_NAME,
                    "source_year": SOURCE_YEAR,
                    "fetched_at": fetched_at,
                })
                written_rows += 1

    result = {
        "raw_rows": raw_rows,
        "filtered_rows": written_rows,
        "excluded_rows": excluded_rows,
        "csv_path": str(output_csv),
    }
    return result


# --------------------------------------------------------------------------- #
# モード: --validate (本実装)
# --------------------------------------------------------------------------- #

SAMPLE_TARGETS: list[tuple[str, str, str]] = [
    ("13103", "東京都", "港区"),
    ("13104", "東京都", "新宿区"),
    ("13201", "東京都", "八王子市"),
    ("23211", "愛知県", "豊田市"),
]


def _expected_rows_from_metadata(metadata_path: Path) -> int | None:
    """metadata の TABLE_INF.OVERALL_TOTAL_NUMBER を expected_total として返す。

    取得不能なら None。
    """
    if not metadata_path.exists():
        return None
    try:
        with open(metadata_path, "r", encoding="utf-8") as fh:
            meta = json.load(fh)
    except Exception:
        return None
    table_inf = (
        meta.get("GET_META_INFO", {})
        .get("METADATA_INF", {})
        .get("TABLE_INF", {})
    )
    val = table_inf.get("OVERALL_TOTAL_NUMBER") or table_inf.get("@overall_total_number")
    if val is None:
        return None
    try:
        return int(val)
    except (TypeError, ValueError):
        return None


def validate_clean_csv(
    csv_path: Path = MERGED_CSV,
    metadata_path: Path = META_FILE,
) -> tuple[bool, list[str]]:
    """マージ後 CSV の整合性検証 (改訂: row count はソフトチェック、軸/サンプル中心)。

    NG (errors): municipality_code 形式違反、全国/都道府県集約混入、PK 重複、サンプル地域欠損、
                 population 数値変換不能、負値、empty CSV
    WARN (logs): row count 期待値乖離、prefecture 数、orphan 率、PK 軸 distinct 不足、
                 総数/不詳の混入痕跡
    """
    errors: list[str] = []
    warnings: list[str] = []

    if not csv_path.exists():
        return False, [f"merged CSV not found: {csv_path}. Run --merge first."]

    try:
        import pandas as pd
    except ImportError:
        return False, ["pandas is required for --validate. pip install pandas"]

    df = pd.read_csv(csv_path, dtype={"municipality_code": str})
    n_rows = len(df)
    print(f"[info] row count: {n_rows:,}")

    if n_rows == 0:
        errors.append("row count is 0 (empty CSV)")
        return False, errors

    # ----------------------------------------------------------------- #
    # 1. row count の soft check (metadata 由来の expected と比較)
    # ----------------------------------------------------------------- #
    expected_total = _expected_rows_from_metadata(metadata_path)
    if expected_total:
        # OVERALL_TOTAL_NUMBER は除外前の cell 数。除外後は概ね 95-105% に収まる想定
        # (除外: 男女総数 ~33%、年齢総数 ~4%、職業総数 ~8%、全国/都道府県 area ~3%)
        # 厳密な乖離判定を避け、極端な乖離のみ警告 (50% 超)
        diff_rate = abs(n_rows - expected_total) / expected_total
        print(f"[info] expected from metadata (raw cells): {expected_total:,}")
        print(f"[info] row count diff vs metadata: {diff_rate:.1%}")
        if diff_rate > 0.50:
            warnings.append(
                f"row count {n_rows:,} differs from metadata raw {expected_total:,} by {diff_rate:.1%}"
            )
    else:
        # metadata 不在時の soft レンジ (実取得結果 1.72M を許容)
        SOFT_MIN, SOFT_MAX = 1_500_000, 1_900_000
        if not (SOFT_MIN <= n_rows <= SOFT_MAX):
            warnings.append(
                f"row count {n_rows:,} outside soft range [{SOFT_MIN:,}, {SOFT_MAX:,}]"
            )

    # ----------------------------------------------------------------- #
    # 2. municipality_code 5 桁
    # ----------------------------------------------------------------- #
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    bad_code = df[~df["municipality_code"].str.match(r"^\d{5}$")]
    if len(bad_code) > 0:
        errors.append(f"municipality_code not 5-digit: {len(bad_code):,} rows")

    # ----------------------------------------------------------------- #
    # 3. 全国 (00000) 除外確認
    # ----------------------------------------------------------------- #
    nationwide = df[df["municipality_code"] == "00000"]
    if len(nationwide) > 0:
        errors.append(f"nationwide rows (code=00000) present: {len(nationwide):,}")

    # ----------------------------------------------------------------- #
    # 4. 都道府県 (xx000) 除外確認
    # ----------------------------------------------------------------- #
    pref_only = df[
        df["municipality_code"].str.endswith("000")
        & (df["municipality_code"] != "00000")
    ]
    if len(pref_only) > 0:
        errors.append(f"prefecture-aggregate rows (xx000) present: {len(pref_only):,}")

    # ----------------------------------------------------------------- #
    # 5. population 数値変換
    # ----------------------------------------------------------------- #
    pop_numeric = pd.to_numeric(df["population"], errors="coerce")
    nan_count = int(pop_numeric.isna().sum())
    if nan_count > 0:
        # 数値変換できない値が多いと NG、少量なら WARN
        nan_rate = nan_count / n_rows
        if nan_rate > 0.05:
            errors.append(f"population numeric conversion failed: {nan_count:,} rows ({nan_rate:.1%})")
        else:
            warnings.append(f"population NaN: {nan_count:,} rows ({nan_rate:.2%})")
    df["population"] = pop_numeric.fillna(0).astype("int64")
    neg_count = int((df["population"] < 0).sum())
    if neg_count > 0:
        errors.append(f"negative population values: {neg_count:,} rows")
    total_pop = int(df["population"].sum())
    print(f"[info] sum population: {total_pop:,}")
    if total_pop < 30_000_000:
        warnings.append(f"sum population {total_pop:,} unusually low (< 30M)")

    # ----------------------------------------------------------------- #
    # 6. gender / age / occupation コード分布 (ログ)
    # ----------------------------------------------------------------- #
    print(f"[dist] gender ({df['gender'].nunique()} distinct):")
    for k, v in df["gender"].value_counts().head(10).items():
        print(f"  {k!r}: {v:,}")
    print(f"[dist] age_class ({df['age_class'].nunique()} distinct):")
    for k, v in df["age_class"].value_counts().head(30).items():
        print(f"  {k!r}: {v:,}")
    print(f"[dist] occupation_code ({df['occupation_code'].nunique()} distinct):")
    for k, v in df["occupation_code"].value_counts().head(20).items():
        print(f"  {k!r}: {v:,}")

    # 軸 distinct の WARN (ハード NG にしない)
    n_gender = df["gender"].nunique()
    if n_gender < 2:
        warnings.append(f"distinct genders = {n_gender}, expected >= 2")
    n_age = df["age_class"].nunique()
    if n_age < 14:
        warnings.append(f"distinct age classes = {n_age}, expected >= 14")
    n_occ = df["occupation_code"].nunique()
    if n_occ < 11:
        warnings.append(f"distinct occupations = {n_occ}, expected >= 11")

    # ----------------------------------------------------------------- #
    # 7. 代表地域サンプル存在確認
    # ----------------------------------------------------------------- #
    print("[sample] representative municipalities:")
    for code, pref, name in SAMPLE_TARGETS:
        sub = df[df["municipality_code"] == code]
        if len(sub) == 0:
            errors.append(f"sample missing: {code} ({pref} {name})")
            print(f"  {code} ({pref} {name}): MISSING")
        else:
            sub_total = int(sub["population"].sum())
            sub_pref = sub["prefecture"].dropna().iloc[0] if not sub["prefecture"].dropna().empty else "?"
            sub_name = (
                sub["municipality_name"].dropna().iloc[0]
                if not sub["municipality_name"].dropna().empty
                else "?"
            )
            print(
                f"  {code} ({pref} {name}): {len(sub):,} rows, "
                f"sum_pop={sub_total:,}, csv_label={sub_pref}/{sub_name}"
            )

    # ----------------------------------------------------------------- #
    # 8. 総数・不詳の取り扱いログ (除外漏れの痕跡検出)
    # ----------------------------------------------------------------- #
    aggregate_pat = "総数|不詳|分類不能"
    for col in ("gender", "age_class", "occupation_code", "occupation_name"):
        if col not in df.columns:
            continue
        m = df[col].astype(str).str.contains(aggregate_pat, na=False)
        n = int(m.sum())
        if n > 0:
            warnings.append(f"aggregate/unknown remnant in {col}: {n:,} rows")
            sample_vals = df.loc[m, col].value_counts().head(3).to_dict()
            print(f"[warn] {col} aggregate/unknown sample: {sample_vals}")

    # ----------------------------------------------------------------- #
    # 9. PK 重複
    # ----------------------------------------------------------------- #
    pk_cols = ["municipality_code", "gender", "age_class", "occupation_code"]
    dup_count = int(df.duplicated(subset=pk_cols).sum())
    if dup_count > 0:
        errors.append(f"PK duplicates: {dup_count:,}")

    # ----------------------------------------------------------------- #
    # 10. 都道府県カバレッジ (WARN)
    # ----------------------------------------------------------------- #
    n_pref = df["prefecture"].dropna().nunique()
    print(f"[info] distinct prefectures: {n_pref}")
    if n_pref != 47:
        warnings.append(f"distinct prefectures = {n_pref}, expected 47")

    # ----------------------------------------------------------------- #
    # 11. master 突合 (WARN、master 不在ならスキップ)
    # ----------------------------------------------------------------- #
    master_codes = _load_master_codes()
    if master_codes:
        csv_codes = set(df["municipality_code"])
        orphan = csv_codes - master_codes
        orphan_rate = len(orphan) / max(len(csv_codes), 1)
        print(f"[info] master orphan: {len(orphan):,} / {len(csv_codes):,} ({orphan_rate:.2%})")
        if orphan_rate > 0.10:
            warnings.append(
                f"master orphan rate {orphan_rate:.2%} ({len(orphan):,} orphans) — review code joining"
            )
    else:
        print("[info] master DB unavailable; orphan check skipped")

    # ----------------------------------------------------------------- #
    # 集約ログ
    # ----------------------------------------------------------------- #
    if warnings:
        print(f"[warn] {len(warnings)} warnings:")
        for w in warnings:
            print(f"  - {w}")

    return (len(errors) == 0, errors)


# --------------------------------------------------------------------------- #
# CLI ラッパ
# --------------------------------------------------------------------------- #

def run_fetch(args: argparse.Namespace) -> int:
    print("[mode] --fetch")
    app_id = get_app_id(args.app_id)
    print(f"[appId] {mask_app_id(app_id)} (length={len(app_id)})")
    print(f"[stats_data_id] {args.stats_data_id}")
    print(f"[page_size] {args.limit}")
    if args.from_page:
        print(f"[from_page] {args.from_page}")
    ensure_dirs()
    progress = fetch_all_pages(
        stats_data_id=args.stats_data_id,
        app_id=app_id,
        page_size=args.limit,
        from_page=args.from_page,
    )
    print(f"[done] completed_pages={progress['completed_pages']}, "
          f"next_position={progress['next_position']}")
    return 0


def run_merge(args: argparse.Namespace) -> int:
    print("[mode] --merge")
    ensure_dirs()
    if not META_FILE.exists():
        raise SystemExit(f"metadata file not found: {META_FILE}. Run --metadata-only first.")
    if not any(TEMP_DIR.glob("estat_15_1_page_*.json")):
        raise SystemExit(f"no page JSONs found in {TEMP_DIR}. Run --fetch first.")
    result = merge_pages(pages_dir=TEMP_DIR, output_csv=MERGED_CSV, metadata_path=META_FILE)
    print(
        f"[merged] raw={result['raw_rows']:,}, "
        f"filtered={result['filtered_rows']:,}, "
        f"excluded={result['excluded_rows']:,}"
    )
    print(f"[output] {result['csv_path']}")
    return 0


def run_validate(args: argparse.Namespace) -> int:
    print("[mode] --validate")
    csv_path = MERGED_CSV
    if not csv_path.exists():
        raise SystemExit(f"merged CSV not found: {csv_path}. Run --merge first.")
    is_valid, errors = validate_clean_csv(csv_path)
    if is_valid:
        print("[OK] CSV validation passed")
        return 0
    print(f"[NG] {len(errors)} errors:")
    for e in errors:
        print(f"  - {e}")
    return 1


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="e-Stat 15-1 (sid=0003454508) fetch script",
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--metadata-only", action="store_true",
                      help="Fetch axis metadata (1 API call), no data rows")
    mode.add_argument("--sample-only", action="store_true",
                      help="Fetch first page (1000 rows) for structure check")
    mode.add_argument("--dry-run", action="store_true",
                      help="Verify CLI args + env without API call")
    mode.add_argument("--fetch", action="store_true",
                      help="Full paginated fetch (uses appId, writes progress.json)")
    mode.add_argument("--merge", action="store_true",
                      help="Merge per-page JSONs into clean CSV (no API call)")
    mode.add_argument("--validate", action="store_true",
                      help="Validate merged CSV (no API call)")

    parser.add_argument("--app-id", default=None,
                        help="e-Stat appId (overrides ESTAT_APP_ID env var)")
    parser.add_argument("--stats-data-id", default=DEFAULT_STATS_DATA_ID,
                        help=f"e-Stat statsDataId (default: {DEFAULT_STATS_DATA_ID})")
    parser.add_argument("--from-page", type=int, default=None,
                        help="(--fetch) Resume from page N (1-indexed)")
    parser.add_argument("--limit", type=int, default=DEFAULT_PAGE_SIZE,
                        help=f"Rows per page (--fetch default {DEFAULT_PAGE_SIZE}, "
                             "--sample-only default 1000, max 100000)")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.dry_run:
        return run_dry_run(args.app_id)
    if args.metadata_only:
        return run_metadata_only(args.stats_data_id, args.app_id)
    if args.sample_only:
        sample_limit = 1000 if args.limit == DEFAULT_PAGE_SIZE else args.limit
        return run_sample_only(args.stats_data_id, args.app_id, sample_limit)
    if args.fetch:
        return run_fetch(args)
    if args.merge:
        return run_merge(args)
    if args.validate:
        return run_validate(args)

    parser.error("no mode selected")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
