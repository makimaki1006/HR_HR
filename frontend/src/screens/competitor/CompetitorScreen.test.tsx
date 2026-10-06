// @vitest-environment happy-dom
// 画面の状態遷移: 送信中 / 成功 / コード付きエラー / 401 (入力保持) / 429 / 二重送信 / PDF。
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiAbortedError,
  ApiHttpError,
  ApiNetworkError,
  ApiTimeoutError,
  AuthRequiredError,
  type ApiResult,
} from '../../api/client';
import type { CompetitorReportResponse } from '../../generated/CompetitorReportResponse';

vi.mock('./api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api')>();
  return { ...actual, fetchOptions: vi.fn(), createReport: vi.fn(), downloadPdf: vi.fn() };
});

import { createReport, downloadPdf, fetchOptions, type PdfResult } from './api';
import { CompetitorScreen } from './CompetitorScreen';
import { makeReport, makeResponse } from './fixtures';

const mCreate = vi.mocked(createReport);
const mPdf = vi.mocked(downloadPdf);
const mOptions = vi.mocked(fetchOptions);

const OPTIONS = {
  titles: ['施設長', '介護職'],
  prefectures: ['北海道', '大阪府'],
  market_available: true,
};

type CreateResult = ApiResult<CompetitorReportResponse>;

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (v: T) => void;
}
function deferred<T>(): Deferred<T> {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

const csv = (name = 'jobs.csv', body = 'a,b\n1,2\n'): File =>
  new File([body], name, { type: 'text/csv' });

async function mount(): Promise<void> {
  render(<CompetitorScreen />);
  await waitFor(() => {
    expect(mOptions).toHaveBeenCalled();
  });
  // options の反映を待つ
  await screen.findByRole('option', { name: '施設長' });
}

function chooseFile(file: File = csv()): void {
  const input = screen.getByLabelText('求人一覧CSV');
  fireEvent.change(input, { target: { files: [file] } });
}

const submitButton = (): HTMLButtonElement =>
  screen.getByRole<HTMLButtonElement>('button', { name: 'レポートを作成' });

beforeEach(() => {
  vi.clearAllMocks();
  mOptions.mockResolvedValue({ ok: true, data: OPTIONS });
  try {
    sessionStorage.clear();
  } catch {
    // ignore
  }
  window.history.replaceState(null, '', '/app/competitor');
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('フォーム', () => {
  it('選択肢を options API から出す。既定値は旧画面と同じ', async () => {
    await mount();
    const pref = screen.getByLabelText('対象都道府県');
    expect(within(pref).getAllByRole('option').map((o) => o.textContent)).toEqual([
      '全国',
      '北海道',
      '大阪府',
    ]);
    expect((screen.getByLabelText<HTMLInputElement>('先頭の求人を何件比較するか')).value).toBe('45');
    expect((screen.getByLabelText<HTMLSelectElement>('CSVの媒体')).value).toBe('indeed_sp');
    expect((screen.getByLabelText<HTMLSelectElement>('給与の表示単位')).value).toBe('monthly');
    expect(
      (screen.getByLabelText<HTMLInputElement>('Google広告APIの検索需要・関連キーワードを取得する'))
        .checked,
    ).toBe(true);
  });

  it('market_available=false なら職種 select を無効にして理由を出す', async () => {
    mOptions.mockResolvedValue({ ok: true, data: { ...OPTIONS, titles: [], market_available: false } });
    render(<CompetitorScreen />);
    await waitFor(() => {
      expect((screen.getByLabelText<HTMLSelectElement>('Indeed採用市場の職種')).disabled).toBe(true);
    });
    expect(screen.getByText(/Indeed採用市場データは取得できません/)).toBeTruthy();
  });

  it('options が取れなくても (失敗) フォームは使える', async () => {
    mOptions.mockResolvedValue({ ok: false, error: new ApiNetworkError('x') });
    render(<CompetitorScreen />);
    await waitFor(() => {
      expect(mOptions).toHaveBeenCalled();
    });
    expect(screen.getByRole('button', { name: 'レポートを作成' })).toBeTruthy();
    expect(screen.getByText(/選択肢を取得できませんでした/)).toBeTruthy();
  });

  it('検証エラーのときは送信せず、項目ごとに alert を出す', async () => {
    await mount();
    fireEvent.click(submitButton());
    expect(mCreate).not.toHaveBeenCalled();
    const alerts = screen.getAllByRole('alert').map((a) => a.textContent);
    expect(alerts.some((t) => t.includes('CSV'))).toBe(true);
    chooseFile(csv('x.xlsx'));
    fireEvent.change(screen.getByLabelText('先頭の求人を何件比較するか'), { target: { value: '0' } });
    fireEvent.click(submitButton());
    expect(mCreate).not.toHaveBeenCalled();
    const text = screen.getAllByRole('alert').map((a) => a.textContent).join('|');
    expect(text).toContain('.csv');
    expect(text).toContain('1〜200');
  });

  it('0 バイトの CSV は選んだ時点で指摘し、送信しない', async () => {
    await mount();
    chooseFile(new File([], 'empty.csv'));
    expect(screen.getByRole('alert').textContent).toContain('空');
    fireEvent.click(submitButton());
    expect(mCreate).not.toHaveBeenCalled();
  });

  it('選んだファイル名を表示する', async () => {
    await mount();
    chooseFile(csv('競合_大阪.csv'));
    expect(screen.getByText(/競合_大阪\.csv/)).toBeTruthy();
  });
});

describe('送信中と二重送信', () => {
  it('送信中は 2 つのボタンが無効で、進捗文とキャンセルが出る', async () => {
    const d = deferred<CreateResult>();
    mCreate.mockReturnValue(d.promise);
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain('レポートを作成しています');
    });
    expect(screen.getByRole('status').textContent).toContain('Google');
    expect(submitButton().disabled).toBe(true);
    expect(screen.getByRole('button', { name: 'キャンセル' })).toBeTruthy();
    await act(async () => {
      d.resolve({ ok: true, data: makeResponse() });
      await d.promise;
    });
  });

  it('同じ tick で submit が 2 回来ても createReport は 1 回だけ (ref で守る)', async () => {
    const d = deferred<CreateResult>();
    mCreate.mockReturnValue(d.promise);
    await mount();
    chooseFile();
    const form = submitButton().closest('form');
    if (!form) throw new Error('form not found');
    act(() => {
      form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
      form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });
    expect(mCreate).toHaveBeenCalledTimes(1);
    await act(async () => {
      d.resolve({ ok: true, data: makeResponse() });
      await d.promise;
    });
    expect(mCreate).toHaveBeenCalledTimes(1);
  });

  it('連打 (click 3 回) でも 1 回', async () => {
    const d = deferred<CreateResult>();
    mCreate.mockReturnValue(d.promise);
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    fireEvent.click(submitButton());
    fireEvent.click(submitButton());
    expect(mCreate).toHaveBeenCalledTimes(1);
    await act(async () => {
      d.resolve({ ok: true, data: makeResponse() });
      await d.promise;
    });
  });

  it('キャンセルすると中断シグナルが立ち、入力は残り、再送信できる', async () => {
    let seen: AbortSignal | undefined;
    mCreate.mockImplementation(
      (_form, opts) =>
        new Promise<CreateResult>((resolve) => {
          seen = opts?.signal;
          opts?.signal?.addEventListener('abort', () => {
            resolve({ ok: false, error: new ApiAbortedError('aborted') });
          });
        }),
    );
    await mount();
    chooseFile(csv('keep.csv'));
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: '保持される調査名' } });
    fireEvent.click(submitButton());
    fireEvent.click(await screen.findByRole('button', { name: 'キャンセル' }));
    await waitFor(() => {
      expect(submitButton().disabled).toBe(false);
    });
    expect(seen?.aborted).toBe(true);
    expect(screen.getByRole('status').textContent).toContain('キャンセル');
    expect((screen.getByLabelText<HTMLInputElement>('調査名')).value).toBe('保持される調査名');
    expect(screen.getByText(/keep\.csv/)).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

describe('成功', () => {
  it('旧画面と同じ FormData で送り、4 タブの結果を出す', async () => {
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    await mount();
    chooseFile(csv('jobs.csv'));
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: '大阪・施設長' } });
    fireEvent.change(screen.getByLabelText('対象都道府県'), { target: { value: '大阪府' } });
    fireEvent.change(screen.getByLabelText('Indeed採用市場の職種'), { target: { value: '施設長' } });
    fireEvent.change(screen.getByLabelText('給与の表示単位'), { target: { value: 'hourly' } });
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(mCreate).toHaveBeenCalledTimes(1);
    const form = mCreate.mock.calls[0]?.[0];
    expect(form?.get('survey_title')).toBe('大阪・施設長');
    expect(form?.get('prefecture')).toBe('大阪府');
    expect(form?.get('market_title')).toBe('施設長');
    expect(form?.get('wage_mode')).toBe('hourly');
    expect(form?.get('top_n')).toBe('45');
    expect(form?.get('include_google')).toBe('1');
    expect((form?.get('csv_file') as File).name).toBe('jobs.csv');
    expect(screen.getAllByRole('tab')).toHaveLength(5);
    expect(screen.getByRole('button', { name: 'PDFをダウンロード' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '条件を変えて再作成' })).toBeTruthy();
  });

  it('結果の上部に調査条件の要約 (件数・単位) を出す', async () => {
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    const head = screen.getByTestId('result-summary').textContent;
    expect(head).toContain('テスト調査');
    expect(head).toContain('1,234');
    expect(head).toContain('万円');
  });

  it('warnings と、top_n の調整を結果に出す', async () => {
    const report = makeReport();
    report.meta.warnings = ['表示件数の指定を 200 件に調整しました (指定できるのは 1〜200 の整数です)。'];
    mCreate.mockResolvedValue({ ok: true, data: makeResponse(report) });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(screen.getByText(/200 件に調整しました/)).toBeTruthy();
  });

  it('給与が 1 件も読めなかったときは警告 (0 件の給与表を黙って出さない)', async () => {
    const report = makeReport();
    report.meta.salary_parsed_count = 0;
    report.meta.salary_missing_count = 1234;
    mCreate.mockResolvedValue({ ok: true, data: makeResponse(report) });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(screen.getByText(/給与を読み取れた求人が 0 件/)).toBeTruthy();
  });

  it('「条件を変えて再作成」でフォームに戻り、入力が残っている', async () => {
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    await mount();
    chooseFile(csv('again.csv'));
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: '再作成の調査名' } });
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    fireEvent.click(screen.getByRole('button', { name: '条件を変えて再作成' }));
    expect(screen.queryByRole('tablist')).toBeNull();
    expect((screen.getByLabelText<HTMLInputElement>('調査名')).value).toBe('再作成の調査名');
    expect(screen.getByText(/again\.csv/)).toBeTruthy();
  });

  it('タブの選択は URL の ?tab= に入り、最初の表示は URL から決まる', async () => {
    window.history.replaceState(null, '', '/app/competitor?tab=indeed');
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(screen.getByRole('tab', { name: 'Indeed採用レポート' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    fireEvent.click(screen.getByRole('tab', { name: 'Google検索需要' }));
    expect(window.location.search).toBe('?tab=google');
  });

  it('不正な ?tab= は無視して給与・待遇', async () => {
    window.history.replaceState(null, '', '/app/competitor?tab=__proto__');
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(screen.getByRole('tab', { name: '給与・待遇' }).getAttribute('aria-selected')).toBe('true');
  });
});

describe('エラー', () => {
  it('401: 入力と CSV を保持し、別タブのログインを案内する (画面遷移しない)', async () => {
    mCreate.mockResolvedValue({ ok: false, error: new AuthRequiredError('x') });
    await mount();
    chooseFile(csv('auth.csv'));
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: '消えない調査名' } });
    fireEvent.change(screen.getByLabelText('先頭の求人を何件比較するか'), { target: { value: '30' } });
    fireEvent.click(submitButton());
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('ログイン');
    const link = within(alert).getByRole('link', { name: /ログイン/ });
    expect(link.getAttribute('href')).toBe('/login');
    expect(link.getAttribute('target')).toBe('_blank');
    expect(link.getAttribute('rel')).toContain('noopener');
    // 入力は全部残っている
    expect((screen.getByLabelText<HTMLInputElement>('調査名')).value).toBe('消えない調査名');
    expect((screen.getByLabelText<HTMLInputElement>('先頭の求人を何件比較するか')).value).toBe('30');
    expect(screen.getByText(/auth\.csv/)).toBeTruthy();
    // 再ログイン後にもう一度押せる
    expect(submitButton().disabled).toBe(false);
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(mCreate).toHaveBeenCalledTimes(2);
  });

  it('429: 前のレポートを作成中 (サーバの同時送信制限)', async () => {
    mCreate.mockResolvedValue({
      ok: false,
      error: new ApiHttpError(429, { error: 'report_in_progress', message: '前のレポートを作成中です。' }),
    });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('作成中');
    expect(submitButton().disabled).toBe(false);
  });

  it.each([
    [422, 'no_indeed_jobs', 'Indeed求人が見つかりません'],
    [422, 'csv_parse_failed', 'CSVを解析できません'],
    [422, 'csv_too_many_rows', '行数が上限'],
    [400, 'invalid_prefecture', '都道府県が正しくありません'],
    [413, 'csv_too_large', 'サイズが上限'],
  ] as const)('%i %s: サーバの固定文を表示し、入力は残る', async (status, code, message) => {
    mCreate.mockResolvedValue({ ok: false, error: new ApiHttpError(status, { error: code, message }) });
    await mount();
    chooseFile(csv('e.csv'));
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: 'エラー後も残る' } });
    fireEvent.click(submitButton());
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain(message);
    expect((screen.getByLabelText<HTMLInputElement>('調査名')).value).toBe('エラー後も残る');
    expect(screen.queryByRole('tablist')).toBeNull();
  });

  it('タイムアウト・通信失敗・500', async () => {
    await mount();
    chooseFile();
    mCreate.mockResolvedValue({ ok: false, error: new ApiTimeoutError(150_000) });
    fireEvent.click(submitButton());
    expect((await screen.findByRole('alert')).textContent).toContain('時間');
    mCreate.mockResolvedValue({ ok: false, error: new ApiNetworkError('x') });
    fireEvent.click(submitButton());
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('通信');
    });
    mCreate.mockResolvedValue({ ok: false, error: new ApiHttpError(500) });
    fireEvent.click(submitButton());
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('作成できませんでした');
    });
  });

  it('エラーの後にもう一度押せる (ref が戻る)', async () => {
    mCreate.mockResolvedValueOnce({ ok: false, error: new ApiHttpError(500) });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('alert');
    mCreate.mockResolvedValueOnce({ ok: true, data: makeResponse() });
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
    expect(mCreate).toHaveBeenCalledTimes(2);
  });
});

describe('PDF', () => {
  async function toResult(): Promise<void> {
    mCreate.mockResolvedValue({ ok: true, data: makeResponse() });
    await mount();
    chooseFile();
    fireEvent.click(submitButton());
    await screen.findByRole('tablist');
  }
  const pdfButton = (): HTMLButtonElement =>
    screen.getByRole<HTMLButtonElement>('button', { name: 'PDFをダウンロード' });

  function stubDownload(): { clicks: { download: string; href: string }[] } {
    const clicks: { download: string; href: string }[] = [];
    vi.stubGlobal(
      'URL',
      Object.assign(URL, {
        createObjectURL: vi.fn(() => 'blob:test'),
        revokeObjectURL: vi.fn(),
      }),
    );
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      clicks.push({ download: this.download, href: this.href });
    });
    return { clicks };
  }

  it('report_id で PDF を作り、サーバが返したファイル名で保存する', async () => {
    const saved = stubDownload();
    mPdf.mockResolvedValue({
      ok: true,
      blob: new Blob(['%PDF-1.7 %%EOF'], { type: 'application/pdf' }),
      filename: '競合調査_大阪府_2026-10-05.pdf',
    });
    await toResult();
    fireEvent.click(pdfButton());
    await waitFor(() => {
      expect(saved.clicks).toHaveLength(1);
    });
    expect(mPdf.mock.calls[0]?.[0]).toBe('a'.repeat(64));
    expect(saved.clicks[0]?.download).toBe('競合調査_大阪府_2026-10-05.pdf');
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain('ダウンロードしました');
    });
    // レポート本体は残る
    expect(screen.getByRole('tablist')).toBeTruthy();
  });

  it('作成中は PDF ボタンを無効にし、連打しても 1 回', async () => {
    stubDownload();
    const d = deferred<PdfResult>();
    mPdf.mockReturnValue(d.promise);
    await toResult();
    fireEvent.click(pdfButton());
    fireEvent.click(pdfButton());
    fireEvent.click(pdfButton());
    expect(mPdf).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(pdfButton().disabled).toBe(true);
    });
    expect(screen.getByRole('status').textContent).toContain('PDF');
    expect((screen.getByRole<HTMLButtonElement>('button', { name: '条件を変えて再作成' })).disabled).toBe(true);
    await act(async () => {
      d.resolve({ ok: false, kind: 'server', message: '失敗' });
      await d.promise;
    });
    await waitFor(() => {
      expect(pdfButton().disabled).toBe(false);
    });
  });

  it('404 (期限切れ): もう一度作成してください。結果は消えない', async () => {
    mPdf.mockResolvedValue({
      ok: false,
      kind: 'expired',
      message: 'このレポートの保持期限が切れました。もう一度作成してください。',
    });
    await toResult();
    fireEvent.click(pdfButton());
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('もう一度作成してください');
    expect(screen.getByRole('tablist')).toBeTruthy();
  });

  it('不完全な PDF は保存しない', async () => {
    const saved = stubDownload();
    mPdf.mockResolvedValue({ ok: false, kind: 'incomplete', message: 'PDFファイルが不完全です。再度お試しください。' });
    await toResult();
    fireEvent.click(pdfButton());
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('不完全');
    expect(saved.clicks).toHaveLength(0);
  });

  it('401: 入力・結果を保持してログインを案内', async () => {
    mPdf.mockResolvedValue({ ok: false, kind: 'auth', message: 'ログインの有効期限が切れました。' });
    await toResult();
    fireEvent.click(pdfButton());
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByRole('link', { name: /ログイン/ }).getAttribute('href')).toBe('/login');
    expect(screen.getByRole('tablist')).toBeTruthy();
  });

  it('エラー後に再試行できる', async () => {
    const saved = stubDownload();
    mPdf.mockResolvedValueOnce({ ok: false, kind: 'busy', message: 'PDF作成が混み合っています。' });
    await toResult();
    fireEvent.click(pdfButton());
    await screen.findByRole('alert');
    mPdf.mockResolvedValueOnce({
      ok: true,
      blob: new Blob(['%PDF-1.7 %%EOF'], { type: 'application/pdf' }),
      filename: 'competitor-report.pdf',
    });
    fireEvent.click(pdfButton());
    await waitFor(() => {
      expect(saved.clicks).toHaveLength(1);
    });
    expect(mPdf).toHaveBeenCalledTimes(2);
  });
});

describe('入力の保存 (リロード対策)', () => {
  it('CSV 以外の入力を sessionStorage に保存し、再表示で復元する', async () => {
    await mount();
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: '復元される' } });
    fireEvent.change(screen.getByLabelText('対象都道府県'), { target: { value: '大阪府' } });
    cleanup();
    await mount();
    expect((screen.getByLabelText<HTMLInputElement>('調査名')).value).toBe('復元される');
    expect((screen.getByLabelText<HTMLSelectElement>('対象都道府県')).value).toBe('大阪府');
  });

  it('sessionStorage が使えなくても動く', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    await mount();
    fireEvent.change(screen.getByLabelText('調査名'), { target: { value: 'x' } });
    expect((screen.getByLabelText<HTMLInputElement>('調査名')).value).toBe('x');
    vi.restoreAllMocks();
  });
});
