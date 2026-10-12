// @vitest-environment happy-dom
import { afterEach, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { QueueGroupEditor, QueueGroupSummary, PropertySortPicker } from './QueueConditionGroups';
import { QueuePropertyFilter } from './QueuePropertyConditions';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { catalogIndex } from './propertyModel';
import { DEFAULT_FILTERS, groupPatch, groupsOf, filtersKey, parseFilters, screenSearch, queueApiPath, errorMessage } from './queueModel';
import { matchesCondition, parseConditionGroups, sortable, LIMIT_MESSAGE, conditionLabel } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
import { fixtureQueuePage } from './queueFixture';
import { fixtureFetch } from './useCallQueue';
const catalog={phase:'ready' as const,catalog:FIXTURE_CATALOG,index:catalogIndex(FIXTURE_CATALOG)};
const c=(op:string,vs:string[]):PropertyCondition=>({property:'amount',operator:op,values:vs});
afterEach(cleanup);
it('旧URLをそのまま読み、新しいまたはと並びをURL/API/キーに残す',()=>{
  const old=[c('GTE',['8000'])];const p=new URLSearchParams({conditions:JSON.stringify(old),sort:'last_call_desc'});
  expect(parseFilters('?'+p.toString())).toEqual({...DEFAULT_FILTERS,conditions:old,sort:'last_call_desc'});
  const gs=[[c('GTE',['8000'])],[c('LTE',['2000'])]];
  const f={...DEFAULT_FILTERS,...groupPatch(gs),sort:'property:amount:desc' as const};
  expect(groupsOf(parseFilters(screenSearch(f,'fixture')))).toEqual(gs);
  expect(new URL(queueApiPath(f,null),'https://sample.invalid').searchParams.has('conditions')).toBe(false);
  expect(new URL(queueApiPath(f,null),'https://sample.invalid').searchParams.get('condition_groups')).toBe(JSON.stringify(gs));
  expect(filtersKey(f)).not.toBe(filtersKey({...f,sort:'property:amount:asc'}));
  expect(filtersKey(f)).not.toBe(filtersKey({...f,...groupPatch([[c('GTE',['8000']),c('LTE',['2000'])]])}));
  expect(parseConditionGroups(JSON.stringify([[],[c('EQ',['17'])]]))).toEqual([]);
});
it('架空値のまたはで3件から5件へ変わり、項目順と続きも同じ順になる',()=>{
  const a={...DEFAULT_FILTERS,conditions:[c('GTE',['8000'])]};expect(fixtureQueuePage(a,null).total).toBe(3);
  const f={...a,...groupPatch([[c('GTE',['8000'])],[c('LTE',['2000'])]]),sort:'property:amount:desc' as const};
  const page=fixtureQueuePage(f,null);expect(page.total).toBe(5);expect(page.items.map(i=>i.deal_id)).toEqual(['f-11','f-10','f-8','f-2','f-1']);
  const all=fixtureQueuePage({...DEFAULT_FILTERS,sort:'property:amount:asc'},null);expect(all.items.map(i=>i.deal_id)).toEqual(['f-1','f-2','f-3','f-4','f-5']);
  expect(fixtureQueuePage({...DEFAULT_FILTERS,sort:'property:amount:asc'},all.next_cursor).items.map(i=>i.deal_id)).toEqual(['f-6','f-7','f-8','f-10','f-11']);
});
it('範囲・相対日付・否定・真偽を具体値で評価する',()=>{
  expect(matchesCondition(c('EQ',['23.0']),'23','2026-02-04','number')).toBe(true);
  expect(matchesCondition(c('NEQ',['23.0']),'23','2026-02-04','number')).toBe(false);
  expect(matchesCondition(c('EQ',['001']),'1','2026-02-04','string')).toBe(false);
  expect(fixtureQueuePage({...DEFAULT_FILTERS,conditions:[c('EQ',['8000.0'])]},null).items.map(i=>i.deal_id)).toEqual(['f-8']);
  expect(matchesCondition(c('BETWEEN',['17','29']),'23','2026-02-04')).toBe(true);
  expect(matchesCondition(c('BETWEEN',['17','29']),'30','2026-02-04')).toBe(false);
  expect(matchesCondition(c('LAST_N_DAYS',['3']),'2026-02-02','2026-02-04')).toBe(true);
  expect(matchesCondition(c('LAST_N_DAYS',['3']),'2026-02-01','2026-02-04')).toBe(false);
  expect(matchesCondition(c('NEXT_N_DAYS',['3']),'2026-02-06','2026-02-04')).toBe(true);
  expect(matchesCondition(c('NEQ',['sample']),'other','2026-02-04')).toBe(true);
  expect(matchesCondition(c('NOT_CONTAINS_TOKEN',['sample']),'sample value','2026-02-04')).toBe(false);
  expect(matchesCondition(c('NOT_CONTAINS_TOKEN',['sample']),null,'2026-02-04')).toBe(false);
  expect(matchesCondition(c('IS_FALSE',[]),'false','2026-02-04')).toBe(true);
});
it('相対指定と上下限の入力を検証し、内部名を表示しない',()=>{
  const added:PropertyCondition[]=[];render(<QueuePropertyFilter catalog={catalog} count={0} onAdd={v=>added.push(v)} onReload={()=>undefined}/>);
  fireEvent.change(screen.getByLabelText('項目',{exact:true}),{target:{value:'bpo_20'}});
  fireEvent.change(screen.getByLabelText('条件',{exact:true}),{target:{value:'LAST_N_DAYS'}});
  fireEvent.change(screen.getByLabelText('日数'),{target:{value:'0'}});expect(screen.getByRole('button',{name:'この条件を追加'}).hasAttribute('disabled')).toBe(true);
  fireEvent.change(screen.getByLabelText('日数'),{target:{value:'3'}});fireEvent.click(screen.getByRole('button',{name:'この条件を追加'}));
  expect(added).toEqual([{property:'bpo_20',operator:'LAST_N_DAYS',values:['3']}]);
  expect(conditionLabel(added[0] ?? c('EQ',[]),catalog.index.deals.get('bpo_20')?.prop)).toBe('最終架電日 過去 3 日以内');
  fireEvent.change(screen.getByLabelText('項目',{exact:true}),{target:{value:'amount'}});fireEvent.change(screen.getByLabelText('条件',{exact:true}),{target:{value:'BETWEEN'}});
  fireEvent.change(screen.getByLabelText('下限'),{target:{value:'29'}});fireEvent.change(screen.getByLabelText('上限'),{target:{value:'17'}});
  expect(screen.getByRole('button',{name:'この条件を追加'}).hasAttribute('disabled')).toBe(true);
});
it('まとまりを追加・解除し、並べ替えできる項目だけを選べる',()=>{
  function Harness(){const[gs,setGs]=useState([[c('GTE',['8000'])]]);return <><QueueGroupEditor catalog={catalog} groups={gs} onChange={setGs} onReload={()=>undefined}/><QueueGroupSummary catalog={catalog} groups={gs} onChange={setGs}/><output>{JSON.stringify(gs)}</output></>;}
  render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'またはのまとまりを追加'}));
  fireEvent.change(screen.getByLabelText('項目',{exact:true}),{target:{value:'amount'}});fireEvent.change(screen.getByLabelText('条件',{exact:true}),{target:{value:'LTE'}});fireEvent.change(screen.getByLabelText('値',{exact:true}),{target:{value:'2000'}});fireEvent.click(screen.getByRole('button',{name:'この条件を追加'}));
  expect(screen.getByRole('status').textContent).toContain('2000');expect(screen.getByText('または')).not.toBeNull();
  const remove=screen.getAllByRole('button',{name:'このまとまりを外す'})[1];if(!remove) throw new Error('2つ目のまとまりが必要');fireEvent.click(remove);expect(screen.queryByText('または')).toBeNull();
  cleanup();const sorts:string[]=[];render(<PropertySortPicker catalog={catalog} sort="default" onChange={v=>sorts.push(v)}/>);
  fireEvent.change(screen.getByLabelText('並べ替える項目'),{target:{value:'amount'}});fireEvent.change(screen.getByLabelText('順序'),{target:{value:'desc'}});fireEvent.click(screen.getByRole('button',{name:'この順に並べ替え'}));expect(sorts).toEqual(['property:amount:desc']);
  const p=catalog.index.deals.get('amount')?.prop;if(!p) throw new Error('架空の数値項目が必要');expect(sortable({...p,property_type:'json'})).toBe(false);expect(sortable({...p,property_type:'enumeration',field_type:'checkbox'})).toBe(false);
});
it('条件の上限超過を失敗として返し、分かる文言を使う',async()=>{
  const result=await fixtureFetch({...DEFAULT_FILTERS,conditions:Array.from({length:5},()=>c('GTE',['17']))},null,new AbortController().signal);
  expect(result.ok).toBe(false);expect(errorMessage('condition_limit',400)).toBe(LIMIT_MESSAGE);
});
