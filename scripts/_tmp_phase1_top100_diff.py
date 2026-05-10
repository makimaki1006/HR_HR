# -*- coding: utf-8 -*-
"""Round 10 Phase 1: 上位 100 自治体の dropped / entered 差分分析 (read-only)。

入力:
  - data/_tmp_phase1_pre_snapshot.csv (Phase 1 前 top_score per muni rank)
  - data/hellowork.db (Phase 1 後)

出力 (stdout):
  - dropped: pre top100 中、post top100 から外れた自治体一覧 (score 変動順)
  - entered: post top100 中、pre top100 にいなかった自治体一覧 (score 上昇順)
  - 上位 100 入れ替えの傾向 (都市部 vs 地方、職業構成)
  - >=160 閾値の妥当性 (Phase 1 後、何 muni が該当するか)
"""
import csv
import sqlite3
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

DB = Path("data/hellowork.db")
PRE = Path("data/_tmp_phase1_pre_snapshot.csv")

con = sqlite3.connect(str(DB))
cur = con.cursor()

# --- pre snapshot (rank, top_score) ---
pre = {}
with PRE.open(encoding="utf-8") as f:
    for r in csv.DictReader(f):
        pre[r["municipality_code"]] = {
            "rk": int(r["rk_pre"]),
            "score": float(r["top_score_pre"]),
        }

# --- post (rank, top_score, area_type, prefecture, name) ---
post_rows = list(cur.execute("""
    SELECT s.municipality_code, MAX(s.distribution_priority_score) AS top_score,
           m.prefecture, m.municipality_name, m.area_type
    FROM municipality_recruiting_scores s
    LEFT JOIN municipality_code_master m ON s.municipality_code = m.municipality_code
    GROUP BY s.municipality_code
    ORDER BY top_score DESC
"""))
post = {}
for rk, (mc, top, pref, name, atype) in enumerate(post_rows, 1):
    post[mc] = {
        "rk": rk,
        "score": top,
        "pref": pref or "",
        "name": name or "",
        "atype": atype or "",
    }

# --- 上位 100 セット ---
pre_top100 = set(m for m, d in pre.items() if d["rk"] <= 100)
post_top100 = set(m for m, d in post.items() if d["rk"] <= 100)

dropped = pre_top100 - post_top100
entered = post_top100 - pre_top100
overlap = pre_top100 & post_top100

print(f"=== 上位 100 自治体 入れ替え差分 ===\n")
print(f"  overlap: {len(overlap)} muni ({100.0 * len(overlap)/100:.0f}%)")
print(f"  dropped (pre top100 → post out): {len(dropped)} muni")
print(f"  entered (pre out → post top100): {len(entered)} muni")
print()

# --- 上位代表職種で、付随情報を取りたい ---
def get_top_occ(muni_code):
    r = cur.execute("""
        SELECT occupation_code, distribution_priority_score
        FROM municipality_recruiting_scores
        WHERE municipality_code = ?
        ORDER BY distribution_priority_score DESC LIMIT 1
    """, (muni_code,)).fetchone()
    return (r[0], r[1]) if r else ("?", 0.0)

print("=== Dropped: pre top100 から外れた自治体 (Phase 1 後の rank 順) ===")
print(f"{'muni_code':<8} {'rk_pre':>6} {'rk_post':>7} {'score_pre':>10} {'score_post':>11} {'top_occ':<14} {'area':<18} {'name'}")
dropped_sorted = sorted(dropped, key=lambda m: -post.get(m, {}).get("score", 0))
for mc in dropped_sorted[:30]:
    p = pre[mc]
    q = post.get(mc, {})
    occ, _ = get_top_occ(mc)
    print(f"  {mc:<8} {p['rk']:>6} {q.get('rk', 99999):>7} {p['score']:>10.2f} {q.get('score', 0):>11.2f} {occ:<14} {q.get('atype', ''):<18} {q.get('pref', '')} {q.get('name', '')}")

print()
print("=== Entered: 新規に top100 入りした自治体 (post rank 順) ===")
entered_sorted = sorted(entered, key=lambda m: post[m]["rk"])
for mc in entered_sorted[:30]:
    p = pre.get(mc, {"rk": 99999, "score": 0})
    q = post[mc]
    occ, _ = get_top_occ(mc)
    print(f"  {mc:<8} {p['rk']:>6} {q['rk']:>7} {p['score']:>10.2f} {q['score']:>11.2f} {occ:<14} {q.get('atype', ''):<18} {q.get('pref', '')} {q.get('name', '')}")

# --- area_type 別 ---
print()
print("=== Dropped/Entered の area_type 内訳 ===")
def area_breakdown(codes, label):
    cnt = {"special_ward": 0, "designated_ward": 0, "municipality": 0, "aggregate": 0, "other": 0}
    for mc in codes:
        atype = post.get(mc, {}).get("atype", "other")
        if atype in cnt:
            cnt[atype] += 1
        else:
            cnt["other"] += 1
    print(f"  {label}: 特別区={cnt['special_ward']} / 政令市行政区={cnt['designated_ward']} / 通常市町村={cnt['municipality']} / aggregate={cnt['aggregate']} / その他={cnt['other']}")

area_breakdown(dropped, "Dropped")
area_breakdown(entered, "Entered")

# --- top_occ 内訳 ---
print()
print("=== Dropped/Entered の代表職種内訳 ===")
def occ_breakdown(codes, label):
    cnt = {}
    for mc in codes:
        occ, _ = get_top_occ(mc)
        cnt[occ] = cnt.get(occ, 0) + 1
    print(f"  {label}:")
    for occ, n in sorted(cnt.items(), key=lambda x: -x[1]):
        print(f"    {occ}: {n}")

occ_breakdown(dropped, "Dropped")
occ_breakdown(entered, "Entered")

# --- >= 160 閾値の妥当性 ---
print()
print("=== >=160 閾値の現状 (Phase 1 後) ===")
high = [m for m in post if post[m]["score"] >= 160]
print(f"  Phase 1 後 >= 160 (重点配信): {len(high)} muni (Phase 1 前: 7)")
for mc in sorted(high, key=lambda m: -post[m]["score"])[:10]:
    occ, score = get_top_occ(mc)
    print(f"    {mc} {post[mc]['pref']} {post[mc]['name']:<10} score={post[mc]['score']:.2f} ({occ})")
print()
print("  >=160 を厳しすぎるなら、現実的閾値は score 降順 100/200 位の値:")
score_at_100 = sorted([d['score'] for d in post.values()], reverse=True)[99]
score_at_200 = sorted([d['score'] for d in post.values()], reverse=True)[199] if len(post) >= 200 else None
print(f"    rank 100 の score: {score_at_100:.2f}")
if score_at_200:
    print(f"    rank 200 の score: {score_at_200:.2f}")

# --- PDF 対象自治体 (新宿区/千代田区/伊達市) の前後比較 ---
print()
print("=== PDF 対象自治体 (新宿区 13104 / 千代田区 13101 / 北海道伊達市 01233 / 福島県伊達市 07213) ===")
for mc in ["13104", "13101", "01233", "07213"]:
    p = pre.get(mc, {})
    q = post.get(mc, {})
    occ, _ = get_top_occ(mc)
    print(f"  {mc} {q.get('pref', '')} {q.get('name', ''):<10} pre rank={p.get('rk', 99999):>5} score={p.get('score', 0):.2f} → post rank={q.get('rk', 99999):>5} score={q.get('score', 0):.2f} (top_occ={occ})")
