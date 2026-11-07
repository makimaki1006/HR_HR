"""2026-09-01 以降に面接数・採用数を減らした書き換えを、書いた人ごとに数える。
同じ人が 30 分以内に打ち間違いを直したもの（直前に増やした分を戻しただけ）は「打ち直し」として分ける。"""
import json, csv, unicodedata
from datetime import datetime
from collections import defaultdict, Counter
raw = json.load(open('raw.json', encoding='utf-8')); users = json.load(open('users.json', encoding='utf-8'))
st = json.load(open('stages.json', encoding='utf-8')); so, lab = st['stage_of'], st['labels']
optst = {k for k, v in lab.items() if 'オプション' in v}
PFX = ("求人追加", "AirWork広告運用", "一次対応", "エントリーフォーム")
num = lambda v: float(v) if v not in (None, '') else None
T = lambda s: datetime.fromisoformat(s.replace('Z', '+00:00'))
rows = []
for i, r in raw.items():
    p = r['properties']
    if p.get('pipeline') != '21596025': continue
    if so.get(i) in optst or unicodedata.normalize('NFKC', p.get('dealname') or '').strip().startswith(PFX): continue
    for prop, key in (('mensetu', '面接数'), ('syoudaku', '採用数')):
        h = sorted(r['propertiesWithHistory'][prop], key=lambda x: x['timestamp'])
        for k in range(1, len(h)):
            a, b = h[k - 1], h[k]
            if b['timestamp'] < '2026-09-01': continue
            va, vb = num(a['value']), num(b['value'])
            if va is None or (vb is not None and vb >= va): continue
            retype = (k >= 2 and b.get('updatedByUserId') == a.get('updatedByUserId')
                      and (T(b['timestamp']) - T(a['timestamp'])).total_seconds() <= 1800
                      and num(h[k - 2]['value']) is not None and vb is not None and vb >= num(h[k - 2]['value']))
            u = str(b.get('updatedByUserId') or b.get('sourceId') or '')
            nm = users.get(u, {}).get('name') or f'不明({b["sourceType"]} {u})'
            rows.append([b['timestamp'][:16].replace('T', ' ') + ' UTC', key, i, p.get('dealname'), p.get('contract_start_date'), p.get('contract_expiration_date'),
                         a['value'], b['value'], nm, b['sourceType'], '打ち直し' if retype else '減らした'])
csv.writer(open('decrease_events_since_sep.csv', 'w', encoding='utf-8-sig', newline='')).writerows(
    [['日時', '項目', '取引ID', '取引名', '契約開始日', '契約満了日', '前の値', '新しい値', '書き換えた人', '経路', '区分']] + rows)
real = [r for r in rows if r[-1] == '減らした']
print('9/1以降の減らす書き換え', len(rows), 'うち打ち直し', len(rows) - len(real))
by = defaultdict(lambda: [0, 0, set(), 0])
for r in real:
    x = by[r[8]]; x[0] += 1; x[2].add(r[2]); x[3] += float(r[6]) - float(r[7] or 0)
    if (r[7] or '0') in ('0', '0.0', ''): x[1] += 1
print('書き換えた人 | 回数 | 取引数 | うち 0 や空に | 減らした量の合計')
for n, x in sorted(by.items(), key=lambda kv: -kv[1][0]): print(n, '|', x[0], '|', len(x[2]), '|', x[1], '|', int(x[3]))
print('経路', Counter(r[9] for r in real))
