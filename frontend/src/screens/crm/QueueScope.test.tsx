// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { DEFAULT_FILTERS, filtersKey, groupsOf, parseFilters, screenSearch, selectedPipelineIds, sortsFor, withPipelines, withTarget } from './queueModel';
import { FIXTURE_PIPELINES, DEFAULT_PIPELINE_ID, FIXTURE_PIPELINE_ID } from './queuePipelines';
import { fixtureQueuePage } from './queueFixture';
import { QueuePipelinePicker } from './QueuePipelinePicker';
import { StageFilter } from './StageFilter';
import { remainingConditions, conditionsFit } from './queueConditionBudget';
import { PhoneRow } from './DealDetail';
import { EMPTY_CALL } from './smartEmbed';
import type { ZoomPhone } from './useZoomPhone';
afterEach(cleanup);
it('単一の旧URLを保ち、複数の集合と対象をURL・比較キーに含める',()=>{
  const c=[{property:'amount',operator:'GTE',values:['8000']}];
  const old=parseFilters('?pipeline='+DEFAULT_PIPELINE_ID+'&conditions='+encodeURIComponent(JSON.stringify(c))+'&sort=last_call_desc');
  expect(old).toEqual({...DEFAULT_FILTERS,conditions:c,sort:'last_call_desc'});expect(fixtureQueuePage(old,null).total).toBe(3);
  const f=withTarget(withPipelines(DEFAULT_FILTERS,[FIXTURE_PIPELINE_ID,DEFAULT_PIPELINE_ID]),'all');
  const q=new URLSearchParams(screenSearch(f,'fixture'));expect(q.has('pipeline')).toBe(false);expect(q.get('pipelines')).toBe([DEFAULT_PIPELINE_ID,FIXTURE_PIPELINE_ID].sort().join(','));expect(q.get('scope')).toBe('all');
  expect(parseFilters(screenSearch(f,'fixture'))).toEqual(f);expect(selectedPipelineIds(f)).toHaveLength(2);
  expect(filtersKey(f)).toBe(filtersKey({...f,pipelines:[FIXTURE_PIPELINE_ID,DEFAULT_PIPELINE_ID]}));expect(filtersKey(f)).not.toBe(filtersKey({...f,target:undefined}));expect(filtersKey(f)).not.toBe(filtersKey({...f,pipelines:undefined}));
  expect(parseFilters('?mode=fixture&scope=all&stage=sample-new-stage').stages).toEqual(['sample-new-stage']);expect(withPipelines(f,[])).toBe(f);expect(groupsOf(old)).toEqual([c]);
});
it('架空の2パイプラインで架電13件から全17件へ広がり、停止と電話なしも残す',()=>{
  const f=withPipelines(DEFAULT_FILTERS,[DEFAULT_PIPELINE_ID,FIXTURE_PIPELINE_ID]);expect(fixtureQueuePage(f,null).total).toBe(13);
  const all=withTarget(f,'all');const first=fixtureQueuePage(all,null);expect(first.total).toBe(17);expect(first.items.map(i=>i.deal_id)).toEqual(['f-17','f-16','f-15','f-14','f-13']);
  expect(first.items[1]?.stop.prohibited_reason).toBe('架空の確認待ち');expect(first.items[1]?.stop.block_reason).toBe('架空の受付停止');
  expect(fixtureQueuePage(all,first.next_cursor).items[0]?.deal_id).toBe('f-12');expect(fixtureQueuePage(all,first.next_cursor).items[0]?.phone).toBeNull();
  const filtered={...all,conditions:[{property:'amount',operator:'GTE',values:['8000']}]};expect(fixtureQueuePage(filtered,null).total).toBe(10);
  expect(sortsFor(all).some(s=>s.value==='default')).toBe(false);expect(sortsFor(all)[0]?.label).toBe('作成日の新しい順');
});
it('最後のパイプラインを外せず、チェックは適用時にだけ反映する',()=>{
  const apply=vi.fn();render(<QueuePipelinePicker pipelines={FIXTURE_PIPELINES} selected={[DEFAULT_PIPELINE_ID]} onApply={apply}/>);
  fireEvent.click(screen.getByRole('button',{name:'パイプライン'}));const region=screen.getByRole('region',{name:'パイプラインの選択'});
  const boxes=within(region).getAllByRole<HTMLInputElement>('checkbox');expect(boxes[0]?.disabled).toBe(true);if(!boxes[1])throw new Error('第二の候補');fireEvent.click(boxes[1]);expect(apply).not.toHaveBeenCalled();expect(boxes[0]?.disabled).toBe(false);
  fireEvent.click(within(region).getByRole('button',{name:'パイプラインを適用'}));expect(apply).toHaveBeenCalledWith([DEFAULT_PIPELINE_ID,FIXTURE_PIPELINE_ID]);
});
it('すべての取引のステージをパイプライン別にまとめ、対象外も選べる',()=>{
  const first=FIXTURE_PIPELINES[0];if(!first)throw new Error('架空の定義');const apply=vi.fn();
  render(<StageFilter pipeline={first} pipelines={[...FIXTURE_PIPELINES]} all selected={[]} onApply={apply}/>);fireEvent.click(screen.getByRole('button',{name:/ステージ/}));
  const group=screen.getByRole('group',{name:'ステージの選択'});expect(within(group).getAllByRole('heading')).toHaveLength(2);
  expect(within(group).getByLabelText('対象外').getAttribute('disabled')).toBeNull();fireEvent.click(within(group).getByLabelText('対象外'));fireEvent.click(within(group).getByRole('button',{name:'適用'}));
  expect(apply.mock.calls[0]?.[0]).not.toContain('fx-stop');expect(apply.mock.calls[0]?.[0]).toHaveLength(first.stages.length+2);
});
it('全取引の単一段階と横断する架電段階に合わせて残り条件数を計算する',()=>{
  const f=withTarget(withPipelines(DEFAULT_FILTERS,[DEFAULT_PIPELINE_ID,FIXTURE_PIPELINE_ID]),'all');expect(remainingConditions(f,[],0)).toBe(5);expect(remainingConditions({...f,owner:'me',nextFrom:'2026-10-01'},[],0)).toBe(3);
  const cs=Array.from({length:5},()=>({property:'amount',operator:'EQ',values:['17']}));expect(conditionsFit(f,[cs])).toBe(true);expect(conditionsFit(f,[[...cs,{property:'amount',operator:'EQ',values:['17']}]])).toBe(false);
});
it('架電禁止・ブロックはZoom発信と端末の発信の両方を止める',()=>{
  const dial=vi.fn<ZoomPhone['dial']>(()=>'sent');const zoom:ZoomPhone={embed:'loaded',onLoad:()=>undefined,call:EMPTY_CALL,pending:null,stalled:false,heard:true,now:()=>0,dial};
  render(<PhoneRow label="架空の番号" raw="0300000016" zoom={zoom} blocked/>);
  const button=screen.getByRole('button',{name:/に発信/});expect(button.hasAttribute('disabled')).toBe(true);fireEvent.click(button);expect(dial).not.toHaveBeenCalled();expect(screen.queryByRole('link',{name:/端末の電話/})).toBeNull();
});
