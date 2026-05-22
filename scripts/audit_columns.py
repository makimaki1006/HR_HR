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


# ===========================================================================
# 2026-05-21 拡張: Python ETL ↔ Rust match cross-check
# ===========================================================================
# 背景: 2026-05-21 keyword_category 言語乖離事故 (Python ETL は日本語キー
# "急募系" 等で書込み、Rust match は英語キー "urgent" 等を想定 → 全件 silent
# fallback)。同種の乖離を機械的に検出するため、定義済みの対応表を
# cross-check する。
#
# 対応表: (ETL ファイル, ETL の dict / list 変数名, Rust ファイル, Rust 関数名)
# 新規追加時はここに 1 行加えるだけで cross-check が走る。
print()
print("=== PYTHON ETL vs RUST MATCH CROSS-CHECK ===")

ETL_RUST_PAIRS = [
    {
        "name": "keyword_category",
        "etl_file": "scripts/compute_v2_text.py",
        "etl_var": "KEYWORD_CATEGORIES",
        "rust_files": [
            ("src/handlers/analysis/helpers.rs", "keyword_category_label"),
            ("src/handlers/analysis/helpers.rs", "keyword_category_color"),
        ],
    },
    # 新規追加時はここに append。
    # 例: {"name": "severity", "etl_file": "...", "etl_var": "SEVERITY_LEVELS",
    #      "rust_files": [("...", "severity_label")]},
]

def extract_python_dict_keys(file_path, var_name):
    """Python ソースから `VAR_NAME = {...}` の辞書キーを **AST 解析で** 抽出。
    regex 解析だと dict value 内 (例: re.compile(r"急募|すぐ") の引数) の文字列も
    拾ってしまうので、ast モジュールで厳密に Dict.keys だけを取り出す。"""
    if not os.path.exists(file_path):
        return None
    import ast
    src = open(file_path, "r", encoding="utf-8").read()
    try:
        tree = ast.parse(src)
    except SyntaxError:
        return None
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == var_name:
                    if isinstance(node.value, ast.Dict):
                        keys = set()
                        for k in node.value.keys:
                            if isinstance(k, ast.Constant) and isinstance(k.value, str):
                                keys.add(k.value)
                        return keys
                    if isinstance(node.value, (ast.List, ast.Tuple, ast.Set)):
                        keys = set()
                        for elt in node.value.elts:
                            if isinstance(elt, ast.Constant) and isinstance(elt.value, str):
                                keys.add(elt.value)
                        return keys
    return None


def extract_rust_match_keys(file_path, fn_name):
    """Rust ソースから `fn fn_name(...) { match x { ... } }` の match キーを抽出。"""
    if not os.path.exists(file_path):
        return None
    src = open(file_path, "r", encoding="utf-8").read()
    # fn FN_NAME ... match ... { ... _ => ... }
    m = re.search(rf"fn\s+{re.escape(fn_name)}\s*\([\s\S]*?match\s+\w+\s*\{{([\s\S]*?)\n\s*_\s*=>", src)
    if not m:
        return None
    body = m.group(1)
    # 各 arm の左辺 "xxx" => ... を抽出
    keys = set()
    for line in body.splitlines():
        if "=>" not in line:
            continue
        lhs = line.split("=>", 1)[0]
        for sm in re.finditer(r'"([^"]+)"', lhs):
            keys.add(sm.group(1))
    return keys


cross_check_total = 0
cross_check_mismatch = 0
for pair in ETL_RUST_PAIRS:
    print()
    print(f"--- {pair['name']} ---")
    etl_keys = extract_python_dict_keys(pair["etl_file"], pair["etl_var"])
    if etl_keys is None:
        print(f"  [WARN]ETL 抽出失敗: {pair['etl_file']} の {pair['etl_var']} が見つからない")
        continue
    print(f"  ETL ({pair['etl_var']}): {sorted(etl_keys)}")
    for rust_file, rust_fn in pair["rust_files"]:
        cross_check_total += 1
        rust_keys = extract_rust_match_keys(rust_file, rust_fn)
        if rust_keys is None:
            print(f"  [WARN]Rust 抽出失敗: {rust_file}::{rust_fn}")
            continue
        only_in_etl = etl_keys - rust_keys
        only_in_rust = rust_keys - etl_keys
        if not only_in_etl and not only_in_rust:
            print(f"  [OK]{rust_fn}: 一致 ({len(etl_keys)} keys)")
        else:
            cross_check_mismatch += 1
            print(f"  [NG]{rust_fn}: 乖離あり")
            if only_in_etl:
                print(f"     ETL のみに存在 (Rust 未対応 → silent fallback): {sorted(only_in_etl)}")
            if only_in_rust:
                print(f"     Rust のみに存在 (ETL 出力なし、デッドコード?): {sorted(only_in_rust)}")

print()
print(f"=== CROSS-CHECK SUMMARY: {cross_check_total - cross_check_mismatch}/{cross_check_total} 一致 ===")
if cross_check_mismatch > 0:
    print(f"[WARN] {cross_check_mismatch} 件の乖離あり (上記参照)")
else:
    print("[OK] 全 pair 整合")


# ===========================================================================
# 2026-05-22 拡張: Rust 内 Vec/const ↔ match arm cross-check
# ===========================================================================
# 背景: 2026-05-21 balance.rs:297 size_bands_list (チルダ ~) と SQL CASE 式
# (波線 〜) の乖離で「産業×従業員規模スタックグラフが全 0 描画」事故。同種
# 構造 (Rust の static 配列 / Vec literal が別ファイルの match キーと乖離) を
# 機械的に検出するため、ペア定義を登録して自動 diff する。
print()
print("=== RUST VEC/CONST vs MATCH CROSS-CHECK ===")

RUST_VEC_PAIRS = [
    {
        "name": "employment_type_expansion",
        "lhs_file": "src/handlers/recruitment_diag/mod.rs",
        "lhs_kind": "match_returns_vec",
        "lhs_fn": "expand_employment_type",
        "rhs_file": "src/handlers/emp_classifier.rs",
        "rhs_kind": "match_returns_vec",
        "rhs_fn": "expand_to_db_values",
        # Other バリアントの値 (両関数で「その他」相当) を取り出す
        "lhs_arm_key": "その他",
        "rhs_arm_key": "Other",
    },
    # 新規追加時はここに append。
    # 例: {"name": "size_bands_vs_sql_case",
    #      "lhs_file": "src/handlers/balance.rs", "lhs_kind": "static_array", "lhs_var": "size_bands_list",
    #      "rhs_file": "src/handlers/balance.rs", "rhs_kind": "sql_case_when_then",
    #      "rhs_fn_pattern": "WHEN.+THEN '([^']+)'"},
]


def extract_rust_match_arm_vec(file_path, fn_name, arm_key):
    """Rust ソースの `fn fn_name(...) { match x { "arm_key" => vec!["a", "b", ...], ... } }`
    から、指定 arm_key の vec! の中身を文字列セットとして返す。"""
    if not os.path.exists(file_path):
        return None
    src = open(file_path, "r", encoding="utf-8").read()
    # fn FN_NAME ... match ... { ... }
    m_fn = re.search(rf"fn\s+{re.escape(fn_name)}\s*\([\s\S]*?\{{([\s\S]*?)^\}}", src, re.MULTILINE)
    if not m_fn:
        return None
    body = m_fn.group(1)
    # 各 arm を行ごとに見て、arm_key と一致する arm の右辺の vec!["...", "..."] を抽出
    # arm の書式は "key" => vec!["a", "b"], または KeyVariant => vec!["a", "b"]
    for line in body.splitlines():
        # arm_key が文字列リテラルとして登場するパターン
        if f'"{arm_key}"' in line or (arm_key in line and "=>" in line):
            # この行 (および続く行) の vec!["..."] を抽出
            m_vec = re.search(r'vec!\s*\[([^\]]+)\]', line)
            if m_vec:
                vec_body = m_vec.group(1)
                keys = set(re.findall(r'"([^"]+)"', vec_body))
                if keys:
                    return keys
    return None


vec_check_total = 0
vec_check_mismatch = 0
for pair in RUST_VEC_PAIRS:
    print()
    print(f"--- {pair['name']} ---")
    lhs_keys = None
    rhs_keys = None
    if pair["lhs_kind"] == "match_returns_vec":
        lhs_keys = extract_rust_match_arm_vec(pair["lhs_file"], pair["lhs_fn"], pair["lhs_arm_key"])
    if pair["rhs_kind"] == "match_returns_vec":
        rhs_keys = extract_rust_match_arm_vec(pair["rhs_file"], pair["rhs_fn"], pair["rhs_arm_key"])
    if lhs_keys is None:
        print(f"  [WARN] lhs 抽出失敗: {pair['lhs_file']}::{pair['lhs_fn']}[{pair['lhs_arm_key']}]")
        continue
    if rhs_keys is None:
        print(f"  [WARN] rhs 抽出失敗: {pair['rhs_file']}::{pair['rhs_fn']}[{pair['rhs_arm_key']}]")
        continue
    vec_check_total += 1
    print(f"  lhs ({pair['lhs_fn']}[{pair['lhs_arm_key']}]): {sorted(lhs_keys)}")
    print(f"  rhs ({pair['rhs_fn']}[{pair['rhs_arm_key']}]): {sorted(rhs_keys)}")
    only_lhs = lhs_keys - rhs_keys
    only_rhs = rhs_keys - lhs_keys
    if not only_lhs and not only_rhs:
        print(f"  [OK] {pair['name']}: 一致 ({len(lhs_keys)} keys)")
    else:
        vec_check_mismatch += 1
        print(f"  [NG] {pair['name']}: 乖離あり")
        if only_lhs:
            print(f"     lhs のみに存在: {sorted(only_lhs)}")
        if only_rhs:
            print(f"     rhs のみに存在: {sorted(only_rhs)}")

print()
print(f"=== RUST VEC/CONST CHECK SUMMARY: {vec_check_total - vec_check_mismatch}/{vec_check_total} 一致 ===")
if vec_check_mismatch > 0:
    print(f"[WARN] {vec_check_mismatch} 件の乖離あり (上記参照)")
else:
    print("[OK] 全 pair 整合")

