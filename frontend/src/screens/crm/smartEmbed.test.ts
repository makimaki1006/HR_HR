import { describe, expect, it, vi } from 'vitest';
import {
  EMPTY_CALL, ZOOM_EMBED_ORIGIN, ZOOM_EMBED_SRC, parseZoomMessage, postMakeCall, reduceCall, toE164Jp,
} from './smartEmbed';
import type { ZoomEvent } from './smartEmbed';

const win = {} as Window;
const msg = (over: Partial<{ origin: string; source: unknown; data: unknown }> = {}) => ({
  origin: ZOOM_EMBED_ORIGIN, source: win as unknown,
  data: { type: 'zp-call-ringing-event', data: { callId: 'c1', direction: 'outbound', callee: { phoneNumber: '+81312345678' } } } as unknown,
  ...over,
});

describe('toE164Jp', () => {
  it('converts domestic and +81 forms to +81 without a leading 0', () => {
    expect(toE164Jp('03-1234-5678')).toBe('+81312345678');
    expect(toE164Jp('090-1234-5678')).toBe('+819012345678');
    expect(toE164Jp('+81 3-1234-5678')).toBe('+81312345678');
    expect(toE164Jp('+81(0)3-1234-5678')).toBe('+81312345678');
    expect(toE164Jp('０３－１２３４－５６７８')).toBe('+81312345678');
  });
  it('keeps other countries as +digits and refuses anything that is not a number', () => {
    expect(toE164Jp('+1 415 555 0100')).toBe('+14155550100');
    expect(toE164Jp('サンプル')).toBeNull();
    expect(toE164Jp('')).toBeNull();
    expect(toE164Jp(null)).toBeNull();
    expect(toE164Jp('12345')).toBeNull();
    expect(toE164Jp('03-12')).toBeNull();
  });
});

describe('postMakeCall', () => {
  it('posts zp-make-call to the Zoom origin only (never "*") and returns true', () => {
    const postMessage = vi.fn();
    const ok = postMakeCall({ postMessage } as unknown as Window, '03-1234-5678');
    expect(ok).toBe(true);
    expect(postMessage).toHaveBeenCalledTimes(1);
    expect(postMessage).toHaveBeenCalledWith({ type: 'zp-make-call', data: { number: '+81312345678', autoDial: true } }, ZOOM_EMBED_ORIGIN);
    expect(postMessage.mock.calls[0]?.[1]).not.toBe('*');
  });
  it('does not post when there is no window or the number is not dialable', () => {
    const postMessage = vi.fn();
    expect(postMakeCall(null, '03-1234-5678')).toBe(false);
    expect(postMakeCall({ postMessage } as unknown as Window, 'abc')).toBe(false);
    expect(postMessage).not.toHaveBeenCalled();
  });
  it('embeds only the documented Zoom URL', () => {
    expect(ZOOM_EMBED_SRC).toBe('https://applications.zoom.us/integration/phone/embeddablephone/home');
    expect(ZOOM_EMBED_SRC.startsWith(ZOOM_EMBED_ORIGIN)).toBe(true);
  });
});

describe('parseZoomMessage', () => {
  it('accepts a call event only from the Zoom origin and the embedded iframe window', () => {
    const ev = parseZoomMessage(msg(), win);
    expect(ev).toMatchObject({ type: 'ringing', callId: 'c1', direction: 'outbound', number: '+81312345678' });
  });
  it('ignores other origins, other windows, lookalike origins and unknown / malformed data', () => {
    for (const origin of ['https://evil.example', 'https://applications.zoom.us.evil.example', 'http://applications.zoom.us', 'https://zoom.us', '']) {
      expect(parseZoomMessage(msg({ origin }), win), origin).toBeNull();
    }
    expect(parseZoomMessage(msg({ source: {} }), win)).toBeNull();
    expect(parseZoomMessage(msg(), null)).toBeNull();
    for (const data of [null, 'x', 3, {}, { type: 'other' }, { type: 'zp-call-ringing-event' }, { type: 'zp-call-ringing-event', data: null }, { type: 'zp-call-ringing-event', data: {} }]) {
      expect(parseZoomMessage(msg({ data }), win), JSON.stringify(data)).toBeNull();
    }
  });
  it('maps connected / ended / log-completed with the result', () => {
    const base = { callId: 'c1', direction: 'outbound', callee: { phoneNumber: '+81312345678' } };
    expect(parseZoomMessage(msg({ data: { type: 'zp-call-connected-event', data: base } }), win)).toMatchObject({ type: 'connected', callId: 'c1' });
    expect(parseZoomMessage(msg({ data: { type: 'zp-call-ended-event', data: { ...base, result: 'missed' } } }), win)).toMatchObject({ type: 'ended', result: 'missed' });
    expect(parseZoomMessage(msg({ data: { type: 'zp-call-ended-event', data: { ...base, result: 'weird' } } }), win)).toMatchObject({ type: 'ended', result: null });
    expect(parseZoomMessage(msg({ data: { type: 'zp-call-log-completed-event', data: base } }), win)).toMatchObject({ type: 'log_completed', callId: 'c1' });
  });
});

describe('reduceCall', () => {
  const ev = (type: ZoomEvent['type'], callId = 'c1', extra: Partial<ZoomEvent> = {}): ZoomEvent =>
    ({ type, callId, direction: 'outbound', number: '+81312345678', result: null, ...extra });
  it('goes ringing -> connected -> ended and computes the talk time from the event times, not from an invented field', () => {
    let s = reduceCall(EMPTY_CALL, ev('ringing'), 1_000);
    expect(s).toMatchObject({ phase: 'ringing', callId: 'c1' });
    s = reduceCall(s, ev('connected'), 5_000);
    expect(s).toMatchObject({ phase: 'connected', connectedAt: 5_000 });
    s = reduceCall(s, ev('ended', 'c1', { result: 'ended' }), 70_000);
    expect(s).toMatchObject({ phase: 'ended', result: 'ended', talkSeconds: 65 });
  });
  it('a missed call has no talk time', () => {
    let s = reduceCall(EMPTY_CALL, ev('ringing'), 1_000);
    s = reduceCall(s, ev('ended', 'c1', { result: 'missed' }), 20_000);
    expect(s).toMatchObject({ phase: 'ended', result: 'missed', talkSeconds: null });
  });
  it('ignores events of another call while one is active, but a new ringing replaces an ended call', () => {
    let s = reduceCall(EMPTY_CALL, ev('ringing', 'c1'), 1_000);
    s = reduceCall(s, ev('ended', 'zzz'), 2_000);
    expect(s.phase).toBe('ringing');
    s = reduceCall(s, ev('ended', 'c1', { result: 'ended' }), 3_000);
    s = reduceCall(s, ev('ringing', 'c2'), 4_000);
    expect(s).toMatchObject({ phase: 'ringing', callId: 'c2', talkSeconds: null });
  });
  it('log_completed keeps the ended state and marks the log', () => {
    let s = reduceCall(EMPTY_CALL, ev('ringing'), 1_000);
    s = reduceCall(s, ev('ended', 'c1', { result: 'ended' }), 3_000);
    s = reduceCall(s, ev('log_completed'), 4_000);
    expect(s).toMatchObject({ phase: 'ended', logCompleted: true });
  });
});
