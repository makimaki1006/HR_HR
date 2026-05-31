"""
fetch_rental_housing.py
========================

e-Stat 住宅・土地統計調査 (政府統計コード 00200522) から
市区町村×構造別×専有面積階級別の借家住戸数と家賃中央値を取得して
CSV に出力するスクリプト。

🟡 ファイルの配置先 (parent コピー先): scripts/fetch_rental_housing.py
   このファイルは agent sandbox 書込制限のため src/handlers/survey/_drafts_rental_2026_05_31/ に
   draft として配置されている。parent (ユーザー側) で scripts/ にコピーすること。

設計方針:
  - 案 R-A (docs/audit_2026_04_24/survey_data_activation_plan.md:831-863) 準拠
  - Phase 3 STEP5 前提 (docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md:104-124)
  - 既存パターン踏襲: fetch_industry_structure.py + fetch_estat_15_1.py の組み合わせ
  - statsDataId 動的取得 (2 段階方式)
    * 住宅・土地統計は表が大量に存在し、家賃・住戸数で別々の statsDataId
    * --metadata-only で一覧取得 → ユーザーが目視で STATS_DATA_ID を確定 → 本実行
    * 固定値ではなく 2 段階方式にする理由:
        - e-Stat 側の表 ID は再公表時に変化する実績あり
        - 2018 年実施分 (旧) と 2023 年実施分 (新) で表 ID が異なる
        - 表が「住戸数のみ」「家賃のみ」「住戸×家賃合体」など複数存在し、用途で選別が必要

データ仕様:
  - 統計表ID群: 00200522 (政府統計コード) の中から借家×家賃の表を選択
  - 対象: 47 都道府県 + 政令指定都市 + 人口 5 万人以上の市区町村 (約 100)
  - セグメント:
      * 構造: 木造 / 防火木造 / 鉄筋・鉄骨コンクリート造 / 鉄骨造
      * 専有面積階級: 29m² 以下 / 30-49 / 50-69 / 70-99 / 100m² 以上

CLI:
  python scripts/fetch_rental_housing.py --dry-run
  $env:ESTAT_APP_ID = "your-app-id"
  python scripts/fetch_rental_housing.py --metadata-only       # statsDataId 候補一覧
  python scripts/fetch_rental_housing.py --inspect-meta        # 確定済 STATS_DATA_ID の軸構造確認
  python scripts/fetch_rental_housing.py --sample-only         # 1 ページサンプル
  python scripts/fetch_rental_housing.py --fetch               # 本実行 (CSV 出力)
  python scripts/fetch_rental_housing.py --validate            # CSV 検証

参考:
  - 既存 fetch_estat_15_1.py (e-Stat API + pagination + 進捗管理)
  - 既存 fetch_industry_structure.py (市区町村ループ + 進捗ファイル)
  - 既存 fetch_geo_supplement.py (47 都道府県マッピング)
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import sys
import time
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# UTF-8 logging (cp932 対応)
try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass


# --------------------------------------------------------------------------- #
# 定数
# --------------------------------------------------------------------------- #

ESTAT_API_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json"
ESTAT_LIST_API = f"{ESTAT_API_BASE}/getStatsList"
ESTAT_META_API = f"{ESTAT_API_BASE}/getMetaInfo"
ESTAT_DATA_API = f"{ESTAT_API_BASE}/getStatsData"

# 政府統計コード: 住宅・土地統計調査
GOV_STATS_CODE = "00200522"

# 🔴 STATS_DATA_ID: 必ず --metadata-only で表一覧を取得した後に手動更新すること
# 暫定値 (要置き換え):
#   2018 年実施の住宅・土地統計の借家家賃に関する代表的な表 ID。
#   2023 年実施分 (2024 公表) は --metadata-only 実行後に最新値を確定する。
STATS_DATA_ID = "0003228366"  # 🔴 要確認: --metadata-only で最新の表 ID に置き換える

# 出力先 (既存 fetch_industry_structure.py 等と同じ scripts/data/ に出力)
SCRIPT_DIR = Path(__file__).parent
DATA_DIR = SCRIPT_DIR / "data"
OUTPUT_CSV = DATA_DIR / "rental_housing_2026.csv"

# メタ情報キャッシュ (statsDataId 候補一覧 / 軸構造)
META_LIST_CACHE = DATA_DIR / "rental_housing_stats_list.json"
META_INFO_CACHE = DATA_DIR / "rental_housing_meta_info.json"

# 進捗ファイル (中断耐性)
PROGRESS_FILE = DATA_DIR / "rental_housing.progress"

# API 呼び出し設定
REQUEST_INTERVAL_SEC = 1.0  # レート制限 (e-Stat 推奨)
MAX_RETRIES = 5
PAGE_SIZE = 100000  # e-Stat API の 1 ページ最大件数

# 出力 CSV カラム
OUTPUT_COLUMNS = [
    "prefecture",       # 都道府県名 (例: "東京都")
    "municipality",     # 市区町村名 (例: "新宿区"、全国/都道府県集計は空)
    "structure",        # 構造 (例: "木造", "鉄筋コンクリート造")
    "area_class",       # 専有面積階級 (例: "29m²以下", "30-49m²")
    "rental_total_units",  # 借家住戸数 (該当セグメント、整数)
    "median_rent_jpy",  # 家賃中央値 (円/月、家賃データがある場合のみ)
    "as_of",            # データ基準年 (例: "2023")
    "fetched_at",       # 取得日時 (ISO8601 UTC)
]

# 都道府県コード (2 桁) → 名称マッピング
PREF_2DIGIT_MAP = {
    "01": "北海道", "02": "青森県", "03": "岩手県", "04": "宮城県",
    "05": "秋田県", "06": "山形県", "07": "福島県", "08": "茨城県",
    "09": "栃木県", "10": "群馬県", "11": "埼玉県", "12": "千葉県",
    "13": "東京都", "14": "神奈川県", "15": "新潟県", "16": "富山県",
    "17": "石川県", "18": "福井県", "19": "山梨県", "20": "長野県",
    "21": "岐阜県", "22": "静岡県", "23": "愛知県", "24": "三重県",
    "25": "滋賀県", "26": "京都府", "27": "大阪府", "28": "兵庫県",
    "29": "奈良県", "30": "和歌山県", "31": "鳥取県", "32": "島根県",
    "33": "岡山県", "34": "広島県", "35": "山口県", "36": "徳島県",
    "37": "香川県", "38": "愛媛県", "39": "高知県", "40": "福岡県",
    "41": "佐賀県", "42": "長崎県", "43": "熊本県", "44": "大分県",
    "45": "宮崎県", "46": "鹿児島県", "47": "沖縄県",
}

# 構造名の正規化マッピング (e-Stat 表記揺れ吸収)
STRUCTURE_NORMALIZE = {
    "木造": "木造",
    "防火木造": "防火木造",
    "木造（防火木造を除く）": "木造",
    "鉄筋・鉄骨コンクリート造": "鉄筋・鉄骨コンクリート造",
    "鉄筋コンクリート造": "鉄筋・鉄骨コンクリート造",
    "鉄骨鉄筋コンクリート造": "鉄筋・鉄骨コンクリート造",
    "鉄骨造": "鉄骨造",
    "その他": "その他",
    "総数": "総数",
}

# 面積階級名の正規化マッピング
AREA_CLASS_NORMALIZE = {
    "29㎡以下": "29m²以下",
    "29m2以下": "29m²以下",
    "30~49㎡": "30-49m²",
    "30〜49㎡": "30-49m²",
    "30~49m2": "30-49m²",
    "30〜49m2": "30-49m²",
    "50~69㎡": "50-69m²",
    "50〜69㎡": "50-69m²",
    "50~69m2": "50-69m²",
    "50〜69m2": "50-69m²",
    "70~99㎡": "70-99m²",
    "70〜99㎡": "70-99m²",
    "70~99m2": "70-99m²",
    "70〜99m2": "70-99m²",
    "100㎡以上": "100m²以上",
    "100m2以上": "100m²以上",
    "総数": "総数",
}

# データ基準年 (2023 年実施・2024 年公表)
AS_OF_YEAR = "2023"


# --------------------------------------------------------------------------- #
# appId 管理 (既存 fetch_estat_15_1.py パターン踏襲)
# --------------------------------------------------------------------------- #

# 既存スクリプトとの互換性のためハードコード fallback も用意
# (fetch_industry_structure.py / fetch_geo_supplement.py 方式)
DEFAULT_APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"


def get_app_id(cli_arg: str | None = None) -> str:
    """appId 取得。優先順: CLI 引数 > 環境変数 ESTAT_APP_ID > DEFAULT_APP_ID。"""
    app_id = cli_arg or os.environ.get("ESTAT_APP_ID") or DEFAULT_APP_ID
    if not app_id:
        raise SystemExit(
            "ERROR: appId not provided.\n"
            "  Set $env:ESTAT_APP_ID='your-app-id' (PowerShell) before running, "
            "or pass --app-id, or rely on DEFAULT_APP_ID."
        )
    return app_id


def mask_app_id(app_id: str) -> str:
    if not app_id or len(app_id) < 6:
        return "***"
    return f"{app_id[:3]}{'*' * (len(app_id) - 5)}{app_id[-2:]}"


# --------------------------------------------------------------------------- #
# API 呼び出し (READ-only GET, urllib ベース)
# --------------------------------------------------------------------------- #

def _http_get(url: str, timeout: int = 60) -> dict[str, Any]:
    """GET request with retry. JSON parse まで実行。"""
    last_err: Exception | None = None
    for attempt in range(MAX_RETRIES):
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except Exception as e:  # noqa: BLE001
            last_err = e
            wait = min((2 ** attempt) * 2, 60)
            print(f"  [retry {attempt + 1}/{MAX_RETRIES}] {e} -> sleep {wait}s")
            time.sleep(wait)
    raise SystemExit(f"HTTP failed after {MAX_RETRIES} retries: {last_err}")


def get_stats_list(app_id: str, gov_stats_code: str) -> dict[str, Any]:
    """政府統計コードから表一覧を取得 (statsDataId 候補発見用)。"""
    params = {
        "appId": app_id,
        "statsCode": gov_stats_code,
        "lang": "J",
        "limit": 1000,
    }
    url = f"{ESTAT_LIST_API}?{urllib.parse.urlencode(params)}"
    return _http_get(url)


def get_meta_info(app_id: str, stats_data_id: str) -> dict[str, Any]:
    """指定 statsDataId の軸情報を取得。"""
    params = {
        "appId": app_id,
        "statsDataId": stats_data_id,
        "lang": "J",
    }
    url = f"{ESTAT_META_API}?{urllib.parse.urlencode(params)}"
    return _http_get(url)


def get_stats_data(
    app_id: str,
    stats_data_id: str,
    start_position: int = 1,
    limit: int = PAGE_SIZE,
    cd_area: str | None = None,
) -> dict[str, Any]:
    """データ取得。cd_area 指定で 1 市区町村に絞り込み可能。"""
    params: dict[str, Any] = {
        "appId": app_id,
        "statsDataId": stats_data_id,
        "lang": "J",
        "limit": limit,
        "startPosition": start_position,
        "metaGetFlg": "Y",
        "cntGetFlg": "N",
        "replaceSpChars": "2",
    }
    if cd_area:
        params["cdArea"] = cd_area
    url = f"{ESTAT_DATA_API}?{urllib.parse.urlencode(params)}"
    return _http_get(url)


# --------------------------------------------------------------------------- #
# ヘルパー
# --------------------------------------------------------------------------- #

def ensure_dirs() -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)


def _normalize_structure(name: str) -> str:
    """構造名を辞書ベースで正規化。マッピングがなければそのまま返す。"""
    return STRUCTURE_NORMALIZE.get(name.strip(), name.strip())


def _normalize_area_class(name: str) -> str:
    """面積階級名を正規化。"""
    return AREA_CLASS_NORMALIZE.get(name.strip(), name.strip())


def _save_json(path: Path, data: Any) -> None:
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")


# --------------------------------------------------------------------------- #
# モード: --dry-run
# --------------------------------------------------------------------------- #

def run_dry_run(args: argparse.Namespace) -> int:
    print("[mode] --dry-run")
    print(f"[time] {datetime.now(timezone.utc).isoformat()}")
    app_id = get_app_id(args.app_id)
    print(f"[appId] {mask_app_id(app_id)} (length={len(app_id)})")
    print(f"[gov_stats_code]   {GOV_STATS_CODE}")
    print(f"[STATS_DATA_ID]    {STATS_DATA_ID}  (定数。--metadata-only で確定)")
    ensure_dirs()
    print(f"[dir] DATA_DIR     = {DATA_DIR.resolve()}")
    print(f"[plan] OUTPUT_CSV  = {OUTPUT_CSV}")
    print(f"[plan] META_LIST   = {META_LIST_CACHE}")
    print(f"[plan] META_INFO   = {META_INFO_CACHE}")
    print(f"[plan] PROGRESS    = {PROGRESS_FILE}")
    print("[OK] dry-run completed (no HTTP request issued).")
    return 0


# --------------------------------------------------------------------------- #
# モード: --metadata-only (statsDataId 候補一覧取得)
# --------------------------------------------------------------------------- #

def run_metadata_only(args: argparse.Namespace) -> int:
    """getStatsList で住宅・土地統計の表一覧を取得 → 候補 ID をリストアップ。"""
    print("[mode] --metadata-only")
    app_id = get_app_id(args.app_id)
    print(f"[appId] {mask_app_id(app_id)}")
    print(f"[gov_stats_code] {GOV_STATS_CODE}")
    ensure_dirs()

    print(f"[GET] {ESTAT_LIST_API} (statsCode={GOV_STATS_CODE})")
    data = get_stats_list(app_id, GOV_STATS_CODE)

    result = data.get("GET_STATS_LIST", {}).get("RESULT", {})
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    _save_json(META_LIST_CACHE, data)
    print(f"[saved] {META_LIST_CACHE} ({META_LIST_CACHE.stat().st_size:,} bytes)")

    if status not in (0, "0"):
        print("[WARN] non-zero STATUS")
        return 2

    # 表一覧を抜き出して借家・家賃関連表をフィルタ
    table_infs = (
        data.get("GET_STATS_LIST", {})
        .get("DATALIST_INF", {})
        .get("TABLE_INF", [])
    )
    if isinstance(table_infs, dict):
        table_infs = [table_infs]

    print(f"[tables] total={len(table_infs)}")
    print()
    print("=== 候補表一覧 (借家/家賃/構造/面積 を含むもの) ===")

    keywords = ["借家", "家賃", "構造", "専有面積", "民営", "公営"]
    matched: list[tuple[str, str, str]] = []
    for t in table_infs:
        stats_data_id = t.get("@id", "")
        title = t.get("TITLE", {})
        if isinstance(title, dict):
            title_text = title.get("$", "")
        else:
            title_text = str(title)
        survey_date = t.get("SURVEY_DATE", "")
        if any(kw in title_text for kw in keywords):
            matched.append((stats_data_id, title_text, str(survey_date)))

    for sid, title, sdate in matched[:50]:  # 上位 50 件まで表示
        print(f"  [{sid}] (date={sdate})")
        print(f"     {title[:120]}")
        print()

    print(f"[matched] {len(matched)} tables match keywords {keywords}")
    print()
    print("🔴 次のアクション:")
    print("  1. 上記から借家×家賃×構造×面積の表を選び STATS_DATA_ID を確定")
    print(f"  2. このスクリプト内の STATS_DATA_ID 定数 (現在: {STATS_DATA_ID}) を更新")
    print("  3. python scripts/fetch_rental_housing.py --inspect-meta で軸構造確認")
    print("  4. python scripts/fetch_rental_housing.py --sample-only でサンプル取得")
    print("  5. python scripts/fetch_rental_housing.py --fetch で本実行")
    return 0


# --------------------------------------------------------------------------- #
# モード: --inspect-meta (確定済 STATS_DATA_ID の軸構造確認)
# --------------------------------------------------------------------------- #

def run_inspect_meta(args: argparse.Namespace) -> int:
    """指定済 STATS_DATA_ID の軸 (area, cat01, cat02, ...) を表示。"""
    print("[mode] --inspect-meta")
    app_id = get_app_id(args.app_id)
    stats_data_id = args.stats_data_id or STATS_DATA_ID
    print(f"[appId] {mask_app_id(app_id)}")
    print(f"[stats_data_id] {stats_data_id}")
    ensure_dirs()

    print(f"[GET] {ESTAT_META_API}")
    data = get_meta_info(app_id, stats_data_id)
    result = data.get("GET_META_INFO", {}).get("RESULT", {})
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    _save_json(META_INFO_CACHE, data)
    print(f"[saved] {META_INFO_CACHE} ({META_INFO_CACHE.stat().st_size:,} bytes)")

    if status not in (0, "0"):
        print("[WARN] non-zero STATUS. statsDataId may need confirmation.")
        return 2

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
        axis_id = axis.get("@id", "")
        axis_name = axis.get("@name", "")
        classes = axis.get("CLASS", [])
        if isinstance(classes, dict):
            classes = [classes]
        print(f"  --- {axis_id} ({axis_name}): {len(classes)} codes ---")
        for cls in classes[:20]:  # 上位 20 件表示
            print(f"    {cls.get('@code', ''):>10}  {cls.get('@name', '')}")
        if len(classes) > 20:
            print(f"    ... (+{len(classes) - 20} more)")
    return 0


# --------------------------------------------------------------------------- #
# モード: --sample-only (1 ページサンプル)
# --------------------------------------------------------------------------- #

def run_sample_only(args: argparse.Namespace) -> int:
    print("[mode] --sample-only")
    app_id = get_app_id(args.app_id)
    stats_data_id = args.stats_data_id or STATS_DATA_ID
    limit = args.limit if args.limit else 1000
    print(f"[appId] {mask_app_id(app_id)}")
    print(f"[stats_data_id] {stats_data_id}")
    print(f"[limit] {limit}")
    ensure_dirs()

    print(f"[GET] {ESTAT_DATA_API} (startPosition=1, limit={limit})")
    data = get_stats_data(app_id, stats_data_id, start_position=1, limit=limit)
    result = data.get("GET_STATS_DATA", {}).get("RESULT", {})
    status = result.get("STATUS")
    err_msg = result.get("ERROR_MSG", "")
    print(f"[result] STATUS={status} MSG={err_msg}")

    sample_path = DATA_DIR / "rental_housing_sample.json"
    _save_json(sample_path, data)
    print(f"[saved] {sample_path} ({sample_path.stat().st_size:,} bytes)")

    values = (
        data.get("GET_STATS_DATA", {})
        .get("STATISTICAL_DATA", {})
        .get("DATA_INF", {})
        .get("VALUE", [])
    )
    if isinstance(values, dict):
        values = [values]
    print(f"[rows] {len(values)} cells in this page")
    for i, row in enumerate(values[:5]):
        print(f"  [{i}] {row}")

    if status not in (0, "0"):
        return 2
    return 0


# --------------------------------------------------------------------------- #
# モード: --fetch (本実行: 全市区町村ループ → CSV 出力)
# --------------------------------------------------------------------------- #

def _build_axis_map(meta_data: dict[str, Any]) -> dict[str, dict[str, str]]:
    """getMetaInfo レスポンスから axis_id → {code: name} 辞書を構築。"""
    class_inf = (
        meta_data.get("GET_META_INFO", {})
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
            code_map[cls.get("@code", "")] = cls.get("@name", "")
        axis_map[axis_id] = code_map
    return axis_map


def _classify_area(area_code: str, area_name: str) -> tuple[str, str]:
    """area code から (prefecture, municipality) を導出。

    - 00000           -> ("全国", "")
    - xx000           -> (都道府県, "")  [都道府県集計]
    - xxxxx (5 桁)    -> (都道府県, area_name)  [市区町村]
    """
    if not area_code:
        return ("", area_name)
    if area_code == "00000":
        return ("全国", "")
    code = str(area_code).zfill(5)
    pref_code = code[:2]
    pref_name = PREF_2DIGIT_MAP.get(pref_code, "")
    if code[2:] == "000":
        # 都道府県集計
        return (pref_name or area_name, "")
    return (pref_name, area_name)


def _load_progress() -> set[str]:
    """進捗ファイルから取得済 cdArea コードを読み込む。"""
    if not PROGRESS_FILE.exists():
        return set()
    done = set()
    with PROGRESS_FILE.open("r", encoding="utf-8") as f:
        for line in f:
            code = line.strip()
            if code:
                done.add(code)
    return done


def _save_progress(area_code: str) -> None:
    with PROGRESS_FILE.open("a", encoding="utf-8") as f:
        f.write(area_code + "\n")


def run_fetch(args: argparse.Namespace) -> int:
    """本実行: 1 statsDataId に対して全データを取得して CSV に出力。

    住宅・土地統計は表によって粒度が異なるため、ここでは axis 構造を読み取って
    area / structure / area_class / value を抽出する汎用的な実装を取る。
    """
    print("[mode] --fetch")
    app_id = get_app_id(args.app_id)
    stats_data_id = args.stats_data_id or STATS_DATA_ID
    print(f"[appId] {mask_app_id(app_id)}")
    print(f"[stats_data_id] {stats_data_id}")
    print(f"[output] {OUTPUT_CSV}")
    ensure_dirs()

    if args.reset and OUTPUT_CSV.exists():
        OUTPUT_CSV.unlink()
        print(f"[reset] {OUTPUT_CSV} removed")
    if args.reset and PROGRESS_FILE.exists():
        PROGRESS_FILE.unlink()
        print(f"[reset] {PROGRESS_FILE} removed")

    # メタ情報を取得して軸構造を把握
    print("[step] getMetaInfo 取得")
    meta = get_meta_info(app_id, stats_data_id)
    meta_result = meta.get("GET_META_INFO", {}).get("RESULT", {})
    if meta_result.get("STATUS") not in (0, "0"):
        raise SystemExit(f"meta error: {meta_result.get('ERROR_MSG')}")
    axis_map = _build_axis_map(meta)
    print(f"[axes] {list(axis_map.keys())}")

    # area 軸の市区町村コード一覧を取得
    area_axis = axis_map.get("area", {})
    target_area_codes: list[tuple[str, str]] = []
    for code, name in area_axis.items():
        # 5 桁数字以外は除外、全国(00000)は集計値として 1 回だけ取得対象に含める
        if len(code) == 5 and code.isdigit():
            target_area_codes.append((code, name))
    print(f"[area] {len(target_area_codes)} area codes")

    # 進捗ロード
    done_codes = _load_progress()
    if done_codes:
        print(f"[resume] 取得済 {len(done_codes)} area をスキップ")

    todo_areas = [(c, n) for c, n in target_area_codes if c not in done_codes]
    print(f"[todo] {len(todo_areas)} area to fetch")

    # CSV 書き込みモード決定
    is_new = (not OUTPUT_CSV.exists()) or (len(done_codes) == 0)
    csv_mode = "w" if is_new else "a"
    fetched_at = datetime.now(timezone.utc).isoformat()

    # 軸 ID 推定 (住宅・土地統計の典型: cat01=構造, cat02=面積階級, tab=表章項目)
    # ただし表によって異なるため、コードレベルでは axis_map をルックアップする方針
    # 表章項目 (tab) の中から「家賃」「住戸数」を識別するため、軸名から推定
    tab_axis = axis_map.get("tab", {})
    rent_tab_codes: set[str] = set()
    unit_tab_codes: set[str] = set()
    for code, name in tab_axis.items():
        if any(k in name for k in ["家賃", "1か月当たり家賃", "1ヵ月当たり家賃", "月額家賃"]):
            rent_tab_codes.add(code)
        if any(k in name for k in ["住宅数", "住戸数", "借家数", "世帯数"]):
            unit_tab_codes.add(code)
    print(f"[tab] rent codes={rent_tab_codes}, unit codes={unit_tab_codes}")

    with OUTPUT_CSV.open(csv_mode, encoding="utf-8-sig", newline="") as f_out:
        writer = csv.DictWriter(f_out, fieldnames=OUTPUT_COLUMNS)
        if is_new:
            writer.writeheader()

        processed = 0
        skipped = 0
        for i, (area_code, area_name) in enumerate(todo_areas):
            if (i + 1) % 20 == 0 or i == 0:
                print(f"[progress] {i + 1}/{len(todo_areas)}: {area_code} {area_name}")
                sys.stdout.flush()

            # 1 area 分のデータ取得 (ページネーション対応)
            start_position = 1
            area_values: list[dict[str, Any]] = []
            while True:
                resp = get_stats_data(
                    app_id, stats_data_id,
                    start_position=start_position,
                    limit=PAGE_SIZE,
                    cd_area=area_code,
                )
                result = resp.get("GET_STATS_DATA", {}).get("RESULT", {})
                if result.get("STATUS") not in (0, "0"):
                    print(f"  [warn] {area_code}: {result.get('ERROR_MSG')}")
                    break
                stat_data = resp.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
                values = stat_data.get("DATA_INF", {}).get("VALUE", [])
                if isinstance(values, dict):
                    values = [values]
                area_values.extend(values)
                result_inf = stat_data.get("RESULT_INF", {})
                total = int(result_inf.get("TOTAL_NUMBER", 0))
                to_num = int(result_inf.get("TO_NUMBER", 0))
                if to_num >= total or len(values) == 0:
                    break
                start_position = to_num + 1
                time.sleep(REQUEST_INTERVAL_SEC)

            if not area_values:
                skipped += 1
                _save_progress(area_code)
                time.sleep(REQUEST_INTERVAL_SEC)
                continue

            # area_values を (structure, area_class) でグルーピングして 1 行にまとめる
            # (tab=住戸数 -> rental_total_units, tab=家賃 -> median_rent_jpy)
            grouped: dict[tuple[str, str], dict[str, Any]] = {}
            for v in area_values:
                tab_code = str(v.get("@tab", ""))
                cat01_code = str(v.get("@cat01", ""))
                cat02_code = str(v.get("@cat02", ""))
                val_raw = v.get("$", "")

                struct_name = _normalize_structure(
                    axis_map.get("cat01", {}).get(cat01_code, "")
                )
                area_class_name = _normalize_area_class(
                    axis_map.get("cat02", {}).get(cat02_code, "")
                )
                key = (struct_name, area_class_name)

                try:
                    val_num = float(val_raw)
                except (TypeError, ValueError):
                    val_num = None

                row = grouped.setdefault(key, {
                    "rental_total_units": None,
                    "median_rent_jpy": None,
                })

                if tab_code in unit_tab_codes and val_num is not None:
                    row["rental_total_units"] = int(val_num)
                elif tab_code in rent_tab_codes and val_num is not None:
                    row["median_rent_jpy"] = int(val_num)

            pref, muni = _classify_area(area_code, area_name)
            for (struct, area_class), vals in grouped.items():
                if not struct and not area_class:
                    continue  # 軸全て空はスキップ
                # 集計軸 (総数同士の組合せ) はノイズ多いが基準値として残す
                writer.writerow({
                    "prefecture": pref,
                    "municipality": muni,
                    "structure": struct,
                    "area_class": area_class,
                    "rental_total_units": vals["rental_total_units"],
                    "median_rent_jpy": vals["median_rent_jpy"],
                    "as_of": AS_OF_YEAR,
                    "fetched_at": fetched_at,
                })

            f_out.flush()
            _save_progress(area_code)
            processed += 1
            time.sleep(REQUEST_INTERVAL_SEC)

        print()
        print("=" * 60)
        print(f"取得完了: processed={processed}, skipped={skipped}")
        print(f"出力先: {OUTPUT_CSV}")

    return 0


# --------------------------------------------------------------------------- #
# モード: --validate (出力 CSV の整合性チェック)
# --------------------------------------------------------------------------- #

def run_validate(args: argparse.Namespace) -> int:
    """出力 CSV の整合性チェック (pandas 必須)。"""
    print("[mode] --validate")
    csv_path = OUTPUT_CSV
    if not csv_path.exists():
        raise SystemExit(f"CSV not found: {csv_path}. Run --fetch first.")

    try:
        import pandas as pd
    except ImportError:
        raise SystemExit("pandas is required. pip install pandas")

    df = pd.read_csv(csv_path, dtype={"prefecture": str, "municipality": str})
    n = len(df)
    print(f"[info] row count: {n:,}")

    errors: list[str] = []
    warnings: list[str] = []

    if n == 0:
        errors.append("row count is 0 (empty CSV)")
        return _print_validation(errors, warnings)

    # 1. カラム存在チェック
    missing_cols = [c for c in OUTPUT_COLUMNS if c not in df.columns]
    if missing_cols:
        errors.append(f"missing columns: {missing_cols}")

    # 2. 都道府県カバレッジ (47 県 + "全国" を期待)
    prefs = set(df["prefecture"].dropna().unique())
    print(f"[info] distinct prefectures: {len(prefs)}")
    expected_prefs = set(PREF_2DIGIT_MAP.values())
    missing_prefs = expected_prefs - prefs
    if missing_prefs:
        warnings.append(f"missing prefectures: {sorted(missing_prefs)[:5]} (+{max(0, len(missing_prefs) - 5)} more)")

    # 3. 市区町村カバレッジ (案 R-A 想定: 約 100 市区町村)
    muni_count = df[df["municipality"] != ""]["municipality"].nunique()
    print(f"[info] distinct municipalities (non-empty): {muni_count}")
    if muni_count < 50:
        warnings.append(f"municipality count = {muni_count}, expected >= 50")
    if muni_count > 1900:
        warnings.append(f"municipality count = {muni_count}, much higher than expected (~100)")

    # 4. median_rent_jpy が 0 超か
    rent_non_null = df["median_rent_jpy"].dropna()
    if len(rent_non_null) == 0:
        warnings.append("no median_rent_jpy values (rent data missing?)")
    else:
        rent_pos = (rent_non_null > 0).sum()
        rent_zero = (rent_non_null <= 0).sum()
        print(f"[info] median_rent_jpy: positive={rent_pos:,}, non-positive={rent_zero:,}")
        if rent_pos == 0:
            errors.append("all median_rent_jpy <= 0")
        # 家賃の妥当性 (10,000 - 500,000 円/月 を想定)
        rent_low = (rent_non_null < 10000).sum()
        rent_high = (rent_non_null > 500000).sum()
        if rent_low > 0:
            warnings.append(f"median_rent_jpy < 10,000: {rent_low:,} rows")
        if rent_high > 0:
            warnings.append(f"median_rent_jpy > 500,000: {rent_high:,} rows")

    # 5. rental_total_units が 0 超か
    units_non_null = df["rental_total_units"].dropna()
    if len(units_non_null) == 0:
        warnings.append("no rental_total_units values")
    else:
        units_neg = (units_non_null < 0).sum()
        if units_neg > 0:
            errors.append(f"negative rental_total_units: {units_neg:,}")

    # 6. PK 重複チェック (prefecture, municipality, structure, area_class)
    pk_cols = ["prefecture", "municipality", "structure", "area_class"]
    dup = df.duplicated(subset=pk_cols).sum()
    if dup > 0:
        warnings.append(f"PK duplicates ({pk_cols}): {dup:,}")

    # 7. structure / area_class の distinct 数
    print(f"[dist] structure ({df['structure'].nunique()} distinct):")
    for k, v in df["structure"].value_counts().head(10).items():
        print(f"  {k!r}: {v:,}")
    print(f"[dist] area_class ({df['area_class'].nunique()} distinct):")
    for k, v in df["area_class"].value_counts().head(10).items():
        print(f"  {k!r}: {v:,}")

    # 8. as_of 一致
    as_of_set = set(df["as_of"].astype(str).unique())
    print(f"[info] as_of values: {as_of_set}")
    if AS_OF_YEAR not in as_of_set:
        warnings.append(f"as_of does not contain expected year {AS_OF_YEAR}")

    return _print_validation(errors, warnings)


def _print_validation(errors: list[str], warnings: list[str]) -> int:
    if warnings:
        print(f"[warn] {len(warnings)} warnings:")
        for w in warnings:
            print(f"  - {w}")
    if errors:
        print(f"[NG] {len(errors)} errors:")
        for e in errors:
            print(f"  - {e}")
        return 1
    print("[OK] CSV validation passed")
    return 0


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="e-Stat 住宅・土地統計調査 (statsCode=00200522) fetch script (Phase 2 案 R-A)",
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true",
                      help="Verify CLI/env without API call")
    mode.add_argument("--metadata-only", action="store_true",
                      help="政府統計コードから表一覧取得 (statsDataId 候補発見)")
    mode.add_argument("--inspect-meta", action="store_true",
                      help="STATS_DATA_ID の軸構造を確認")
    mode.add_argument("--sample-only", action="store_true",
                      help="STATS_DATA_ID の 1 ページサンプル取得")
    mode.add_argument("--fetch", action="store_true",
                      help="本実行 (全 area ループ -> CSV 出力)")
    mode.add_argument("--validate", action="store_true",
                      help="出力 CSV の整合性検証")

    parser.add_argument("--app-id", default=None,
                        help="e-Stat appId (overrides ESTAT_APP_ID env var / DEFAULT_APP_ID)")
    parser.add_argument("--stats-data-id", default=None,
                        help=f"e-Stat statsDataId (default: {STATS_DATA_ID})")
    parser.add_argument("--limit", type=int, default=None,
                        help=f"Rows per page (default: {PAGE_SIZE}, --sample-only default: 1000)")
    parser.add_argument("--reset", action="store_true",
                        help="(--fetch) 進捗ファイルと CSV を削除して最初から実行")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.dry_run:
        return run_dry_run(args)
    if args.metadata_only:
        return run_metadata_only(args)
    if args.inspect_meta:
        return run_inspect_meta(args)
    if args.sample_only:
        return run_sample_only(args)
    if args.fetch:
        return run_fetch(args)
    if args.validate:
        return run_validate(args)

    parser.error("no mode selected")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
