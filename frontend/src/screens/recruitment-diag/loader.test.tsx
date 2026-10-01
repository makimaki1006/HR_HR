// @vitest-environment happy-dom
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as fx from './fixtures';
import { urlOf } from './testUtils';
import { loadAllPanels, PANELS, type PanelName } from './loader';
import type { PanelView } from './panels';
import { EMPTY_FORM, type DiagnosisForm } from './query';

vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string }) => <div data-testid={props.testId} />,
}));

const form: DiagnosisForm = {
  ...EMPTY_FORM,
  jobType: '老人福祉・介護',
  prefecture: '東京都',
  prefcode: 13,
  municipality: '新宿区',
  citycode: 13104,
  ownSalaryMan: '25',
};

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

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

/** Answers each panel endpoint from OK_BODIES unless `override` returns something for it. */
function stubFetch(override: Partial<Record<PanelName, () => Response>>): void {
  fetchMock = vi.fn<typeof fetch>((input) => {
    const path = new URL(urlOf(input), 'http://localhost').pathname.split('/').pop() as PanelName;
    const custom = override[path];
    if (custom) return Promise.resolve(custom());
    return Promise.resolve(json(OK_BODIES[path]));
  });
  vi.stubGlobal('fetch', fetchMock);
}

beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

async function loadAll(): Promise<Partial<Record<PanelName, PanelView>>> {
  const views: Partial<Record<PanelName, PanelView>> = {};
  await loadAllPanels(form, new AbortController().signal, (name, view) => {
    views[name] = view;
  });
  return views;
}

describe('loadAllPanels', () => {
  it('requests 8 endpoints (not inflow) with the right query strings', async () => {
    stubFetch({});
    const views = await loadAll();
    const urls = fetchMock.mock.calls.map(([u]) => urlOf(u));
    expect(urls).toHaveLength(8);
    expect(urls.some((u) => u.includes('/inflow'))).toBe(false);
    const common =
      'job_type=%E8%80%81%E4%BA%BA%E7%A6%8F%E7%A5%89%E3%83%BB%E4%BB%8B%E8%AD%B7&emp_type=%E6%AD%A3%E7%A4%BE%E5%93%A1' +
      '&prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD&municipality=%E6%96%B0%E5%AE%BF%E5%8C%BA&prefcode=13&citycode=13104';
    expect(urls).toContain(`/api/recruitment_diag/difficulty?${common}`);
    expect(urls).toContain(`/api/recruitment_diag/competitors?${common}&limit=100`);
    expect(urls).toContain(`/api/recruitment_diag/condition_gap?${common}&company_salary_min=250000`);
    expect(views.inflow?.statusText).toBe('開発中');
    expect(Object.keys(views)).toHaveLength(PANELS.length);
    for (const name of Object.keys(OK_BODIES) as PanelName[]) {
      expect(views[name]?.status, name).toBe('done');
    }
  });

  it('an HTTP 500, an {"error"} body, a network failure and a render exception stay inside their own panels', async () => {
    stubFetch({
      difficulty: () => json({ error: 'boom' }, 500),
      talent_pool: () => json({ error: 'DB未接続', notes: { hw_scope: 'x', causation: 'y' } }),
      competitors: () => {
        throw new TypeError('network down');
      },
      // no months -> the renderer function itself throws a TypeError
      market_trend: () => json({ interpretation: 'x' }),
    });
    const views = await loadAll();
    const bodyText = (name: PanelName): string => {
      const { container } = render(<div>{views[name]?.body}</div>);
      const t = container.textContent;
      cleanup();
      return t;
    };

    expect(views.difficulty?.status).toBe('error');
    expect(bodyText('difficulty')).toBe('❌ 取得失敗: HTTP 500');
    expect(views.difficulty?.statusText).toBe('取得失敗');

    expect(views.talent_pool?.status).toBe('error');
    expect(bodyText('talent_pool')).toBe('❌ 取得失敗: DB未接続');

    expect(views.competitors?.status).toBe('error');
    expect(bodyText('competitors')).toBe('❌ 取得失敗: network down');

    expect(views.market_trend?.status).toBe('error');
    expect(views.market_trend?.statusText).toBe('取得失敗');

    // the other panels are untouched and show real values
    for (const name of ['condition_gap', 'opportunity_map', 'insights', 'talent_pool_expansion', 'inflow'] as const) {
      expect(views[name]?.status, name).toBe('done');
    }
    expect(views.insights?.statusText).toBe('完了（4件）');
    expect(views.condition_gap?.statusText).toBe('完了');
    expect(views.opportunity_map?.statusText).toBe('完了（3件）');
    expect(views.talent_pool_expansion?.statusText).toBe('完了（2 市区町村）');
  });

  it('a non-object JSON body is データ形式不正 for that panel only', async () => {
    stubFetch({ insights: () => json(null) });
    const views = await loadAll();
    expect(views.insights?.status).toBe('error');
    expect(views.difficulty?.status).toBe('done');
  });

  it('aborting reports nothing for panels still in flight', async () => {
    stubFetch({});
    const controller = new AbortController();
    const views: Partial<Record<PanelName, PanelView>> = {};
    fetchMock.mockImplementation(
      (_input, init) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new DOMException('aborted', 'AbortError'));
          });
        }),
    );
    const pending = loadAllPanels(form, controller.signal, (name, view) => {
      views[name] = view;
    });
    controller.abort();
    await pending;
    // only the placeholder (never fetched) is reported
    expect(Object.keys(views)).toEqual(['inflow']);
  });
});
