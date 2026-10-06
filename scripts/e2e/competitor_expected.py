"""競合調査 E2E の既知値を、fixture CSV (tests/fixtures/competitor/sp_utf8.csv) から Rust と無関係に計算する。

Rust の出力をコピーして期待値にしないための独立計算。出力 (JSON) を
tests/e2e/pr/helpers/fixture_values.ts の COMPETITOR_FIXTURE に貼る。
  python scripts/e2e/competitor_expected.py

前提 (仕様の読み取り):
- 行の完全重複は 1 件に (62 行 -> 60 件)。
- 月給モードの給与表は「月給」表記の行だけ (44 件)。時給モードは「時給」表記の行だけ (16 件)。
- 人気求人 = タグ列が 人気 / 超人気 の行。
- 平均は四捨五入 (月給: 万円 小数 2 桁、時給: 円 整数)。中央値は偶数件で中間 2 値の平均。最頻値は同数なら低い額。
- ボリュームゾーンは全 60 件を月給換算 (時給 x 167 時間) した円を 1 万円 (時給モードは 50 円) で切り捨てた階級。
  時給モードの階級は時給の行 (16 件) の円/時を 50 円刻み。
- キーワードは「、」区切りのタグ (カテゴリ列) + 人気/超人気。件数降順、同数は文字コード順。上位 N 件は重複排除後の先頭 N 行。
"""
import collections, csv, json, re, statistics
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

SRC = Path(__file__).resolve().parents[2] / "tests/fixtures/competitor/sp_utf8.csv"
HOURS = 167
TOP_N = 10


def q(x, places):
    return str(Decimal(str(x)).quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_UP))


def raw_stats(vals):
    """(平均, 中央値, 最頻値) を整数円で返す。平均・中央値は四捨五入 (差は整数円のまま引くため)。"""
    mean = int((Decimal(sum(vals)) / len(vals)).quantize(Decimal(1), rounding=ROUND_HALF_UP))
    med = int((Decimal(str(statistics.median(vals)))).quantize(Decimal(1), rounding=ROUND_HALF_UP))
    cnt = collections.Counter(vals)
    top = max(cnt.values())
    return [mean, med, min(v for v, c in cnt.items() if c == top)]


def stats(vals, scale, places):
    if not vals:
        return None
    return [q(Decimal(x) / scale, places) for x in raw_stats(vals)]


def main():
    rows = list(csv.reader(open(SRC, encoding="utf8")))[1:]
    seen, uniq = set(), []
    for r in rows:
        if tuple(r) in seen:
            continue
        seen.add(tuple(r))
        uniq.append(r)

    def parse(r):
        a = int(re.search(r"(\d+)(?:万)?円 ~", r[5]).group(1))
        b = int(re.search(r"~ (\d+)", r[5]).group(1))
        if "月給" in r[5]:
            return "monthly", a * 10000, b * 10000
        return "hourly", a, b

    parsed = [(r, *parse(r)) for r in uniq]
    popular = lambda r: r[-1] in ("人気", "超人気")

    def keywords(rs):
        c = collections.Counter()
        for r in rs:
            for t in r[6].split("、"):
                if t:
                    c[t] += 1
            if r[-1] in ("人気", "超人気"):
                c[r[-1]] += 1
        order = sorted(c.items(), key=lambda kv: (-kv[1], kv[0]))
        return [[w, n, len(rs), q(Decimal(n) / len(rs) * 100, 0)] for w, n in order[:10]]

    out = {"total": len(uniq)}
    for mode, scale, places in (("monthly", 10000, 2), ("hourly", 1, 0)):
        sel = [p for p in parsed if p[1] == mode]
        sel_pop = [p for p in sel if popular(p[0])]
        lo, hi = [p[2] for p in sel], [p[3] for p in sel]
        plo, phi = [p[2] for p in sel_pop], [p[3] for p in sel_pop]
        out[mode] = {
            "counts": [len(lo), len(hi), len(plo), len(phi)],
            "all_lower": stats(lo, scale, places), "all_upper": stats(hi, scale, places),
            "pop_lower": stats(plo, scale, places), "pop_upper": stats(phi, scale, places),
            # 差 (総合 - 人気求人): 整数円のまま引いてから表示単位に直す。行は 平均値 / 中央値 / 最頻値
            "diff_lower": [q(Decimal(a - b) / scale, places) for a, b in zip(raw_stats(lo), raw_stats(plo))],
            "diff_upper": [q(Decimal(a - b) / scale, places) for a, b in zip(raw_stats(hi), raw_stats(phi))],
        }
    # ボリュームゾーン
    def hist(vals, step, label_div):
        c = collections.Counter(v // step * step for v in vals)
        return [[q(Decimal(k) / label_div, 0), n] for k, n in sorted(c.items())]

    mon_all = [(p[2] if p[1] == "monthly" else p[2] * HOURS, p[3] if p[1] == "monthly" else p[3] * HOURS) for p in parsed]
    out["monthly"]["hist_upper"] = hist([b for _, b in mon_all], 10000, 10000)
    out["monthly"]["hist_lower"] = hist([a for a, _ in mon_all], 10000, 10000)
    hp = [p for p in parsed if p[1] == "hourly"]
    out["hourly"]["hist_upper"] = hist([p[3] for p in hp], 50, 1)
    out["hourly"]["hist_lower"] = hist([p[2] for p in hp], 50, 1)
    out["keywords_all"] = keywords(uniq)
    out["keywords_head"] = keywords(uniq[:TOP_N])
    print(json.dumps(out, ensure_ascii=False, indent=1))


main()
