# -*- coding: utf-8 -*-
"""Round 10 Phase 1 検証 (read-only, 適用後に実行)。

事前: scripts/_tmp_phase1_dump_pre.py で pre snapshot を取得済 (data/_tmp_phase1_pre_snapshot.csv)
本スクリプト: build_municipality_target_thickness.py で再生成 + ingest 適用後に実行。

検証項目:
  - Q1: 同値率 (distinct score per muni 分布) → distinct=1 muni < 5 期待
  - Q2: cap saturation muni 数 → 0 期待
  - Q3: ランキング整合性 (上位 100 重複率) → ≥ 80% 期待
  - Q4: 順位差中央値 → ≤ 50 位
  - Q5: range 不変条件 → score/thickness とも 0..200 範囲内
  - Q6: top_score バケット (重点/拡張/維持/低)
"""
import csv
import sqlite3
import statistics
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

DB = Path("data/hellowork.db")
PRE_CSV = Path("data/_tmp_phase1_pre_snapshot.csv")

con = sqlite3.connect(str(DB))
cur = con.cursor()

print("=== Round 10 Phase 1 検証レポート ===\n")

# Q1: 同値率
print("--- Q1: 同値率 (distinct score per muni) ---")
distinct_dist = list(cur.execute("""
    SELECT distinct_scores, COUNT(*) AS muni_count
    FROM (
        SELECT municipality_code,
               COUNT(DISTINCT distribution_priority_score) AS distinct_scores
        FROM municipality_recruiting_scores
        GROUP BY municipality_code
    )
    GROUP BY distinct_scores
    ORDER BY distinct_scores
"""))
for d, c in distinct_dist:
    marker = " ← Phase 1 で消滅期待" if d == 1 else ""
    print(f"  distinct={d:2d}: {c:5d} muni{marker}")
distinct1 = next((c for d, c in distinct_dist if d == 1), 0)

# Q2: cap saturation
print("\n--- Q2: thickness=200 cap saturation muni 数 ---")
capped = cur.execute("""
    SELECT COUNT(*) FROM (
        SELECT municipality_code, COUNT(*) AS n_occ,
               SUM(CASE WHEN target_thickness_index >= 199.99 THEN 1 ELSE 0 END) AS n_capped
        FROM municipality_recruiting_scores
        GROUP BY municipality_code
        HAVING n_capped = n_occ AND n_occ >= 10
    )
""").fetchone()[0]
print(f"  全職業 cap muni: {capped} (期待: 0)")

# Q3-Q4: ランキング整合性
print("\n--- Q3-Q4: ランキング整合性 (pre snapshot 比較) ---")
if not PRE_CSV.exists():
    print(f"  ERROR: pre snapshot 不在 ({PRE_CSV})")
    overlap_pct = None
    median_diff = None
else:
    pre = {}
    with PRE_CSV.open(encoding="utf-8") as f:
        for r in csv.DictReader(f):
            pre[r["municipality_code"]] = (float(r["top_score_pre"]), int(r["rk_pre"]))
    post_rows = list(cur.execute("""
        SELECT municipality_code, MAX(distribution_priority_score) AS top
        FROM municipality_recruiting_scores GROUP BY municipality_code
        ORDER BY top DESC
    """))
    post = {m: (t, rk) for rk, (m, t) in enumerate(post_rows, 1)}
    pre_top100 = {m for m, (_, rk) in pre.items() if rk <= 100}
    post_top100 = {m for m, (_, rk) in post.items() if rk <= 100}
    overlap_pct = 100.0 * len(pre_top100 & post_top100) / 100
    rank_diffs = [abs(post[m][1] - pre[m][1]) for m in pre if m in post]
    median_diff = statistics.median(rank_diffs) if rank_diffs else None
    mean_diff = statistics.mean(rank_diffs) if rank_diffs else None
    print(f"  上位 100 重複率: {overlap_pct:.1f}% (期待: ≥80%)")
    print(f"  順位差中央値: {median_diff:.1f}, 平均: {mean_diff:.1f}, 最大: {max(rank_diffs)}")

# Q5: range 不変条件
print("\n--- Q5: range 不変条件 ---")
score_oor, thick_oor = cur.execute("""
    SELECT
        SUM(CASE WHEN distribution_priority_score < 0 OR distribution_priority_score > 200 THEN 1 ELSE 0 END),
        SUM(CASE WHEN target_thickness_index < 0 OR target_thickness_index > 200 THEN 1 ELSE 0 END)
    FROM municipality_recruiting_scores
""").fetchone()
print(f"  score range OOR: {score_oor} (期待: 0)")
print(f"  thickness range OOR: {thick_oor} (期待: 0)")

# Q6: top_score バケット
print("\n--- Q6: top_score バケット (Phase 1 前: 重点 7 / 拡張 102 / 維持 322 / 低 1,464) ---")
b = cur.execute("""
    SELECT
        SUM(CASE WHEN top >= 160 THEN 1 ELSE 0 END),
        SUM(CASE WHEN top >= 130 AND top < 160 THEN 1 ELSE 0 END),
        SUM(CASE WHEN top >= 100 AND top < 130 THEN 1 ELSE 0 END),
        SUM(CASE WHEN top < 100 THEN 1 ELSE 0 END)
    FROM (SELECT municipality_code, MAX(distribution_priority_score) AS top
          FROM municipality_recruiting_scores GROUP BY municipality_code)
""").fetchone()
print(f"  重点配信(>=160): {b[0]} / 拡張(130-160): {b[1]} / 維持(100-130): {b[2]} / 低(<100): {b[3]}")

# 不変条件チェック
print("\n=== 不変条件 PASS/FAIL ===")
failures = []
if score_oor and score_oor > 0:
    failures.append(f"score range OOR: {score_oor}")
if thick_oor and thick_oor > 0:
    failures.append(f"thickness range OOR: {thick_oor}")
if capped > 0:
    failures.append(f"all-capped muni: {capped} (期待 0)")
if distinct1 >= 5:
    failures.append(f"distinct=1 muni: {distinct1} (期待 <5)")
if overlap_pct is not None and overlap_pct < 90:
    failures.append(f"top100 overlap: {overlap_pct:.1f}% (Phase 1B 期待 ≥90%)")

# Phase 1B 追加検証
b160 = cur.execute("""
    SELECT COUNT(*) FROM (
        SELECT municipality_code, MAX(distribution_priority_score) AS top
        FROM municipality_recruiting_scores GROUP BY municipality_code
    ) WHERE top >= 160
""").fetchone()[0]
print(f"\n--- Phase 1B: >=160 muni 数 = {b160} (旧 7、許容 1-30) ---")
if b160 < 1 or b160 > 30:
    failures.append(f">=160 件数: {b160} (許容 1-30 を逸脱)")

if failures:
    print("❌ 不変条件違反:")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
else:
    print("✅ 全不変条件 PASS")
