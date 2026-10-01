// @vitest-environment happy-dom
// The session filters (header bar) are the source of the prefecture / municipality of this screen.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as fx from './fixtures';
import { makeFiltersProvider } from './testFilters';
import { urlOf } from './testUtils';
import { RecruitmentDiag } from './RecruitmentDiagScreen';
import type { FiltersCurrent } from '../../shell/types';

vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string }) => <div data-testid={props.testId} />,
}));

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

const prefectures = [
  { name: '北海道', prefcode: 1 },
  { name: '東京都', prefcode: 13 },
];
const CITIES: Record<string, { name: string; citycode: number }[]> = {
  東京都: [
    { name: '千代田区', citycode: 13101 },
    { name: '新宿区', citycode: 13104 },
  ],
  北海道: [{ name: '札幌市', citycode: 1100 }],
};
const OK_BODIES: Record<string, unknown> = {
  difficulty: fx.difficulty,
  talent_pool: fx.talentPool,
  competitors: fx.competitors,
  condition_gap: fx.conditionGap,
  market_trend: fx.marketTrend,
  opportunity_map: fx.opportunity,
  insights: fx.insights,
  talent_pool_expansion: fx.expansion,
};

let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>((input) => {
    const url = new URL(urlOf(input), 'http://localhost');
    if (url.pathname === '/api/app/geo/prefectures') return Promise.resolve(json(prefectures));
    if (url.pathname === '/api/app/geo/municipalities') {
      return Promise.resolve(json(CITIES[url.searchParams.get('prefecture') ?? ''] ?? []));
    }
    return Promise.resolve(json(OK_BODIES[url.pathname.split('/').pop() ?? '']));
  });
  vi.stubGlobal('fetch', fetchMock);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const diagUrls = (): string[] =>
  fetchMock.mock.calls.map(([u]) => urlOf(u)).filter((u) => u.startsWith('/api/recruitment_diag/'));

function renderDiag(initial: Partial<FiltersCurrent> = {}, options: { syncing?: boolean } = {}) {
  const h = makeFiltersProvider(initial, options);
  const view = render(
    <h.Provider>
      <RecruitmentDiag />
    </h.Provider>,
  );
  return { ...h, view };
}

const sel = (id: string): HTMLSelectElement => screen.getByTestId<HTMLSelectElement>(id);

async function waitCities(n: number): Promise<void> {
  await waitFor(() => {
    expect(sel('rd-form-city').querySelectorAll('option')).toHaveLength(n);
  });
}

describe('session filters are the source of the form', () => {
  it('starts with the session prefecture and municipality selected', async () => {
    renderDiag({ prefecture: '東京都', municipality: '千代田区' });
    await waitCities(3);
    expect(sel('rd-form-pref').value).toBe('東京都');
    expect(sel('rd-form-city').value).toBe('千代田区');
  });

  it('the session municipality reaches the requests with its citycode', async () => {
    renderDiag({ prefecture: '東京都', municipality: '千代田区' });
    await waitCities(3);
    fireEvent.change(sel('rd-form-job-type'), { target: { value: '小売業' } });
    fireEvent.click(screen.getByTestId('rd-run'));
    await waitFor(() => {
      expect(screen.getByTestId('rd-global-status').textContent).toBe('診断完了');
    });
    const urls = diagUrls();
    expect(urls).toHaveLength(8);
    expect(urls.every((u) => u.includes('prefcode=13') && u.includes('citycode=13101'))).toBe(true);
    expect(urls.every((u) => u.includes('municipality=%E5%8D%83%E4%BB%A3%E7%94%B0%E5%8C%BA'))).toBe(true);
  });

  it('choosing "すべて" clears the session municipality and no request carries municipality / citycode', async () => {
    const { spy } = renderDiag({ prefecture: '東京都', municipality: '千代田区' });
    await waitCities(3);
    fireEvent.change(sel('rd-form-city'), { target: { value: '' } });
    expect(spy.setMunicipality).toHaveBeenCalledWith('');
    expect(sel('rd-form-city').value).toBe('');
    fireEvent.change(sel('rd-form-job-type'), { target: { value: '小売業' } });
    fireEvent.click(screen.getByTestId('rd-run'));
    await waitFor(() => {
      expect(screen.getByTestId('rd-global-status').textContent).toBe('診断完了');
    });
    const urls = diagUrls();
    expect(urls).toHaveLength(8);
    for (const u of urls) {
      expect(u, u).not.toContain('municipality=');
      expect(u, u).not.toContain('citycode=');
      expect(u, u).toContain('prefcode=13');
    }
  });

  it('changing the prefecture / municipality on the screen writes them to the session', async () => {
    const { spy } = renderDiag({ prefecture: '東京都' });
    await waitCities(3);
    fireEvent.change(sel('rd-form-city'), { target: { value: '新宿区' } });
    expect(spy.setMunicipality).toHaveBeenLastCalledWith('新宿区');
    fireEvent.change(sel('rd-form-pref'), { target: { value: '北海道' } });
    expect(spy.setPrefecture).toHaveBeenLastCalledWith('北海道');
    await waitCities(2);
    expect(sel('rd-form-pref').value).toBe('北海道');
    expect(sel('rd-form-city').value).toBe('');
  });

  it('a change made in the header filter bar shows up in the form (and reloads the city list)', async () => {
    const { headerSet } = renderDiag({ prefecture: '東京都', municipality: '千代田区' });
    await waitCities(3);
    act(() => {
      headerSet({ prefecture: '北海道', municipality: '' });
    });
    await waitFor(() => {
      expect(sel('rd-form-pref').value).toBe('北海道');
    });
    await waitCities(2);
    expect(sel('rd-form-city').textContent).toContain('札幌市');
    act(() => {
      headerSet({ municipality: '札幌市' });
    });
    expect(sel('rd-form-city').value).toBe('札幌市');
  });

  it('a session municipality that is not in the list is still shown (what is displayed is what is sent)', async () => {
    renderDiag({ prefecture: '東京都', municipality: '存在しない区' });
    await waitCities(4);
    expect(sel('rd-form-city').value).toBe('存在しない区');
  });

  it('the run button waits while a filter save is in flight', async () => {
    renderDiag({ prefecture: '東京都' }, { syncing: true });
    await waitCities(3);
    expect(screen.getByTestId<HTMLButtonElement>('rd-run').disabled).toBe(true);
  });
});
