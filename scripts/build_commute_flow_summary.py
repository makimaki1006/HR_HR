# -*- coding: utf-8 -*-
"""
Phase 3 Step 5 前提 (Step A): commute_flow_summary 生成スクリプト
==============================================================

入力: `data/hellowork.db` の `v2_external_commute_od` (国勢調査 OD 行列、83,402 行)
出力:
  1. ローカル `data/hellowork.db` の `commute_flow_summary` テーブル (DROP + CREATE + INSERT)
  2. CSV `data/generated/commute_flow_summary.csv` (Turso 投入用)

集計仕様:
  - destination ごとに origin TOP 20
  - self-loop 除外 (origin == destination)
  - flow_share = origin flow_count / destination 総流入数
  - occupation_group_code = 'all'  (Step 5 後続で職業別に拡張)
  - occupation_group_name = '全職業'
  - estimated_target_flow_* は初期 NULL (Step 5 後続で計算)
  - estimation_method = 'commute_od_top20_all_occupation'
  - municipality_code = `prefecture:municipality_name` 擬似コード (JIS マスタ未整備のため)

ヘッダー混入ガード:
  - prefecture / municipality が NULL / 空 / '{都道府県,市区町村}' のレコードは除外

使い方:
  python scripts/build_commute_flow_summary.py            # CSV + ローカル DB 投入
  python scripts/build_commute_flow_summary.py --dry-run  # CSV 出力のみ、DB 書き込みなし
  python scripts/build_commute_flow_summary.py --csv-only # 同上

設計原則:
  - **本番 (Turso) には書き込まない** (ユーザー手動 upload に委ねる)
  - 既存 `data/hellowork.db` 上の他テーブルには触らない (commute_flow_summary のみ DROP + CREATE)
"""
import argparse
import csv
import sqlite3
import sys
import io
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
DEFAULT_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
DEFAULT_CSV = SCRIPT_DIR.parent / "data" / "generated" / "commute_flow_summary.csv"

TOP_N = 20
ESTIMATION_METHOD = "commute_od_top20_all_occupation"
DEFAULT_SOURCE_YEAR = 2020

# DDL: docs/survey_market_intelligence_phase0_2_schema.sql の同名テーブル定義と整合
DDL = """
CREATE TABLE IF NOT EXISTS commute_flow_summary (
    destination_municipality_code TEXT NOT NULL,
    destination_prefecture TEXT NOT NULL,
    destination_municipality_name TEXT NOT NULL,
    origin_municipality_code TEXT NOT NULL,
    origin_prefecture TEXT NOT NULL,
    origin_municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL DEFAULT 'all',
    occupation_group_name TEXT NOT NULL DEFAULT '全職業',
    flow_count INTEGER NOT NULL DEFAULT 0,
    flow_share REAL,
    target_origin_population INTEGER,
    estimated_target_flow_conservative INTEGER,
    estimated_target_flow_standard INTEGER,
    estimated_target_flow_aggressive INTEGER,
    estimation_method TEXT,
    estimated_at TEXT,
    rank_to_destination INTEGER NOT NULL,
    source_year INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        destination_municipality_code,
        origin_municipality_code,
        occupation_group_code,
        source_year
    )
)
"""

# 出力カラム順 (CSV ヘッダーと INSERT で共通化)
COLUMNS = [
    "destination_municipality_code",
    "destination_prefecture",
    "destination_municipality_name",
    "origin_municipality_code",
    "origin_prefecture",
    "origin_municipality_name",
    "occupation_group_code",
    "occupation_group_name",
    "flow_count",
    "flow_share",
    "target_origin_population",
    "estimated_target_flow_conservative",
    "estimated_target_flow_standard",
    "estimated_target_flow_aggressive",
    "estimation_method",
    "estimated_at",
    "rank_to_destination",
    "source_year",
]


def make_pseudo_code(prefecture: str, municipality: str) -> str:
    """JIS 市区町村コードのマスタ未整備のため、擬似コードを `prefecture:municipality_name` 形式で生成。

    PK 一意性を保証する (commute_flow_summary の PK は dest_code + origin_code + occupation + year)。
    将来 JIS コード投入時に置換可能 (UPDATE で全行 code を書き換えるか、別テーブルで mapping)。
    """
    return f"{prefecture}:{municipality}"


def fetch_clean_commute_od(conn: sqlite3.Connection):
    """v2_external_commute_od からヘッダー混入ガード適用 + self-loop 除外 で全レコード取得。"""
    sql = """
    SELECT origin_pref, origin_muni, dest_pref, dest_muni,
           total_commuters, male_commuters, female_commuters, reference_year
    FROM v2_external_commute_od
    WHERE origin_pref IS NOT NULL AND origin_pref <> ''
      AND origin_pref NOT IN ('都道府県', '出発地')
      AND origin_muni IS NOT NULL AND origin_muni <> '' AND origin_muni <> '市区町村'
      AND dest_pref IS NOT NULL AND dest_pref <> ''
      AND dest_pref NOT IN ('都道府県', '到着地')
      AND dest_muni IS NOT NULL AND dest_muni <> '' AND dest_muni <> '市区町村'
      AND NOT (origin_pref = dest_pref AND origin_muni = dest_muni)
      AND total_commuters IS NOT NULL AND total_commuters > 0
    """
    return conn.execute(sql).fetchall()


def build_summary_rows(raw_rows):
    """destination ごとに TOP N を抽出し flow_share を計算 → list[dict]。"""
    by_dest = defaultdict(list)
    for op, om, dp, dm, total, male, female, year in raw_rows:
        by_dest[(dp, dm, year or DEFAULT_SOURCE_YEAR)].append((op, om, total))

    estimated_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
    out = []
    for (dp, dm, year), origins in by_dest.items():
        # destination 総流入数
        dest_total = sum(c for _, _, c in origins)
        if dest_total <= 0:
            continue
        # TOP N
        origins.sort(key=lambda x: x[2], reverse=True)
        for rank, (op, om, count) in enumerate(origins[:TOP_N], start=1):
            out.append(
                {
                    "destination_municipality_code": make_pseudo_code(dp, dm),
                    "destination_prefecture": dp,
                    "destination_municipality_name": dm,
                    "origin_municipality_code": make_pseudo_code(op, om),
                    "origin_prefecture": op,
                    "origin_municipality_name": om,
                    "occupation_group_code": "all",
                    "occupation_group_name": "全職業",
                    "flow_count": count,
                    "flow_share": round(count / dest_total, 6),
                    "target_origin_population": None,
                    "estimated_target_flow_conservative": None,
                    "estimated_target_flow_standard": None,
                    "estimated_target_flow_aggressive": None,
                    "estimation_method": ESTIMATION_METHOD,
                    "estimated_at": estimated_at,
                    "rank_to_destination": rank,
                    "source_year": year,
                }
            )
    return out


def write_csv(rows, csv_path: Path):
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    with open(csv_path, "w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


def write_db(rows, conn: sqlite3.Connection):
    """commute_flow_summary を DROP + CREATE + INSERT。他テーブルには触らない。"""
    conn.execute("DROP TABLE IF EXISTS commute_flow_summary")
    conn.execute(DDL)
    insert_sql = f"INSERT INTO commute_flow_summary ({', '.join(COLUMNS)}) VALUES ({', '.join('?' * len(COLUMNS))})"
    conn.executemany(insert_sql, [tuple(r[c] for c in COLUMNS) for r in rows])
    conn.commit()


def verify(conn: sqlite3.Connection):
    """検証 SQL: 行数 / rank 重複 / rank 範囲 / self-loop / share 範囲 / 日本語表示。"""
    print("\n--- 検証 SQL ---")
    cur = conn.cursor()

    # 1. 行数
    n = cur.execute("SELECT COUNT(*) FROM commute_flow_summary").fetchone()[0]
    print(f"  [1] 行数: {n:,}")

    # 2. destination ごとの rank 重複なし
    dup = cur.execute(
        """
        SELECT destination_municipality_code, occupation_group_code, source_year, rank_to_destination, COUNT(*) AS c
        FROM commute_flow_summary
        GROUP BY destination_municipality_code, occupation_group_code, source_year, rank_to_destination
        HAVING c > 1
        """
    ).fetchall()
    if dup:
        print(f"  [2] ❌ rank 重複あり: {len(dup)} 件")
    else:
        print(f"  [2] ✅ rank 重複なし")

    # 3. rank が 1〜TOP_N
    out_of_range = cur.execute(
        f"SELECT COUNT(*) FROM commute_flow_summary WHERE rank_to_destination < 1 OR rank_to_destination > {TOP_N}"
    ).fetchone()[0]
    print(f"  [3] {'✅' if out_of_range == 0 else '❌'} rank 範囲外: {out_of_range} 件")

    # 4. self-loop なし
    self_loop = cur.execute(
        """
        SELECT COUNT(*) FROM commute_flow_summary
        WHERE destination_prefecture = origin_prefecture
          AND destination_municipality_name = origin_municipality_name
        """
    ).fetchone()[0]
    print(f"  [4] {'✅' if self_loop == 0 else '❌'} self-loop: {self_loop} 件")

    # 5. flow_share が 0〜1
    bad_share = cur.execute(
        "SELECT COUNT(*) FROM commute_flow_summary WHERE flow_share < 0 OR flow_share > 1.0"
    ).fetchone()[0]
    print(f"  [5] {'✅' if bad_share == 0 else '❌'} flow_share 範囲外: {bad_share} 件")

    # 6. flow_share の合計が destination ごとに 1 以下 (TOP N で打ち切るため <= 1)
    over_total = cur.execute(
        """
        SELECT destination_municipality_code, SUM(flow_share) AS total
        FROM commute_flow_summary
        GROUP BY destination_municipality_code
        HAVING total > 1.0001
        """
    ).fetchall()
    print(f"  [6] {'✅' if not over_total else '❌'} destination 別 flow_share 合計 > 1.0: {len(over_total)} 件")

    # 7. 日本語表示正常 (DISTINCT prefecture サンプル)
    samples = cur.execute(
        "SELECT DISTINCT destination_prefecture FROM commute_flow_summary ORDER BY destination_prefecture LIMIT 8"
    ).fetchall()
    print(f"  [7] DISTINCT destination_prefecture サンプル: {[s[0] for s in samples]}")

    # 8. destination 数
    dest_count = cur.execute(
        "SELECT COUNT(DISTINCT destination_municipality_code) FROM commute_flow_summary"
    ).fetchone()[0]
    print(f"  [8] DISTINCT destination 数: {dest_count:,}")

    # 9. occupation_group の確認
    occ = cur.execute(
        "SELECT occupation_group_code, occupation_group_name, COUNT(*) FROM commute_flow_summary GROUP BY occupation_group_code"
    ).fetchall()
    print(f"  [9] occupation_group: {occ}")

    # 10. estimated_target_flow_* がすべて NULL であること (初期値仕様通り)
    non_null = cur.execute(
        """
        SELECT COUNT(*) FROM commute_flow_summary
        WHERE estimated_target_flow_conservative IS NOT NULL
           OR estimated_target_flow_standard IS NOT NULL
           OR estimated_target_flow_aggressive IS NOT NULL
        """
    ).fetchone()[0]
    print(f"  [10] {'✅' if non_null == 0 else '⚠️'} estimated_target_flow_* 非 NULL: {non_null} 件 (期待: 0)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1] if __doc__ else "")
    parser.add_argument("--db", type=Path, default=DEFAULT_DB, help=f"ローカル sqlite DB (default: {DEFAULT_DB})")
    parser.add_argument("--csv", type=Path, default=DEFAULT_CSV, help=f"CSV 出力先 (default: {DEFAULT_CSV})")
    parser.add_argument("--dry-run", action="store_true", help="CSV のみ出力、DB 書き込みなし")
    parser.add_argument("--csv-only", action="store_true", help="--dry-run のエイリアス")
    args = parser.parse_args()

    print("=" * 70)
    print("Phase 3 Step A: commute_flow_summary 生成 (READ-only on Turso)")
    print("=" * 70)

    if not args.db.exists():
        print(f"ERROR: DB が見つかりません: {args.db}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(str(args.db))

    # 入力テーブル確認
    cnt = conn.execute("SELECT COUNT(*) FROM v2_external_commute_od").fetchone()[0]
    print(f"\n入力: v2_external_commute_od = {cnt:,} 行 (ローカル {args.db})")

    # 集計
    print("集計中...")
    raw = fetch_clean_commute_od(conn)
    print(f"  ヘッダーガード + self-loop 除外 後: {len(raw):,} 行")

    rows = build_summary_rows(raw)
    print(f"  集計後 (TOP {TOP_N} × destination): {len(rows):,} 行")

    # CSV 出力
    write_csv(rows, args.csv)
    print(f"\nCSV 出力: {args.csv} ({args.csv.stat().st_size:,} B)")

    # DB 書き込み (dry-run でなければ)
    dry_run = args.dry_run or args.csv_only
    if dry_run:
        print("\n[--dry-run / --csv-only] DB 書き込みスキップ")
    else:
        write_db(rows, conn)
        print(f"\nDB 書き込み: {args.db}::commute_flow_summary (DROP + CREATE + INSERT)")
        # 検証は DB に書いた場合のみ
        verify(conn)

    conn.close()
    print("\n完了。次のステップ:")
    print("  1. ユーザー手動: scripts/upload_to_turso.py の TABLES に commute_flow_summary 追加")
    print("  2. ユーザー手動: python scripts/upload_to_turso.py で Turso 反映")
    print("  3. python scripts/verify_turso_v2_sync.py で MATCH 確認")
    return 0


if __name__ == "__main__":
    sys.exit(main())
