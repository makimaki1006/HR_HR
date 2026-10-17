// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import type { CustomMetricsResponse } from '../../generated/CustomMetricsResponse';
import type { MetricPrefs } from '../../generated/MetricPrefs';
import { ApiHttpError } from '../../api/client';
import { MetricsSettings } from './MetricsSettings';
import { FIXTURE_MONTH, FIXTURE_TODAY, FIXTURE_TODAY_SALES } from './metricsModel';
import { DEFAULT_PREFS, arrangeStrip, customStripItem, fixtureCustomResponse, formatMetricNumber, memoryPrefsApi } from './metricPrefs';
import type { PrefsApi, SaveResult } from './metricPrefs';
import { TodayMetrics } from './TodayMetrics';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import type { CatalogState } from './usePropertyCatalog';
import { catalogIndex } from './propertyModel';

afterEach(() => { cleanup(); vi.useRealTimers(); window.localStorage.clear(); });

const catalog: CatalogState = { phase: 'ready', catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) };
const ok = <T,>(data: T) => () => Promise.resolve({ ok: true as const, data });

const WITH_CUSTOM: MetricPrefs = {
  items: ['outbound', 'c-absent', 'duration', 'c-amount'],
  custom: [
    { id: 'c-absent', name: '不在', kind: 'count', condition_groups: [[{ property: 'bpo_20', operator: 'TODAY', values: [] }]], owner_property: 'hubspot_owner_id' },
    { id: 'c-amount', name: '今月の金額', kind: 'sum', property: 'amount', condition_groups: [], owner_property: 'hubspot_owner_id' },
  ],
};

function customResponse(over: Partial<CustomMetricsResponse['items'][number]>[] = []): CustomMetricsResponse {
  const base: CustomMetricsResponse['items'] = [
    { id: 'c-absent', name: '不在', kind: 'count', status: 'ready', value: 12, team_avg: 9.5, rank: 3, ranked: 8, stale: false, as_of: FIXTURE_TODAY.as_of, message: null },
    { id: 'c-amount', name: '今月の金額', kind: 'sum', status: 'ready', value: 1_250_000, team_avg: 980_000, rank: 2, ranked: 8, stale: false, as_of: FIXTURE_TODAY.as_of, message: null },
  ];
  const items = base.map((it, i) => ({ ...it, ...over[i] }));
  return { pending: false, as_of: FIXTURE_TODAY.as_of, population: 'team', population_size: 8, team_names: ['架空BPOチーム'], items, message: null };
}

function settingsPanel(onSave: Mock<(p: MetricPrefs) => Promise<SaveResult>> = vi.fn<(p: MetricPrefs) => Promise<SaveResult>>(() => Promise.resolve<SaveResult>({ ok: true })), prefs: MetricPrefs = DEFAULT_PREFS, available = true) {
  render(<MetricsSettings prefs={prefs} available={available} note={available ? null : 'いまは項目の設定を読み書きできません。帯は標準の項目で表示しています。'}
    monthView={false} catalog={catalog} onReloadCatalog={() => undefined} onSave={onSave} onClose={() => undefined} />);
  return onSave;
}

const shownList = () => within(screen.getByRole('list', { name: '出す項目' })).getAllByRole('listitem').map(li => li.querySelector('.tm-set-name span')?.textContent);

describe('帯の項目の設定パネル', () => {
  it('出す項目を並べ替え・隠し、保存のときだけ 1 回送る', async () => {
    const onSave = settingsPanel();
    expect(shownList()).toEqual(['架電数', '90秒以上', '通話時間', 'アポ']);
    fireEvent.click(screen.getByRole('button', { name: '通話時間を上へ' }));
    expect(shownList()).toEqual(['架電数', '通話時間', '90秒以上', 'アポ']);
    fireEvent.click(screen.getByRole('checkbox', { name: 'アポを帯に出す' }));
    expect(shownList()).toEqual(['架電数', '通話時間', '90秒以上']);
    // 出さない項目から 60 秒以上を出す (最後に足す)
    fireEvent.click(screen.getByRole('checkbox', { name: '60秒以上を帯に出す' }));
    expect(shownList()).toEqual(['架電数', '通話時間', '90秒以上', '60秒以上']);
    expect(onSave).not.toHaveBeenCalled();
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存' })); await Promise.resolve(); });
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith({ items: ['outbound', 'duration', 'over_90s', 'over_60s'], custom: [] });
    expect(screen.getByText('保存しました')).toBeTruthy();
  });

  it('自分の項目を足す: 名前・数え方・数値の項目・条件。足すと帯の最後に入る', async () => {
    const onSave = settingsPanel();
    fireEvent.click(screen.getByRole('button', { name: '＋ 自分の項目を足す' }));
    const decide = screen.getByRole<HTMLButtonElement>('button', { name: 'この項目を決める' });
    expect(decide.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('名前'), { target: { value: '今月の金額' } });
    fireEvent.click(screen.getByRole('radio', { name: '合計（数値の項目）' }));
    // 合計は数値の項目を選ぶまで決められない
    expect(decide.disabled).toBe(true);
    const picker = screen.getByRole('combobox', { name: '合計・平均にする数値の項目' });
    fireEvent.focus(picker);
    fireEvent.click(screen.getByRole('option', { name: '金額' }));
    expect(screen.getByRole('combobox', { name: '誰の取引か' })).toBeTruthy();
    // 条件: 最終架電日が今月
    fireEvent.click(screen.getByRole('button', { name: '＋ 条件を追加' }));
    const cond = screen.getByRole('combobox', { name: '項目を検索' });
    fireEvent.focus(cond);
    fireEvent.click(screen.getByRole('option', { name: '最終架電日' }));
    fireEvent.change(screen.getByRole('combobox', { name: '条件' }), { target: { value: 'THIS_MONTH' } });
    fireEvent.click(screen.getByRole('button', { name: 'この条件を追加' }));
    expect(screen.getByText('最終架電日 今月')).toBeTruthy();
    expect(screen.getByText('条件はあと 4 個です')).toBeTruthy();
    fireEvent.click(decide);
    expect(shownList()).toEqual(['架電数', '90秒以上', '通話時間', 'アポ', '今月の金額']);
    expect(screen.getByText('自分の項目・合計')).toBeTruthy();
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存' })); await Promise.resolve(); });
    const saved = onSave.mock.calls[0]?.[0];
    if (!saved) throw new Error('not saved');
    expect(saved.items.at(-1)).toMatch(/^c-[a-z0-9]+$/);
    expect(saved.custom).toEqual([{ id: saved.items.at(-1), name: '今月の金額', kind: 'sum', property: 'amount', owner_property: 'hubspot_owner_id',
      condition_groups: [[{ property: 'bpo_20', operator: 'THIS_MONTH', values: [] }]] }]);
  });

  it('自分の項目を直す・消す。数え方を件数に戻すと数値の項目は外れる', async () => {
    const onSave = settingsPanel(undefined, WITH_CUSTOM);
    fireEvent.click(within(screen.getByTestId('metric-setting-c-amount')).getByRole('button', { name: '直す' }));
    fireEvent.change(screen.getByLabelText('名前'), { target: { value: '今月の件数' } });
    fireEvent.click(screen.getByRole('radio', { name: '件数（条件に合う取引の数）' }));
    fireEvent.click(screen.getByRole('button', { name: 'この項目を決める' }));
    fireEvent.click(screen.getByRole('button', { name: '不在を消す' }));
    expect(shownList()).toEqual(['架電数', '通話時間', '今月の件数']);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存' })); await Promise.resolve(); });
    expect(onSave).toHaveBeenCalledWith({ items: ['outbound', 'duration', 'c-amount'], custom: [{ id: 'c-amount', name: '今月の件数', kind: 'count', condition_groups: [], owner_property: 'hubspot_owner_id' }] });
  });

  it('自分の項目は 6 個まで。条件は 5 個まで', () => {
    const six: MetricPrefs = { items: [], custom: Array.from({ length: 6 }, (_, i) => ({ id: `c-${String(i)}`, name: `架空${String(i)}`, kind: 'count' as const, condition_groups: [], owner_property: 'hubspot_owner_id' })) };
    settingsPanel(undefined, six);
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '＋ 自分の項目を足す' }).disabled).toBe(true);
    expect(screen.getByText(/あと 0 個/)).toBeTruthy();
    cleanup();
    const five: MetricPrefs = { items: ['c-a'], custom: [{ id: 'c-a', name: '架空', kind: 'count', condition_groups: [Array.from({ length: 5 }, () => ({ property: 'amount', operator: 'GTE', values: ['1'] }))], owner_property: 'hubspot_owner_id' }] };
    settingsPanel(undefined, five);
    fireEvent.click(screen.getByRole('button', { name: '直す' }));
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '＋ 条件を追加' }).disabled).toBe(true);
    expect(screen.getByText('条件はあと 0 個です')).toBeTruthy();
  });

  it('保存できなかった理由をそのまま出す (サーバの 400)', async () => {
    settingsPanel(vi.fn<(p: MetricPrefs) => Promise<SaveResult>>(() => Promise.resolve<SaveResult>({ ok: false, message: '「誰の」には担当者を選ぶ型の項目を選んでください。' })));
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存' })); await Promise.resolve(); });
    expect(screen.getByRole('alert').textContent).toBe('「誰の」には担当者を選ぶ型の項目を選んでください。');
  });

  it('監査 DB が使えないときは 1 行の案内を出し、保存させない', () => {
    settingsPanel(undefined, DEFAULT_PREFS, false);
    expect(screen.getByText('いまは項目の設定を読み書きできません。帯は標準の項目で表示しています。')).toBeTruthy();
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '保存' }).disabled).toBe(true);
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '＋ 自分の項目を足す' }).disabled).toBe(true);
  });
});

describe('帯の自分の項目', () => {
  const settings = { catalog, onReloadCatalog: () => undefined };

  it('もともとの項目と同じ見た目で、設定の並びに出す。合計は桁区切り', async () => {
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY)} prefsApi={memoryPrefsApi({ prefs: WITH_CUSTOM, saved: true, available: true, message: null })}
      customFetcher={() => ok(customResponse())} settings={settings} />);
    await waitFor(() => { expect(screen.getByTestId('metric-c-absent').textContent).toBe('不在12（チーム平均 9.5・3位/8人）'); });
    expect(screen.getByTestId('metric-c-amount').textContent).toBe('今月の金額1,250,000（チーム平均 980,000・2位/8人）');
    const keys = [...screen.getByTestId('today-metrics').querySelectorAll('.tm-item')].map(li => li.getAttribute('data-testid'));
    expect(keys).toEqual(['metric-outbound', 'metric-c-absent', 'metric-duration', 'metric-c-amount']);
    // 外した 90 秒以上・アポは出ない
    expect(screen.queryByTestId('metric-over_90s')).toBeNull();
    expect(screen.getByTestId('metric-c-amount').querySelector('.tm-cmp')?.getAttribute('data-short')).toBe('（平均980,000・2位/8）');
  });

  it('件数が多すぎる項目は数字を出さず理由を出す。少し前の値には印を付ける', async () => {
    const res = customResponse([{ status: 'too_many', value: null, team_avg: null, rank: null, message: '件数が多すぎます（条件を絞ってください）' }, { stale: true }]);
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY)} prefsApi={memoryPrefsApi({ prefs: WITH_CUSTOM, saved: true, available: true, message: null })}
      customFetcher={() => ok(res)} settings={settings} />);
    await waitFor(() => { expect(screen.getByTestId('metric-c-absent').textContent).toBe('不在件数が多すぎます（条件を絞ってください）'); });
    expect(screen.getByTestId('metric-c-amount').textContent).toContain('少し前の値');
    expect(screen.getByText(/前回の数字を表示中/)).toBeTruthy();
  });

  it('今月の帯 (新規営業) でも同じ設定を使い、今月で数えていない項目は出さない', async () => {
    const prefs: MetricPrefs = { ...WITH_CUSTOM, items: ['over_90s', ...WITH_CUSTOM.items] };
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY_SALES)} monthFetcher={ok(FIXTURE_MONTH)}
      prefsApi={memoryPrefsApi({ prefs, saved: true, available: true, message: null })} customFetcher={() => ok(customResponse())} settings={settings} />);
    await screen.findByText('10月の成績');
    await waitFor(() => { expect(screen.getByTestId('metric-c-absent').textContent).toContain('不在12'); });
    const keys = [...screen.getByTestId('month-metrics').querySelectorAll('.tm-item')].map(li => li.getAttribute('data-testid'));
    expect(keys).toEqual(['metric-outbound', 'metric-c-absent', 'metric-duration', 'metric-c-amount']);
  });

  it('監査 DB が使えないとき、帯は標準の項目で出し、設定のパネルに 1 行の案内', async () => {
    const api: PrefsApi = {
      load: () => Promise.resolve({ ok: true, data: { prefs: DEFAULT_PREFS, saved: false, available: false, message: 'いまは項目の設定を読み書きできません。帯は標準の項目で表示しています。' } }),
      save: () => Promise.resolve({ ok: false, error: new ApiHttpError(503) }),
    };
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY)} prefsApi={api} settings={settings} />);
    await screen.findByTestId('metric-outbound');
    expect([...screen.getByTestId('today-metrics').querySelectorAll('.tm-item')].map(li => li.getAttribute('data-testid')))
      .toEqual(['metric-outbound', 'metric-over_90s', 'metric-duration', 'metric-appointments']);
    fireEvent.click(screen.getByRole('button', { name: '⚙ 項目' }));
    expect(screen.getByTestId('metrics-settings').textContent).toContain('いまは項目の設定を読み書きできません。');
  });

  it('設定を読めない (通信の失敗) ときも標準の項目で出す', async () => {
    const api: PrefsApi = { load: () => Promise.resolve({ ok: false, error: new ApiHttpError(500) }), save: () => Promise.resolve({ ok: false, error: new ApiHttpError(500) }) };
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY)} prefsApi={api} settings={settings} />);
    expect((await screen.findByTestId('metric-appointments')).textContent).toBe('アポ2（チーム平均 1.1・1位/8人）');
  });

  it('保存すると新しい設定で帯が変わり、自分の項目を読み直す', async () => {
    const customFetcher = vi.fn((p: MetricPrefs) => ok(fixtureCustomResponse(p)));
    render(<TodayMetrics fixture={false} fetcher={ok(FIXTURE_TODAY)} prefsApi={memoryPrefsApi({ prefs: DEFAULT_PREFS, saved: false, available: true, message: null })}
      customFetcher={customFetcher} settings={settings} />);
    await screen.findByTestId('metric-outbound');
    expect(customFetcher).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '⚙ 項目' }));
    fireEvent.click(screen.getByRole('button', { name: '＋ 自分の項目を足す' }));
    fireEvent.change(screen.getByLabelText('名前'), { target: { value: '不在' } });
    fireEvent.click(screen.getByRole('button', { name: 'この項目を決める' }));
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '保存' })); await Promise.resolve(); await Promise.resolve(); });
    expect(customFetcher).toHaveBeenCalled();
    const item = await screen.findByText('不在', { selector: '.tm-label' });
    expect(item.closest('li')?.querySelector('.tm-value')?.textContent).toMatch(/^\d+$/);
  });
});

describe('自分の項目の表示の計算', () => {
  it('件数は整数、平均・合計は小数 1 位まで、桁区切り', () => {
    expect(formatMetricNumber(1234567, true)).toBe('1,234,567');
    expect(formatMetricNumber(1234.56)).toBe('1,234.6');
    expect(formatMetricNumber(null)).toBe('-');
  });
  it('全員と比べるときは「全体平均」、順位なしは「順位なし」', () => {
    const v = customResponse([{ rank: null }]).items[0];
    if (!v) throw new Error('fixture');
    expect(customStripItem(v, { population: 'all' }).compare).toBe('全体平均 9.5・順位なし');
  });
  it('数字の応答が来るまでは集計中、読めなかったら理由', () => {
    const builtin: never[] = [];
    expect(arrangeStrip(builtin, WITH_CUSTOM, null).map(i => i.note)).toEqual(['集計しています…', '集計しています…']);
    expect(arrangeStrip(builtin, WITH_CUSTOM, 'failed').map(i => i.note)).toEqual(['いまは数字を読めません', 'いまは数字を読めません']);
  });
});
