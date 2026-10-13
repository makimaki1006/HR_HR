// @vitest-environment jsdom
import { cleanup,fireEvent,render,screen,waitFor } from '@testing-library/react';
import { afterEach,describe,expect,it,vi } from 'vitest';
import { SavedViews } from './SavedViews';
import { changedView,fixtureViews,queueQuery,selectedViewId,viewSearch } from './savedViewsModel';
import type { SavedView,ViewsClient,ViewInput } from './savedViewsModel';
afterEach(()=>{cleanup();localStorage.clear();sessionStorage.clear();vi.restoreAllMocks();});
const id='00000000-0000-4000-8000-000000000001';
const view:SavedView={id,name:'架空の未担当一覧',query:'owner=unassigned',scope:'shared',position:1,can_edit:true,created_at:'2026-10-12T00:00:00Z',updated_at:'2026-10-12T00:00:00Z'};
function client(rows:SavedView[]=[view]):ViewsClient {return {list:vi.fn(()=>Promise.resolve(rows)),create:vi.fn((id:string,input:ViewInput)=>Promise.resolve({...view,id,...input})),patch:vi.fn((id:string,input:Partial<ViewInput>)=>Promise.resolve({...view,id,...input})),remove:vi.fn(()=>Promise.resolve())};}
describe('saved views',()=>{
 it('preserves opaque encoded filters and repeated parameters; changes are exact comparisons',()=>{
  const raw='conditions=%7B%22future%22%3A%5B1%2C2%5D%7D&stage=fake-a&stage=fake-b&q=two%20words';
  expect(queueQuery(`?mode=fixture&${raw}&view=${id}`)).toBe(raw);
  expect(viewSearch(raw,id,true)).toBe(`?mode=fixture&${raw}&view=${id}`);
  expect(selectedViewId(`?view=${id}`)).toBe(id);expect(selectedViewId('?view=queue')).toBeNull();
  expect(changedView(view,'owner=all')).toBe(true);expect(changedView(view,'owner=unassigned')).toBe(false);expect(changedView(null,'owner=all')).toBe(false);
 });
 it('restores a visible view when URL contains only its id; an inaccessible id returns to all with a notice',async()=>{
  const navigate=vi.fn();const api=client();const r=render(<SavedViews query="" search={`?view=${id}`} fixture={false} client={api} onNavigate={navigate}/>);
  await waitFor(()=>{expect(navigate).toHaveBeenCalledWith(`?owner=unassigned&view=${id}`,{quiet:true});});
  r.unmount();navigate.mockClear();render(<SavedViews query="" search="?view=not-visible" fixture={false} client={client([])} onNavigate={navigate}/>);
  await waitFor(()=>{expect(navigate).toHaveBeenCalledWith('');});expect(screen.getByText('このビューは見られないため、「すべて」に戻しました。')).toBeTruthy();
 });
 it('shows modified state, overwrites the opaque query, and restores the saved URL',async()=>{
  const api=client();const navigate=vi.fn();render(<SavedViews query="owner=all" search={`?owner=all&view=${id}`} fixture={false} client={api} onNavigate={navigate}/>);
  await screen.findByText('変更あり');fireEvent.click(screen.getByText('上書き保存'));
  await waitFor(()=>{expect(api.patch).toHaveBeenCalledWith(id,{query:'owner=all'});});
  fireEvent.click(screen.getByText('元に戻す'));expect(navigate).toHaveBeenCalledWith(`?owner=unassigned&view=${id}`);
 });
 it('requires deletion confirmation and hides modification controls for someone else’s shared view',async()=>{
  const api=client();const navigate=vi.fn();const r=render(<SavedViews query={view.query} search={`?${view.query}&view=${id}`} fixture={false} client={api} onNavigate={navigate}/>);
  fireEvent.click(await screen.findByLabelText('架空の未担当一覧のメニュー'));fireEvent.click(screen.getByText('削除',{exact:true}));expect(api.remove).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('削除する'));await waitFor(()=>{expect(api.remove).toHaveBeenCalledWith(id);});
  r.unmount();render(<SavedViews query="owner=all" search={`?owner=all&view=${id}`} fixture={false} client={client([{...view,can_edit:false}])} onNavigate={navigate}/>);
  await screen.findByText('変更あり');expect(screen.queryByText('上書き保存')).toBeNull();expect(screen.queryByLabelText('架空の未担当一覧のメニュー')).toBeNull();
 });
 it('keeps the queue usable with one line when audit views cannot be loaded',async()=>{
  const api=client();api.list=()=>Promise.reject(new Error('offline'));render(<SavedViews query="" search="" fixture={false} client={api} onNavigate={vi.fn()}/>);
  await screen.findByText('ビューを読み込めませんでした');expect(screen.queryByRole('tablist')).toBeNull();
 });
 it('removes page and view metadata while preserving the original filter bytes',()=>{
  expect(queueQuery(`?q=two%20words&cursor=page-two&view=${id}&owner=all&%63ursor=page-three`)).toBe('q=two%20words&owner=all');
 });
 it('fixture saves persist across a reopened client without API writes',async()=>{
  const api=fixtureViews(localStorage);await api.create(id,{name:'架空一覧',scope:'private',query:'owner=unassigned'});
  expect((await fixtureViews(localStorage).list(new AbortController().signal)).map(v=>[v.name,v.query])).toEqual([['架空一覧','owner=unassigned']]);
  await api.patch(id,{name:'架空の共有一覧',scope:'shared',position:-1});expect((await api.list(new AbortController().signal))[0]?.scope).toBe('shared');
  await api.remove(id);expect(await api.list(new AbortController().signal)).toEqual([]);
 });

 it('panel: lists すべて + saved views as pressed buttons; opening one navigates; 別名で保存 saves the current query as a new view; the name is reported for the bar',async()=>{
  const api=client();const navigate=vi.fn();const names:(string|null)[]=[];
  render(<SavedViews query="owner=all" search={`?owner=all&view=${id}`} fixture={false} client={api} onNavigate={navigate} onActiveName={n=>{names.push(n);}}/>);
  const open=await screen.findByRole('button',{name:'架空の未担当一覧（全員に共有）'});
  expect(open.getAttribute('aria-pressed')).toBe('true');expect(screen.getByRole('button',{name:/^すべて/u}).getAttribute('aria-pressed')).toBe('false');
  expect(names.at(-1)).toBe('架空の未担当一覧');
  fireEvent.click(screen.getByRole('button',{name:/^すべて/u}));expect(navigate).toHaveBeenCalledWith('');
  fireEvent.click(screen.getByRole('button',{name:'別名で保存'}));
  fireEvent.change(screen.getByLabelText('ビューの名前'),{target:{value:'架空の別名'}});
  fireEvent.click(screen.getByRole('button',{name:/^保存$/u}));
  await waitFor(()=>{expect(api.create).toHaveBeenCalledWith(expect.any(String),{name:'架空の別名',scope:'private',query:'owner=all'});});
  await waitFor(()=>{expect(navigate).toHaveBeenCalledWith(expect.stringMatching(/^\?owner=all&view=/u));});
 });
 it('panel: ＋ ビューを保存 on the unmodified list saves the current query; the dialog is outside the surrounding form',async()=>{
  const api=client([]);const navigate=vi.fn();
  const r=render(<form><SavedViews query="q=%E6%9E%B6%E7%A9%BA" search="?q=%E6%9E%B6%E7%A9%BA" fixture={false} client={api} onNavigate={navigate}/></form>);
  fireEvent.click(await screen.findByRole('button',{name:'＋ ビューを保存'}));
  expect(r.container.querySelector('form form')).toBeNull();expect(r.container.querySelector('[role=dialog]')).toBeNull();
  fireEvent.change(screen.getByLabelText('ビューの名前'),{target:{value:'架空の新規'}});fireEvent.click(screen.getByRole('button',{name:/^保存$/u}));
  await waitFor(()=>{expect(api.create).toHaveBeenCalledWith(expect.any(String),{name:'架空の新規',scope:'private',query:'q=%E6%9E%B6%E7%A9%BA'});});
 });
 it('panel: audit failure shows one line inside the section',async()=>{
  const api=client();api.list=()=>Promise.reject(new Error('offline'));render(<SavedViews query="" search="" fixture={false} client={api} onNavigate={vi.fn()}/>);
  const note=await screen.findByText('ビューを読み込めませんでした');expect(note.closest('[data-testid=saved-views]')).not.toBeNull();
 });
});
