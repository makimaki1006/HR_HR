# -*- coding: utf-8 -*-
"""Turso へ投入する前に「無駄な書き込みが無いか」を逆証明する。

課金は行単位なので、次を実測する:
  (1) ファイル内に重複 PK が無いか (あると自己 REPLACE で二重書き込み)
  (2) 破壊的文 (DROP/DELETE/UPDATE) が混ざっていないか
  (3) 実際の書き込み行数 (SQLite の total_changes で計測)
  (4) 再実行したときの追加コスト
  (5) 情報を持たない行 (employee_count が NULL) の割合
  (6) 索引が書き込みを増幅していないか
"""
import os
import re
import sqlite3
import sys

SP = os.path.dirname(os.path.abspath(__file__))
SQL_PATH = os.path.join(SP, "snapout", "snapshot_2026-08-12.sql")
TABLE = "v2_salesnow_headcount_snapshot"


def hr(t):
    print("\n" + "=" * 76 + f"\n## {t}\n" + "=" * 76)


sql = open(SQL_PATH, encoding="utf-8").read()
# コメント行 (-- で始まる) を除いた「実行される SQL」だけを判定対象にする。
# 初版はコメント中の "DELETE" を破壊的文として誤検出していた。
exec_sql = "\n".join(
    line for line in sql.splitlines() if not line.lstrip().startswith("--")
)

hr("(1) 破壊的な文が混ざっていないか")
DANGER = ["DROP ", "DELETE ", "UPDATE ", "TRUNCATE", "ALTER "]
for kw in DANGER:
    n = len(re.findall(kw, exec_sql, re.IGNORECASE))
    mark = "★検出" if n else "無し"
    print(f"  {kw.strip():<10}: {n:>3} 件  {mark}")
print("\n  → DROP/DELETE が無いこと = 既存データを消さない (feedback_turso_upload_once)")

hr("(2) ファイル内の PK 重複 (自己 REPLACE = 二重書き込み)")
vals = re.findall(r"\('(\d{4}-\d{2}-\d{2})','([^']*)'", sql)
print(f"  VALUES の総数: {len(vals):,}")
keys = set(vals)
print(f"  ユニークな (snapshot_date, corporate_number): {len(keys):,}")
dupn = len(vals) - len(keys)
print(f"  重複: {dupn} 件  {'★無駄な書き込みが発生する' if dupn else '無し'}")
dates = {v[0] for v in vals}
print(f"  snapshot_date の種類: {len(dates)} → {sorted(dates)}")

hr("(3) 文の数とバッチ効率")
stmts = exec_sql.count("INSERT OR IGNORE INTO") + exec_sql.count("INSERT OR REPLACE INTO")
mode = "OR IGNORE" if "INSERT OR IGNORE" in exec_sql else "OR REPLACE"
print(f"  INSERT 文の数: {stmts} (方式: {mode})")
print(f"  1 文あたりの行数: {len(vals) / max(stmts,1):.0f}")
print(f"  ファイルサイズ: {os.path.getsize(SQL_PATH)/1024/1024:.1f} MB")
print(f"  → 1 行ずつの INSERT なら {len(vals):,} 文になるところを {stmts} 文に圧縮している")

hr("(4) 実際の書き込み行数を計測 (SQLite total_changes)")
db = os.path.join(SP, "waste_test.db")
if os.path.exists(db):
    os.remove(db)
c = sqlite3.connect(db)
before = c.total_changes
c.executescript(sql)
c.commit()
run1 = c.total_changes - before
rows1 = c.execute(f"SELECT COUNT(*) FROM {TABLE}").fetchone()[0]
print(f"  1 回目: total_changes = {run1:,} / テーブル行数 = {rows1:,}")
print(f"    書き込み増幅率: {run1/max(rows1,1):.2f} 倍 (1.00 が理想)")

before = c.total_changes
c.executescript(sql)
c.commit()
run2 = c.total_changes - before
rows2 = c.execute(f"SELECT COUNT(*) FROM {TABLE}").fetchone()[0]
print(f"  2 回目: total_changes = {run2:,} / テーブル行数 = {rows2:,}")
if run2 == 0:
    print(f"    → 行数も書き込みも増えない。誤って再実行しても課金されない")
else:
    print(f"    → 行数は変わらない ({rows1:,} → {rows2:,}) が、"
          f"書き込みは {run2:,} 行分課金される")
    print("    ⚠ OR REPLACE を使うとここが全額再課金になる")

hr("(5) 情報を持たない行はどれだけあるか")
nullc = c.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE employee_count IS NULL").fetchone()[0]
print(f"  employee_count が NULL: {nullc:,} 行 ({nullc/rows1*100:.2f}%)")
print(f"  除外した場合の書き込み: {rows1-nullc:,} 行 (削減 {nullc/rows1*100:.2f}%)")
print("\n  判断材料: NULL 行は増減計算には使えないが、")
print("  「その時点でこの企業が収録されていた」という事実は記録される。")
print("  次のスナップで値が入れば、収録開始の検出に使える。")

hr("(6) 索引による書き込み増幅")
idx = c.execute(
    "SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name=?", (TABLE,)
).fetchall()
print(f"  この表に付く索引: {len(idx)} 個")
for name, s in idx:
    kind = "PRIMARY KEY 由来 (自動)" if s is None else "明示的に作成"
    print(f"    {name:<40} {kind}")
print(f"\n  → 索引 1 個につき行挿入時に別 B-tree への書き込みが発生する。")
print(f"    Turso が『行』をどう数えるかは公表仕様を確認できていないため、")
print(f"    最悪ケースとして {len(idx)+1} 倍を見込むと "
      f"{rows1*(len(idx)+1):,} 行相当。")
print(f"    Scaler 100M/月に対し {rows1*(len(idx)+1)/100_000_000*100:.3f}%")

hr("(7) 不変条件")
checks = [
    ("PK 重複が無い",
     c.execute(f"SELECT COUNT(*) FROM (SELECT snapshot_date,corporate_number FROM {TABLE} "
               f"GROUP BY 1,2 HAVING COUNT(*)>1)").fetchone()[0] == 0),
    ("corporate_number に NULL/空が無い",
     c.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE corporate_number IS NULL "
               f"OR TRIM(corporate_number)=''").fetchone()[0] == 0),
    ("snapshot_date が 1 種類",
     c.execute(f"SELECT COUNT(DISTINCT snapshot_date) FROM {TABLE}").fetchone()[0] == 1),
    ("employee_count に負値が無い",
     c.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE employee_count < 0").fetchone()[0] == 0),
]
for label, ok in checks:
    print(f"  {'OK ' if ok else '★NG'} {label}")

c.close()
os.remove(db)
print("\n(一時 DB は削除済み)")
