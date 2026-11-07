import json,os,time,requests
H={"Authorization":f"Bearer {os.environ['HUBSPOT_ACCESS_TOKEN']}"}
raw=json.load(open('raw.json',encoding='utf-8'))
ids=[k for k,v in raw.items() if v['properties'].get('pipeline')=='21596025']
out={}
for i in range(0,len(ids),100):
    for _ in range(3):
        r=requests.post("https://api.hubapi.com/crm/v4/associations/deals/companies/batch/read",headers=H,json={"inputs":[{"id":x} for x in ids[i:i+100]]})
        if r.status_code==429: time.sleep(int(r.headers.get('Retry-After',2))); continue
        break
    if r.status_code not in (200,207): r.raise_for_status()
    for x in r.json().get('results',[]): out[str(x['from']['id'])]=[str(t['toObjectId']) for t in x.get('to',[])]
    time.sleep(0.3)
json.dump(out,open('deal_companies.json','w'),ensure_ascii=False); print(len(out))
