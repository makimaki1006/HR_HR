import json,os,requests
H={"Authorization":f"Bearer {os.environ['HUBSPOT_ACCESS_TOKEN']}"}
out={}; after=None
for archived in ('false','true'):
    after=None
    while True:
        p={"limit":100,"archived":archived}; 
        if after: p["after"]=after
        r=requests.get("https://api.hubapi.com/crm/v3/owners",headers=H,params=p).json()
        for o in r.get('results',[]):
            if o.get('userId'): out[str(o['userId'])]={"name":f"{o.get('lastName','')}{o.get('firstName','')}","email":o.get('email'),"teams":[t['name'] for t in o.get('teams',[])],"archived":archived=='true'}
        after=(r.get('paging') or {}).get('next',{}).get('after')
        if not after: break
json.dump(out,open('users.json','w',encoding='utf-8'),ensure_ascii=False); print(len(out))
