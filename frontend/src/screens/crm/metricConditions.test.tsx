// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { MetricsSettings } from './MetricsSettings';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { catalogIndex } from './propertyModel';
import { FIXTURE_METRIC_PIPELINES } from './useQueuePipelines';
import { metricGroupChoices, metricGroupValid } from './metricConditions';
import { metricRemaining } from './metricPrefs';
import type { PropertyCondition } from './queuePropertyFilter';
import type { MetricPrefs } from '../../generated/MetricPrefs';
import type { ValueChoiceSource } from './queueValueChoices';

afterEach(cleanup);
const catalog = { phase: 'ready' as const, catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) };
const choices: ValueChoiceSource = { pipelines: FIXTURE_METRIC_PIPELINES, selectedPipelineIds: ['default'], allDeals: false, owners: { phase: 'idle' } };
const c = (property: string, values: string[], operator = 'IN'): PropertyCondition => ({ property, operator, values });
function panel(groups: PropertyCondition[][]) {
  const onSave = vi.fn<(p: MetricPrefs) => Promise<{ ok: true }>>(() => Promise.resolve({ ok: true }));
  render(<MetricsSettings prefs={{ items: ['c-a'], custom: [{ id: 'c-a', name: '接触数', kind: 'count', condition_groups: groups, owner_property: 'hubspot_owner_id' }] }} available note={null} monthView={false} catalog={catalog} choices={choices} onReloadCatalog={() => undefined} onClose={() => undefined} onSave={onSave} />);
  fireEvent.click(screen.getByRole('button', { name: '直す' }));
  return onSave;
}
function pick(name: string) {
  fireEvent.focus(screen.getByRole('combobox', { name: '項目を検索' }));
  fireEvent.click(screen.getByRole('option', { name }));
}

it('担当者を各まとまりに含め5条件・3まとまり・18条件の残りを計算する', () => {
  const five = Array.from({ length: 5 }, () => c('amount', ['1'], 'GTE'));
  expect(metricRemaining([five], 0)).toBe(0);
  expect(metricRemaining([five], 1)).toBe(5);
  expect(metricRemaining([five, five], 2)).toBe(5);
  expect(metricRemaining([five, five, five], 2)).toBe(0);
  panel([five, five, five]);
  expect(screen.getAllByRole<HTMLButtonElement>('button', { name: '＋ 条件を追加' }).map(b => b.disabled)).toEqual([true, true, true]);
  expect(screen.getByRole<HTMLButtonElement>('button', { name: '＋ 「または」の条件を追加' }).disabled).toBe(true);
});

it('8パイプラインを対象にし、まとまり内のパイプラインでステージ候補を絞る', () => {
  expect(choices.pipelines).toHaveLength(8);
  const group = [c('pipeline', ['753186575'])];
  expect(metricGroupChoices(group, choices).selectedPipelineIds).toEqual(['753186575']);
  expect(metricGroupValid([...group, c('dealstage', ['1095387445'])], choices)).toBe(true);
  expect(metricGroupValid([...group, c('dealstage', ['appointmentscheduled'])], choices)).toBe(false);
  expect(metricGroupValid([c('pipeline', ['753186575'], 'NOT_IN'), c('dealstage', ['1095387445'])], choices)).toBe(false);
  panel([group]);
  fireEvent.click(screen.getByRole('button', { name: '＋ 条件を追加' }));
  fireEvent.focus(screen.getByRole('combobox', { name: '項目を検索' }));
  const special = screen.getByRole('group', { name: '取引の絞り込み' });
  expect(within(special).getAllByRole('option').map(o => o.textContent)).toEqual(['パイプライン', '取引ステージ']);
  expect(screen.getAllByRole('option', { name: 'パイプライン' })).toHaveLength(1);
  expect(screen.getAllByRole('option', { name: '取引ステージ' })).toHaveLength(1);
  fireEvent.click(within(special).getByRole('option', { name: '取引ステージ' }));
  expect(screen.getAllByRole('heading').map(h => h.textContent)).toEqual(['bpo_リクロジ']);
  expect(screen.getByRole('checkbox', { name: '不在' })).toBeTruthy();
  expect(screen.queryByRole('checkbox', { name: '架空ステージ 1' })).toBeNull();
});

it('先にステージがあっても食い違うパイプラインを追加できない', () => {
  panel([[c('dealstage', ['1095387445'])]]);
  fireEvent.click(screen.getByRole('button', { name: '＋ 条件を追加' }));
  pick('パイプライン');
  fireEvent.click(screen.getByRole('checkbox', { name: 'リクロジ受注管理_アポ前' }));
  expect(screen.getByRole<HTMLButtonElement>('button', { name: 'この条件を追加' }).disabled).toBe(true);
  expect(screen.getByText('選んだパイプラインに含まれる取引ステージを選んでください。')).toBeTruthy();
});

it('またはのまとまりを追加し各配列を保存する', () => {
  const save = panel([[c('amount', ['10'], 'GTE')]]);
  fireEvent.click(screen.getByRole('button', { name: '＋ 「または」の条件を追加' }));
  pick('取引ステージ');
  expect(screen.getAllByRole('heading')).toHaveLength(8);
  fireEvent.click(screen.getByRole('checkbox', { name: '不在' }));
  fireEvent.click(screen.getByRole('button', { name: 'この条件を追加' }));
  expect(screen.getByText('または')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'この項目を決める' }));
  fireEvent.click(screen.getByRole('button', { name: '保存' }));
  expect(save.mock.calls[0]?.[0].custom[0]?.condition_groups).toEqual([[c('amount', ['10'], 'GTE')], [c('dealstage', ['1095387445'])]]);
});
