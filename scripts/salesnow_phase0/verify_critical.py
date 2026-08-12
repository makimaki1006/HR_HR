# -*- coding: utf-8 -*-
"""adversarial2 の「致命的1・2」を自分で再現検証する。

論点: 極近傍 (delta ≈ -100) の企業が、表示中セルの増減率を支配していないか。
私の §11.1 は「復元過去人数が最大 7,857 人で破綻しない」と書いたが、
それは「桁が壊れるか」の話で、「表示値を支配するか」には答えていない。
"""
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MIN_COMPANIES, MAX_TOP1 = 30, 50.0


def hr(t):
    print("\n" + "=" * 78 + f"\n## {t}\n" + "=" * 78)


df = pd.read_csv(CSV, usecols=["sn_company_name", "prefecture", "sn_industry",
                               "employee_count", "employee_delta_1y"], low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
ok = (ec.notna() & (ec > 0) & d.notna() & (d > -100)
      & df["sn_industry"].notna() & (df["sn_industry"].astype(str) != ""))
w = df[ok].copy()
w["ec"] = ec[ok]
w["d"] = d[ok]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))
w["past"] = w["ec"] - w["chg"]


def cell_stats(sub):
    """1 セルの集計とゲート判定 (Rust 実装と同じ規則)"""
    n = len(sub)
    past = sub["past"].sum()
    net = sub["chg"].sum()
    tot_abs = sub["chg"].abs().sum()
    top1 = sub["chg"].abs().max() if n else 0
    share = (top1 / tot_abs * 100) if tot_abs > 0 else None
    if n == 0 or past <= 0:
        gate = "NoData"
    elif n < MIN_COMPANIES:
        gate = "TooFew"
    elif share is not None and share >= MAX_TOP1:
        gate = "Concentrated"
    else:
        gate = "Show"
    rate = (net / past * 100) if past > 0 else None
    return dict(n=n, past=past, net=net, tot_abs=tot_abs, top1=top1,
                share=share, gate=gate, rate=rate)


hr("(1) 極近傍の企業は実データにどれだけあるか")
for lo in [-100, -99, -98, -95, -90]:
    hi = -99 if lo == -100 else (lo + 1 if lo < -90 else -85)
    m = (w["d"] > lo) & (w["d"] <= (lo + 1 if lo != -100 else -99))
    print(f"  {lo} < d <= {lo+1 if lo!=-100 else -99}: {m.sum():>4} 社")
for th in [-99, -95, -90]:
    m = w["d"] <= th
    print(f"  d <= {th}: {m.sum():>4} 社")
lev = w["chg"].abs() > w["ec"] * 10
print(f"\n  |増減人数| > 従業員数 × 10 の企業: {lev.sum()} 社")
print(f"  |増減人数| > 従業員数 × 100 の企業: {(w['chg'].abs() > w['ec']*100).sum()} 社")

hr("(2) 表示中セルのうち、極近傍 1 社を除くと結論が変わるものはいくつか")
g = w.groupby(["prefecture", "sn_industry"])
flip_sign, flip_gate, contains = [], [], 0
rows = []
for key, sub in g:
    base = cell_stats(sub)
    if base["gate"] != "Show":
        continue
    extreme = sub[sub["d"] <= -95]
    if len(extreme) == 0:
        continue
    contains += 1
    kept = sub[sub["d"] > -95]
    alt = cell_stats(kept)
    rows.append((key, base, alt, extreme))
    if base["rate"] is not None and alt["rate"] is not None:
        if np.sign(base["rate"]) != np.sign(alt["rate"]) and abs(base["rate"]) > 0.01:
            flip_sign.append((key, base, alt, extreme))
    if alt["gate"] != "Show":
        flip_gate.append((key, base, alt, extreme))

shown = sum(1 for _, sub in g if cell_stats(sub)["gate"] == "Show")
print(f"表示されるセル                      : {shown}")
print(f"  うち d <= -95 の企業を含むセル    : {contains}")
print(f"  うち極近傍を除くと符号が反転      : {len(flip_sign)}")
print(f"  うち極近傍を除くとゲートで抑制    : {len(flip_gate)}")

print("\n--- 符号が反転するセル (現在表示中) ---")
print(f"  {'都道府県':<8} {'業種':<16} {'社数':>5} {'現在':>9} {'除外後':>9}  元凶企業")
for key, base, alt, ext in sorted(flip_sign, key=lambda x: -x[1]['n'])[:12]:
    e = ext.reindex(ext["chg"].abs().sort_values(ascending=False).index).iloc[0]
    print(f"  {key[0]:<8} {str(key[1])[:14]:<16} {base['n']:>5} "
          f"{base['rate']:>8.2f}% {alt['rate']:>8.2f}%  "
          f"{str(e['sn_company_name'])[:22]} ec={e['ec']:.0f} d={e['d']:.2f} chg={e['chg']:+.0f}")

print("\n--- 極近傍を除くとゲートで抑制されるセル ---")
for key, base, alt, ext in flip_gate:
    e = ext.reindex(ext["chg"].abs().sort_values(ascending=False).index).iloc[0]
    print(f"  {key[0]} × {key[1]} ({base['n']} 社)")
    print(f"    現在  : rate={base['rate']:+.2f}%  top1={base['share']:.2f}%  → 表示")
    print(f"    除外後: rate={alt['rate']:+.2f}%  top1={alt['share']:.2f}%  → {alt['gate']}")
    print(f"    元凶  : {e['sn_company_name']} ec={e['ec']:.0f} d={e['d']:.2f} → {e['chg']:+.0f} 人")

hr("(3) 致命的 2: 復元値の丸め由来の不確かさ")
print("delta は小数第 2 位まで。真値は d ± 0.005。")
print("感度 d(chg)/d(delta) = ec * 100 / (100+d)^2  → 不確かさ ≈ 感度 * 0.005")
sens = w["ec"] * 100.0 / (100.0 + w["d"]) ** 2
unc = sens * 0.005
print(f"\n  不確かさ > ±1 人 の企業  : {(unc > 1).sum():>5} 社")
print(f"  不確かさ > ±10 人 の企業 : {(unc > 10).sum():>5} 社")
print(f"  不確かさ > ±100 人 の企業: {(unc > 100).sum():>5} 社")
print(f"  全国純増減 {w['chg'].sum():+,.0f} 人 に対する不確かさの総和: ±{unc.sum():,.0f} 人")
print("\n  不確かさが大きい企業 上位5:")
for i in unc.sort_values(ascending=False).head(5).index:
    print(f"    {str(w.loc[i,'sn_company_name'])[:26]:<28} ec={w.loc[i,'ec']:>6.0f} "
          f"d={w.loc[i,'d']:>8.2f} chg={w.loc[i,'chg']:>+8.0f} 人  ±{unc[i]:,.0f} 人")

print("\n  『|x - round(x)| < 0.5 は恒真か』の確認:")
frac = np.abs(w["ec"] * w["d"] / (100.0 + w["d"]) - w["chg"])
print(f"    実測の最大: {frac.max():.4f}  (0.5 未満なのは数学的に当然)")
rnd = np.random.default_rng(0).uniform(-1000, 1000, 200_000)
print(f"    乱数 20 万件でも: {np.abs(rnd - np.round(rnd)).max():.4f}  ← 同じ")
print("    → この指標は復元の正しさを何も証明しない。§1.3 の根拠としては無効")
