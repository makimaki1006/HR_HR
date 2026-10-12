import { apiGet } from '../../api/client';
export interface SavedView { id:string;name:string;scope:'private'|'shared';query:string;position:number;created_at:string;updated_at:string;can_edit:boolean }
export interface ViewInput { name:string;scope:'private'|'shared';query:string;position?:number }
export interface ViewsClient {
  list: (signal:AbortSignal)=>Promise<SavedView[]>;
  create: (id:string,input:ViewInput)=>Promise<SavedView>;
  patch: (id:string,input:Partial<ViewInput>)=>Promise<SavedView>;
  remove: (id:string)=>Promise<void>;
}
/** Only screen metadata is removed. Filter bytes, order and repeated keys are preserved. */
export function queueQuery(search:string):string {
  return search.replace(/^\?/u,'').split('&').filter(part=>{
    let key=part.split('=')[0] ?? '';try{key=decodeURIComponent(key);}catch{/* preserve invalid input for server validation */}
    return key!=='view' && key!=='mode';
  }).join('&');
}
export function selectedViewId(search:string):string|null {
  const id=new URLSearchParams(search).get('view');
  return !id || ['queue','moc','calling','reference','single'].includes(id)?null:id;
}
export function viewSearch(query:string,id:string|null,fixture:boolean):string {
  const parts=[fixture?'mode=fixture':'',query,id?`view=${encodeURIComponent(id)}`:''].filter(Boolean);
  return parts.length?`?${parts.join('&')}`:'';
}
export function changedView(view:SavedView|null,query:string):boolean {return view!==null && view.query!==query;}
async function write<T>(method:string,path:string,body?:unknown):Promise<T> {
  const response=await fetch(path,{method,credentials:'same-origin',headers:{'Content-Type':'application/json','X-Requested-With':'fetch'},...(body===undefined?{}:{body:JSON.stringify(body)}),signal:AbortSignal.timeout(15000)});
  if(!response.ok) {let kind='';try{kind=(await response.json() as {error_kind?:string}).error_kind ?? '';}catch{/* fixed message */}
    throw new Error(kind==='views_limit'?'自分用は50件、共有は100件まで保存できます。':'ビューを保存できませんでした。もう一度お試しください。');}
  return response.json() as Promise<T>;
}
export const liveViews:ViewsClient={
  async list(signal){const result=await apiGet<{views:SavedView[]}>('/api/crm/views',{signal});if(!result.ok)throw new Error('ビューを読み込めませんでした');return result.data.views;},
  create:(id,input)=>write('POST','/api/crm/views',{id,...input}),
  patch:(id,input)=>write('PATCH',`/api/crm/views/${encodeURIComponent(id)}`,input),
  async remove(id){await write('DELETE',`/api/crm/views/${encodeURIComponent(id)}`);},
};
/** Fictional mode only: no API or audit writes, separate from every real user's views. */
export function fixtureViews(storage:Storage):ViewsClient {
  const key='hrhr.crm.fixtureSavedViews.v1';
  const read=():SavedView[]=>{try{return JSON.parse(storage.getItem(key) ?? '[]') as SavedView[];}catch{return [];}};
  const save=(rows:SavedView[])=>{storage.setItem(key,JSON.stringify(rows));};
  return {
    list:()=>Promise.resolve(read().sort((a,b)=>a.position-b.position)),
    create:(id,input)=>{const rows=read();const old=rows.find(v=>v.id===id);if(old)return Promise.resolve(old);
      if(rows.filter(v=>v.scope===input.scope).length>=(input.scope==='private'?50:100))return Promise.reject(new Error('自分用は50件、共有は100件まで保存できます。'));
      const now=new Date().toISOString();const view:SavedView={id,...input,position:input.position ?? Math.max(0,...rows.map(v=>v.position))+1,created_at:now,updated_at:now,can_edit:true};save([...rows,view]);return Promise.resolve(view);},
    patch:(id,input)=>{const rows=read();const old=rows.find(v=>v.id===id);if(!old)return Promise.reject(new Error('ビューが見つかりませんでした。'));const view={...old,...input,updated_at:new Date().toISOString()};save(rows.map(v=>v.id===id?view:v));return Promise.resolve(view);},
    remove:(id)=>{save(read().filter(v=>v.id!==id));return Promise.resolve();},
  };
}
