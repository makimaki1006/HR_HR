// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { QueueToolbar } from './QueueToolbar';

afterEach(() => { cleanup(); vi.useRealTimers(); });
const summary = 'bpo_リクロジ · 架電対象だけ · 全員分 · 標準';
function setup() {
  vi.useFakeTimers();
  render(<QueueToolbar summary={summary} keepOpen={false} onClose={() => undefined}><input aria-label="検索" /><select aria-label="担当"><option>全員分</option></select></QueueToolbar>);
  const toggle = screen.getByRole('button', { name: `条件を変更: ${summary}` });
  const root = toggle.parentElement;
  if (!root) throw new Error('toolbar missing');
  const controls = document.getElementById('cq-toolbar-controls');
  if (!controls) throw new Error('controls missing');
  const tick = (ms: number) => { act(() => { vi.advanceTimersByTime(ms); }); };
  return { toggle, root, controls, tick };
}
it('要約を表示し、通り過ぎたときは開かず、300ms 後に開閉する', () => {
  const { toggle, root, controls, tick } = setup();
  expect(toggle.textContent).toContain(summary);
  expect(controls.hidden).toBe(true);
  fireEvent.pointerEnter(root, { pointerType: 'mouse' }); tick(299);
  expect(controls.hidden).toBe(true);
  fireEvent.pointerLeave(root); tick(300);
  expect(controls.hidden).toBe(true);
  fireEvent.pointerEnter(root, { pointerType: 'mouse' }); tick(300);
  expect(controls.hidden).toBe(false);
  fireEvent.pointerLeave(root); tick(299);
  expect(controls.hidden).toBe(false);
  tick(1); expect(controls.hidden).toBe(true);
});
it.each(['検索', '担当'])('%s にフォーカスがある間は閉じず、離れた300ms後に閉じる', label => {
  const { root, controls, tick } = setup();
  fireEvent.pointerEnter(root); tick(300);
  act(() => { screen.getByLabelText(label).focus(); });
  fireEvent.pointerLeave(root); tick(600);
  expect(controls.hidden).toBe(false);
  act(() => { screen.getByLabelText(label).blur(); }); tick(300);
  expect(controls.hidden).toBe(true);
});
it('クリックで固定し、再クリックと Esc で閉じて要約にフォーカスを戻す', () => {
  const { toggle, root, controls, tick } = setup();
  fireEvent.click(toggle); fireEvent.pointerLeave(root); tick(600);
  expect(toggle.getAttribute('aria-pressed')).toBe('true');
  expect(controls.hidden).toBe(false);
  fireEvent.click(toggle);
  expect(controls.hidden).toBe(true);
  fireEvent.click(toggle);
  act(() => { screen.getByLabelText('検索').focus(); });
  fireEvent.keyDown(screen.getByLabelText('検索'), { key: 'Escape' });
  expect(controls.hidden).toBe(true);
  expect(document.activeElement).toBe(toggle);
  tick(600); expect(controls.hidden).toBe(true);
});
it('詳細条件のパネルを開いている間は保持する', () => {
  vi.useFakeTimers();
  const { rerender } = render(<QueueToolbar summary={summary} keepOpen onClose={() => undefined}>詳細条件の内容</QueueToolbar>);
  const toggle = screen.getByRole('button');
  const root = toggle.parentElement;
  if (!root) throw new Error('toolbar missing');
  fireEvent.pointerLeave(root);
  act(() => { vi.advanceTimersByTime(600); });
  expect(toggle.getAttribute('aria-expanded')).toBe('true');
  rerender(<QueueToolbar summary={summary} keepOpen={false} onClose={() => undefined}>詳細条件の内容</QueueToolbar>);
  expect(toggle.getAttribute('aria-expanded')).toBe('false');
});
