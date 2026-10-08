// Zoom Phone Smart Embed との postMessage 契約。
// 出典 (2026-10-06 確認。確認できなかった点は PR の報告に「未確認」と書いてある):
//  - https://developers.zoom.us/docs/phone/smart-embed-guide/ (iframe の URL、zp-make-call、zp-call-*-event)
//  - https://developers.zoom.us/blog/phone-smart-embed-reactapp-part2/ (postMessage の宛先 origin)
import { toDomesticPhone } from './phone';

/** Smart Embed の origin。送信の宛先にも、受信の送信元検証にも、この 1 つだけを使う ('*' は使わない) */
export const ZOOM_EMBED_ORIGIN = 'https://applications.zoom.us';
export const ZOOM_EMBED_SRC = `${ZOOM_EMBED_ORIGIN}/integration/phone/embeddablephone/home`;

/** 表示用の番号 → 発信に渡す番号 (+81…)。ダイヤルできる形でなければ null */
export function toE164Jp(raw: string | null | undefined): string | null {
  const domestic = toDomesticPhone(raw);
  if (domestic === null) return null;
  if (/^0\d{8,10}$/.test(domestic)) return `+81${domestic.slice(1)}`;
  // 日本以外の国番号 (+1…)。toDomesticPhone は元の文字列のまま返す
  const s = domestic.normalize('NFKC');
  if (/^\+[\d\s\-().]+$/.test(s)) {
    const digits = s.replace(/\D/g, '');
    if (digits.length >= 8 && digits.length <= 15) return `+${digits}`;
  }
  return null;
}

export interface MakeCallMessage {
  type: 'zp-make-call';
  data: { number: string; autoDial: true };
}

export function buildMakeCall(e164: string): MakeCallMessage {
  return { type: 'zp-make-call', data: { number: e164, autoDial: true } };
}

/** iframe の window へ発信を依頼する。送れたら true。宛先 origin は Zoom に固定 */
export function postMakeCall(target: Window | null, rawNumber: string | null | undefined): boolean {
  const e164 = toE164Jp(rawNumber);
  if (!target || e164 === null) return false;
  target.postMessage(buildMakeCall(e164), ZOOM_EMBED_ORIGIN);
  return true;
}

export type CallDirection = string | null;
export interface ZoomEvent {
  type: 'ringing' | 'connected' | 'ended' | 'log_completed';
  callId: string;
  direction: CallDirection;
  /** 相手の番号 (outbound は callee、それ以外は caller) */
  number: string | null;
  /** ended のときだけ: missed / rejected / ended (公式の列挙。それ以外は null) */
  result: 'missed' | 'rejected' | 'ended' | null;
}

const TYPES: Record<string, ZoomEvent['type']> = {
  'zp-call-ringing-event': 'ringing',
  'zp-call-connected-event': 'connected',
  'zp-call-ended-event': 'ended',
  'zp-call-log-completed-event': 'log_completed',
};

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null;
const str = (v: unknown): string | null => (typeof v === 'string' && v !== '' ? v : null);

/**
 * window の message を Zoom の通話イベントにする。
 * origin が Zoom の Smart Embed で、かつ送信元がこの画面の iframe の window であるものだけ受け取る。
 * 形が違うもの・知らない種類は null (無視)。
 */
/** origin が Zoom の Smart Embed で、送信元がこの画面の iframe の window か (中身の形は問わない) */
export function isFromEmbed(ev: { origin: string; source: unknown }, iframeWindow: Window | null): boolean {
  return ev.origin === ZOOM_EMBED_ORIGIN && iframeWindow !== null && ev.source === iframeWindow;
}

export function parseZoomMessage(
  ev: { origin: string; source: unknown; data: unknown }, iframeWindow: Window | null,
): ZoomEvent | null {
  if (!isFromEmbed(ev, iframeWindow)) return null;
  if (!isRecord(ev.data)) return null;
  const type = typeof ev.data.type === 'string' ? TYPES[ev.data.type] : undefined;
  if (type === undefined) return null;
  const d = ev.data.data;
  if (!isRecord(d)) return null;
  const callId = str(d.callId);
  if (callId === null) return null;
  const direction = str(d.direction);
  const side = direction === 'outbound' ? d.callee : d.caller;
  const number = isRecord(side) ? str(side.phoneNumber) : null;
  const r = d.result;
  const result = type === 'ended' && (r === 'missed' || r === 'rejected' || r === 'ended') ? r : null;
  return { type, callId, direction, number, result };
}

export interface CallState {
  phase: 'idle' | 'ringing' | 'connected' | 'ended';
  callId: string | null;
  direction: CallDirection;
  number: string | null;
  startedAt: number | null;
  connectedAt: number | null;
  /** 通話時間 (秒)。イベントを受けた時刻の差で、Zoom が通話時間を送ってくるわけではない。つながらなかった通話は null */
  talkSeconds: number | null;
  result: ZoomEvent['result'];
  logCompleted: boolean;
}

export const EMPTY_CALL: CallState = {
  phase: 'idle', callId: null, direction: null, number: null, startedAt: null, connectedAt: null,
  talkSeconds: null, result: null, logCompleted: false,
};

/** 通話の状態遷移。`now` は呼び出し側 (テストで固定できるよう引数) の epoch ms */
export function reduceCall(state: CallState, ev: ZoomEvent, now: number): CallState {
  if (ev.type === 'ringing') {
    // 新しい呼び出し (別の callId、または終わった通話の後)
    if (state.phase === 'idle' || state.phase === 'ended' || state.callId !== ev.callId) {
      return { ...EMPTY_CALL, phase: 'ringing', callId: ev.callId, direction: ev.direction, number: ev.number, startedAt: now };
    }
    return state;
  }
  // 別の通話のイベントは、いまの通話の状態を壊さない
  if (state.callId !== null && state.callId !== ev.callId) return state;
  if (ev.type === 'connected') {
    return { ...state, phase: 'connected', callId: ev.callId, direction: state.direction ?? ev.direction,
      number: state.number ?? ev.number, startedAt: state.startedAt ?? now, connectedAt: now };
  }
  if (ev.type === 'ended') {
    const talk = state.connectedAt !== null ? Math.max(0, Math.round((now - state.connectedAt) / 1000)) : null;
    return { ...state, phase: 'ended', callId: ev.callId, direction: state.direction ?? ev.direction,
      number: state.number ?? ev.number, result: ev.result, talkSeconds: ev.result === 'missed' || ev.result === 'rejected' ? null : talk };
  }
  // log_completed: 状態は変えず、印だけ付ける
  return { ...state, logCompleted: true };
}
