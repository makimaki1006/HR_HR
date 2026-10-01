"""Generate history-only upsert SQL, without credentials, network or DB writes.

User-only execution: inspect existing schema, review generated SQL, execute once
with stop-on-error and rollback. Historical rows are retained, never DROPped.
2025/2026 prefecture amounts and effective dates use the shared official CSV.
"""
import argparse
from pathlib import Path
from minimum_wage_rates import DEFAULT_SOURCE, load_rates
from update_minimum_wages import build_sql, inspect_schema

# === 全国加重平均（2016-2025） ===
NATIONAL_AVERAGES = {
    2016: 823,
    2017: 848,
    2018: 874,
    2019: 901,
    2020: 902,
    2021: 930,
    2022: 961,
    2023: 1004,
    2024: 1055,
    2025: 1121,
}

# === 都道府県別データ（2023年度） ===
PREF_2023 = {
    "北海道": 960, "青森県": 898, "岩手県": 893, "宮城県": 923, "秋田県": 897,
    "山形県": 900, "福島県": 900, "茨城県": 953, "栃木県": 954, "群馬県": 935,
    "埼玉県": 1028, "千葉県": 1026, "東京都": 1113, "神奈川県": 1112,
    "新潟県": 931, "富山県": 948, "石川県": 933, "福井県": 931, "山梨県": 938,
    "長野県": 948, "岐阜県": 950, "静岡県": 984, "愛知県": 1027, "三重県": 973,
    "滋賀県": 967, "京都府": 1008, "大阪府": 1064, "兵庫県": 1001, "奈良県": 936,
    "和歌山県": 929, "鳥取県": 900, "島根県": 904, "岡山県": 932, "広島県": 970,
    "山口県": 928, "徳島県": 896, "香川県": 918, "愛媛県": 897, "高知県": 897,
    "福岡県": 941, "佐賀県": 900, "長崎県": 898, "熊本県": 898, "大分県": 899,
    "宮崎県": 897, "鹿児島県": 897, "沖縄県": 896,
}

# === 都道府県別データ（2024年度） ===
PREF_2024 = {
    "北海道": 1010, "青森県": 953, "岩手県": 952, "宮城県": 973, "秋田県": 951,
    "山形県": 955, "福島県": 955, "茨城県": 1005, "栃木県": 1004, "群馬県": 985,
    "埼玉県": 1078, "千葉県": 1076, "東京都": 1163, "神奈川県": 1162,
    "新潟県": 985, "富山県": 998, "石川県": 984, "福井県": 984, "山梨県": 988,
    "長野県": 998, "岐阜県": 1001, "静岡県": 1034, "愛知県": 1077, "三重県": 1023,
    "滋賀県": 1017, "京都府": 1058, "大阪府": 1114, "兵庫県": 1052, "奈良県": 986,
    "和歌山県": 980, "鳥取県": 957, "島根県": 962, "岡山県": 982, "広島県": 1020,
    "山口県": 979, "徳島県": 955, "香川県": 970, "愛媛県": 956, "高知県": 952,
    "福岡県": 992, "佐賀県": 956, "長崎県": 953, "熊本県": 952, "大分県": 954,
    "宮崎県": 952, "鹿児島県": 953, "沖縄県": 952,
}


def build_rows():
    rows = [(year, "全国", wage) for year, wage in NATIONAL_AVERAGES.items()]
    rows += [(2023, pref, wage) for pref, wage in PREF_2023.items()]
    rows += [(2024, pref, wage) for pref, wage in PREF_2024.items()]
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--as-of")
    parser.add_argument("--schema-db", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true", help="Compatibility flag: this script always generates SQL only")
    parser.add_argument("--include-legacy", action="store_true", help="Opt-in 2016-2024 initialization from the legacy constants; review before use")
    args = parser.parse_args()
    schemas = inspect_schema(args.schema_db)[0] if args.schema_db else None
    args.output.write_text(build_sql(load_rates(args.source), args.as_of, schemas,
                                    history_only=True, legacy_history=build_rows() if args.include_legacy else ()), encoding="utf-8")
    print(f"Generated {args.output}; no database writes. Existing history is preserved.")
    if not schemas:
        print("Schema not inspected; verify tables/keys before user execution.")


if __name__ == "__main__":
    main()
