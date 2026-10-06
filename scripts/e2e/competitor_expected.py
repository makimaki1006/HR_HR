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
  時給モードの階級は時給の行 (16 件) の円/時を 50 円刻み。最小〜最大の間の空の階級も 0 件の階級として残す。
  階級が 81 個を超えるときだけ、幅を基準幅の整数倍に広げる。
- キーワードは「、」区切りのタグ (カテゴリ列) + 人気/超人気。1 求人に同じ語が複数あっても 1 回と数える。
  件数降順、同数は文字コード順。上位 N 件は重複排除後の先頭 N 行 (N = TOP_N)。
- 先頭 N 件と全体の比較: 先頭率 = 先頭件数 / N、全体率 = 全体件数 / 全体の求人数。差 = 先頭率 − 全体率 (pt)。
- 採用のヒント: 先頭率が全体率より低い語を差の小さい順 (同じなら語の文字コード順) に最大 3 つ。
"""
import collections, csv, json, math, re, statistics
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

SRC = Path(__file__).resolve().parents[2] / "tests/fixtures/competitor/sp_utf8.csv"
HOURS = 167
TOP_N = 10
CHART_WORDS = 20


def q(x, places):
    return str(Decimal(str(x)).quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_UP))


def js_fixed1(x):
    """JS の Number.prototype.toFixed(1)。2 進の正確な値を四捨五入 (同点は大きい方へ)。"""
    return str(Decimal(x).quantize(Decimal("0.1"), rounding=ROUND_HALF_UP))


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


def hist(vals, base, label_div):
    """空の階級も残した [ラベル, 件数] と、階級の幅 (表示単位の文字列)。"""
    lo, hi = min(vals), max(vals)
    span = (hi - lo) // base + 1
    step = base * max(-(-span // 80), 1)
    start, end = lo // step, hi // step
    c = collections.Counter(v // step for v in vals)
    bins = [[q(Decimal((k) * step) / label_div, 0), c.get(k, 0)] for k in range(start, end + 1)]
    return bins, f"{step / label_div:.0f}"


def signed(text):
    return text if text.startswith("-") else "+" + text


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

    def row_tags(r):
        tags = {t for t in r[6].split("、") if t}
        if r[-1] in ("人気", "超人気"):
            tags.add(r[-1])
        return tags

    def counts(rs):
        c = collections.Counter()
        for r in rs:
            for t in row_tags(r):
                c[t] += 1
        return c

    def ordered(c):
        return sorted(c.items(), key=lambda kv: (-kv[1], kv[0]))

    all_n = len(uniq)
    head = uniq[:TOP_N]
    head_n = len(head)
    call = counts(uniq)
    chead = counts(head)
    all_order = ordered(call)
    head_order = ordered(chead)

    out = {"total": all_n, "head_n": head_n}
    # 全体の表 (上位 10 語): [語, 件数, 求人数, 占有率 %(小数 0 桁)]
    out["keywords_all"] = [[w, n, all_n, q(Decimal(n) / all_n * 100, 0)] for w, n in all_order[:10]]
    # 全体のグラフ (上位 20 語): 棒の title は「語: N件」
    out["keywords_all_chart"] = [[w, n] for w, n in all_order[:CHART_WORDS]]
    # 先頭 N 件と全体の比較。比較表 (上位 10 語) とグラフ (上位 20 語) は同じ行から作る
    comparison = []
    for w, n in head_order[:CHART_WORDS]:
        head_share = n / head_n * 100
        all_share = call[w] / all_n * 100 if call[w] else None
        comparison.append({
            "word": w,
            "head": n,
            "head_share": f"{head_share:.1f}",
            "all_share": None if all_share is None else f"{all_share:.1f}",
            "delta": None if all_share is None else ("0.0" if abs(head_share - all_share) < 0.05 else f"{head_share - all_share:+.1f}"),
            "head_js": js_fixed1(head_share),
            "all_js": None if all_share is None else js_fixed1(all_share),
        })
    out["comparison"] = comparison

    for mode, scale, places in (("monthly", 10000, 2), ("hourly", 1, 0)):
        sel = [p for p in parsed if p[1] == mode]
        sel_pop = [p for p in sel if popular(p[0])]
        lo, hi = [p[2] for p in sel], [p[3] for p in sel]
        plo, phi = [p[2] for p in sel_pop], [p[3] for p in sel_pop]
        a_lo, a_hi, p_lo, p_hi = raw_stats(lo), raw_stats(hi), raw_stats(plo), raw_stats(phi)
        out[mode] = {
            "counts": [len(lo), len(hi), len(plo), len(phi)],
            "all_lower": stats(lo, scale, places), "all_upper": stats(hi, scale, places),
            "pop_lower": stats(plo, scale, places), "pop_upper": stats(phi, scale, places),
            # 差 (総合 - 人気求人): 整数円のまま引いてから表示単位に直す。行は 平均値 / 中央値 / 最頻値
            "diff_lower": [q(Decimal(a - b) / scale, places) for a, b in zip(a_lo, p_lo)],
            "diff_upper": [q(Decimal(a - b) / scale, places) for a, b in zip(a_hi, p_hi)],
        }

    # ボリュームゾーン
    mon_all = [(p[2] if p[1] == "monthly" else p[2] * HOURS, p[3] if p[1] == "monthly" else p[3] * HOURS) for p in parsed]
    out["monthly"]["hist_upper"], out["monthly"]["hist_step"] = hist([b for _, b in mon_all], 10000, 10000)
    out["monthly"]["hist_lower"], _ = hist([a for a, _ in mon_all], 10000, 10000)
    out["monthly"]["hist_n"] = len(mon_all)
    hp = [p for p in parsed if p[1] == "hourly"]
    out["hourly"]["hist_upper"], out["hourly"]["hist_step"] = hist([p[3] for p in hp], 50, 1)
    out["hourly"]["hist_lower"], _ = hist([p[2] for p in hp], 50, 1)
    out["hourly"]["hist_n"] = len(hp)

    # 採用のヒント: 訴求の確認候補 (先頭率が全体率より低い語)
    gaps = []
    for w, c in call.items():
        h = chead.get(w, 0)
        if h * all_n < c * head_n:
            gaps.append((100 * (h / head_n - c / all_n), w, h, c))
    gaps.sort()
    out["gaps"] = [
        [w, f"{h}/{head_n} ({h / head_n * 100:.1f}%)", f"{c}/{all_n} ({c / all_n * 100:.1f}%)", f"{pts:+.1f}"]
        for pts, w, h, c in gaps[:3]
    ]
    for mode in ("monthly", "hourly"):
        m = out[mode]
        # 中央値の行: [ラベル, "総合 / 件数", "人気 / 件数", 総合−人気 (符号つき)]
        m["consultation"] = [
            ["下限", f"{m['all_lower'][1]} / {m['counts'][0]}件", f"{m['pop_lower'][1]} / {m['counts'][2]}件", signed(m["diff_lower"][1])],
            ["上限", f"{m['all_upper'][1]} / {m['counts'][1]}件", f"{m['pop_upper'][1]} / {m['counts'][3]}件", signed(m["diff_upper"][1])],
        ]
    print(json.dumps(out, ensure_ascii=False, indent=1))


main()
