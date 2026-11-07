"""各取引の月末の値の「前の月からの増え」を月ごとに足す。満了で外れる分は引かない。
満了月は、その月の最後の値を使う。満了日の値と月末の値の差も出す。"""
import json, csv, unicodedata, calendar
from datetime import datetime, timedelta, timezone
from collections import defaultdict
JST = timezone(timedelta(hours=9))
raw = json.load(open('raw.json', encoding='utf-8'))
st = json.load(open('stages.json', encoding='utf-8')); so, lab = st['stage_of'], st['labels']
optst = {k for k, v in lab.items() if 'オプション' in v}
PFX = ("求人追加", "AirWork広告運用", "一次対応", "エントリーフォーム")
FIRST = (2025, 7); NOW = datetime.now(JST)
months = []
y, m = FIRST
while (y, m) <= (NOW.year, NOW.month):
    months.append((y, m)); y, m = (y + 1, 1) if m == 12 else (y, m + 1)
def mend(y, m): return datetime(y, m, calendar.monthrange(y, m)[1], 23, 59, 59, 999999, tzinfo=JST)
def val_at(hist, t):
    best = None
    for h in hist:
        ts = datetime.fromisoformat(h['timestamp'].replace('Z', '+00:00'))
        if ts <= t and (best is None or ts > best[0]): best = (ts, h['value'])
    if best is None or best[1] in (None, ''): return None
    try: return float(best[1])
    except ValueError: return None
def d(s): return datetime.strptime(s[:10], '%Y-%m-%d').date() if s else None
HIST0 = datetime(2025, 7, 11, tzinfo=JST)
agg = defaultdict(lambda: defaultdict(float)); neg = []; diff = []; lines = []
for i, r in raw.items():
    p = r['properties']
    if p.get('pipeline') != '21596025': continue
    if so.get(i) in optst or unicodedata.normalize('NFKC', p.get('dealname') or '').strip().startswith(PFX): continue
    s, e = d(p.get('contract_start_date')), d(p.get('contract_expiration_date'))
    if not s: continue
    for prop, key in (('mensetu', '面接'), ('syoudaku', '採用')):
        hist = r['propertiesWithHistory'][prop]
        prev = None; prev_known = False
        for (y, m) in months:
            first_day = datetime(y, m, 1).date(); last_day = mend(y, m).date()
            if last_day < s: continue
            if e and first_day > e: break            # 満了の翌月からは数えない（落ちた分は引かない）
            v = val_at(hist, mend(y, m))            # 満了月もその月の最後の値
            if not prev_known:
                # 前の値: 契約が履歴より後に始まったなら 0、前から続いていれば前月末の値（履歴開始前なら不明）
                if s >= HIST0.date() or (y, m) != FIRST:
                    prev = 0.0 if s >= first_day.replace(day=1) else (val_at(hist, mend(*((y, m - 1) if m > 1 else (y - 1, 12)))) )
                    prev_known = True
                    if prev is None and s < HIST0.date(): prev_known = False
                if not prev_known:
                    if v is not None: prev, prev_known = v, True   # 履歴より前からの値は基準にするだけで増えには数えない
                    agg[(y, m)][key + '_基準のみ'] += 1; continue
            if v is None: continue                   # 空は無視。前の値はそのまま
            inc = v - (prev or 0)
            agg[(y, m)][key] += inc; agg[(y, m)][key + '_取引'] += 1
            if inc < 0: agg[(y, m)][key + '_減'] += inc; neg.append([f'{y}-{m:02d}', key, i, p.get('dealname'), prev, v])
            lines.append([f'{y}-{m:02d}', key, i, p.get('dealname'), str(s), str(e or ''), prev, v, inc])
            prev = v
        if e and e < mend(e.year, e.month).date() and (e.year, e.month) in months:
            at_end = val_at(hist, datetime(e.year, e.month, e.day, 23, 59, 59, 999999, tzinfo=JST))
            at_mend = val_at(hist, mend(e.year, e.month))
            if at_end != at_mend:
                diff.append([f'{e.year}-{e.month:02d}', key, i, p.get('dealname'), str(e), at_end, at_mend, (at_mend or 0) - (at_end or 0)])
W = lambda n, h, rows: csv.writer(open(n, 'w', encoding='utf-8-sig', newline='')).writerows([h] + rows)
out = []
for (y, m) in months:
    a = agg[(y, m)]
    out.append([f'{y}-{m:02d}', int(a['面接']), int(a['採用']), int(a['面接_取引']), int(a['採用_取引']), int(a['面接_減']), int(a['採用_減']), int(a['面接_基準のみ']), '途中' if (y, m) == (NOW.year, NOW.month) else ''])
W('monthly_increase.csv', ['月', '面接数の増え', '採用数の増え', '数えた取引(面接)', '数えた取引(採用)', 'うち減った分(面接)', 'うち減った分(採用)', '基準づくりだけの取引(面接)', '途中の月'], out)
W('increase_detail.csv', ['月', '項目', '取引ID', '取引名', '契約開始日', '契約満了日', '前の月末', 'この月末', '増え'], lines)
W('decreases.csv', ['月', '項目', '取引ID', '取引名', '前の月末', 'この月末'], neg)
W('expiry_vs_monthend.csv', ['満了の月', '項目', '取引ID', '取引名', '満了日', '満了日の値', '月末の値', '差(月末−満了日)'], diff)
for o in out: print(*o, sep=' | ')
print('減った件数', len(neg), '満了日と月末で違う', len(diff))
