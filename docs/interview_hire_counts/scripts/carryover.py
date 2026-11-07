"""継続の取引に前の契約の数を引き継いで入れていたかを調べる。
- 9/1 以降に値を減らした書き換え（decrease_classified.csv）を、契約開始より前から数が入っていた取引か・満了後の書き換えかで分ける。
- 2026 年に始まった継続の取引で、契約開始日より前に 0 より大きい数が入っていたものを数え、最初に減らされた時期と人を出す。
出力: carryover.json"""
import json, csv
from collections import Counter
raw = json.load(open('raw.json', encoding='utf-8')); users = json.load(open('users.json', encoding='utf-8'))
num = lambda v: float(v) if v not in (None, '') else None
P = {'面接数': 'mensetu', '採用数': 'syoudaku'}
name = lambda x: users.get(str(x.get('updatedByUserId')), {}).get('name', '不明')

# 1. 9/1 以降の減らした書き換えの分類
kinds = Counter(); by_person = {}
for r in csv.DictReader(open('decrease_classified.csv', encoding='utf-8-sig')):
    d = raw[r['取引ID']]; p = d['properties']
    h = sorted(d['propertiesWithHistory'][P[r['項目']]], key=lambda x: x['timestamp'])
    t = r['日時(UTC)'].replace(' ', 'T')
    nz = [x for x in h if num(x['value']) and x['timestamp'][:16] < t]
    if t[:10] <= (p.get('contract_expiration_date') or '9999') and nz and nz[0]['timestamp'][:10] < (p.get('contract_start_date') or ''):
        k = '契約開始より前から数が入っていた取引で減らした'
    elif t[:10] > (p.get('contract_expiration_date') or '9999'):
        k = '満了した契約の数を後から減らした'
    else:
        k = 'それ以外'
    kinds[k] += 1; by_person.setdefault(r['書き換えた人'], Counter())[k] += 1

# 2. 2026 年開始の継続の取引で、開始前に数が入っていたもの
pre = Counter(); first_drop = Counter(); who = Counter()
for i, d in raw.items():
    p = d['properties']; s = p.get('contract_start_date') or ''
    if p.get('pipeline') != '21596025' or s < '2026-01-01': continue
    cont = '継続' in (p.get('dealname') or '') or '再契約' in (p.get('dealname') or '')
    for prop in P.values():
        h = sorted(d['propertiesWithHistory'][prop], key=lambda x: x['timestamp'])
        f = [x for x in h if num(x['value'])]
        if not f: continue
        before = f[0]['timestamp'][:10] < s
        pre[('継続' if cont else '新規', before)] += 1
        if not (cont and before): continue
        drops = [h[k] for k in range(1, len(h)) if num(h[k - 1]['value']) and (num(h[k]['value']) or 0) < num(h[k - 1]['value'])]
        if drops:
            ts = drops[0]['timestamp'][:10]
            first_drop['2026-08-12' if ts == '2026-08-12' else ts[:7]] += 1; who[name(drops[0])] += 1
        else:
            first_drop['減らしていない（満了済み）' if (p.get('contract_expiration_date') or '9') < '2026-11-06' else '減らしていない（契約中）'] += 1

out = {'kinds': dict(kinds), 'by_person': {k: dict(v) for k, v in by_person.items()},
       'pre': {f'{a}|{"開始前" if b else "開始後"}': n for (a, b), n in pre.items()},
       'first_drop': dict(sorted(first_drop.items())), 'who': who.most_common()}
json.dump(out, open('carryover.json', 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
print(json.dumps(out, ensure_ascii=False, indent=1))
