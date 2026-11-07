import os,requests,json,time
H={'Authorization':'Bearer '+os.environ['HUBSPOT_ACCESS_TOKEN'],'Content-Type':'application/json'}
B='https://api.hubapi.com'
def call(m,u,**k):
    while True:
        r=requests.request(m,B+u,headers=H,timeout=60,**k)
        if r.status_code==429:
            time.sleep(max(1,float(r.headers.get('Retry-After',1)))); continue
        if r.status_code>=500: time.sleep(2); continue
        r.raise_for_status(); time.sleep(0.25); return r.json()
ids=[];after=None
while True:
    body={'filterGroups':[{'filters':[{'propertyName':'pipeline','operator':'IN','values':['21596025','743961896']}]}],'properties':['dealname'],'limit':100}
    if after: body['after']=after
    r=call('POST','/crm/v3/objects/deals/search',json=body)
    ids+=[x['id'] for x in r['results']]
    after=r.get('paging',{}).get('next',{}).get('after')
    if not after: break
print(len(ids))
out={}
P=['dealname','pipeline','mensetu','syoudaku','contract_start_date','contract_expiration_date']
for i in range(0,len(ids),50):
    r=call('POST','/crm/v3/objects/deals/batch/read',json={'inputs':[{'id':x} for x in ids[i:i+50]],'properties':P,'propertiesWithHistory':['mensetu','syoudaku']})
    for x in r['results']: out[x['id']]=x
json.dump(out,open('raw.json','w'),ensure_ascii=False)
print(len(out))
