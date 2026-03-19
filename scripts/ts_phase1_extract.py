"""
Phase 1: CSV → Parquet 抽出（30カラムのみ、pyarrow）
=====================================================
HW月次スナップショットCSVから必要カラムのみ抽出し、
Parquetファイルに変換する。4並列処理。

使い方:
    python ts_phase1_extract.py
    python ts_phase1_extract.py --workers 2
"""

import argparse
import json
import os
import sys
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

import pandas as pd

SCRIPT_DIR = Path(__file__).parent
METADATA_PATH = SCRIPT_DIR.parent / "data" / "snapshot_metadata.json"
CACHE_DIR = SCRIPT_DIR.parent / "data" / "ts_parquet_cache"
DEFAULT_DATA_DIR = r"C:\Users\fuji1\OneDrive\デスクトップ\ハローワーク2025.02.25"

# 抽出するカラム（インデックス指定 — ヘッダー名はcp932で不安定なため）
REQUIRED_COL_INDICES = [
    0,    # 求人番号
    1,    # 事業所番号
    2,    # 受付年月日（西暦）
    4,    # 求人有効年月日（西暦）
    20,   # 事業所住所（コード）
    21,   # 事業所住所（名称）
    26,   # 就業場所住所１（名称）
    30,   # 職業分類１（コード）
    33,   # 職業分類１（大分類コード）
    36,   # 産業分類（コード）
    37,   # 産業分類（大分類コード）
    38,   # 産業分類（名称）
    46,   # 雇用形態（コード）
    47,   # 雇用形態
    55,   # 雇用期間の定め（コード）
    129,  # 支給額（a+b）下限
    130,  # 支給額（a+b）上限
    160,  # 賃金形態（コード）
    227,  # 時間外月平均時間
    234,  # 年間休日数
    253,  # 週休二日制（コード）
    297,  # 従業員数企業全体（コード）
    298,  # 従業員数企業全体
    316,  # 法人番号
    336,  # 採用人数（コード）
    337,  # 採用人数
    338,  # 募集理由区分（コード）
    339,  # 募集理由区分
]

# 正規化後のカラム名
RENAMED_COLUMNS = {
    0: "job_number",
    1: "facility_id",
    2: "reception_date_raw",
    4: "expiry_date_raw",
    20: "pref_code",
    21: "pref_name",
    26: "work_location",
    30: "occupation_code",
    33: "occupation_major_code",
    36: "industry_code",
    37: "industry_major_code",
    38: "industry_name",
    46: "emp_type_code",
    47: "emp_type_name",
    55: "emp_period_code",
    129: "salary_min_raw",
    130: "salary_max_raw",
    160: "salary_type_code",
    227: "overtime_hours_raw",
    234: "annual_holidays_raw",
    253: "weekly_holiday_code",
    297: "employee_count_code",
    298: "employee_count_raw",
    316: "corp_number",
    336: "recruit_count_code",
    337: "recruit_count_raw",
    338: "recruit_reason_code_raw",
    339: "recruit_reason_name",
}

# 都道府県抽出用
PREFECTURES = [
    "北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県",
    "茨城県", "栃木県", "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県",
    "新潟県", "富山県", "石川県", "福井県", "山梨県", "長野県",
    "岐阜県", "静岡県", "愛知県", "三重県",
    "滋賀県", "京都府", "大阪府", "兵庫県", "奈良県", "和歌山県",
    "鳥取県", "島根県", "岡山県", "広島県", "山口県",
    "徳島県", "香川県", "愛媛県", "高知県",
    "福岡県", "佐賀県", "長崎県", "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県",
]


def emp_group(emp_type: str) -> str:
    """雇用形態を3グループに正規化"""
    if not emp_type:
        return "その他"
    if emp_type == "正社員":
        return "正社員"
    if "パート" in emp_type:
        return "パート"
    return "その他"


def extract_prefecture(location: str) -> str:
    """住所文字列から都道府県名を抽出"""
    if not location or not isinstance(location, str):
        return ""
    for pref in PREFECTURES:
        if location.startswith(pref):
            return pref
    return ""


def extract_municipality(location: str, prefecture: str) -> str:
    """住所文字列から市区町村名を抽出"""
    if not location or not prefecture:
        return ""
    rest = location[len(prefecture):]
    # 市区町村の抽出（正規表現を使わず高速化）
    for suffix in ["市", "区", "町", "村"]:
        idx = rest.find(suffix)
        if idx >= 0:
            return rest[:idx + 1]
    return ""


def process_single_file(args_tuple):
    """1ファイルを処理してParquetに変換（並列処理用）"""
    csv_path, snapshot_id, sort_order, output_path = args_tuple

    if output_path.exists():
        return str(output_path), 0, "cached"

    import csv as csv_module

    rows = []
    with open(csv_path, "r", encoding="cp932", errors="replace") as f:
        reader = csv_module.reader(f)
        header = next(reader)

        for row in reader:
            if len(row) < 340:
                continue

            extracted = {}
            for idx in REQUIRED_COL_INDICES:
                col_name = RENAMED_COLUMNS[idx]
                extracted[col_name] = row[idx] if idx < len(row) else ""

            rows.append(extracted)

    df = pd.DataFrame(rows)

    # 型変換
    for col in ["salary_min_raw", "salary_max_raw", "annual_holidays_raw"]:
        df[col] = pd.to_numeric(df[col], errors="coerce")

    df["recruit_reason_code"] = pd.to_numeric(
        df["recruit_reason_code_raw"], errors="coerce"
    ).fillna(0).astype(int)

    df["employee_count"] = pd.to_numeric(
        df["employee_count_raw"], errors="coerce"
    )

    df["overtime_hours"] = pd.to_numeric(
        df["overtime_hours_raw"], errors="coerce"
    )

    df["recruit_count"] = pd.to_numeric(
        df["recruit_count_raw"], errors="coerce"
    )

    # 雇用形態グループ
    df["emp_group"] = df["emp_type_name"].apply(emp_group)

    # 複合追跡キー
    df["tracking_key"] = (
        df["facility_id"].fillna("") + "|" +
        df["occupation_code"].fillna("") + "|" +
        df["emp_type_code"].fillna("")
    )

    # 都道府県・市区町村抽出
    df["prefecture"] = df["work_location"].apply(extract_prefecture)
    # pref_nameからのフォールバック
    mask = df["prefecture"] == ""
    if mask.any():
        df.loc[mask, "prefecture"] = df.loc[mask, "pref_name"].apply(extract_prefecture)

    df["municipality"] = df.apply(
        lambda r: extract_municipality(r["work_location"], r["prefecture"]), axis=1
    )

    # 日付パース
    df["reception_date"] = pd.to_datetime(
        df["reception_date_raw"], format="%Y/%m/%d", errors="coerce"
    )
    df["expiry_date"] = pd.to_datetime(
        df["expiry_date_raw"], format="%Y/%m/%d", errors="coerce"
    )

    # 掲載日数
    df["listing_days"] = (df["expiry_date"] - df["reception_date"]).dt.days

    # スナップショット情報
    df["snapshot_id"] = snapshot_id
    df["sort_order"] = sort_order

    # 不要な中間カラムを削除
    drop_cols = [
        "reception_date_raw", "expiry_date_raw", "recruit_reason_code_raw",
        "employee_count_raw", "employee_count_code", "overtime_hours_raw",
        "recruit_count_raw", "recruit_count_code", "salary_type_code",
        "emp_period_code", "weekly_holiday_code",
    ]
    df.drop(columns=[c for c in drop_cols if c in df.columns], inplace=True)

    # Parquet保存
    output_path.parent.mkdir(parents=True, exist_ok=True)
    df.to_parquet(str(output_path), engine="pyarrow", compression="snappy", index=False)

    return str(output_path), len(df), "processed"


def main():
    parser = argparse.ArgumentParser(description="Phase 1: CSV→Parquet抽出")
    parser.add_argument("--workers", type=int, default=4, help="並列数")
    parser.add_argument("--metadata", default=str(METADATA_PATH), help="メタデータJSON")
    args = parser.parse_args()

    with open(args.metadata, "r", encoding="utf-8") as f:
        metadata = json.load(f)

    data_dir = Path(metadata["data_dir"])
    CACHE_DIR.mkdir(parents=True, exist_ok=True)

    snapshots = metadata["snapshots"]
    print(f"=" * 60)
    print(f"Phase 1: CSV → Parquet 抽出")
    print(f"  ファイル数: {len(snapshots)}")
    print(f"  並列数: {args.workers}")
    print(f"  出力先: {CACHE_DIR}")
    print(f"=" * 60)

    # 処理タスクを準備
    tasks = []
    for snap in snapshots:
        csv_path = data_dir / snap["file"]
        output_path = CACHE_DIR / f"snapshot_{snap['sort_order']:02d}_{snap['snapshot_id']}.parquet"
        tasks.append((
            str(csv_path),
            snap["snapshot_id"],
            snap["sort_order"],
            output_path,
        ))

    # 並列処理
    start_time = time.time()
    results = []

    if args.workers <= 1:
        for task in tasks:
            result = process_single_file(task)
            results.append(result)
            print(f"  {Path(result[0]).name}: {result[1]:,}行 ({result[2]})")
    else:
        with ProcessPoolExecutor(max_workers=args.workers) as pool:
            futures = {pool.submit(process_single_file, t): t for t in tasks}
            for future in as_completed(futures):
                result = future.result()
                results.append(result)
                print(f"  {Path(result[0]).name}: {result[1]:,}行 ({result[2]})")

    elapsed = time.time() - start_time
    total_rows = sum(r[1] for r in results)
    cached = sum(1 for r in results if r[2] == "cached")
    processed = sum(1 for r in results if r[2] == "processed")

    # キャッシュサイズ確認
    cache_size = sum(f.stat().st_size for f in CACHE_DIR.glob("*.parquet"))

    print(f"\n{'=' * 60}")
    print(f"Phase 1 完了")
    print(f"  処理: {processed}件, キャッシュ: {cached}件")
    print(f"  総行数: {total_rows:,}")
    print(f"  Parquetサイズ: {cache_size / 1024 / 1024:.0f}MB")
    print(f"  処理時間: {elapsed:.1f}秒 ({elapsed/60:.1f}分)")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
