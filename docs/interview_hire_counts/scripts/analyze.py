import json,csv,collections
from datetime import datetime,timezone,timedelta,date
JST=timezone(timedelta(hours=9))
raw=json.load(open('raw.json'))
deals={k:v for k,v in raw.items() if v['properties'].get('pipeline')=='21596025'}
print('deals',len(deals),'dropped',len(raw)-len(deals))
def ts(s): return datetime.fromisoformat(s.replace('Z','+00:00'))
def hist(d,p):
    h=d.get('propertiesWithHistory',{}).get(p,[])
    return sorted([(ts(x['timestamp']),x['value']) for x in h])
def pd(s):
    return date.fromisoformat(s[:10]) if s else None
# history length stats
mx=collections.Counter()
trunc={}
for k,d in deals.items():
    t=False
    for p in('mensetu','syoudaku'):
        n=len(d.get('propertiesWithHistory',{}).get(p,[])); mx[n]+=1
        if n>=20: t=True
    trunc[k]=t
print('hist len dist top',sorted(mx.items())[-6:], 'trunc deals',sum(trunc.values()))
H={k:{p:hist(d,p) for p in('mensetu','syoudaku')} for k,d in deals.items()}
allts=[t for k in H for p in H[k] for t,_ in H[k][p]]
first=min(allts).astimezone(JST); today=datetime.now(JST)
months=[];y,m=first.year,first.month
while (y,m)<=(today.year,today.month):
    months.append((y,m)); m+=1
    if m>12:y+=1;m=1
nostart=[k for k,d in deals.items() if not pd(d['properties'].get('contract_start_date'))]
noend=[k for k,d in deals.items() if pd(d['properties'].get('contract_start_date')) and not pd(d['properties'].get('contract_expiration_date'))]
print('first',first,'nostart',len(nostart),'noend(active-eligible)',len(noend))
def val_at(h,end):
    v=None;seen=False
    for t,x in h:
        if t<=end: v=x;seen=True
    if not seen or v in (None,''): return None
    return float(v)
rows=[];det=[]
for (y,m) in months:
    ms=date(y,m,1); me=(date(y+1,1,1) if m==12 else date(y,m+1,1))-timedelta(days=1)
    end=datetime(me.year,me.month,me.day,23,59,59,tzinfo=JST)
    si=sh=ni=nh=empty=tr=act=0
    for k,d in deals.items():
        pr=d['properties'];s=pd(pr.get('contract_start_date'));e=pd(pr.get('contract_expiration_date'))
        if not s or s>me or (e and e<ms): continue
        act+=1
        a=val_at(H[k]['mensetu'],end);b=val_at(H[k]['syoudaku'],end)
        if trunc[k]: tr+=1
        if a is None and b is None: empty+=1
        if a is not None: si+=a;ni+=1
        if b is not None: sh+=b;nh+=1
        if a is not None or b is not None:
            det.append([f'{y}-{m:02d}',k,pr.get('dealname'),pr.get('contract_start_date'),pr.get('contract_expiration_date') or '',a if a is not None else '',b if b is not None else ''])
    rows.append([f'{y}-{m:02d}',si,sh,ni,nh,act,empty,tr,'途中' if (y,m)==(today.year,today.month) else ''])
w=csv.writer(open('monthly.csv','w',newline='',encoding='utf-8-sig'))
w.writerow(['月','面接数の合計','採用数の合計','数えた取引数(面接数)','数えた取引数(採用数)','アクティブ取引数','アクティブだが両方空で無視','履歴が切れている可能性のある取引数','途中の月か']);w.writerows(rows)
w=csv.writer(open('detail.csv','w',newline='',encoding='utf-8-sig'))
w.writerow(['月','取引ID','取引名','契約開始日','契約終了日','月末の面接数','月末の採用数']);w.writerows(det)
for r in rows[-14:]:print(r)
print(len(rows),rows[0])
