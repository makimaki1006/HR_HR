#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""HRハッカー84列CSVダウンロード機能の実ファイル検証スクリプト。

工程⑦(static/jobgen.html の downloadCsv())が生成するCSVを、Gemini APIを一切呼ばずに
Pythonで忠実に再現し、実バイト列を検算する。対象:
  - src/job_gen/hrhacker.rs の HRHACKER_COLUMNS(84列・正典順)
  - static/jobgen.html の downloadCsv() のエスケープ・BOM・改行ロジック

検証項目:
  1. 列数が84
  2. ヘッダ1列目が「求人id」
  3. 列順が HRHACKER_COLUMNS と完全一致
  4. UTF-8 BOM (EF BB BF) が先頭に付く
  5. カンマ・改行(\\n, \\r)・ダブルクォートを含む値のRFC4180準拠エスケープ
     (""でエスケープ、フィールド全体を"で囲む)
  6. Excelで開いても文字化けしない(BOM+UTF-8で正しくデコードできる)
  7. 実際にPythonのcsvモジュードでパースし直して元の値と一致する(ラウンドトリップ)

実行方法:
  python scripts/verify_hrhacker_csv.py
"""
import csv
import io
import re
import sys
from pathlib import Path

# Windows端末(既定cp932)でも日本語出力が文字化けしないようにする。
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")

REPO_ROOT = Path(__file__).resolve().parent.parent
HRHACKER_RS = REPO_ROOT / "src" / "job_gen" / "hrhacker.rs"
JOBGEN_HTML = REPO_ROOT / "static" / "jobgen.html"

results: list[tuple[str, bool, str]] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    results.append((name, ok, detail))


def extract_hrhacker_columns() -> list[str]:
    """hrhacker.rs から HRHACKER_COLUMNS 定数の中身(84列・正典順)を抽出する。"""
    text = HRHACKER_RS.read_text(encoding="utf-8")
    m = re.search(
        r"pub const HRHACKER_COLUMNS: \[&str; 84\] = \[(.*?)\];", text, re.S
    )
    if not m:
        raise RuntimeError("HRHACKER_COLUMNS が hrhacker.rs 内に見つからない")
    body = m.group(1)
    # Rust の文字列リテラル "..." を1つずつ拾う(単純な非エスケープ日本語文字列前提)。
    cols = re.findall(r'"([^"]*)"', body)
    return cols


def extract_bom_prefix() -> bytes:
    """jobgen.html の downloadCsv() 内、csv文字列先頭に埋め込まれたBOMの生バイト列を取り出す。"""
    data = JOBGEN_HTML.read_bytes()
    idx = data.find(b"const csv=")
    if idx == -1:
        raise RuntimeError("downloadCsv() の csv= 代入行が見つからない")
    # `const csv="<BOMバイト列>"+cols...` の "..." の中身(先頭の引用符の直後3バイト程度)を取得。
    quote_start = data.index(b'"', idx) + 1
    return data[quote_start : quote_start + 3]


def js_q(v: str) -> str:
    """static/jobgen.html downloadCsv() 内の q() 関数を1文字も違わずPythonで再現。

    元のJS:
      const q=v=>{v=v==null?"":String(v);
        return /[",\\n\\r]/.test(v)?'"'+v.replace(/"/g,'""')+'"':v;};
    """
    if v is None:
        v = ""
    v = str(v)
    if re.search(r'[",\n\r]', v):
        return '"' + v.replace('"', '""') + '"'
    return v


def build_csv_like_js(cols: list[str], row: dict) -> str:
    """downloadCsv() の以下の行を再現:
      const csv="﻿"+cols.map(q).join(",")+"\\r\\n"+cols.map(c=>q(row[c])).join(",")+"\\r\\n";
    """
    header = ",".join(js_q(c) for c in cols)
    data_line = ",".join(js_q(row.get(c, "")) for c in cols)
    return "﻿" + header + "\r\n" + data_line + "\r\n"


def main() -> int:
    # ---- 1. 正典列定義の読み込み ----
    hrhacker_columns = extract_hrhacker_columns()
    check(
        "HRHACKER_COLUMNS を hrhacker.rs から抽出",
        len(hrhacker_columns) == 84,
        f"抽出件数={len(hrhacker_columns)}",
    )
    if len(hrhacker_columns) != 84:
        # 以降の検証は無意味なのでここで打ち切る。
        print_results()
        return 1

    # ---- 2. downloadCsv() のロジックをUI(row=Object.keys順)に忠実に再現するための行データ生成 ----
    # サーバ側 handlers.rs は HRHACKER_COLUMNS の順で serde_json::Map(preserve_order) に
    # 詰め直してから row として返すため、ブラウザ側 Object.keys(row) は HRHACKER_COLUMNS
    # 順になる。ここでは同じ前提(row の並び=HRHACKER_COLUMNS順)でCSVを組み立てる。
    row: dict[str, str] = {}
    for i, col in enumerate(hrhacker_columns):
        if col == "案件名":
            row[col] = '訪問介護スタッフ,急募"新規オープン"'  # カンマ+二重引用符を含む
        elif col == "仕事内容":
            row[col] = "1行目\n2行目\r\n3行目"  # LF, CRLF混在の改行を含む
        elif col == "キャッチコピー":
            row[col] = 'そのまま"引用"を含む見出し'
        elif col == "求人id":
            row[col] = "JOB-000123"
        else:
            row[col] = f"値_{i:02d}"

    csv_text = build_csv_like_js(hrhacker_columns, row)
    csv_bytes = csv_text.encode("utf-8")  # Blob([csv],{type:"text/csv;charset=utf-8"}) 相当

    # ---- 3. 列数チェック ----
    header_line = csv_text.split("\r\n", 1)[0].lstrip("﻿")
    parsed_header = next(csv.reader(io.StringIO(header_line)))
    check("列数が84", len(parsed_header) == 84, f"実列数={len(parsed_header)}")

    # ---- 4. ヘッダ1列目が「求人id」 ----
    check(
        "ヘッダ1列目が「求人id」",
        parsed_header[0] == "求人id",
        f"実際の1列目='{parsed_header[0]}'",
    )

    # ---- 5. 列順が HRHACKER_COLUMNS と完全一致 ----
    order_match = parsed_header == hrhacker_columns
    mismatch_detail = ""
    if not order_match:
        diffs = [
            (i, a, b)
            for i, (a, b) in enumerate(zip(parsed_header, hrhacker_columns))
            if a != b
        ]
        mismatch_detail = f"不一致箇所(先頭5件)={diffs[:5]}"
    check("列順がHRHACKER_COLUMNSと完全一致", order_match, mismatch_detail)

    # ---- 6. UTF-8 BOM (EF BB BF) が先頭に付く ----
    check(
        "生成CSVバイト列の先頭がEF BB BF",
        csv_bytes[:3] == b"\xef\xbb\xbf",
        f"実際の先頭バイト={csv_bytes[:3]!r}",
    )

    # ---- 6b. jobgen.html のソースコード自体に同じBOMバイト列が埋め込まれているか ----
    bom_in_source = extract_bom_prefix()
    check(
        "jobgen.html の downloadCsv() ソース内に UTF-8 BOM (EF BB BF) が literal で埋め込まれている",
        bom_in_source == b"\xef\xbb\xbf",
        f"実際={bom_in_source!r}",
    )

    # ---- 7. RFC4180準拠エスケープの検証(個別ケース) ----
    esc_cases = [
        ("カンマを含む値", "a,b", '"a,b"'),
        ("二重引用符を含む値", 'a"b', '"a""b"'),
        ("LF改行を含む値", "a\nb", '"a\nb"'),
        ("CR改行を含む値", "a\rb", '"a\rb"'),
        ("CRLF改行を含む値", "a\r\nb", '"a\r\nb"'),
        ("特殊文字なしの値", "abc", "abc"),
        ("空値", "", ""),
        ("None相当(未設定列)", None, ""),
    ]
    esc_all_ok = True
    esc_detail_fail = []
    for label, input_v, expected in esc_cases:
        actual = js_q(input_v)
        ok = actual == expected
        esc_all_ok = esc_all_ok and ok
        if not ok:
            esc_detail_fail.append(f"{label}: input={input_v!r} expected={expected!r} actual={actual!r}")
    check(
        "RFC4180準拠エスケープ(カンマ/改行/二重引用符/空値)",
        esc_all_ok,
        "; ".join(esc_detail_fail) if esc_detail_fail else "全ケースOK",
    )

    # ---- 8. ラウンドトリップ検証: Python csvモジュールでBOM付きUTF-8ファイルとして
    #          読み直し、元のrow値と完全一致するか ----
    decoded = csv_bytes.decode("utf-8-sig")  # Excelが読む時と同じ BOM 自動除去付きデコード
    reader = csv.reader(io.StringIO(decoded))
    parsed_rows = list(reader)
    roundtrip_ok = len(parsed_rows) == 2
    roundtrip_detail = ""
    if roundtrip_ok:
        parsed_header2, parsed_data = parsed_rows
        expected_data = [row.get(c, "") for c in hrhacker_columns]
        roundtrip_ok = parsed_data == expected_data
        if not roundtrip_ok:
            diffs = [
                (i, a, b)
                for i, (a, b) in enumerate(zip(parsed_data, expected_data))
                if a != b
            ]
            roundtrip_detail = f"不一致(先頭5件)={diffs[:5]}"
    else:
        roundtrip_detail = f"データ行数異常: {len(parsed_rows)}行(期待2行)"
    check("ラウンドトリップ(BOM付きUTF-8→csv.reader→元値と一致)", roundtrip_ok, roundtrip_detail)

    # ---- 9. Excel文字化けなしの傍証: BOM込みUTF-8として日本語列名がデコードできる ----
    # (全バイト列をデコードする。マルチバイト文字境界を壊す固定バイト数スライスは使わない)
    try:
        redecoded_full = csv_bytes.decode("utf-8")
        excel_ok = redecoded_full.startswith("﻿求人id")
    except UnicodeDecodeError as e:
        excel_ok = False
        redecoded_full = str(e)
    check(
        "UTF-8としてデコード可能(Excel文字化け対策の傍証)",
        excel_ok,
        "" if excel_ok else redecoded_full,
    )

    print_results()
    return 0 if all(ok for _, ok, _ in results) else 1


def print_results() -> None:
    print("=" * 70)
    print("HRハッカー84列CSV 実ファイル検証結果")
    print("=" * 70)
    for name, ok, detail in results:
        mark = "OK" if ok else "NG"
        line = f"[{mark}] {name}"
        if detail:
            line += f"  ({detail})"
        print(line)
    print("-" * 70)
    total = len(results)
    passed = sum(1 for _, ok, _ in results if ok)
    print(f"合計 {passed}/{total} 件 OK")


if __name__ == "__main__":
    sys.exit(main())
