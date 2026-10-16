// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { CallQueueScreen } from './CallQueueScreen';
import { QueueToolbar } from './QueueToolbar';
import { StageFilter } from './StageFilter';
import { FIXTURE_PIPELINES, DEFAULT_PIPELINE_ID } from './queuePipelines';
import { DealOverview } from './DealDetail';
import { RowPreview } from './RowPreview';
import { fixtureDetail } from './workspaceFixture';
import { fixtureQueuePage } from './queueFixture';
import { DEFAULT_FILTERS } from './queueModel';
import { resetDockStorage } from './queueTestUtil';
import { EMPTY_CALL } from './smartEmbed';
import type { ZoomPhone } from './useZoomPhone';

function required<T>(value: T | null | undefined): T {
  if (value == null) throw new Error('架空サンプルの値がありません');
  return value;
}

afterEach(() => { cleanup(); vi.useRealTimers(); window.history.replaceState(null, '', '/'); });

it.each(['', '&scope=all'])('上段から複数パイプラインのステージを適用し、URL・チップ・再表示に同じ値を使う %s', async scope => {
  resetDockStorage();
  window.history.replaceState(null, '', `/?mode=fixture&pipelines=${DEFAULT_PIPELINE_ID},fx-sample${scope}`);
  render(<CallQueueScreen />);
  fireEvent.click(screen.getByRole('button', { name: /^条件を変更:/ }));
  const button = screen.getByRole('button', { name: /^ステージ（/ });
  expect(button.closest('.cq-fields-scope')).not.toBeNull();
  expect(document.getElementById('cq-advanced')?.hidden).toBe(true);
  fireEvent.click(button);
  const panel = screen.getByRole('group', { name: 'ステージの選択' });
  expect(within(panel).getAllByRole('heading').map(el => el.textContent)).toEqual(['bpo_リクロジ', '架空パイプライン(確認用)']);
  fireEvent.click(within(panel).getByRole('button', { name: 'すべて外す' }));
  fireEvent.click(within(panel).getByLabelText(scope ? /^対象外$/ : /^追客/));
  const selected = scope ? 'fx-stop' : 'fx-follow';
  fireEvent.click(within(panel).getByRole('button', { name: '適用' }));
  await waitFor(() => { expect(new URLSearchParams(window.location.search).getAll('stage')).toEqual([selected]); });
  expect(screen.getByRole('list', { name: '適用中の条件' }).textContent).toContain('ステージ: 1 件を選択');
  fireEvent.click(screen.getByRole('button', { name: 'ステージ（1件選択）' }));
  expect(screen.getAllByRole<HTMLInputElement>('checkbox').filter(el => el.checked).map(el => el.closest('label')?.textContent)).toContain(scope ? '対象外' : '追客次回架電日が来たら表示');
});

it('ステージ選択中はフォーカスとマウスが外れても保持し、Esc は選択を破棄してから条件欄を閉じる', () => {
  vi.useFakeTimers();
  const pipeline = FIXTURE_PIPELINES[1];
  if (!pipeline) throw new Error('fixture');
  const apply = vi.fn();
  render(<QueueToolbar summary="架空の条件" keepOpen={false} onClose={() => undefined}><StageFilter pipeline={pipeline} selected={[]} onApply={apply} /></QueueToolbar>);
  const summary = screen.getByRole('button', { name: /^条件を変更:/ });
  const root = required(summary.parentElement);
  fireEvent.pointerEnter(root);
  act(() => { vi.advanceTimersByTime(300); });
  fireEvent.click(screen.getByRole('button', { name: 'ステージ（2件選択）' }));
  fireEvent.click(screen.getByLabelText(/^新規/));
  fireEvent.pointerLeave(root);
  act(() => { vi.advanceTimersByTime(600); });
  expect(summary.getAttribute('aria-expanded')).toBe('true');
  fireEvent.keyDown(screen.getByLabelText(/^追客/), { key: 'Escape' });
  expect(screen.queryByRole('group', { name: 'ステージの選択' })).toBeNull();
  expect(apply).not.toHaveBeenCalled();
  expect(summary.getAttribute('aria-expanded')).toBe('true');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'ステージ（2件選択）' }));
  fireEvent.keyDown(required(document.activeElement), { key: 'Escape' });
  expect(summary.getAttribute('aria-expanded')).toBe('false');
});

const zoom: ZoomPhone = { embed: 'loaded', onLoad: () => undefined, call: EMPTY_CALL, pending: null, stalled: false, heard: true, now: () => 0, dial: () => 'sent' };
function data() {
  const item = required(fixtureQueuePage(DEFAULT_FILTERS, null).items[0]);
  const detail = required(fixtureDetail(item.deal_id));
  const primary = { ...required(detail.contacts[0]), name: '架空 花子', job_title: '採用部長', labels: ['主', '採用窓口'], is_primary: true };
  detail.contacts = [{ ...primary, id: 'fictional-secondary', name: '架空 次郎', phone: '0300000088', mobile: '09000000088', is_primary: false }, primary];
  return { item, detail, primary };
}
it.each(['contact', 'mobile', 'deal', 'company'])('%s の番号の箱に第一候補の名前・役職・関連ラベルを出す', source => {
  const { detail, primary } = data();
  detail.dial = { number: source === 'mobile' ? required(primary.mobile) : required(primary.phone), source };
  render(<DealOverview state={{ phase: 'ready', reqId: 'fixture', message: '', errorKind: null, data: detail, refreshing: false, refreshError: null }} reload={() => undefined} zoom={zoom} sample />);
  const box = screen.getByRole('region', { name: '架ける番号' });
  expect(within(box).getByRole('link', { name: '架空 花子' }).getAttribute('href')).toBe(primary.deep_link);
  expect(box.textContent).toContain('採用部長主採用窓口');
  expect(box.querySelector('.wd-dial-contact')?.textContent).not.toContain('架空 次郎');
});
it.each([null, '', '   '])('名前が %s なら名前・役職・ラベルを増やさない', name => {
  const { detail } = data();
  detail.contacts = detail.contacts.filter(c => c.is_primary).map(c => ({ ...c, name }));
  render(<DealOverview state={{ phase: 'ready', reqId: 'fixture', message: '', errorKind: null, data: detail, refreshing: false, refreshError: null }} reload={() => undefined} zoom={zoom} sample />);
  const box = screen.getByRole('region', { name: '架ける番号' });
  expect(box.querySelector('.wd-dial-contact')).toBeNull();
  expect(box.textContent).not.toContain('採用部長');
});
it.each([true, false])('第一候補が取得できなければ別人を出さない (空=%s)', empty => {
  const { detail } = data();
  detail.contacts = empty ? [] : [{ ...required(detail.contacts[0]), is_primary: false }];
  render(<DealOverview state={{ phase: 'ready', reqId: 'fixture', message: '', errorKind: null, data: detail, refreshing: false, refreshError: null }} reload={() => undefined} zoom={zoom} sample />);
  expect(screen.getByRole('region', { name: '架ける番号' }).querySelector('.wd-dial-contact')).toBeNull();
});
it('compact と読み込み中の行の見出しに同じ名前を表示する', () => {
  const { item, detail } = data();
  render(<DealOverview state={{ phase: 'ready', reqId: 'fixture', message: '', errorKind: null, data: detail, refreshing: false, refreshError: null }} reload={() => undefined} zoom={zoom} density={{ compact: true, onToggle: () => undefined }} sample />);
  expect(screen.getByTestId('head-line-1').querySelector('.wd-line-contact')?.textContent).toBe('架空 花子');
  cleanup();
  render(<RowPreview item={{ ...item, contact: { ...required(item.contact), name: '架空 花子' } }} />);
  expect(screen.getByTestId('row-preview').textContent).toContain('架空 花子');
});
