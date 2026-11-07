"""9/1 以降の「減らす書き換え」を、直前の値がどれだけの間そのままだったかで分ける。
- 訂正とみられる: 減らす前の値が入ってから 4 日以内で、減らした後の値が、その前の値（増やす前）以上。直前に増やしすぎた分を戻しただけ。
- 訂正では説明しにくい: 4 日を超えて残っていた値を減らした、または直前の増えより多く減らした。
  さらに 0・空にした／半分以上減らした／それ以外 で分ける。意図はデータからは分からない。"""
import json, csv, unicodedata
from datetime import datetime
from collections import defaultdict, Counter
raw = json.load(open('raw.json', encoding='utf-8')); users = json.load(open('users.json', encoding='utf-8'))
st = json.load(open('stages.json', encoding='utf-8')); so, lab = st['stage_of'], st['labels']
optst = {k for k, v in lab.items() if 'オプション' in v}
PFX = ("求人追加", "AirWork広告運用", "一次対応", "エントリーフォーム")
num = lambda v: float(v) if v not in (None, '') else None
T = lambda s: datetime.fromisoformat(s.replace('Z', '+00:00'))
DAYS = 4
out = []
for i, r in raw.items():
    p = r['properties']
    if p.get('pipeline') != '21596025': continue
    if so.get(i) in optst or unicodedata.normalize('NFKC', p.get('dealname') or '').strip().startswith(PFX): continue
    for prop, key in (('mensetu', '面接数'), ('syoudaku', '採用数')):
        h0 = sorted(r['propertiesWithHistory'][prop], key=lambda x: x['timestamp'])
        # 同じ人が 30 分以内に続けて書いたものは 1 回にまとめ、最後の値だけを見る（88 と打って 7 に直した、など）
        h = []
        for x in h0:
            if h and x.get('updatedByUserId') == h[-1].get('updatedByUserId') and (T(x['timestamp']) - T(h[-1]['last'])).total_seconds() <= 1800:
                h[-1].update(value=x['value'], last=x['timestamp'])
            else:
                h.append(dict(x, last=x['timestamp']))
        for k in range(1, len(h)):
            a, b = h[k - 1], h[k]
            if b['timestamp'] < '2026-09-01': continue
            va, vb = num(a['value']), num(b['value'])
            if va is None or (vb is not None and vb >= va): continue
            held = (T(b['timestamp']) - T(a['last'])).total_seconds() / 86400
            before = num(h[k - 2]['value']) if k >= 2 else None
            undo = held <= DAYS and k >= 2 and (vb or 0) >= (before or 0)
            if undo: kind = '訂正とみられる'
            else:
                if vb in (None, 0): kind = '訂正では説明しにくい：0・空に'
                elif vb <= va / 2: kind = '訂正では説明しにくい：半分以上減'
                else: kind = '訂正では説明しにくい：少し減'
            later = [num(x['value']) for x in h[k + 1:] if num(x['value']) is not None]
            back = '後で元の値以上に戻った' if later and max(later) >= va else ('後で増えた' if later and max(later) > (vb or 0) else '')
            u = str(b.get('updatedByUserId') or '')
            out.append([b['timestamp'][:16].replace('T', ' '), key, i, p.get('dealname'), (p.get('contract_start_date') or '')[:10], (p.get('contract_expiration_date') or '')[:10],
                        before if before is not None else '', a['value'], b['value'] or '', round(held, 1), users.get(u, {}).get('name') or f'不明({u})', kind, back])
csv.writer(open('decrease_classified.csv', 'w', encoding='utf-8-sig', newline='')).writerows(
    [['日時(UTC)', '項目', '取引ID', '取引名', '契約開始日', '契約満了日', 'その前の値', '減らす前の値', '新しい値', '減らす前の値が残っていた日数', '書き換えた人', '区分', 'その後']] + out)
print(Counter(r[11] for r in out))
by = defaultdict(Counter)
for r in out: by[r[10]][r[11]] += 1
print('人 | 訂正 | 0・空 | 半分以上 | 少し')
for n, c in sorted(by.items(), key=lambda kv: -sum(v for k, v in kv[1].items() if '説明しにくい' in k)):
    print(n, c['訂正とみられる'], c['訂正では説明しにくい：0・空に'], c['訂正では説明しにくい：半分以上減'], c['訂正では説明しにくい：少し減'], sep=' | ')
print('その後', Counter((r[11].split('：')[0], r[12]) for r in out))
