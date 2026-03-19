"""
Phase 2-4: 複合キー追跡 → 集計テーブル生成 → SQLite構築（軽量版）
================================================================
Parquetをファイル単位でストリーム処理し、メモリを節約する。

使い方:
    python ts_phase2_to_phase4.py
"""

import json
import sqlite3
import sys
import time
from pathlib import Path

import numpy as np
import pandas as pd

SCRIPT_DIR = Path(__file__).parent
CACHE_DIR = SCRIPT_DIR.parent / "data" / "ts_parquet_cache"
DB_PATH = SCRIPT_DIR.parent / "data" / "hellowork.db"


def get_parquet_files():
    """ソート済みParquetファイルリスト"""
    files = sorted(CACHE_DIR.glob("snapshot_*.parquet"))
    if not files:
        print("エラー: Parquetが見つかりません。Phase 1を先に実行してください。")
        sys.exit(1)
    return files


def load_light(path: Path, columns: list) -> pd.DataFrame:
    """必要カラムのみ読み込み（メモリ節約）"""
    return pd.read_parquet(path, columns=columns)


# ============================================================
# Phase 2+3 統合: ファイル単位でストリーム集計
# ============================================================

def stream_aggregate(parquet_files: list) -> dict:
    """各Parquetを1つずつ読み込み、集計結果を蓄積"""

    all_counts = []
    all_vacancy = []
    all_salary = []
    all_fulfillment = []
    all_workstyle = []
    all_tracking = []

    # 追跡用: 前スナップショットの複合キー集合
    prev_keys = set()

    for i, pf in enumerate(parquet_files):
        # snapshot_idをファイル名から取得
        parts = pf.stem.split("_")
        snapshot_id = parts[2] if len(parts) >= 3 else pf.stem
        sort_order = int(parts[1]) if len(parts) >= 2 else i + 1

        print(f"  [{i+1}/{len(parquet_files)}] {pf.name} ({snapshot_id})...", end=" ", flush=True)

        # --- 集計用カラムのみ読み込み ---
        cols_basic = ["job_number", "facility_id", "tracking_key", "emp_group",
                      "prefecture", "municipality", "industry_major_code",
                      "industry_name", "recruit_reason_code"]
        df = load_light(pf, cols_basic + [
            "salary_min_raw", "salary_max_raw", "listing_days",
            "annual_holidays_raw", "overtime_hours", "recruit_count",
        ])

        n = len(df)

        # --- [3a] ts_agg_counts ---
        counts = (
            df.groupby(["prefecture", "municipality", "industry_major_code",
                         "industry_name", "emp_group"])
            .agg(posting_count=("job_number", "count"),
                 facility_count=("facility_id", "nunique"),
                 recruit_total=("recruit_count", "sum"))
            .reset_index()
        )
        counts["snapshot_id"] = snapshot_id
        counts["recruit_total"] = counts["recruit_total"].fillna(0).astype(int)
        all_counts.append(counts)

        # --- [3b] ts_agg_vacancy ---
        df["is_vacancy"] = (df["recruit_reason_code"] == 1).astype(int)
        df["is_growth"] = (df["recruit_reason_code"] == 2).astype(int)
        df["is_new_fac"] = (df["recruit_reason_code"] == 3).astype(int)
        df["is_unsel"] = (~df["recruit_reason_code"].isin([1, 2, 3])).astype(int)

        vacancy = (
            df.groupby(["prefecture", "municipality", "industry_major_code", "emp_group"])
            .agg(total_count=("job_number", "count"),
                 vacancy_count=("is_vacancy", "sum"),
                 growth_count=("is_growth", "sum"),
                 new_facility_count=("is_new_fac", "sum"),
                 unselected_count=("is_unsel", "sum"))
            .reset_index()
        )
        vacancy["vacancy_rate"] = (vacancy["vacancy_count"] / vacancy["total_count"] * 100).round(2)
        vacancy["growth_rate"] = (vacancy["growth_count"] / vacancy["total_count"] * 100).round(2)
        vacancy["snapshot_id"] = snapshot_id
        all_vacancy.append(vacancy)

        # --- [3c] ts_agg_salary ---
        valid_sal = df[df["salary_min_raw"].notna() & (df["salary_min_raw"] > 0)]
        if len(valid_sal) > 0:
            salary = (
                valid_sal.groupby(["prefecture", "municipality", "industry_major_code", "emp_group"])
                .agg(count=("salary_min_raw", "count"),
                     mean_min=("salary_min_raw", "mean"),
                     mean_max=("salary_max_raw", "mean"),
                     median_min=("salary_min_raw", "median"),
                     min_val=("salary_min_raw", "min"),
                     max_val=("salary_max_raw", "max"))
                .reset_index()
            )
            salary["mean_min"] = salary["mean_min"].round(0)
            salary["mean_max"] = salary["mean_max"].round(0)
            salary["median_min"] = salary["median_min"].round(0)
            salary["snapshot_id"] = snapshot_id
            all_salary.append(salary)

        # --- [3d] ts_agg_fulfillment ---
        valid_ld = df[df["listing_days"].notna() & (df["listing_days"] > 0)]
        if len(valid_ld) > 0:
            fulfill = (
                valid_ld.groupby(["prefecture", "municipality", "industry_major_code", "emp_group"])
                .agg(count=("listing_days", "count"),
                     avg_listing_days=("listing_days", "mean"),
                     median_listing_days=("listing_days", "median"),
                     long_term_count=("listing_days", lambda x: (x > 90).sum()),
                     very_long_count=("listing_days", lambda x: (x > 180).sum()))
                .reset_index()
            )
            fulfill["avg_listing_days"] = fulfill["avg_listing_days"].round(1)
            fulfill["median_listing_days"] = fulfill["median_listing_days"].round(1)
            fulfill["snapshot_id"] = snapshot_id
            all_fulfillment.append(fulfill)

        # --- [3e] ts_agg_workstyle ---
        workstyle = (
            df.groupby(["prefecture", "emp_group"])
            .agg(count=("job_number", "count"),
                 avg_annual_holidays=("annual_holidays_raw", "mean"),
                 avg_overtime=("overtime_hours", "mean"))
            .reset_index()
        )
        workstyle["avg_annual_holidays"] = workstyle["avg_annual_holidays"].round(1)
        workstyle["avg_overtime"] = workstyle["avg_overtime"].round(1)
        workstyle["snapshot_id"] = snapshot_id
        all_workstyle.append(workstyle)

        # --- [3g] ts_agg_tracking ---
        current_keys = set(df["tracking_key"].unique())
        new_keys = current_keys - prev_keys
        continue_keys = current_keys & prev_keys
        end_keys = prev_keys - current_keys

        # tracking_keyの地域・産業情報を取得
        key_info = (
            df.groupby("tracking_key")
            .agg(prefecture=("prefecture", "first"),
                 industry_major_code=("industry_major_code", "first"),
                 emp_group=("emp_group", "first"))
            .reset_index()
        )

        # 新規/継続のカウント
        key_info_dict = key_info.set_index("tracking_key").to_dict("index")

        tracking_agg = {}
        for tk in new_keys:
            info = key_info_dict.get(tk, {})
            k = (info.get("prefecture", ""), info.get("industry_major_code", ""), info.get("emp_group", ""))
            if k not in tracking_agg:
                tracking_agg[k] = {"new": 0, "continue": 0, "end": 0}
            tracking_agg[k]["new"] += 1

        for tk in continue_keys:
            info = key_info_dict.get(tk, {})
            k = (info.get("prefecture", ""), info.get("industry_major_code", ""), info.get("emp_group", ""))
            if k not in tracking_agg:
                tracking_agg[k] = {"new": 0, "continue": 0, "end": 0}
            tracking_agg[k]["continue"] += 1

        # end_keysの情報は前回のデータから取得する必要があるが、
        # 軽量版では都道府県レベルの集計のみにする
        # end_countは全体カウントのみ
        total_end = len(end_keys)

        tracking_rows = []
        for (pref, ind, eg), counts in tracking_agg.items():
            total = counts["continue"] + counts.get("end_approx", 0)
            tracking_rows.append({
                "snapshot_id": snapshot_id,
                "prefecture": pref,
                "industry_major_code": ind,
                "emp_group": eg,
                "new_count": counts["new"],
                "continue_count": counts["continue"],
                "end_count": 0,  # endは地域別配分が困難なため全体のみ
            })

        if tracking_rows:
            tracking_df = pd.DataFrame(tracking_rows)
            # 全体のend_countを都道府県別に按分（暫定）
            if total_end > 0 and len(tracking_df) > 0:
                total_continue = tracking_df["continue_count"].sum()
                if total_continue > 0:
                    tracking_df["end_count"] = (
                        tracking_df["continue_count"] / total_continue * total_end
                    ).round(0).astype(int)

            total_base = tracking_df["continue_count"] + tracking_df["end_count"]
            tracking_df["churn_rate"] = np.where(
                total_base > 0,
                (tracking_df["end_count"] / total_base * 100).round(2),
                0
            )
            all_tracking.append(tracking_df)

        prev_keys = current_keys
        print(f"{n:,}行 (new={len(new_keys):,}, cont={len(continue_keys):,}, end={len(end_keys):,})")

    # 全スナップショットを結合
    results = {
        "ts_agg_counts": pd.concat(all_counts, ignore_index=True) if all_counts else pd.DataFrame(),
        "ts_agg_vacancy": pd.concat(all_vacancy, ignore_index=True) if all_vacancy else pd.DataFrame(),
        "ts_agg_salary": pd.concat(all_salary, ignore_index=True) if all_salary else pd.DataFrame(),
        "ts_agg_fulfillment": pd.concat(all_fulfillment, ignore_index=True) if all_fulfillment else pd.DataFrame(),
        "ts_agg_workstyle": pd.concat(all_workstyle, ignore_index=True) if all_workstyle else pd.DataFrame(),
        "ts_agg_tracking": pd.concat(all_tracking, ignore_index=True) if all_tracking else pd.DataFrame(),
    }

    return results


def save_to_sqlite(tables: dict, db_path: Path):
    """集計テーブルをSQLiteに保存"""
    print(f"\n[Phase 4] SQLite構築: {db_path}")

    conn = sqlite3.connect(str(db_path))

    for table_name, df in tables.items():
        if df.empty:
            print(f"  {table_name}: 空（スキップ）")
            continue
        conn.execute(f"DROP TABLE IF EXISTS {table_name}")
        df.to_sql(table_name, conn, index=False, if_exists="replace")
        count = conn.execute(f"SELECT COUNT(*) FROM {table_name}").fetchone()[0]
        print(f"  {table_name}: {count:,}行")

    # インデックス作成
    index_defs = [
        "CREATE INDEX IF NOT EXISTS idx_ts_counts_pref ON ts_agg_counts(prefecture, snapshot_id)",
        "CREATE INDEX IF NOT EXISTS idx_ts_vacancy_pref ON ts_agg_vacancy(prefecture, snapshot_id)",
        "CREATE INDEX IF NOT EXISTS idx_ts_salary_pref ON ts_agg_salary(prefecture, snapshot_id)",
        "CREATE INDEX IF NOT EXISTS idx_ts_fulfill_pref ON ts_agg_fulfillment(prefecture, snapshot_id)",
        "CREATE INDEX IF NOT EXISTS idx_ts_tracking_pref ON ts_agg_tracking(prefecture, snapshot_id)",
    ]
    for idx_sql in index_defs:
        try:
            conn.execute(idx_sql)
        except Exception:
            pass

    conn.execute("ANALYZE")
    conn.commit()
    conn.close()

    db_size = db_path.stat().st_size / 1024 / 1024
    print(f"  DBサイズ: {db_size:.1f}MB")


def main():
    start_time = time.time()

    print("=" * 60)
    print("Phase 2-4: 追跡 → 集計 → SQLite構築（軽量版）")
    print("=" * 60)

    parquet_files = get_parquet_files()
    print(f"  Parquetファイル: {len(parquet_files)}件")

    # ストリーム集計
    print("\n[Phase 2-3] ストリーム集計...")
    tables = stream_aggregate(parquet_files)

    # SQLite保存
    save_to_sqlite(tables, DB_PATH)

    elapsed = time.time() - start_time
    total_rows = sum(len(t) for t in tables.values())

    print(f"\n{'=' * 60}")
    print(f"Phase 2-4 完了")
    print(f"  集計テーブル: {len(tables)}件")
    print(f"  総集計行数: {total_rows:,}")
    print(f"  処理時間: {elapsed:.1f}秒 ({elapsed/60:.1f}分)")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
