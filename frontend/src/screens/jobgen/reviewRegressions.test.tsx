// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import fixtures from '../../generated/jobgen/fixtures.json';
import type { PostFn } from './api';
import { JobgenScreen } from './JobgenScreen';
import type { HrhackerResult } from './state';
import { HrhackerSection } from './sections/HrhackerSection';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function element(root: ParentNode, selector: string): HTMLElement {
  const found = root.querySelector<HTMLElement>(selector);
  if (!found) throw new Error(`画面に ${selector} がありません`);
  return found;
}

it('旧求人があってもCSVの読込開始から工程①を止め、新しい求人を取り込む', async () => {
  const post = vi.fn((path: string, body: { kind?: string }) => Promise.resolve({ ok: true, data: path.endsWith('/normalize') ? { status: 'ok', jobs: [{ title_hint: body.kind === 'csv' ? '配送スタッフ' : '営業', source_text: body.kind === 'csv' ? '配送スタッフ 月給300000円' : '営業 月給250000円' }] } : fixtures.responses.extract })) as unknown as PostFn;
  const view = render(<JobgenScreen post={post} />);
  fireEvent.change(element(view.container, '#freeText'), { target: { value: '営業 月給250000円' } });
  fireEvent.click(element(view.container, '#normBtn'));
  await waitFor(() => { expect((element(view.container, '#jobTitle') as HTMLInputElement).value).toBe('営業'); });
  expect((element(view.container, '#jobTitle') as HTMLInputElement).value).toBe('営業');
  fireEvent.click(element(view.container, '[data-kind="csv"]'));
  const originalRead = Reflect.get(FileReader.prototype, 'readAsArrayBuffer') as FileReader['readAsArrayBuffer'];
  let resume: () => void = () => { throw new Error('ファイル読込が始まっていません'); };
  vi.spyOn(FileReader.prototype, 'readAsArrayBuffer').mockImplementation(function (this: FileReader, blob: Blob) { resume = () => { originalRead.call(this, blob); }; });
  fireEvent.change(element(view.container, '#fileInput'), { target: { files: [new File(['職種ID,職種名\n42,配送スタッフ\n'], '求人.csv')] } });
  fireEvent.click(element(view.container, '#normBtn'));
  const step = element(view.container, '[data-rerun="extract"]') as HTMLButtonElement;
  expect(step.disabled).toBe(true);
  fireEvent.click(step);
  expect(vi.mocked(post).mock.calls.map(([path]) => path)).toEqual(['/api/jobgen/normalize']);
  act(() => { resume(); });
  await waitFor(() => { expect((element(view.container, '#jobTitle') as HTMLInputElement).value).toBe('配送スタッフ'); });
  expect(step.disabled).toBe(false);
});

it('画面下部は利用者向けの案内だけを表示する', () => {
  const view = render(<JobgenScreen />);
  const footer = element(view.container, '.foot');
  expect(footer.textContent).not.toMatch(/docs\/|job_media_engine_rs|LLM|機械データ|設計正本/);
  expect(footer.textContent).toContain('確認');
});

it('確認表の要確認・検証済・元資料・未取得を各行で区別し、要確認だけに絞れる', () => {
  const h: HrhackerResult = structuredClone(fixtures.responses.hrhacker);
  h.generated_fields = {
    job_title: { column: '案件名', value: '配送スタッフ', status: 'generated_verified', issues: [] },
    merit: { column: 'メリット', value: '', status: 'review_required', issues: ['確認が必要'] },
  };
  h.row['案件名'] = '配送スタッフ';
  h.row['メリット'] = '';
  h.row['給与補足'] = '月給300000円';
  h.row['自由項目1の内容'] = '';
  h.review_required_fields = ['メリット'];
  const view = render(<HrhackerSection h={h} confirmed={false} onConfirm={() => undefined} />);
  const table = element(view.container, '#reviewTable');
  const rowFor = (label: string): HTMLElement => {
    const row = Array.from(table.querySelectorAll<HTMLTableRowElement>('tbody tr')).find(row => row.querySelector('td')?.textContent === label);
    if (!row) throw new Error(`確認表に ${label} がありません`);
    return row;
  };
  expect(within(rowFor('メリット')).getAllByRole('cell').map(c => c.textContent)).toEqual(['メリット', '未取得', '要確認']);
  expect(within(rowFor('求人の見出し')).getAllByRole('cell')[2]?.textContent).toBe('生成（検証済）');
  expect(within(rowFor('給与補足')).getAllByRole('cell')[2]?.textContent).toBe('元の資料から転記');
  expect(within(rowFor('休日・休暇')).getAllByRole('cell')[2]?.textContent).toBe('未取得');
  fireEvent.change(element(view.container, '#reviewFilter'), { target: { value: 'review' } });
  expect(table.querySelectorAll('tbody tr')).toHaveLength(1);
  expect(element(table, 'tbody').textContent).toBe('メリット未取得要確認');
});
