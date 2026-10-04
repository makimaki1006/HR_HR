"""競合調査(/competitor) golden 用の合成 CSV を作る。実データではない。決定的(乱数なし)。

使い方: python scripts/make_competitor_fixtures.py
出力: tests/fixtures/competitor/*.csv
文字コード違い(UTF-8 / BOM / Shift-JIS / UTF-16LE)は同じ内容を符号化し直したもの。
"""
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "competitor"
WARDS = ["北区", "中央区", "西区", "天王寺区", "淀川区", "住之江区"]
TAGS = ["賞与あり", "昇給あり", "社会保険完備", "交通費支給", "未経験歓迎", "駅チカ"]
SP_HEADER = [
    "css-1hwmqh1", "css-bxyec3 href", "css-bxyec3", "css-14qk2ra",
    "css-18rxko3", "css-18rxko3 (2)", "jobsearch-JobCard-tag", "css-1vlebyu", "css-u74ql7",
]


def rows(n_monthly=44, n_hourly=16):
    out = []
    for i in range(n_monthly):
        lo = 20 + (i * 7) % 15          # 20..34 万円
        hi = lo + 3 + (i % 5)
        pop = "超人気" if i % 11 == 0 else ("人気" if i % 4 == 0 else "")
        out.append([
            "正社員", f"https://example.com/m/{i}", f"施設長 募集{i:03d}", f"合成法人{i % 30:02d}",
            f"大阪府大阪市{WARDS[i % 6]}", f"月給 {lo}万円 ~ {hi}万円",
            f"{TAGS[i % 6]}、{TAGS[(i + 2) % 6]}", f"年間休日{105 + i % 20}日", pop,
        ])
    for i in range(n_hourly):
        lo = 1150 + (i * 30) % 300
        hi = lo + 100 + (i % 3) * 50
        pop = "人気" if i % 5 == 0 else ""
        out.append([
            "パート・アルバイト", f"https://example.com/h/{i}", f"介護補助 募集{i:03d}", f"合成施設{i % 8:02d}",
            f"大阪府堺市{WARDS[i % 6]}", f"時給 {lo}円 ~ {hi}円",
            f"{TAGS[(i + 1) % 6]}", f"短時間OK", pop,
        ])
    # 重複排除の確認用: 先頭行をそのまま 2 回繰り返す
    out.append(list(out[0]))
    out.append(list(out[1]))
    return out


def to_csv(header, data):
    def q(v):
        return '"' + v.replace('"', '""') + '"' if ("," in v or '"' in v) else v
    return "\n".join(",".join(q(c) for c in r) for r in [header] + data) + "\n"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    text = to_csv(SP_HEADER, rows())
    (OUT / "sp_utf8.csv").write_bytes(text.encode("utf-8"))
    (OUT / "sp_utf8_bom.csv").write_bytes(b"\xef\xbb\xbf" + text.encode("utf-8"))
    (OUT / "sp_sjis.csv").write_bytes(text.encode("cp932"))
    (OUT / "sp_utf16le.csv").write_bytes(b"\xff\xfe" + text.encode("utf-16-le"))
    # 列欠落: 給与列(css-18rxko3 (2))を丸ごと落とす
    keep = [i for i, h in enumerate(SP_HEADER) if h != "css-18rxko3 (2)"]
    (OUT / "sp_missing_salary_column.csv").write_bytes(
        to_csv([SP_HEADER[i] for i in keep], [[r[i] for i in keep] for r in rows()]).encode("utf-8")
    )
    # SP 固有の人気列・年間休日列が無い通常形式(SP データなし)
    plain_header = ["タイトル", "会社名", "勤務地", "給与", "雇用形態"]
    plain = [[r[2], r[3], r[4], r[5], r[0]] for r in rows(12, 4)]
    (OUT / "plain_no_sp.csv").write_bytes(to_csv(plain_header, plain).encode("utf-8"))


if __name__ == "__main__":
    main()
