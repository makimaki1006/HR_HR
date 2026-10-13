import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { changedView, fixtureViews, liveViews, queueQuery, selectedViewId, viewSearch } from './savedViewsModel';
import type { SavedView, ViewsClient, ViewInput } from './savedViewsModel';
import './saved-views.css';
/** 詳細条件のパネルの中の「ビュー」のまとまり。開いているビューの名前は onActiveName で設定のバーへ知らせる */
interface Props { query:string;search:string;fixture:boolean;client?:ViewsClient;onNavigate:(search:string,opts?:{quiet?:boolean})=>void;onActiveName?:(name:string|null)=>void }
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
export function SavedViews({query,search,fixture,client:provided,onNavigate,onActiveName}:Props) {
  const client=useMemo(()=>provided ?? (fixture?fixtureViews(localStorage):liveViews),[provided,fixture]);
  const [loaded,setLoaded]=useState<{client:ViewsClient;views:SavedView[]}|null>(null);const [failed,setFailed]=useState<ViewsClient|null>(null);
  const rows=loaded?.client===client?loaded.views:null;const error=failed===client;const [note,setNote]=useState(() => { try { return sessionStorage.getItem('hrhr.crm.viewNotice') ?? ''; } catch { return ''; } });
  useEffect(() => { try { sessionStorage.removeItem('hrhr.crm.viewNotice'); } catch { /* optional notice */ } }, []);
  const [menu,setMenu]=useState<string|null>(null);const [dialog,setDialog]=useState<{view:SavedView|null;query:string}|null>(null);
  const [deleting,setDeleting]=useState<SavedView|null>(null);const [busy,setBusy]=useState(false);
  const id=selectedViewId(search);const active=rows?.find(v=>v.id===id) ?? null;
  const activeName=active?.name ?? null;
  useEffect(()=>{onActiveName?.(activeName);},[activeName,onActiveName]);
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
      else if(initial.current.query===''&&view.query!=='')onNavigate(viewSearch(view.query,view.id,fixture),{quiet:true});
    }
  },[rows,fixture,onNavigate]);
  async function refresh(){const list=await client.list(new AbortController().signal);setLoaded({client,views:list});return list;}
  async function run(task:()=>Promise<void>){setBusy(true);setNote('');try{await task();}catch(e){setNote(e instanceof Error?e.message:'変更できませんでした。');}finally{setBusy(false);}}
  if(error)return <section className="sv" data-testid="saved-views" aria-label="ビュー"><h3>ビュー</h3><p className="sv-notice" role="status">ビューを読み込めませんでした</p></section>;
  if(rows===null)return <section className="sv" data-testid="saved-views" aria-label="ビュー"><h3>ビュー</h3><p className="sv-notice" role="status">ビューを読み込み中…</p></section>;
  const changed=changedView(active,query);
  async function save(id:string,input:ViewInput){
    if(dialog?.view){await client.patch(id,{name:input.name,scope:input.scope});const next=await refresh();setDialog(null);if(id===active?.id&&!next.some(v=>v.id===id))onNavigate(viewSearch('',null,fixture));}
    else {await client.create(id,input);await refresh();setDialog(null);onNavigate(viewSearch(input.query,id,fixture));}
  }
  async function move(view:SavedView,delta:number){const index=rows?.findIndex(v=>v.id===view.id) ?? -1;const neighbor=rows?.[index+delta];if(!neighbor)return;
    const beyond=rows[index+2*delta];const position=beyond?(neighbor.position+beyond.position)/2:neighbor.position+delta;
    await client.patch(view.id,{position});await refresh();}
  const open=(v:SavedView)=>{onNavigate(viewSearch(v.query,v.id,fixture));};
  const modal=(node:React.ReactNode)=>createPortal(node,document.body);
  return <section className="sv" data-testid="saved-views" aria-label="ビュー">
    <h3>ビュー</h3>
    <p className="sv-hint">条件の組み合わせを名前を付けて保存し、あとで開き直せます。</p>
    <ul className="sv-list" aria-label="保存したビュー">
      <li className="sv-item"><button type="button" className="sv-open" aria-pressed={!active} onClick={()=>{onNavigate(viewSearch('',null,fixture));}}>すべて<small className="sv-shared">条件なし</small></button></li>
      {rows.map(v=><li className="sv-item" key={v.id}>
        <button type="button" className="sv-open" aria-label={v.scope==='shared'?`${v.name}（全員に共有）`:v.name} aria-pressed={id===v.id} onClick={()=>{open(v);}}><span className="sv-name" title={v.name}>{v.name}</span><small className="sv-shared">{v.scope==='shared'?'共有':'自分用'}</small></button>
        {v.can_edit&&id===v.id&&<button className="sv-menu-toggle" type="button" aria-label={`${v.name}のメニュー`} aria-expanded={menu===v.id} onClick={()=>{setMenu(menu===v.id?null:v.id);}}>⋯</button>}
        {menu===v.id&&<div className="sv-menu" role="group" aria-label={`${v.name}の操作`}>
          <button type="button" disabled={busy} onClick={()=>{setMenu(null);setDialog({view:v,query:v.query});}}>名前の変更</button>
          <button type="button" disabled={busy} onClick={()=>{void run(async()=>{await client.patch(v.id,{scope:v.scope==='shared'?'private':'shared'});const next=await refresh();setMenu(null);if(id===v.id&&!next.some(r=>r.id===id))onNavigate(viewSearch('',null,fixture));});}}>{v.scope==='shared'?'自分だけにする':'全員に共有する'}</button>
          <button type="button" disabled={busy||rows.indexOf(v)===0} onClick={()=>{void run(()=>move(v,-1));}}>上へ移す</button>
          <button type="button" disabled={busy||rows.indexOf(v)===rows.length-1} onClick={()=>{void run(()=>move(v,1));}}>下へ移す</button>
          <button className="sv-danger" type="button" disabled={busy} onClick={()=>{setMenu(null);setDeleting(v);}}>削除</button>
        </div>}
      </li>)}
    </ul>
    {changed&&active!==null&&<div className="sv-changed-row" role="group" aria-label="開いているビューの変更"><strong className="sv-changed">変更あり</strong>{active.can_edit&&<button type="button" disabled={busy} onClick={()=>{void run(async()=>{await client.patch(active.id,{query});await refresh();});}}>上書き保存</button>}<button type="button" onClick={()=>{setDialog({view:null,query});}}>別名で保存</button><button type="button" onClick={()=>{open(active);}}>元に戻す</button></div>}
    <div className="sv-bar-actions"><button type="button" className="sv-save" onClick={()=>{setDialog({view:null,query});}}>＋ ビューを保存</button></div>
    {note&&<p role="status" className="sv-notice">{note}</p>}
    {dialog&&modal(<SaveDialog view={dialog.view} query={dialog.query} onSave={save} onClose={()=>{setDialog(null);}} />)}
    {deleting&&modal(<div className="sv-backdrop"><section className="sv-dialog" role="dialog" aria-modal="true" aria-label="ビューの削除確認"><h2>「{deleting.name}」を削除しますか？</h2><p>一覧のビューだけを削除します。</p><div className="sv-actions"><button type="button" disabled={busy} onClick={()=>{void run(async()=>{await client.remove(deleting.id);await refresh();setDeleting(null);if(id===deleting.id)onNavigate(viewSearch('',null,fixture));});}}>削除する</button><button type="button" disabled={busy} onClick={()=>{setDeleting(null);}}>やめる</button></div>{note&&<p role="alert">{note}</p>}</section></div>)}
  </section>;
}
