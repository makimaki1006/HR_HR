// @vitest-environment happy-dom
// State races. The fetch mocks below deliberately IGNORE the AbortSignal: a response that was
// already on its way (or one the browser delivered a moment before the abort) must still not
// overwrite a newer state.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as fx from './fixtures';
import { makeFiltersProvider } from './testFilters';
import { urlOf } from './testUtils';
import { RecruitmentDiag } from './RecruitmentDiagScreen';

vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string }) => <div data-testid={props.testId} />,
}));

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

interface Deferred {
  resolve: (r: Response) => void;
  promise: Promise<Response>;
}
function deferred(): Deferred {
  let resolve!: (r: Response) => void;
  const promise = new Promise<Response>((r) => {
    resolve = r;
  });
  return { resolve, promise };
}

let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const prefList = [
  { name: '北海道', prefcode: 1 },
  { name: '東京都', prefcode: 13 },
];

function renderDiag() {
  const { Provider } = makeFiltersProvider();
  return render(
    <Provider>
      <RecruitmentDiag />
    </Provider>,
  );
}

describe('state races', () => {
  it('a slow municipality list for prefecture A does not replace the list of the later prefecture B', async () => {
    const slowA = deferred();
    fetchMock = vi.fn<typeof fetch>((input) => {
      const url = new URL(urlOf(input), 'http://localhost');
      if (url.pathname === '/api/app/geo/prefectures') return Promise.resolve(json(prefList));
      if (url.pathname === '/api/app/geo/municipalities') {
        if (url.searchParams.get('prefecture') === '北海道') return slowA.promise;
        return Promise.resolve(json([{ name: '新宿区', citycode: 13104 }]));
      }
      return Promise.reject(new Error('unexpected'));
    });
    vi.stubGlobal('fetch', fetchMock);
    renderDiag();
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-pref').querySelectorAll('option')).toHaveLength(3);
    });
    fireEvent.change(screen.getByTestId('rd-form-pref'), { target: { value: '北海道' } });
    fireEvent.change(screen.getByTestId('rd-form-pref'), { target: { value: '東京都' } });
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-city').textContent).toContain('新宿区');
    });
    await act(async () => {
      slowA.resolve(json([{ name: '札幌市', citycode: 1100 }]));
      await Promise.resolve();
    });
    expect(screen.getByTestId('rd-form-city').textContent).toContain('新宿区');
    expect(screen.getByTestId('rd-form-city').textContent).not.toContain('札幌市');
  });

  it('a second click on the run button while a run is in flight sends no new requests', async () => {
    const hold = deferred();
    fetchMock = vi.fn<typeof fetch>((input) => {
      const url = new URL(urlOf(input), 'http://localhost');
      if (url.pathname === '/api/app/geo/prefectures') return Promise.resolve(json(prefList));
      if (url.pathname === '/api/app/geo/municipalities') return Promise.resolve(json([]));
      return hold.promise;
    });
    vi.stubGlobal('fetch', fetchMock);
    renderDiag();
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-pref').querySelectorAll('option')).toHaveLength(3);
    });
    fireEvent.change(screen.getByTestId('rd-form-job-type'), { target: { value: '小売業' } });
    fireEvent.change(screen.getByTestId('rd-form-pref'), { target: { value: '東京都' } });
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-city').querySelectorAll('option')).toHaveLength(1);
    });
    fireEvent.click(screen.getByTestId('rd-run'));
    fireEvent.click(screen.getByTestId('rd-run'));
    const diag = (): number =>
      fetchMock.mock.calls.filter(([u]) => urlOf(u).startsWith('/api/recruitment_diag/')).length;
    expect(diag()).toBe(8);
    await act(async () => {
      hold.resolve(json({}));
      await Promise.resolve();
    });
  });

  it('responses that arrive after unmount neither throw nor log a React error', async () => {
    const hold = deferred();
    fetchMock = vi.fn<typeof fetch>((input) => {
      const url = new URL(urlOf(input), 'http://localhost');
      if (url.pathname === '/api/app/geo/prefectures') return Promise.resolve(json(prefList));
      if (url.pathname === '/api/app/geo/municipalities') return Promise.resolve(json([]));
      return hold.promise;
    });
    vi.stubGlobal('fetch', fetchMock);
    const view = renderDiag();
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-pref').querySelectorAll('option')).toHaveLength(3);
    });
    fireEvent.change(screen.getByTestId('rd-form-job-type'), { target: { value: '小売業' } });
    fireEvent.change(screen.getByTestId('rd-form-pref'), { target: { value: '東京都' } });
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-city').querySelectorAll('option')).toHaveLength(1);
    });
    fireEvent.click(screen.getByTestId('rd-run'));
    view.unmount();
    await act(async () => {
      hold.resolve(json(fx.difficulty));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(console.error).not.toHaveBeenCalled();
  });
});
