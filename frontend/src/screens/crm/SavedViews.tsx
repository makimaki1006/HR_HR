import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { changedView, fixtureViews, liveViews, queueQuery, selectedViewId, viewSearch } from './savedViewsModel';
import type { SavedView, ViewsClient, ViewInput } from './savedViewsModel';
import './saved-views.css';
interface Props { query:string;search:string;fixture:boolean;client?:ViewsClient;onNavigate:(search:string)=>void }
function SaveDialog({view,query,onSave,onClose}:{view:SavedView|null;query:string;onSave:(id:string,input:ViewInput)=>Promise<void>;onClose:()=>void}) {
  const [id]=useState(()=>view?.id ?? crypto.randomUUID());
  const [name,setName]=useState(view?.name ?? '');const [scope,setScope]=useState<'private'|'shared'>(view?.scope ?? 'private');
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');const title=useId();const input=useRef<HTMLInputElement>(null);
  useEffect(()=>{input.current?.focus();},[]);
  return <div className="sv-backdrop"><section className="sv-dialog" role="dialog" aria-modal="true" aria-labelledby={title} onKeyDown={e=>{if(e.key==='Escape'&&!busy){e.preventDefault();onClose();}}}>
    <h2 id={title}>{view?'ビューを変更':'新しいビューを保存'}</h2>
    <form onSubmit={e=>{e.preventDefault();setBusy(true);setError('');void onSave(id,{name:name.trim(),scope,query}).catch((e:unknown)=>{setError(e instanceof Error?e.message:'保存できませんでした。');}).finally(()=>{setBusy(false);});}}>
      <label>ビューの名前<input ref={input} value={name} maxLength={80} required onChange={e=>{setName(e.target.value);}} /></label>
      <label>共有範囲<select value={scope} onChange={e=>{setScope(e.target.value as 'private'|'shared');}}><option value="private">自分だけ</option><option value="shared">全員に共有</option></select></label>
      {query.length>2000&&<p role="alert">条件が長すぎるため保存できません。条件を減らしてください。</p>}
      {error&&<p role="alert">{error}</p>}
      <div className="sv-actions"><button className="sv-primary" type="submit" disabled={busy||!name.trim()||query.length>2000}>{busy?'保存中…':'保存'}</button><button type="button" disabled={busy} onClick={onClose}>やめる</button></div>
    </form>
  </section></div>;
}
export function SavedViews({query,search,fixture,client:provided,onNavigate}:Props) {
  const client=useMemo(()=>provided ?? (fixture?fixtureViews(localStorage):liveViews),[provided,fixture]);
  const [loaded,setLoaded]=useState<{client:ViewsClient;views:SavedView[]}|null>(null);const [failed,setFailed]=useState<ViewsClient|null>(null);
  const rows=loaded?.client===client?loaded.views:null;const error=failed===client;const [note,setNote]=useState(() => { try { return sessionStorage.getItem('hrhr.crm.viewNotice') ?? ''; } catch { return ''; } });
  useEffect(() => { try { sessionStorage.removeItem('hrhr.crm.viewNotice'); } catch { /* optional notice */ } }, []);
  const [menu,setMenu]=useState<string|null>(null);const [dialog,setDialog]=useState<{view:SavedView|null;query:string}|null>(null);
  const [deleting,setDeleting]=useState<SavedView|null>(null);const [busy,setBusy]=useState(false);
  const [overflow,setOverflow]=useState(false);
  const [visibleIds,setVisibleIds]=useState<string[]|null>(null);
  const strip=useRef<HTMLDivElement>(null);const actions=useRef<HTMLDivElement>(null);const measurements=useRef<HTMLDivElement>(null);
  const id=selectedViewId(search);const active=rows?.find(v=>v.id===id) ?? null;
  const initial=useRef({id,query:queueQuery(search)});const restored=useRef(false);
  useEffect(()=>{
    const controller=new AbortController();restored.current=false;
    void client.list(controller.signal).then(list=>{if(!controller.signal.aborted){setLoaded({client,views:list});setFailed(null);}}).catch(()=>{if(!controller.signal.aborted)setFailed(client);});
    return ()=>{controller.abort();};
  },[client]);
  useEffect(()=>{
    if(rows===null||restored.current)return;restored.current=true;
    if(initial.current.id){const view=rows.find(v=>v.id===initial.current.id);
      if(!view){const message='このビューは見られないため、「すべて」に戻しました。';setNote(message);try{sessionStorage.setItem('hrhr.crm.viewNotice',message);}catch{/* optional notice */}onNavigate(viewSearch('',null,fixture));}
      else if(initial.current.query===''&&view.query!=='')onNavigate(viewSearch(view.query,view.id,fixture));
    }
  },[rows,fixture,onNavigate]);
  useEffect(()=>{
    if(!rows || !strip.current || !actions.current || !measurements.current)return;
    const measure=()=>{
      if(!strip.current || !actions.current || !measurements.current)return;
      const widths=new Map(Array.from(measurements.current.children).map((el)=>[(el as HTMLElement).dataset.id ?? '',el.getBoundingClientRect().width]));
      const budget=strip.current.clientWidth-actions.current.getBoundingClientRect().width-24-(widths.get('all') ?? 70);
      const width=(v:SavedView)=>(widths.get(v.id) ?? 160)+(v.id===id&&v.can_edit?28:0);
      const total=rows.reduce((sum,v)=>sum+width(v),0);
      let remaining=budget-(total>budget?84:0)-(active?width(active):0);
      const selected=new Set(active?[active.id]:[]);
      for(const v of rows){if(v.id!==active?.id&&width(v)<=remaining){selected.add(v.id);remaining-=width(v);}}
      const next=rows.filter(v=>selected.has(v.id)).map(v=>v.id);
      setVisibleIds(old=>old?.join('|')===next.join('|')?old:next);
    };
    const observer=typeof ResizeObserver==='undefined'?null:new ResizeObserver(measure);observer?.observe(strip.current);observer?.observe(actions.current);
    window.addEventListener('resize',measure);const frame=window.setTimeout(measure,0);
    return ()=>{observer?.disconnect();window.removeEventListener('resize',measure);window.clearTimeout(frame);};
  },[rows,id,active,query]);
  async function refresh(){const list=await client.list(new AbortController().signal);setLoaded({client,views:list});return list;}
  async function run(task:()=>Promise<void>){setBusy(true);setNote('');try{await task();}catch(e){setNote(e instanceof Error?e.message:'変更できませんでした。');}finally{setBusy(false);}}
  if(error)return <p className="sv-notice" role="status">ビューを読み込めませんでした</p>;
  if(rows===null)return null;
  const changed=changedView(active,query);
  async function save(id:string,input:ViewInput){
    if(dialog?.view){await client.patch(id,{name:input.name,scope:input.scope});const next=await refresh();setDialog(null);if(id===active?.id&&!next.some(v=>v.id===id))onNavigate(viewSearch('',null,fixture));}
    else {await client.create(id,input);await refresh();setDialog(null);onNavigate(viewSearch(input.query,id,fixture));}
  }
  async function move(view:SavedView,delta:number){const index=rows?.findIndex(v=>v.id===view.id) ?? -1;const neighbor=rows?.[index+delta];if(!neighbor)return;
    const beyond=rows[index+2*delta];const position=beyond?(neighbor.position+beyond.position)/2:neighbor.position+delta;
    await client.patch(view.id,{position});await refresh();}
  const visible=rows.filter(v=>visibleIds===null||visibleIds.includes(v.id));
  const hidden=rows.filter(v=>visibleIds!==null&&!visibleIds.includes(v.id));
  return <div className="sv" data-testid="saved-views" ref={strip}>
    <div className="sv-measurements" ref={measurements} aria-hidden="true">
      <span data-id="all" className="sv-label">すべて</span>
      {rows.map(v=><span className="sv-label" data-id={v.id} key={v.id}>{v.name}{v.scope==='shared'&&<small className="sv-shared">共有</small>}</span>)}
    </div>
    <div className="sv-tabs" role="tablist" aria-label="架電一覧のビュー">
      <button type="button" role="tab" aria-selected={!active} onClick={()=>{onNavigate(viewSearch('',null,fixture));}}>すべて</button>
      {visible.map(v=><div className="sv-tab" key={v.id}>
        <button type="button" role="tab" aria-label={v.scope==='shared'?`${v.name}（全員に共有）`:v.name} aria-selected={id===v.id} onClick={()=>{onNavigate(viewSearch(v.query,v.id,fixture));}}><span className="sv-name" title={v.name}>{v.name}</span>{v.scope==='shared'&&<small className="sv-shared">共有</small>}</button>
        {v.can_edit&&id===v.id&&<button className="sv-menu-toggle" type="button" aria-label={`${v.name}のメニュー`} aria-expanded={menu===v.id} onClick={()=>{setMenu(menu===v.id?null:v.id);}}>⋯</button>}
        {menu===v.id&&<div className="sv-menu" role="group" aria-label={`${v.name}の操作`}>
          <button type="button" disabled={busy} onClick={()=>{setMenu(null);setDialog({view:v,query:v.query});}}>名前の変更</button>
          <button type="button" disabled={busy} onClick={()=>{void run(async()=>{await client.patch(v.id,{scope:v.scope==='shared'?'private':'shared'});const next=await refresh();setMenu(null);if(id===v.id&&!next.some(r=>r.id===id))onNavigate(viewSearch('',null,fixture));});}}>{v.scope==='shared'?'自分だけにする':'全員に共有する'}</button>
          <button type="button" disabled={busy||rows.indexOf(v)===0} onClick={()=>{void run(()=>move(v,-1));}}>左へ移す</button>
          <button type="button" disabled={busy||rows.indexOf(v)===rows.length-1} onClick={()=>{void run(()=>move(v,1));}}>右へ移す</button>
          <button className="sv-danger" type="button" disabled={busy} onClick={()=>{setMenu(null);setDeleting(v);}}>削除</button>
        </div>}
      </div>)}
      {hidden.length>0&&<div className="sv-tab sv-overflow">
        <button type="button" aria-expanded={overflow} onClick={()=>{setOverflow(o=>!o);}}>その他 ▾</button>
        {overflow&&<div className="sv-menu" role="group" aria-label="その他のビュー">{hidden.map(v=><button type="button" key={v.id} onClick={()=>{onNavigate(viewSearch(v.query,v.id,fixture));}}>{v.name}{v.scope==='shared'&&<small className="sv-shared">共有</small>}</button>)}</div>}
      </div>}
    </div>
    <div className="sv-bar-actions" ref={actions}>
      {changed&&active!==null&&<><strong className="sv-changed">変更あり</strong>{active.can_edit&&<button type="button" disabled={busy} onClick={()=>{void run(async()=>{await client.patch(active.id,{query});await refresh();});}}>上書き保存</button>}<button type="button" onClick={()=>{onNavigate(viewSearch(active.query,active.id,fixture));}}>元に戻す</button></>}
      <button type="button" onClick={()=>{setDialog({view:null,query});}}>＋ ビューを保存</button>
    </div>
    {note&&<p role="status" className="sv-notice">{note}</p>}
    {dialog&&<SaveDialog view={dialog.view} query={dialog.query} onSave={save} onClose={()=>{setDialog(null);}} />}
    {deleting&&<div className="sv-backdrop"><section className="sv-dialog" role="dialog" aria-modal="true" aria-label="ビューの削除確認"><h2>「{deleting.name}」を削除しますか？</h2><p>一覧のビューだけを削除します。</p><div className="sv-actions"><button type="button" disabled={busy} onClick={()=>{void run(async()=>{await client.remove(deleting.id);await refresh();setDeleting(null);if(id===deleting.id)onNavigate(viewSearch('',null,fixture));});}}>削除する</button><button type="button" disabled={busy} onClick={()=>{setDeleting(null);}}>やめる</button></div>{note&&<p role="alert">{note}</p>}</section></div>}
  </div>;
}
