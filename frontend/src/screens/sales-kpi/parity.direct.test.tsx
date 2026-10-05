// @vitest-environment happy-dom
// HubSpot 直読みの状態表示 (#hsstatus) を、旧画面の script をそのまま動かした DOM と React 版で突き合わせる。
// meta だけで決まる 4 通り (無し=シート / ok / loading / stale) で同じ文言・同じ class になること、
// loading の自動再読み込みが上限回数で止まること、シートのときは何も足さないことを見る。
import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { loadFixture } from './__fixtures__/load';
import { mountNew, mountOld, outline } from './__fixtures__/dual';
import { DIRECT_RELOAD_MAX, DIRECT_RELOAD_MS } from './directStatus';
import { SalesKpiScreen } from './SalesKpiScreen';
import type { SalesKpiData } from './types';

const META = {
  ok: { HubSpot取得状態: 'ok', HubSpot取得時刻: '2026-10-01 09:30' },
  loading: { HubSpot取得状態: 'loading', HubSpot取得時刻: '' },
  stale: {
    HubSpot取得状態: 'stale',
    HubSpot取得時刻: '2026-10-01 09:30',
    HubSpot最終失敗種別: 'hubspot_rate_limited',
    HubSpot最終失敗時刻: '2026-10-01 09:35',
  },
} as const;

const RELOAD_KEY = 'salesKpi.directReload.v1';

function withMeta(extra: Record<string, string> | null): SalesKpiData {
  const d = loadFixture();
  return extra === null ? d : { ...d, meta: { ...d.meta, ...extra } };
}

function banner(): { text: string[]; cls: string } | null {
  const e = document.getElementById('hsstatus');
  return e ? { text: outline(e), cls: e.className } : null;
}

afterEach(() => {
  document.body.innerHTML = '';
  sessionStorage.clear();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('直読みの状態表示: 旧画面と React 版が同じ', () => {
  it.each([['ok'], ['loading'], ['stale']] as const)('%s: 同じ文言・同じ class', async (kind) => {
    const old = await mountOld(withMeta(META[kind]));
    const a = banner();
    old.teardown();
    const cur = await mountNew(withMeta(META[kind]));
    const b = banner();
    cur.teardown();
    expect(a).not.toBeNull();
    expect(b).toEqual(a);
    expect(a?.cls).toBe('hsstatus ' + kind);
  });

  it('具体値: ok / stale の文言', async () => {
    const o = await mountOld(withMeta(META.ok));
    expect(banner()?.text[0]).toBe('HubSpot から直接取得: 09:30 時点(5 分ごとに更新)');
    o.teardown();
    const s = await mountNew(withMeta(META.stale));
    expect(banner()?.text[0]).toBe(
      '最新の取得に失敗しました(hubspot_rate_limited、2026-10-01 09:35)。2026-10-01 09:30 時点の値を表示しています',
    );
    s.teardown();
  });

  it('シート (キー無し): どちらも何も足さず、残りの画面は従来どおり', async () => {
    const old = await mountOld(withMeta(null));
    expect(banner()).toBeNull();
    const oldRange = document.getElementById('range')?.textContent;
    old.teardown();
    const cur = await mountNew(withMeta(null));
    expect(banner()).toBeNull();
    expect(document.getElementById('range')?.textContent).toBe(oldRange);
    cur.teardown();
  });

  it('未知の状態値・欠落キー: どちらも何も出さない', async () => {
    for (const extra of [{ HubSpot取得状態: 'weird' }, { HubSpot取得状態: '' }, { HubSpot取得時刻: '2026-10-01 09:30' }]) {
      const o = await mountOld(withMeta(extra));
      expect(banner()).toBeNull();
      o.teardown();
      const n = await mountNew(withMeta(extra));
      expect(banner()).toBeNull();
      n.teardown();
    }
  });

  it('時刻の形式が不正: どちらも「時刻不明」で落ちない', async () => {
    const bad = { ...META.ok, HubSpot取得時刻: '09:30' };
    const o = await mountOld(withMeta(bad));
    const a = banner();
    o.teardown();
    const n = await mountNew(withMeta(bad));
    expect(banner()).toEqual(a);
    expect(a?.text[0]).toBe('HubSpot から直接取得: 時刻不明(5 分ごとに更新)');
    n.teardown();
  });

  it('banner は header の直後 (画面上部) にある', async () => {
    const o = await mountOld(withMeta(META.ok));
    expect(document.getElementById('hsstatus')?.previousElementSibling?.tagName).toBe('HEADER');
    o.teardown();
    const n = await mountNew(withMeta(META.ok));
    expect(document.getElementById('hsstatus')?.previousElementSibling?.tagName).toBe('HEADER');
    n.teardown();
  });
});

describe('loading の自動再読み込み', () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = () => undefined;
  });

  /** React の更新も一緒に流す (act の外だと scheduler が止まった時計に載ってしまう) */
  async function tick(ms: number): Promise<void> {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ms);
    });
  }

  function stubSeq(bodies: SalesKpiData[]): { calls: () => number } {
    let n = 0;
    vi.stubGlobal('fetch', () => {
      const b = bodies[Math.min(n, bodies.length - 1)];
      n += 1;
      return Promise.resolve(
        new Response(JSON.stringify(b), { status: 200, headers: { 'Content-Type': 'application/json' } }),
      );
    });
    return { calls: () => n };
  }

  it('React: 15 秒ごとに読み直し、上限で止まって案内に変わる', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const f = stubSeq([withMeta(META.loading)]);
    const r = render(<SalesKpiScreen />);
    await vi.waitFor(() => {
      if (!banner()) throw new Error('まだ');
    });
    expect(f.calls()).toBe(1);
    for (let i = 1; i <= DIRECT_RELOAD_MAX; i++) {
      await tick(DIRECT_RELOAD_MS);
      expect(f.calls()).toBe(1 + i);
    }
    await tick(DIRECT_RELOAD_MS * 5);
    expect(f.calls()).toBe(1 + DIRECT_RELOAD_MAX);
    expect(banner()?.text.join(' ')).toContain('自動の再読み込みを止めました');
    r.unmount();
  });

  it('React: ok になったら読み直しをやめ、banner が ok に変わる', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const f = stubSeq([withMeta(META.loading), withMeta(META.ok)]);
    const r = render(<SalesKpiScreen />);
    await vi.waitFor(() => {
      if (!banner()) throw new Error('まだ');
    });
    await tick(DIRECT_RELOAD_MS);
    await vi.waitFor(() => {
      if (banner()?.cls !== 'hsstatus ok') throw new Error('まだ');
    });
    await tick(DIRECT_RELOAD_MS * 3);
    expect(f.calls()).toBe(2);
    r.unmount();
  });

  it('React: シート (キー無し) は読み直さない', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const f = stubSeq([withMeta(null)]);
    const r = render(<SalesKpiScreen />);
    await vi.waitFor(() => {
      if (!document.getElementById('cards1')?.children.length) throw new Error('まだ');
    });
    await tick(DIRECT_RELOAD_MS * 20);
    expect(f.calls()).toBe(1);
    r.unmount();
  });

  it('旧画面: loading なら 15 秒後にページを読み直す。上限回数で止まり、シートでは読み直さない', async () => {
    const reload = vi.fn();
    // teardown が vi.unstubAllGlobals() を呼ぶので、毎回つけ直す
    const stubLocation = (): void => {
      vi.stubGlobal('location', { search: '', reload });
    };
    stubLocation();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    // 上限まで (毎回ページを開き直したのと同じ: sessionStorage の回数を引き継ぐ)
    for (let i = 1; i <= DIRECT_RELOAD_MAX; i++) {
      const o = await mountOld(withMeta(META.loading));
      expect(banner()?.text.join(' ')).toContain('15 秒ごとに自動で再読み込み');
      await vi.advanceTimersByTimeAsync(DIRECT_RELOAD_MS);
      expect(reload).toHaveBeenCalledTimes(i);
      expect(sessionStorage.getItem(RELOAD_KEY)).toBe(String(i));
      const keep = sessionStorage.getItem(RELOAD_KEY) ?? '';
      document.body.innerHTML = '';
      o.teardown(); // localStorage.clear() と stub の解除。sessionStorage は残る
      sessionStorage.setItem(RELOAD_KEY, keep);
      stubLocation();
    }
    const last = await mountOld(withMeta(META.loading));
    await vi.advanceTimersByTimeAsync(DIRECT_RELOAD_MS * 5);
    expect(reload).toHaveBeenCalledTimes(DIRECT_RELOAD_MAX);
    expect(banner()?.text.join(' ')).toContain('自動の再読み込みを止めました');
    // 止めたあとの手動の再読み込みは、また最初から
    expect(sessionStorage.getItem(RELOAD_KEY)).toBeNull();
    last.teardown();
    stubLocation();
    // シートでは回数も残さず、読み直さない
    reload.mockClear();
    const sheet = await mountOld(withMeta(null));
    await vi.advanceTimersByTimeAsync(DIRECT_RELOAD_MS * 5);
    expect(reload).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(RELOAD_KEY)).toBeNull();
    sheet.teardown();
  }, 60_000);

  it('旧画面: ok に変わったら回数の記録を消す', async () => {
    sessionStorage.setItem(RELOAD_KEY, '3');
    const o = await mountOld(withMeta(META.ok));
    expect(sessionStorage.getItem(RELOAD_KEY)).toBeNull();
    o.teardown();
  });
});

describe('CSS: 旧画面と React 版の両方に 3 状態の見た目がある', () => {
  const css = readFileSync(path.resolve(__dirname, './sales-kpi.css'), 'utf8');
  const tpl = readFileSync(path.resolve(__dirname, '../../../../templates/tabs/sales_kpi.html'), 'utf8');
  it.each(['ok', 'loading', 'stale'])('.hsstatus.%s', (k) => {
    expect(css).toContain('.hsstatus.' + k);
    expect(tpl).toContain('.hsstatus.' + k);
  });
});
