import json,os,time,requests
H={"Authorization":f"Bearer {os.environ['HUBSPOT_ACCESS_TOKEN']}"}
raw=json.load(open('raw.json',encoding='utf-8'))
ids=[k for k,v in raw.items() if v['properties'].get('pipeline')=='21596025']
out={}
for i in range(0,len(ids),100):
    r=requests.post("https://api.hubapi.com/crm/v3/objects/deals/batch/read",headers=H,json={"inputs":[{"id":x} for x in ids[i:i+100]],"properties":["dealstage"]})
    if r.status_code==429: time.sleep(int(r.headers.get('Retry-After',2))); r=requests.post("https://api.hubapi.com/crm/v3/objects/deals/batch/read",headers=H,json={"inputs":[{"id":x} for x in ids[i:i+100]],"properties":["dealstage"]})
    r.raise_for_status()
    for x in r.json()['results']: out[x['id']]=x['properties'].get('dealstage')
    time.sleep(0.3)
st=requests.get("https://api.hubapi.com/crm/v3/pipelines/deals/21596025",headers=H).json()
json.dump({"stage_of":out,"labels":{s['id']:s['label'] for s in st['stages']}},open('stages.json','w',encoding='utf-8'),ensure_ascii=False)
print(len(out))
