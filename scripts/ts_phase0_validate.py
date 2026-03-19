"""
Phase 0: ファイル検証・メタデータ抽出
=============================================
HW月次スナップショットCSVファイルを検証し、
snapshot_metadata.json を生成する。

使い方:
    python ts_phase0_validate.py
    python ts_phase0_validate.py --data-dir "C:\path\to\csvs"
"""

import argparse
import csv
import hashlib
import json
import os
import sys
from datetime import datetime
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
DEFAULT_DATA_DIR = r"C:\Users\fuji1\OneDrive\デスクトップ\ハローワーク2025.02.25"
OUTPUT_PATH = SCRIPT_DIR.parent / "data" / "snapshot_metadata.json"

# 除外ファイル名パターン
EXCLUDE_PATTERNS = ["テスト"]


def md5_first_100kb(filepath: str) -> str:
    """先頭100KBのMD5ハッシュ"""
    with open(filepath, "rb") as f:
        return hashlib.md5(f.read(102400)).hexdigest()


def get_date_range(filepath: str) -> dict:
    """受付年月日の先頭行から日付情報を取得"""
    with open(filepath, "r", encoding="cp932") as f:
        reader = csv.reader(f)
        header = next(reader)
        first_row = next(reader)

    reception_date = first_row[2]  # 受付年月日（西暦）
    expiry_date = first_row[4]     # 求人有効年月日（西暦）

    return {
        "first_reception_date": reception_date,
        "first_expiry_date": expiry_date,
    }


def count_rows(filepath: str) -> int:
    """行数カウント（ヘッダー除く）"""
    count = 0
    with open(filepath, "r", encoding="cp932", errors="replace") as f:
        for _ in f:
            count += 1
    return count - 1  # ヘッダー分を引く


def derive_snapshot_id(reception_date: str) -> str:
    """受付年月日からスナップショットIDを生成（YYYY-MM形式）"""
    try:
        dt = datetime.strptime(reception_date.strip(), "%Y/%m/%d")
        return dt.strftime("%Y-%m")
    except ValueError:
        try:
            dt = datetime.strptime(reception_date.strip(), "%Y/%m/%d")
            return dt.strftime("%Y-%m")
        except ValueError:
            return "unknown"


def main():
    parser = argparse.ArgumentParser(description="Phase 0: ファイル検証")
    parser.add_argument("--data-dir", default=DEFAULT_DATA_DIR, help="CSVディレクトリ")
    parser.add_argument("--output", default=str(OUTPUT_PATH), help="出力JSONパス")
    args = parser.parse_args()

    data_dir = Path(args.data_dir)
    if not data_dir.exists():
        print(f"エラー: {data_dir} が見つかりません")
        sys.exit(1)

    # M100 CSVファイルを列挙
    csv_files = sorted([
        f for f in data_dir.iterdir()
        if f.suffix == ".csv" and "M100" in f.name
        and not any(p in f.name for p in EXCLUDE_PATTERNS)
    ])

    print(f"=" * 60)
    print(f"Phase 0: ファイル検証・メタデータ抽出")
    print(f"  データディレクトリ: {data_dir}")
    print(f"  対象CSVファイル: {len(csv_files)}件")
    print(f"=" * 60)

    # Step 1: ハッシュ計算 → 重複検出
    print("\n[1/3] ハッシュ計算・重複検出...")
    file_hashes = {}
    for f in csv_files:
        h = md5_first_100kb(str(f))
        if h not in file_hashes:
            file_hashes[h] = []
        file_hashes[h].append(f.name)

    duplicates = {h: fs for h, fs in file_hashes.items() if len(fs) > 1}
    excluded_files = set()
    for h, fs in duplicates.items():
        # 重複グループの最初のファイルを残し、残りを除外
        for f in fs[1:]:
            excluded_files.add(f)
            print(f"  重複除外: {f} (= {fs[0]})")

    # Step 2: 各ファイルのメタデータ抽出
    print("\n[2/3] メタデータ抽出...")
    snapshots = []
    excluded = []
    valid_files = [f for f in csv_files if f.name not in excluded_files]

    for i, f in enumerate(valid_files):
        print(f"  [{i+1}/{len(valid_files)}] {f.name}...", end=" ", flush=True)

        date_info = get_date_range(str(f))
        snapshot_date = date_info["first_reception_date"]
        snapshot_id = derive_snapshot_id(snapshot_date)
        row_count = count_rows(str(f))
        file_size = f.stat().st_size

        snap = {
            "file": f.name,
            "snapshot_id": snapshot_id,
            "snapshot_date": snapshot_date,
            "row_count": row_count,
            "file_size": file_size,
            "file_size_mb": round(file_size / 1024 / 1024, 1),
        }
        snapshots.append(snap)
        print(f"{snapshot_id} ({row_count:,}行, {snap['file_size_mb']}MB)")

    for f in excluded_files:
        excluded.append({"file": f, "reason": "duplicate"})
    for f in data_dir.iterdir():
        if f.suffix == ".csv" and "M100" in f.name and any(p in f.name for p in EXCLUDE_PATTERNS):
            excluded.append({"file": f.name, "reason": "test_file"})

    # ソート（snapshot_date順）
    snapshots.sort(key=lambda s: s["snapshot_date"])

    # sort_orderを付与
    for i, snap in enumerate(snapshots):
        snap["sort_order"] = i + 1

    # Step 3: サマリー
    print("\n[3/3] サマリー生成...")
    total_rows = sum(s["row_count"] for s in snapshots)
    total_size = sum(s["file_size"] for s in snapshots)
    date_range = f"{snapshots[0]['snapshot_id']} ～ {snapshots[-1]['snapshot_id']}"

    # 月別カバレッジ
    months_covered = sorted(set(s["snapshot_id"] for s in snapshots))

    metadata = {
        "generated_at": datetime.now().isoformat(),
        "data_dir": str(data_dir),
        "summary": {
            "total_valid_files": len(snapshots),
            "total_excluded": len(excluded),
            "total_rows": total_rows,
            "total_size_gb": round(total_size / 1024 / 1024 / 1024, 1),
            "date_range": date_range,
            "months_covered": months_covered,
            "months_count": len(months_covered),
        },
        "snapshots": snapshots,
        "excluded": excluded,
    }

    # JSON出力
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(metadata, f, ensure_ascii=False, indent=2)

    print(f"\n{'=' * 60}")
    print(f"Phase 0 完了")
    print(f"  有効ファイル: {len(snapshots)}件")
    print(f"  除外: {len(excluded)}件")
    print(f"  総行数: {total_rows:,}")
    print(f"  総サイズ: {metadata['summary']['total_size_gb']}GB")
    print(f"  期間: {date_range}")
    print(f"  月カバレッジ: {len(months_covered)}ヶ月")
    print(f"  出力: {output_path}")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
