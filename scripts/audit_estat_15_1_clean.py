# -*- coding: utf-8 -*-
"""
audit_estat_15_1_clean.py
==========================

15-1 clean CSV (`data/generated/estat_15_1_merged.csv`) の二重計上・除外漏れ監査。

DB 投入前の最終チェック。9 項目を実行し、Markdown レポートを出力する。

CLI:
  python scripts/audit_estat_15_1_clean.py
"""
from __future__ import annotations

import json
import sqlite3
import sys
from collections import defaultdict
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

CSV_PATH = Path("data/generated/estat_15_1_merged.csv")
TEMP_DIR = Path("data/generated/temp")
META_PATH = Path("data/generated/estat_15_1_metadata.json")
DOC_PATH = Path("docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_CSV_AUDIT.md")

# 期待値
ALLOWED_AGE = {
    "15-19", "20-24", "25-29", "30-34", "35-39", "40-44", "45-49",
    "50-54", "55-59", "60-64", "65-69", "70-74", "75-79", "80-84",
    "85-89", "90-94", "95+",
}  # 17 階級
FORBIDDEN_AGE_PATTERNS = ["総数", "再掲", "15-64", "20-69"]
FORBIDDEN_AGE_EXACT = {"65+", "75+", "85+"}

ALLOWED_GENDER = {"male", "female"}
FORBIDDEN_GENDER = {"total", "総数"}

EXPECTED_OCC_COUNT = 11
FORBIDDEN_OCC_NAME = {"分類不能の職業", "総数"}
FORBIDDEN_OCC_CODE = {"0", "00000", "999"}

EXPECTED_ROWS_PER_MUNI = 374  # = 2 gender × 17 age × 11 occupation

SAMPLE_MUNICIPALITIES = [
    ("13103", "東京都", "港区"),
    ("13104", "東京都", "新宿区"),
    ("13201", "東京都", "八王子市"),
    ("23211", "愛知県", "豊田市"),
    ("01101", "北海道", "札幌市中央区"),
    ("13100", "東京都", "特別区部"),
]


def main() -> int:
    import pandas as pd

    print(f"[audit] reading {CSV_PATH}")
    if not CSV_PATH.exists():
        print(f"ERROR: CSV not found: {CSV_PATH}", file=sys.stderr)
        return 1

    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    n_rows = len(df)
    print(f"[audit] rows: {n_rows:,}")

    report: list[str] = []
    overall_pass = True

    def add(line: str = "") -> None:
        report.append(line)

    add("# 15-1 clean CSV 監査レポート")
    add()
    add(f"対象: `{CSV_PATH}`  ")
    add(f"行数: **{n_rows:,}**  ")
    add(f"期待: 716,958 (≈ 2 male/female × 17 age × 11 occ × ~1,917 muni)")
    add()
    add("---")
    add()

    # ----------------------------------------------------------------- #
    # 項目 1: age_class distinct
    # ----------------------------------------------------------------- #
    add("## 1. age_class distinct")
    add()
    age_set = set(df["age_class"].astype(str).unique())
    add(f"distinct count: **{len(age_set)}**  ")
    add(f"values: {sorted(age_set)}")
    add()
    forbidden_found = []
    for v in age_set:
        if v in FORBIDDEN_AGE_EXACT:
            forbidden_found.append(v)
            continue
        for pat in FORBIDDEN_AGE_PATTERNS:
            if pat in v:
                forbidden_found.append(v)
                break
    missing = ALLOWED_AGE - age_set
    extras = age_set - ALLOWED_AGE
    if forbidden_found:
        add(f"❌ 禁止値検出: {forbidden_found}")
        overall_pass = False
    if missing:
        add(f"❌ 必須階級欠損: {sorted(missing)}")
        overall_pass = False
    if extras and not forbidden_found:
        add(f"⚠ 想定外: {sorted(extras)}")
    if not forbidden_found and not missing:
        add(f"✅ **PASS** — 17 階級のみ、禁止値なし")
    add()

    # ----------------------------------------------------------------- #
    # 項目 2: gender distinct
    # ----------------------------------------------------------------- #
    add("## 2. gender distinct")
    add()
    gender_set = set(df["gender"].astype(str).unique())
    add(f"distinct count: **{len(gender_set)}**  ")
    add(f"values: {sorted(gender_set)}")
    add()
    g_forbidden = gender_set & FORBIDDEN_GENDER
    g_extra = gender_set - ALLOWED_GENDER
    if g_forbidden:
        add(f"❌ 禁止値: {sorted(g_forbidden)}")
        overall_pass = False
    elif g_extra:
        add(f"⚠ 想定外: {sorted(g_extra)}")
        overall_pass = False
    elif gender_set != ALLOWED_GENDER:
        add(f"❌ 期待 (male/female) と不一致")
        overall_pass = False
    else:
        add(f"✅ **PASS** — male/female の 2 値のみ")
    add()

    # ----------------------------------------------------------------- #
    # 項目 3: occupation_name distinct
    # ----------------------------------------------------------------- #
    add("## 3. occupation distinct")
    add()
    occ_name_set = set(df["occupation_name"].astype(str).unique())
    occ_code_set = set(df["occupation_code"].astype(str).unique())
    add(f"distinct occupation_code count: **{len(occ_code_set)}**  ")
    add(f"codes: {sorted(occ_code_set)}  ")
    add(f"distinct occupation_name count: **{len(occ_name_set)}**  ")
    add("names:")
    for n in sorted(occ_name_set):
        add(f"- `{n}`")
    add()
    occ_n_forbidden = occ_name_set & FORBIDDEN_OCC_NAME
    occ_c_forbidden = occ_code_set & FORBIDDEN_OCC_CODE
    if occ_n_forbidden:
        add(f"❌ 禁止 occupation_name: {sorted(occ_n_forbidden)}")
        overall_pass = False
    if occ_c_forbidden:
        add(f"❌ 禁止 occupation_code: {sorted(occ_c_forbidden)}")
        overall_pass = False
    if len(occ_code_set) != EXPECTED_OCC_COUNT:
        add(f"❌ occupation_code 件数 {len(occ_code_set)} ≠ 期待 {EXPECTED_OCC_COUNT}")
        overall_pass = False
    if not occ_n_forbidden and not occ_c_forbidden and len(occ_code_set) == EXPECTED_OCC_COUNT:
        add(f"✅ **PASS** — 11 職業大分類のみ、禁止値なし")
    add()

    # ----------------------------------------------------------------- #
    # 項目 4: municipality_code ごとの行数
    # ----------------------------------------------------------------- #
    add("## 4. 市区町村別行数 (期待: 374 = 2 × 17 × 11)")
    add()
    counts = df.groupby("municipality_code").size()
    n_muni = len(counts)
    add(f"distinct municipalities: **{n_muni:,}**  ")
    add(f"row count distribution: {counts.value_counts().to_dict()}")
    add()
    bad_munis = counts[counts != EXPECTED_ROWS_PER_MUNI]
    if len(bad_munis) > 0:
        add(f"❌ 374 行でない自治体: **{len(bad_munis):,}** 件")
        add()
        add("サンプル (上位 10 件):")
        add()
        add("| municipality_code | rows |")
        add("|------|---:|")
        for code, cnt in bad_munis.head(10).items():
            add(f"| {code} | {cnt:,} |")
        overall_pass = False
    else:
        add(f"✅ **PASS** — 全 {n_muni:,} 自治体が 374 行")
    add()

    # ----------------------------------------------------------------- #
    # 項目 5: muni × gender × age ごとの職業数 (期待 11)
    # ----------------------------------------------------------------- #
    add("## 5. (muni × gender × age) ごとの職業数 (期待: 11)")
    add()
    g5 = df.groupby(["municipality_code", "gender", "age_class"])["occupation_code"].nunique()
    bad5 = g5[g5 != 11]
    if len(bad5) > 0:
        add(f"❌ 11 職業でない組合せ: **{len(bad5):,}** 件")
        add()
        add("サンプル (上位 10 件):")
        add()
        add("| muni | gender | age | occ_count |")
        add("|---|---|---|---:|")
        for (m, g, a), v in bad5.head(10).items():
            add(f"| {m} | {g} | {a} | {v} |")
        overall_pass = False
    else:
        add(f"✅ **PASS** — 全 {len(g5):,} 組合せが 11 職業")
    add()

    # ----------------------------------------------------------------- #
    # 項目 6: muni × gender × occupation ごとの年齢階級数 (期待 17)
    # ----------------------------------------------------------------- #
    add("## 6. (muni × gender × occupation) ごとの年齢階級数 (期待: 17)")
    add()
    g6 = df.groupby(["municipality_code", "gender", "occupation_code"])["age_class"].nunique()
    bad6 = g6[g6 != 17]
    if len(bad6) > 0:
        add(f"❌ 17 年齢階級でない組合せ: **{len(bad6):,}** 件")
        add()
        add("サンプル (上位 10 件):")
        add()
        add("| muni | gender | occ | age_count |")
        add("|---|---|---|---:|")
        for (m, g, o), v in bad6.head(10).items():
            add(f"| {m} | {g} | {o} | {v} |")
        overall_pass = False
    else:
        add(f"✅ **PASS** — 全 {len(g6):,} 組合せが 17 年齢階級")
    add()

    # ----------------------------------------------------------------- #
    # 項目 7: PK 重複
    # ----------------------------------------------------------------- #
    add("## 7. PK 重複 (CSV 段階: muni × gender × age × occ)")
    add()
    pk_cols = ["municipality_code", "gender", "age_class", "occupation_code"]
    dup_count = int(df.duplicated(subset=pk_cols).sum())
    if dup_count > 0:
        add(f"❌ PK 重複: {dup_count:,} 件")
        overall_pass = False
        dup_sample = df[df.duplicated(subset=pk_cols, keep=False)].head(10)
        add()
        add("サンプル (上位 10 件):")
        add()
        add("```")
        add(str(dup_sample))
        add("```")
    else:
        add(f"✅ **PASS** — PK 重複なし")
    add()
    add(f"DB 投入時の PK は `(municipality_code, basis, occupation_code, age_class, gender, source_year, data_label)`。")
    add(f"basis='workplace', data_label='measured', source_year=2020 を固定で付与すれば衝突なし。")
    add()

    # ----------------------------------------------------------------- #
    # 項目 8: サンプル自治体
    # ----------------------------------------------------------------- #
    add("## 8. サンプル自治体 (期待: 各 374 行)")
    add()
    add("| code | 期待名 | rows | csv_label | 判定 |")
    add("|---|---|---:|---|:---:|")
    for code, pref, name in SAMPLE_MUNICIPALITIES:
        sub = df[df["municipality_code"] == code]
        n = len(sub)
        if n == 0:
            add(f"| {code} | {pref} {name} | 0 | (missing) | ❌ |")
            overall_pass = False
        else:
            csv_pref = sub["prefecture"].dropna().iloc[0] if not sub["prefecture"].dropna().empty else "?"
            csv_name = sub["municipality_name"].dropna().iloc[0] if not sub["municipality_name"].dropna().empty else "?"
            ok = "✅" if n == EXPECTED_ROWS_PER_MUNI else "❌"
            add(f"| {code} | {pref} {name} | {n:,} | {csv_pref}/{csv_name} | {ok} |")
            if n != EXPECTED_ROWS_PER_MUNI:
                overall_pass = False
    add()

    # ----------------------------------------------------------------- #
    # 項目 9: raw 総数との整合 (代表 4 自治体)
    # ----------------------------------------------------------------- #
    add("## 9. raw 総数との差分検証 (4 自治体: 港区/新宿区/八王子/豊田)")
    add()
    add("clean 11 職業合計と raw '総数' (cat03='00000') の差 ≈ raw '分類不能の職業'  ")
    add(f"raw 取得元: {TEMP_DIR}/estat_15_1_page_*.json  ")
    add()

    raw_lookup = _build_raw_total_unclassified_lookup(
        TEMP_DIR, target_codes={c for c, _, _ in SAMPLE_MUNICIPALITIES[:4]}
    )

    add("| muni | gender | age | clean 11職業合計 | raw 総数 | raw 分類不能 | 差 (clean11-rawtotal) | 期待差 (=-rawunc) | 判定 |")
    add("|---|---|---|---:|---:|---:|---:|---:|:---:|")
    for code, pref, name in SAMPLE_MUNICIPALITIES[:4]:
        for gender in ("male", "female"):
            for age in ("25-29", "40-44", "60-64"):
                sub = df[
                    (df["municipality_code"] == code)
                    & (df["gender"] == gender)
                    & (df["age_class"] == age)
                ]
                clean11 = int(sub["population"].sum())
                # raw lookup
                raw_total, raw_unc = raw_lookup.get((code, gender, age), (None, None))
                if raw_total is None:
                    add(f"| {code} | {gender} | {age} | {clean11:,} | (n/a) | (n/a) | - | - | ⚠ |")
                    continue
                diff = clean11 - raw_total
                expected = -raw_unc if raw_unc is not None else None
                ok = (
                    "✅" if expected is not None and diff == expected
                    else ("⚠" if expected is None else "❌")
                )
                if expected is not None and diff != expected:
                    overall_pass = False
                exp_str = f"{expected:,}" if expected is not None else "-"
                add(f"| {code} | {gender} | {age} | {clean11:,} | {raw_total:,} | {raw_unc:,} | {diff:,} | {exp_str} | {ok} |")
    add()
    add("補足: raw 総数 = (clean 11 職業合計) + (raw 分類不能の職業)。  ")
    add("clean は分類不能を除外しているため、`clean11 - rawtotal == -raw_unclassified` が期待値。")
    add()

    # ----------------------------------------------------------------- #
    # 総合判定
    # ----------------------------------------------------------------- #
    add("---")
    add()
    add("## 総合判定")
    add()
    if overall_pass:
        add(f"## ✅ **PASS** — DB 投入承認")
        add()
        add("`municipality_occupation_population` (basis='workplace', data_label='measured', ")
        add("source_name='census_15_1', source_year=2020) への投入に進めます。")
    else:
        add(f"## ❌ **FAIL** — CSV 修正後に再 merge 必要")
    add()
    add("---")
    add()
    add(f"監査スクリプト: `scripts/audit_estat_15_1_clean.py`  ")
    add(f"CSV: `{CSV_PATH}`  ")
    add(f"出力: `{DOC_PATH}`")

    DOC_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(DOC_PATH, "w", encoding="utf-8") as f:
        f.write("\n".join(report))
    print(f"[audit] report: {DOC_PATH}")
    print(f"[audit] overall: {'PASS' if overall_pass else 'FAIL'}")
    return 0 if overall_pass else 1


def _build_raw_total_unclassified_lookup(
    temp_dir: Path, target_codes: set[str]
) -> dict[tuple[str, str, str], tuple[int, int]]:
    """
    raw page JSON から (muni_code, gender, age_class) -> (raw_total, raw_unclassified) を作る。
    raw_total = cat03 '総数 (00000)' のセル
    raw_unclassified = cat03 '999' or '0' (分類不能) のセル
    """
    if not META_PATH.exists():
        print(f"[warn] metadata not found: {META_PATH}")
        return {}
    with open(META_PATH, "r", encoding="utf-8") as f:
        meta = json.load(f)
    axis_map = _load_axis_from_meta(meta)
    cat01_axis = axis_map.get("cat01", {})
    cat02_axis = axis_map.get("cat02", {})

    # 「分類不能」コードを特定
    cat03_axis = axis_map.get("cat03", {})
    UNCLASSIFIED_CODES = {
        code for code, name in cat03_axis.items()
        if "分類不能" in name
    }
    TOTAL_CAT03_CODES = {
        code for code, name in cat03_axis.items() if name in ("総数", "総数（再掲）")
    }

    lookup: dict[tuple[str, str, str], tuple[int, int]] = {}
    raw_total_lookup: dict[tuple[str, str, str], int] = {}
    raw_unc_lookup: dict[tuple[str, str, str], int] = {}

    page_files = sorted(temp_dir.glob("estat_15_1_page_*.json"))
    if not page_files:
        return {}
    for pf in page_files:
        with open(pf, "r", encoding="utf-8") as fh:
            data = json.load(fh)
        values = (
            data.get("GET_STATS_DATA", {})
            .get("STATISTICAL_DATA", {})
            .get("DATA_INF", {})
            .get("VALUE", [])
        )
        if isinstance(values, dict):
            values = [values]
        for rec in values:
            area = str(rec.get("@area", "")).zfill(5)
            if area not in target_codes:
                continue
            cat01 = str(rec.get("@cat01", ""))
            cat02 = str(rec.get("@cat02", ""))
            cat03 = str(rec.get("@cat03", ""))
            cat01_name = cat01_axis.get(cat01, "")
            cat02_name = cat02_axis.get(cat02, "")
            # 男女個別のみ対象
            gender = "male" if "男" in cat01_name else ("female" if "女" in cat01_name else None)
            if gender is None:
                continue
            # 年齢個別 5 歳階級のみ
            age = _norm_age(cat02_name)
            if age not in ALLOWED_AGE:
                continue
            try:
                pop = int(rec.get("$", 0))
            except (TypeError, ValueError):
                pop = 0
            key = (area, gender, age)
            if cat03 in TOTAL_CAT03_CODES or cat03 == "00000":
                raw_total_lookup[key] = pop
            elif cat03 in UNCLASSIFIED_CODES or cat03 == "0":
                raw_unc_lookup[key] = pop

    # マージ
    for key, total in raw_total_lookup.items():
        unc = raw_unc_lookup.get(key, 0)
        lookup[key] = (total, unc)
    return lookup


def _load_axis_from_meta(meta: dict) -> dict[str, dict[str, str]]:
    """metadata JSON から axis_map を構築 (load_axis_metadata の簡略コピー)"""
    out: dict[str, dict[str, str]] = {}
    class_objs = (
        meta.get("GET_META_INFO", {})
        .get("METADATA_INF", {})
        .get("CLASS_INF", {})
        .get("CLASS_OBJ", [])
    )
    if isinstance(class_objs, dict):
        class_objs = [class_objs]
    for cobj in class_objs:
        cid = cobj.get("@id", "")
        classes = cobj.get("CLASS", [])
        if isinstance(classes, dict):
            classes = [classes]
        out[cid] = {str(c.get("@code", "")): str(c.get("@name", "")) for c in classes}
    return out


def _norm_age(name: str) -> str:
    """`15～19歳` -> `15-19`、`95歳以上` -> `95+`"""
    for sep in ("～", "〜", "-", "－", "ー"):
        if sep in name:
            parts = name.split(sep, 1)
            left = parts[0].replace("歳", "").strip()
            right = parts[1].replace("歳", "").replace("以上", "").strip()
            if left and right:
                return f"{left}-{right}"
            if left:
                return f"{left}+"
    if "以上" in name:
        digits = "".join(ch for ch in name if ch.isdigit())
        if digits:
            return f"{digits}+"
    return name


if __name__ == "__main__":
    sys.exit(main())
