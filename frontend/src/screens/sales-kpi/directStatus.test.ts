// HubSpot 直読みの状態表示 (meta の値だけで決める)。サーバ側の文字列 (hubspot_direct.rs) と
// 食い違わないこと、欠落・未知の値・時刻の形式不正で壊れないこと、TZ に左右されないことを見る。
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { DIRECT_RELOAD_MAX, DIRECT_RELOAD_MS, directStatus, nextReloadDelay } from './directStatus';

const ok = { HubSpot取得状態: 'ok', HubSpot取得時刻: '2026-10-01 09:30' };
const stale = {
  HubSpot取得状態: 'stale',
  HubSpot取得時刻: '2026-10-01 09:30',
  HubSpot最終失敗種別: 'hubspot_rate_limited',
  HubSpot最終失敗時刻: '2026-10-01 09:35',
};
const loading = { HubSpot取得状態: 'loading', HubSpot取得時刻: '' };

describe('directStatus', () => {
  it('シート経路 (キー無し) は何も出さない', () => {
    expect(directStatus({})).toBeNull();
    expect(directStatus({ データ元: 'シート', 取得時刻: '2026-10-01 09:30' })).toBeNull();
    expect(directStatus(undefined)).toBeNull();
    expect(directStatus(null)).toBeNull();
  });
  it('未知の状態値・空・型違いは何も出さない', () => {
    for (const v of ['', 'OK', 'unknown', ' ok', 'ok ']) {
      expect(directStatus({ HubSpot取得状態: v })).toBeNull();
    }
    expect(directStatus({ HubSpot取得状態: 1 as unknown as string })).toBeNull();
    expect(directStatus('ok' as unknown as Record<string, string>)).toBeNull();
  });
  it('ok: HH:MM 時点と 5 分ごと、即時/シートの区別', () => {
    const s = directStatus(ok);
    expect(s?.kind).toBe('ok');
    expect(s?.head).toBe('HubSpot から直接取得: 09:30 時点(5 分ごとに更新)');
    expect(s?.note).toContain('商談・アポ・Cヨミ・決定者の当日分・メンバー・架電リスト');
    expect(s?.note).toContain('担当別・リスト在庫・架電日次・週次');
    expect(s?.sub).toBeNull();
  });
  it('loading: 取得中の文言と自動再読み込みの案内、0 件ではなく未取得', () => {
    const s = directStatus(loading);
    expect(s?.kind).toBe('loading');
    expect(s?.head).toBe('HubSpot から取得中です。数十秒かかります');
    expect(s?.sub).toContain('0 件ではなく未取得');
    expect(s?.sub).toContain('15 秒ごとに自動で再読み込み');
  });
  it('loading: 上限に達したら自動再読み込みを止めた旨', () => {
    const s = directStatus(loading, true);
    expect(s?.sub).toContain('自動の再読み込みを止めました');
    expect(s?.sub).not.toContain('15 秒ごとに自動で再読み込み');
  });
  it('exhausted は loading 以外に影響しない', () => {
    expect(directStatus(ok, true)).toEqual(directStatus(ok, false));
    expect(directStatus(stale, true)).toEqual(directStatus(stale, false));
  });
  it('stale: 失敗の種別・時刻と、いつの値かを出す', () => {
    const s = directStatus(stale);
    expect(s?.kind).toBe('stale');
    expect(s?.head).toBe(
      '最新の取得に失敗しました(hubspot_rate_limited、2026-10-01 09:35)。2026-10-01 09:30 時点の値を表示しています',
    );
  });
  it('時刻の形式が不正でも落ちず、時刻を作らない', () => {
    for (const bad of ['', '09:30', '2026-10-01T09:30', '2026-10-01 25:00', '2026-10-01 09:60', 'abc', '2026-10-01 09:30:00']) {
      expect(directStatus({ ...ok, HubSpot取得時刻: bad })?.head).toBe('HubSpot から直接取得: 時刻不明(5 分ごとに更新)');
    }
    const s = directStatus({ ...stale, HubSpot取得時刻: 'zzz', HubSpot最終失敗時刻: '' });
    expect(s?.head).toBe('最新の取得に失敗しました(hubspot_rate_limited、時刻不明)。前回取得した値を表示しています');
  });
  it('stale で失敗種別が欠落しても落ちない', () => {
    const s = directStatus({ HubSpot取得状態: 'stale' });
    expect(s?.head).toBe('最新の取得に失敗しました(種別不明、時刻不明)。前回取得した値を表示しています');
  });
  it('時刻は文字列のまま使う (TZ に依存しない)', () => {
    const saved = process.env.TZ;
    try {
      for (const tz of ['Asia/Tokyo', 'UTC', 'America/Los_Angeles', 'Pacific/Kiritimati']) {
        process.env.TZ = tz;
        expect(directStatus(ok)?.head).toBe('HubSpot から直接取得: 09:30 時点(5 分ごとに更新)');
        expect(directStatus({ ...ok, HubSpot取得時刻: '2026-10-01 00:05' })?.head).toContain('00:05 時点');
      }
    } finally {
      process.env.TZ = saved;
    }
  });
});

describe('nextReloadDelay', () => {
  it('loading だけ 15 秒、上限回数で止まる', () => {
    expect(DIRECT_RELOAD_MS).toBe(15_000);
    expect(nextReloadDelay(loading, 0)).toBe(15_000);
    expect(nextReloadDelay(loading, DIRECT_RELOAD_MAX - 1)).toBe(15_000);
    expect(nextReloadDelay(loading, DIRECT_RELOAD_MAX)).toBeNull();
    expect(nextReloadDelay(loading, DIRECT_RELOAD_MAX + 5)).toBeNull();
  });
  it('ok / stale / シート / 未知では再読み込みしない', () => {
    for (const m of [ok, stale, {}, { HubSpot取得状態: 'x' }]) expect(nextReloadDelay(m, 0)).toBeNull();
  });
});

describe('サーバ (hubspot_direct.rs) の meta キー・状態値と一致している', () => {
  const src = readFileSync(
    path.resolve(__dirname, '../../../../src/handlers/sales_kpi/hubspot_direct.rs'),
    'utf8',
  );
  afterEach(() => undefined);
  it.each(['HubSpot取得状態', 'HubSpot取得時刻', 'HubSpot最終失敗種別', 'HubSpot最終失敗時刻'])('キー %s', (k) => {
    expect(src).toContain('"' + k + '"');
  });
  it.each(['loading', 'stale', 'ok'])('状態値 %s', (v) => {
    expect(src).toContain('set("HubSpot取得状態", "' + v + '".to_string())');
  });
  it('時刻は JST の yyyy-MM-dd HH:mm', () => {
    expect(src).toContain('"%Y-%m-%d %H:%M"');
  });
});
