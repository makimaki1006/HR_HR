"""SQL列抽出スクリプト (調査専用、コミット不要)
src/handlers配下の全 .rs から SELECT ... FROM v2_external_* / v2_salesnow_* / salesnow_*
を抽出し、各テーブルごとのユニーク列を出力する。
"""
import re
import os

pat_block = re.compile(
    r"(SELECT[\s\S]{1,3000}?FROM\s+(v2_external_\w+|v2_salesnow_\w+|salesnow_\w+))",
    re.IGNORECASE,
)

# SELECT 句が Rust 文字列リテラル内にあるかを判定するため、前文脈を確認
# 簡易版: 直前の引用符状況をスキャン
def in_rust_string(text, idx):
    """SELECT block が Rust の文字列リテラル内にあるか粗く判定。
    現在の方針: SELECT 直前の最寄り見える区切りが `"` または `r#"` ならOK。
    ただし誤検出を防ぐため、blockに変数代入っぽい記号 (`let ` , `:` 型注釈, `vec`)
    が含まれていたら除外。"""
    return True  # ここでは block 内不要トークン除外で対処

files = []
for root, dirs, fns in os.walk("src/handlers"):
    for fn in fns:
        if fn.endswith(".rs"):
            files.append(os.path.join(root, fn))

cols_per_table = {}
select_with_star = []
col_to_locations = {}  # col -> list of (file, line, table)

UKW = {
    "FROM","SELECT","WHERE","AND","OR","BY","ORDER","GROUP","CAST","REAL","NULL",
    "SUM","AVG","MAX","MIN","COUNT","IS","NOT","IN","LIKE","LIMIT","DESC","ASC",
    "TRUE","FALSE","THEN","ELSE","CASE","END","WHEN","HAVING","DISTINCT","UNION",
    "INNER","LEFT","RIGHT","JOIN","ON","NULLIF","COALESCE","ABS","ROUND","CTE","WITH",
}

# Rust シンボル / 非SQL列 を除外するブラックリスト
RUST_TOKENS = {
    "Option","String","Vec","HashMap","INTEGER","TEXT","assert_ne","query","query_turso",
    "query_turso_or_local","get_i64","get_str_ref","sort_by","sql","to_string","vec",
    "table","table_exists","insert","len","db","national_select","empty_choropleth",
    "basis","data_label","source_name","source_year","weight_source","_tmp","muni","pref",
    "super","companies","get_f64","get_str",
}

# Rust ブロック (let、変数代入、関数引数) の典型コードトークン
RUST_FRAGMENT_RE = re.compile(r"\blet\s+|::|\bfn\s+|\bpub\s+|\bmut\s+|\bimpl\s+|->\s")

for fp in files:
    try:
        s = open(fp, "r", encoding="utf-8").read()
    except Exception:
        continue
    for m in pat_block.finditer(s):
        block = m.group(1)
        tbl = m.group(2)
        # block位置 → 該当行番号
        line_no = s.count("\n", 0, m.start()) + 1
        # block内に Rust 構文の痕跡があれば除外 (SELECT が変数名でないか確認)
        if RUST_FRAGMENT_RE.search(block):
            continue
        # block を実際にSQL文字列として扱うため、最初の SELECT トークンが
        # 直前にどう囲まれているかチェック
        # 簡易: 直前 20 文字に `"` または `#"` (raw string) があれば SQL とみなす
        pre = s[max(0, m.start()-30):m.start()]
        if '"' not in pre and "#\"" not in pre and "r#" not in pre:
            # SQL リテラルの開始ではない可能性が高い
            continue
        # SELECT と FROM の間を抽出
        upper = block.upper()
        from_idx = upper.rfind("FROM")
        body = block[len("SELECT"):from_idx].strip()
        # Rust の文字列継続 \ + 改行、 \n エスケープ、ダブルクオート、バックスラッシュを除去
        body = body.replace("\\n", " ").replace('"', " ").replace("\\", " ")
        # 改行をスペースに
        body = body.replace("\n", " ").replace("\r", " ")
        # トップレベルのカンマで分割
        depth = 0
        parts = []
        cur = []
        for c in body:
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
            if c == "," and depth == 0:
                parts.append("".join(cur).strip())
                cur = []
            else:
                cur.append(c)
        if cur:
            parts.append("".join(cur).strip())
        for p in parts:
            pl = p.strip()
            if not pl:
                continue
            if pl == "*":
                select_with_star.append((fp, line_no, tbl))
                continue
            # alias 抽出
            m_as = re.search(r"\s+(?:as|AS)\s+(\w+)\s*$", pl)
            if m_as:
                col = m_as.group(1)
            else:
                # 最後のトークン、識別子のみ
                last = pl.split()[-1]
                # remove parens / dots / quotes
                last = last.strip("()'\"")
                if "." in last:
                    last = last.split(".")[-1]
                col = last
            col = re.sub(r"[^A-Za-z0-9_].*$", "", col)
            if not col:
                continue
            if col[0].isdigit():
                continue
            if col.upper() in UKW:
                continue
            if col in RUST_TOKENS:
                continue
            cols_per_table.setdefault(tbl, set()).add(col)
            col_to_locations.setdefault(col, []).append((fp, line_no, tbl))

all_cols = set()
for t, cs in cols_per_table.items():
    all_cols |= cs

print(f"TABLES: {len(cols_per_table)}")
print(f"UNIQUE_COLS: {len(all_cols)}")
print()
for t in sorted(cols_per_table.keys()):
    print(f"== {t} ==")
    print(", ".join(sorted(cols_per_table[t])))
    print()
print("=== SELECT * USAGES ===")
if not select_with_star:
    print("(none)")
for fp, ln, tbl in select_with_star:
    print(f"{fp}:{ln} - {tbl}")

print()
print("=== ALL_COLS sorted ===")
for c in sorted(all_cols):
    print(c)

# === label_for_column の match arm を抽出 ===
print()
print("=== label_for_column registered keys ===")
navy = open("src/handlers/survey/report_html/navy_report.rs", "r", encoding="utf-8").read()
# 関数本体: fn label_for_column(...) { match key { ... _ => key } }
m_fn = re.search(r"fn label_for_column[\s\S]*?match key\s*\{([\s\S]*?)\n\s*_\s*=>\s*key", navy)
if not m_fn:
    print("ERROR: label_for_column not found")
else:
    body = m_fn.group(1)
    # 各 arm: "key" | "key2" => "label",
    arm_re = re.compile(r'"([^"]+)"')
    keys = set()
    # 行ごとに左辺キー (=> より前) のみ
    for line in body.splitlines():
        # 右辺 (=>以降) を除去
        if "=>" not in line:
            continue
        lhs = line.split("=>", 1)[0]
        for k in arm_re.findall(lhs):
            keys.add(k)
    print(f"REGISTERED: {len(keys)}")
    for k in sorted(keys):
        print(k)

    # 差分
    print()
    print("=== UNMAPPED COLUMNS (SQL extracted - registered) ===")
    unmapped = sorted(all_cols - keys)
    print(f"COUNT: {len(unmapped)}")
    for c in unmapped:
        locs = col_to_locations.get(c, [])
        loc_str = "; ".join(f"{os.path.basename(fp)}:{ln}({tbl})" for fp, ln, tbl in locs[:3])
        print(f"{c}\t{loc_str}")

    # v2_external_* 限定
    print()
    print("=== UNMAPPED v2_external_* COLUMNS ===")
    ext_cols = set()
    ext_locs = {}
    for tbl, cs in cols_per_table.items():
        if not tbl.startswith("v2_external_"):
            continue
        for c in cs:
            ext_cols.add(c)
            # この col が登場する v2_external_* のロケーションのみ
            for fp, ln, t in col_to_locations.get(c, []):
                if t == tbl:
                    ext_locs.setdefault(c, []).append((fp, ln, tbl))
    ext_unmapped = sorted(ext_cols - keys)
    print(f"COUNT: {len(ext_unmapped)}")
    for c in ext_unmapped:
        locs = ext_locs.get(c, [])
        # ユニークテーブル
        seen = set()
        tbls = []
        for fp, ln, t in locs:
            if t not in seen:
                seen.add(t)
                tbls.append(t)
        first = locs[0] if locs else (None, 0, "")
        print(f"{c}\t{','.join(tbls)}\t{os.path.basename(first[0]) if first[0] else ''}:{first[1]}")

