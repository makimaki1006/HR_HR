// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { PortalSheetRows } from '../../../../generated/PortalSheetRows';
import { ApplicantsScreen } from './ApplicantsScreen';
import type { EditableApplicant, EditableColumn } from './editing';

const columns: EditableColumn[] = [
  { id: '応募者名', label: '応募者名', kind: 'text', group: 'summary', in_list: true, multiline: false, writable: false, options: null },
  { id: '備考', label: '備考', kind: 'text', group: 'customer', in_list: true, multiline: true, writable: true, options: null },
];
const first: EditableApplicant = { row: 3, cells: ['架空 花子', '前の備考'], revision: 'r1' };
const me = { email: 'customer@example.invalid', customer_id: 'c1' };
let row: EditableApplicant;
let currentColumns: EditableColumn[];
let patch: (body: { revision: string; changes: { column: string; value: string }[] }) => Promise<Response>;
let requests: { revision: string; changes: { column: string; value: string }[] }[];
let signedOut = vi.fn<() => void>();
let nextRead: (() => Promise<Response>) | undefined;
const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
const data = (): PortalSheetRows => ({ sheet: { id: 's1', label: '応募者一覧' }, version: row.revision, read_at: '2026-10-28T09:00:00Z', stale: false, columns: currentColumns, applicants: [row] });

beforeEach(() => {
  row = { ...first, cells: first.cells.slice() };
  currentColumns = columns;
  requests = [];
  signedOut = vi.fn<() => void>();
  nextRead = undefined;
  window.history.replaceState(null, '', '/portal?sheet=s1&row=3');
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
  patch = (body) => {
    row = { ...row, cells: [row.cells[0] ?? '', body.changes[0]?.value ?? ''], revision: 'r2' };
    return Promise.resolve(response({ applicant: row }));
  };
  vi.stubGlobal('fetch', vi.fn(async (path: string, init?: RequestInit) => {
    if (init?.method === 'PATCH') {
      if (typeof init.body !== 'string') throw new Error('JSON body required');
      const body = JSON.parse(init.body) as { revision: string; changes: { column: string; value: string }[] };
      requests.push(body); return patch(body);
    }
    if (path === '/api/portal/sheets') return response({ sheets: [{ id: 's1', label: '応募者一覧' }] });
    if (path.endsWith('/version')) return response({ version: row.revision });
    if (path.endsWith('/rows')) { const read = nextRead; nextRead = undefined; return read ? read() : response(data()); }
    throw new Error(`unknown path ${path}`);
  }));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

async function open() {
  render(<ApplicantsScreen me={me} onSignedOut={signedOut} onLogout={() => undefined} />);
  const dialog = await screen.findByRole('dialog');
  const input = within(within(dialog).getByRole('group', { name: '備考' })).getByRole<HTMLTextAreaElement>('textbox');
  return { dialog, input, save: within(dialog).getByRole('button', { name: '変更を保存' }) };
}

it('開いた行のrevisionと変更列だけを送り、数式に見える入力も文字列のまま保持する', async () => {
  const { input, save } = await open();
  const text = '=IMPORTXML("https://example.invalid", "//x")';
  fireEvent.change(input, { target: { value: text } });
  fireEvent.click(save);
  expect(await screen.findByText('保存しました')).toBeTruthy();
  expect(requests).toEqual([{ revision: 'r1', changes: [{ column: '備考', value: text }] }]);
  expect(input.value).toBe(text);
  expect(screen.queryByRole('textbox', { name: '応募者名' })).toBeNull();
});

it('競合時は入力を残し、自動で上書きせず最新の内容を確認させる', async () => {
  patch = () => {
    row = { ...row, cells: ['架空 花子', '社内で変更'], revision: 'r2' };
    return Promise.resolve(response({ error: 'conflict', message: 'シートで変更されています', latest: row }, 409));
  };
  const { dialog, input, save } = await open();
  fireEvent.change(input, { target: { value: '顧客の入力' } });
  fireEvent.click(save);
  expect(await within(dialog).findByText('シートで変更されています')).toBeTruthy();
  expect(input.value).toBe('顧客の入力');
  expect(requests).toHaveLength(1);
  expect((save as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(within(dialog).getByRole('button', { name: '最新の内容で開き直す' }));
  expect(input.value).toBe('社内で変更');
  expect(requests).toHaveLength(1);
});

it('通信失敗時は入力を残して再試行できる', async () => {
  patch = () => { throw new Error('offline'); };
  const { input, save } = await open();
  fireEvent.change(input, { target: { value: '保存したい内容' } });
  fireEvent.click(save);
  expect(await screen.findByRole('alert')).toBeTruthy();
  expect(input.value).toBe('保存したい内容');
  expect((save as HTMLButtonElement).disabled).toBe(false);
});

it('保存が401なら、入力中の個人情報も詳細も消す', async () => {
  patch = () => Promise.resolve(response({ error: 'unauthorized', message: 'ログインしてください' }, 401));
  const { input, save } = await open();
  fireEvent.change(input, { target: { value: '個人情報の下書き' } });
  fireEvent.click(save);
  await waitFor(() => { expect(signedOut).toHaveBeenCalledOnce(); });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.queryAllByText('架空 花子')).toHaveLength(0);
  expect(screen.queryByDisplayValue('個人情報の下書き')).toBeNull();
});

it('保存前に始まった古い読み直しが遅れて届いても保存結果を戻さない', async () => {
  const { input, save } = await open();
  let finishOld: ((response: Response) => void) | undefined;
  const old = data();
  row = { ...row, revision: 'server-new-version' };
  nextRead = () => new Promise<Response>((resolve) => { finishOld = resolve; });
  await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await Promise.resolve(); });
  await waitFor(() => { expect(finishOld).toBeTypeOf('function'); });
  fireEvent.change(input, { target: { value: '新しい保存結果' } });
  fireEvent.click(save);
  expect(await screen.findByText('保存しました')).toBeTruthy();
  await act(async () => { finishOld?.(response(old)); await Promise.resolve(); });
  expect(input.value).toBe('新しい保存結果');
  expect(row.cells[1]).toBe('新しい保存結果');
  expect(requests[0]?.revision).toBe('r1');
});


it('設定が変わった409で最新行が無ければ個人情報の下書きを捨てて一覧を読み直す', async () => {
  patch = () => {
    currentColumns = columns.slice(0, 1);
    row = { row: 3, cells: ['架空 花子'], revision: 'schema-new' };
    return Promise.resolve(response({ error: 'conflict', message: '項目の設定が変わりました' }, 409));
  };
  const { input, save } = await open();
  fireEvent.change(input, { target: { value: '旧列の個人情報' } });
  fireEvent.click(save);
  await waitFor(() => { expect(screen.queryByRole('dialog')).toBeNull(); });
  expect(screen.queryByDisplayValue('旧列の個人情報')).toBeNull();
  expect(await screen.findByText('項目の設定が変わったため、一覧を読み直しました。応募者を開き直してください。')).toBeTruthy();
});

it('保存中に始めたGETの401も破棄せず個人情報を消す', async () => {
  let finishSave: ((value: Response) => void) | undefined;
  patch = () => new Promise<Response>((resolve) => { finishSave = resolve; });
  const { input, save } = await open();
  let finishRead: ((value: Response) => void) | undefined;
  row = { ...row, revision: 'changed-version' };
  nextRead = () => new Promise<Response>((resolve) => { finishRead = resolve; });
  await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await Promise.resolve(); });
  await waitFor(() => { expect(finishRead).toBeTypeOf('function'); });
  fireEvent.change(input, { target: { value: '認可失効前の下書き' } });
  fireEvent.click(save);
  await waitFor(() => { expect(finishSave).toBeTypeOf('function'); });
  await act(async () => { finishRead?.(response({ error: 'unauthorized' }, 401)); await Promise.resolve(); });
  expect(signedOut).toHaveBeenCalledOnce();
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.queryByDisplayValue('認可失効前の下書き')).toBeNull();
  await act(async () => { finishSave?.(response({ applicant: row })); await Promise.resolve(); });
  expect(screen.queryAllByText('架空 花子')).toHaveLength(0);
});
