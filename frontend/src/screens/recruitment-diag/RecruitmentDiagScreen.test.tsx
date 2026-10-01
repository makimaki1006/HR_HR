// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GeoMunicipalityOption } from '../../generated/GeoMunicipalityOption';
import type { GeoPrefectureOption } from '../../generated/GeoPrefectureOption';
import * as fx from './fixtures';
import { makeFiltersProvider } from './testFilters';
import { urlOf } from './testUtils';
import { RecruitmentDiag } from './RecruitmentDiagScreen';

vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string }) => <div data-testid={props.testId} />,
}));

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

const prefectures = [
  { name: '北海道', prefcode: 1 },
  { name: '東京都', prefcode: 13 },
] satisfies GeoPrefectureOption[];
const municipalities = [
  { name: '新宿区', citycode: 13104 },
  { name: '渋谷区', citycode: 13113 },
] satisfies GeoMunicipalityOption[];

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

function renderDiag() {
  const { Provider } = makeFiltersProvider();
  return render(
    <Provider>
      <RecruitmentDiag />
    </Provider>,
  );
}

let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>((input) => {
    const url = new URL(urlOf(input), 'http://localhost');
    if (url.pathname === '/api/app/geo/prefectures') return Promise.resolve(json(prefectures));
    if (url.pathname === '/api/app/geo/municipalities') return Promise.resolve(json(municipalities));
    const name = url.pathname.split('/').pop() ?? '';
    // difficulty fails with HTTP 500, everything else answers normally
    if (name === 'difficulty') return Promise.resolve(json({ error: 'x' }, 500));
    return Promise.resolve(json(OK_BODIES[name]));
  });
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const diagUrls = (): string[] =>
  fetchMock.mock.calls.map(([u]) => urlOf(u)).filter((u) => u.startsWith('/api/recruitment_diag/'));

describe('RecruitmentDiag screen', () => {
  it('shows the scope notes at the top and the panels as idle before the first run', () => {
    renderDiag();
    expect(screen.getByTestId('rd-scope-notes').textContent).toContain('ハローワーク掲載求人のみが対象');
    expect(screen.getByTestId('rd-scope-notes').textContent).toContain('因果関係を示すものではありません');
    expect(screen.getByTestId('rd-panel-difficulty').getAttribute('data-status')).toBe('idle');
    expect(screen.getByTestId('rd-panel-difficulty-status').textContent).toBe('待機中');
    expect(screen.getByTestId('rd-results').className).toContain('hidden');
    expect(screen.getByTestId<HTMLSelectElement>('rd-form-emp-type').value).toBe('正社員');
  });

  it('stops with an on-screen message (no alert, no request) when the industry or prefecture is missing', async () => {
    const alertSpy = vi.fn();
    vi.stubGlobal('alert', alertSpy);
    renderDiag();
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-pref').querySelectorAll('option')).toHaveLength(3);
    });

    fireEvent.click(screen.getByTestId('rd-run'));
    expect(screen.getByTestId('rd-form-message').textContent).toBe('業種を選択してください');

    fireEvent.change(screen.getByTestId('rd-form-job-type'), { target: { value: '小売業' } });
    fireEvent.click(screen.getByTestId('rd-run'));
    expect(screen.getByTestId('rd-form-message').textContent).toBe('都道府県を選択してください');

    expect(diagUrls()).toHaveLength(0);
    expect(alertSpy).not.toHaveBeenCalled();
    expect(screen.getByTestId('rd-panel-difficulty').getAttribute('data-status')).toBe('idle');
  });

  it('loads cities for the chosen prefecture, sends prefcode/citycode, and isolates the failing panel', async () => {
    renderDiag();
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-pref').querySelectorAll('option')).toHaveLength(3);
    });
    fireEvent.change(screen.getByTestId('rd-form-job-type'), { target: { value: '老人福祉・介護' } });
    fireEvent.change(screen.getByTestId('rd-form-pref'), { target: { value: '東京都' } });
    await waitFor(() => {
      // 先頭「すべて（都道府県全体）」 + 2 municipalities
      expect(screen.getByTestId('rd-form-city').querySelectorAll('option')).toHaveLength(3);
    });
    expect(
      fetchMock.mock.calls.some(
        ([u]) => urlOf(u) === '/api/app/geo/municipalities?prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD',
      ),
    ).toBe(true);
    expect(screen.getByTestId('rd-form-city').querySelector('option')?.textContent).toBe('すべて（都道府県全体）');
    fireEvent.change(screen.getByTestId('rd-form-city'), { target: { value: '新宿区' } });
    fireEvent.change(screen.getByTestId('rd-form-own-salary'), { target: { value: '25' } });

    fireEvent.click(screen.getByTestId('rd-run'));
    await waitFor(() => {
      expect(screen.getByTestId('rd-global-status').textContent).toBe('診断完了');
    });

    const urls = diagUrls();
    expect(urls).toHaveLength(8);
    expect(urls.every((u) => u.includes('prefcode=13') && u.includes('citycode=13104'))).toBe(true);
    expect(urls.find((u) => u.includes('/condition_gap'))).toContain('company_salary_min=250000');
    expect(urls.find((u) => u.includes('/competitors'))).toContain('&limit=100');

    // difficulty failed with HTTP 500: error state, the others are done with real values
    expect(screen.getByTestId('rd-panel-difficulty').getAttribute('data-status')).toBe('error');
    expect(screen.getByTestId('rd-panel-difficulty-status').textContent).toBe('取得失敗');
    expect(screen.getByTestId('rd-panel-difficulty').textContent).toContain('❌ 取得失敗: HTTP 500');
    for (const name of [
      'talent_pool',
      'inflow',
      'competitors',
      'condition_gap',
      'market_trend',
      'opportunity_map',
      'insights',
      'talent_pool_expansion',
    ]) {
      expect(screen.getByTestId(`rd-panel-${name}`).getAttribute('data-status'), name).toBe('done');
    }
    expect(screen.getByTestId('rd-panel-inflow-status').textContent).toBe('開発中');
    expect(screen.getByTestId('rd-panel-competitors-status').textContent).toBe('完了（2社）');
    expect(screen.getByTestId('rd-talent_pool-metrics-day_population').textContent).toBe('1,234,567');
    expect(screen.getByTestId('rd-results').className).not.toContain('hidden');
    expect(screen.queryByTestId('rd-initial-msg')).toBeNull();
  });

  it('a panel whose body throws while rendering becomes that panel error; the others keep their values', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const base = fetchMock.getMockImplementation();
    fetchMock.mockImplementation((input, init) => {
      // condition_gap parses fine as an object but is missing industry_median: GapBox throws on mount
      if (urlOf(input).startsWith('/api/recruitment_diag/condition_gap')) return Promise.resolve(json({ interpretation: 'x' }));
      return base ? base(input, init) : Promise.reject(new Error('no base'));
    });
    renderDiag();
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-pref').querySelectorAll('option')).toHaveLength(3);
    });
    fireEvent.change(screen.getByTestId('rd-form-job-type'), { target: { value: '小売業' } });
    fireEvent.change(screen.getByTestId('rd-form-pref'), { target: { value: '東京都' } });
    await waitFor(() => {
      expect(screen.getByTestId('rd-form-city').querySelectorAll('option')).toHaveLength(3);
    });
    fireEvent.click(screen.getByTestId('rd-run'));
    await waitFor(() => {
      expect(screen.getByTestId('rd-global-status').textContent).toBe('診断完了');
    });
    expect(screen.getByTestId('rd-panel-condition_gap').getAttribute('data-status')).toBe('error');
    expect(screen.getByTestId('rd-panel-condition_gap').textContent).toContain('❌ 取得失敗:');
    expect(screen.getByTestId('rd-panel-insights').getAttribute('data-status')).toBe('done');
    expect(screen.getByTestId('rd-talent_pool-metrics-day_population').textContent).toBe('1,234,567');
  });
});
