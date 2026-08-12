# -*- coding: utf-8 -*-
"""企業データの従業員数スナップショットを作る (Turso 投入用)。

# なぜ必要か

現在の人員推移は `employee_delta_*` (対過去比パーセント) しか無く、増減人数は
`employee_count * d / (100 + d)` で逆算している。この式は `d → -100` で発散し、
`delta` が小数第 2 位までしか無いため復元値に最大 ±281 人の不確かさが乗る。
さらに `collated_at` が 244 日に分散しているため「1 年前」の基準日が企業ごとに違う。

従業員数そのものを定点で保存すれば、増減は引き算になり、これらが全て消える。
(根拠: claudedocs/SALESNOW_MAP_PHASE0_FINDINGS_2026-08-12.md §1.3 / §4 / §11.5)

# 使い方

    python scripts/salesnow_snapshot/build_snapshot.py \
        --source "C:/.../salesnow_companies.csv" \
        --date 2026-08-12 \
        --outdir scripts/salesnow_snapshot/out

生成物:
  snapshot_YYYY-MM-DD.csv  … 確認用 (4 列)
  snapshot_YYYY-MM-DD.sql  … Turso 投入用 (冪等)

投入はユーザーが実行する (`feedback_turso_priority`: DB 書き込みはユーザー実行のみ)。

    turso db shell salesnow < scripts/salesnow_snapshot/out/snapshot_2026-08-12.sql

# 冪等性

`CREATE TABLE IF NOT EXISTS` + `INSERT OR REPLACE` で、同じ snapshot_date を
何度流しても行数は増えない (`feedback_turso_upload_once`)。DROP は一切しない。
"""
import argparse
import csv
import io
import os
import sys

import pandas as pd

TABLE = "v2_salesnow_headcount_snapshot"
BATCH = 500

DDL = f"""-- 従業員数の定点スナップショット
-- snapshot_date : この一括取得を行った日 (同一バッチは同じ値)
-- collated_at   : 元データ側がその企業を収集した日。snapshot_date とは別物で、
--                 実測で 244 日に分散している。鮮度の記録として必ず残す。
CREATE TABLE IF NOT EXISTS {TABLE} (
  snapshot_date    TEXT    NOT NULL,
  corporate_number TEXT    NOT NULL,
  employee_count   INTEGER,
  collated_at      TEXT,
  PRIMARY KEY (snapshot_date, corporate_number)
);
CREATE INDEX IF NOT EXISTS idx_headcount_snapshot_corp
  ON {TABLE}(corporate_number, snapshot_date);
"""


def sq(v):
    """SQL 文字列リテラル。None は NULL。"""
    if v is None or (isinstance(v, float) and pd.isna(v)):
        return "NULL"
    return "'" + str(v).replace("'", "''") + "'"


def sn(v):
    """SQL 数値リテラル。欠損は NULL。"""
    if v is None or pd.isna(v):
        return "NULL"
    return str(int(v))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True, help="salesnow_companies.csv のパス")
    ap.add_argument("--date", required=True, help="スナップショット日 (YYYY-MM-DD)")
    ap.add_argument("--outdir", default="scripts/salesnow_snapshot/out")
    args = ap.parse_args()

    os.makedirs(args.outdir, exist_ok=True)
    print(f"読込: {args.source}", file=sys.stderr)
    df = pd.read_csv(
        args.source,
        usecols=["corporate_number", "employee_count", "collated_at"],
        dtype={"corporate_number": str},
        low_memory=False,
    )
    total = len(df)

    # 法人番号が無い行は主キーを作れないため除外する (黙って落とさず件数を出す)
    no_key = df["corporate_number"].isna() | (df["corporate_number"].astype(str).str.strip() == "")
    df = df[~no_key].copy()

    # 同一法人が collated_at 違いで複数行ある (実測 54 法人 / 109 行)。
    # 主キーが衝突するため、収集日が最新の 1 行に寄せる。
    # これは phase0 §3 が「地域集計時に二重計上される」と指摘した前処理そのもの。
    df["_ca"] = pd.to_datetime(df["collated_at"], errors="coerce")
    before = len(df)
    df = df.sort_values("_ca").drop_duplicates("corporate_number", keep="last")
    deduped = before - len(df)

    df = df.sort_values("corporate_number")
    rows = len(df)

    stem = f"snapshot_{args.date}"
    csv_path = os.path.join(args.outdir, stem + ".csv")
    sql_path = os.path.join(args.outdir, stem + ".sql")

    with io.open(csv_path, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(["snapshot_date", "corporate_number", "employee_count", "collated_at"])
        for r in df.itertuples(index=False):
            w.writerow([args.date, r.corporate_number,
                        "" if pd.isna(r.employee_count) else int(r.employee_count),
                        "" if pd.isna(r.collated_at) else r.collated_at])

    with io.open(sql_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(f"-- 生成: build_snapshot.py / snapshot_date = {args.date}\n")
        f.write(f"-- 書き込み行数: {rows:,} (Turso Scaler の月間 100M 行に対し "
                f"{rows / 100_000_000 * 100:.3f}%)\n")
        f.write(f"-- 元データ {total:,} 行 → 法人番号なし {int(no_key.sum())} 行を除外, "
                f"重複 {deduped} 行を最新 collated_at に集約\n")
        f.write("-- 冪等: 同じ snapshot_date を再実行しても行数は増えない\n\n")
        f.write(DDL)
        f.write("\n")
        recs = list(df.itertuples(index=False))
        for i in range(0, len(recs), BATCH):
            chunk = recs[i:i + BATCH]
            f.write(f"INSERT OR REPLACE INTO {TABLE} "
                    "(snapshot_date, corporate_number, employee_count, collated_at) VALUES\n")
            vals = [
                f"({sq(args.date)},{sq(r.corporate_number)},"
                f"{sn(r.employee_count)},{sq(r.collated_at)})"
                for r in chunk
            ]
            f.write(",\n".join(vals))
            f.write(";\n")

    print()
    print("=" * 70)
    print(f"snapshot_date        : {args.date}")
    print(f"元データ             : {total:,} 行")
    print(f"  法人番号なしで除外 : {int(no_key.sum())} 行")
    print(f"  重複を集約         : {deduped} 行")
    print(f"書き込む行数         : {rows:,} 行")
    print(f"  Rows Written 消費  : {rows / 100_000_000 * 100:.3f}% "
          f"(Scaler の月間 100M 行に対して)")
    print(f"  概算ストレージ     : {rows * 51 / 1024 / 1024:.1f} MB")
    print("=" * 70)
    print(f"CSV : {csv_path}")
    print(f"SQL : {sql_path}  ({os.path.getsize(sql_path) / 1024 / 1024:.1f} MB)")
    print()
    print("投入 (ユーザー実行):")
    print(f"  turso db shell salesnow < {sql_path}")
    print()
    print("投入後の確認:")
    print(f"  turso db shell salesnow \"SELECT snapshot_date, COUNT(*), "
          f"SUM(employee_count) FROM {TABLE} GROUP BY snapshot_date\"")


if __name__ == "__main__":
    main()
