// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { OwnerFilter } from './OwnerTodo';
import { demoOwnerTodo } from './demoOwnerTodo';

beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?owner=11&owner_field=consultant'); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const owner = () => new URLSearchParams(window.location.search).get('owner');

it('フォーカスで空にして今の名前を薄く出し、打った途中では切り替えず、Enter で切り替える。IME の確定の Enter は無視する', () => {
  const owners = [{ id: '11', name: '田中太郎', archived: false }, { id: '12', name: '田中', archived: false }, { id: '13', name: '佐藤', archived: false }];
  render(<OwnerFilter work={{ ...demoOwnerTodo, owners }} />);
  const input = screen.getByRole<HTMLInputElement>('combobox', { name: '担当者' });
  expect(input.value).toBe('田中太郎');
  fireEvent.focus(input);
  expect(input.value).toBe('');
  expect(input.placeholder).toBe('田中太郎');
  // 文字を打っている間は、「田中」と完全に一致しても切り替えない（「田中太郎」を打つ途中のため）。
  fireEvent.input(input, { target: { value: '田中' }, inputType: 'insertText' });
  expect(owner()).toBe('11');
  fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
  fireEvent.keyDown(input, { key: 'Enter', keyCode: 229 });
  expect(owner()).toBe('11');
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(owner()).toBe('12');
});

it('候補から選んだとき（文字の入力でないとき）はすぐ切り替える', () => {
  render(<OwnerFilter work={demoOwnerTodo} />);
  const input = screen.getByRole<HTMLInputElement>('combobox', { name: '担当者' });
  fireEvent.focus(input);
  fireEvent.input(input, { target: { value: '合成担当B（退職）' }, inputType: 'insertReplacementText' });
  expect(owner()).toBe('22');
});

it('合う人がいなければ「見つかりません」を1行出し、離れると元の名前に戻す', () => {
  render(<OwnerFilter work={demoOwnerTodo} />);
  const input = screen.getByRole<HTMLInputElement>('combobox', { name: '担当者' });
  fireEvent.focus(input);
  fireEvent.input(input, { target: { value: '存在しない人' }, inputType: 'insertText' });
  expect(screen.getByRole('status').textContent).toBe('「存在しない人」に合う担当者が見つかりません');
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(owner()).toBe('11');
  fireEvent.blur(input);
  expect(input.value).toBe('合成担当A');
  expect(screen.queryByRole('status')).toBeNull();
});

it('同じ名前の担当者は「（2人目）」で区別し、HubSpot の番号は出さない', () => {
  render(<OwnerFilter work={{ ...demoOwnerTodo, owner: '31', owners: [{ id: '31', name: '山田', archived: false }, { id: '32', name: '山田', archived: false }] }} />);
  expect([...document.querySelectorAll('datalist option')].map(o => o.getAttribute('value'))).toEqual(['山田', '山田（2人目）']);
  expect(document.body.innerHTML).not.toContain('（32）');
});

it('担当の種類のそばに、何で絞っているかの説明を畳んで置く', () => {
  render(<OwnerFilter work={demoOwnerTodo} />);
  expect(document.querySelector('.jh-owner-tip .jc-infotip-body')?.textContent).toBe('HubSpot の案件の「コンサル担当」の項目で絞っています。');
  expect(document.querySelector('.jh-owner-tip')?.hasAttribute('open')).toBe(false);
});
