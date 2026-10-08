import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { EMPTY_CALL, parseZoomMessage, postMakeCall, reduceCall, toE164Jp } from './smartEmbed';
import type { CallState } from './smartEmbed';

/** iframe が読み込まれるまでの待ち。超えたら「読み込めない」案内に切り替える */
export const EMBED_LOAD_TIMEOUT_MS = 12_000;
/** 発信を依頼してから「呼び出し中」のイベントが来るまでの待ち。超えたら案内を出す */
export const DIAL_STALL_MS = 10_000;

export type EmbedPhase = 'disabled' | 'loading' | 'loaded' | 'timeout';
export type DialResult = 'sent' | 'not_dialable' | 'embed_loading' | 'embed_unavailable' | 'busy';

export interface DialRequest { number: string; at: number }

export interface ZoomPhone {
  embed: EmbedPhase;
  /** iframe の load が起きた (サインイン済みか・許可ドメインに入っているかは分からない) */
  onLoad: () => void;
  call: CallState;
  /** 直近の発信依頼 (呼び出し中のイベントを受けるまで残る) */
  pending: DialRequest | null;
  /** 発信を依頼したのに呼び出し中のイベントが来ない */
  stalled: boolean;
  dial: (rawNumber: string | null | undefined) => DialResult;
}

export interface ZoomOptions { loadTimeoutMs?: number; stallMs?: number; now?: () => number }

/**
 * Zoom Phone Smart Embed (iframe) の状態。
 * - 受信は `window` の message のうち、origin が Zoom で送信元がこの iframe の window のものだけ
 * - 発信は `zp-make-call` を Zoom の origin 宛てにだけ送る ('*' は使わない)
 * - `enabled` が false (架空サンプル等) の間は iframe を出さず、発信も受け付けない。
 *   false になったら通話・発信依頼・読み込みの状態を捨てる (iframe が外れると通話も切れ、終了のイベントは届かない。
 *   戻したときは新しい iframe として読み込みから数え直す)
 * - iframe が読み込み中・読み込めない間は発信を送らない (届かない依頼を「依頼しました」と見せない)
 */
export function useZoomPhone(enabled: boolean, opts: ZoomOptions = {}): { zoom: ZoomPhone; iframeRef: React.RefObject<HTMLIFrameElement | null> } {
  const { loadTimeoutMs = EMBED_LOAD_TIMEOUT_MS, stallMs = DIAL_STALL_MS } = opts;
  const nowRef = useRef(opts.now ?? Date.now);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [timedOut, setTimedOut] = useState(false);
  const [call, setCall] = useState<CallState>(EMPTY_CALL);
  const [pending, setPending] = useState<DialRequest | null>(null);
  const [stalled, setStalled] = useState(false);
  const [prevEnabled, setPrevEnabled] = useState(enabled);
  if (prevEnabled !== enabled) {
    setPrevEnabled(enabled);
    if (!enabled) { setLoaded(false); setTimedOut(false); setCall(EMPTY_CALL); setPending(null); setStalled(false); }
  }

  useEffect(() => {
    if (!enabled) return;
    const onMessage = (e: MessageEvent) => {
      const ev = parseZoomMessage({ origin: e.origin, source: e.source, data: e.data }, iframeRef.current?.contentWindow ?? null);
      if (ev === null) return;
      setCall(prev => reduceCall(prev, ev, nowRef.current()));
      if (ev.type === 'ringing') { setPending(null); setStalled(false); }
    };
    window.addEventListener('message', onMessage);
    return () => { window.removeEventListener('message', onMessage); };
  }, [enabled]);

  useEffect(() => {
    if (!enabled || loaded) return;
    const t = window.setTimeout(() => { setTimedOut(true); }, loadTimeoutMs);
    return () => { window.clearTimeout(t); };
  }, [enabled, loaded, loadTimeoutMs]);

  useEffect(() => {
    if (pending === null) return;
    const t = window.setTimeout(() => { setStalled(true); }, stallMs);
    return () => { window.clearTimeout(t); };
  }, [pending, stallMs]);

  const onLoad = useCallback(() => { setLoaded(true); setTimedOut(false); }, []);

  const busy = call.phase === 'ringing' || call.phase === 'connected';
  const embed: EmbedPhase = !enabled ? 'disabled' : loaded ? 'loaded' : timedOut ? 'timeout' : 'loading';
  const dial = useCallback((rawNumber: string | null | undefined): DialResult => {
    if (embed === 'disabled' || embed === 'timeout') return 'embed_unavailable';
    if (toE164Jp(rawNumber) === null) return 'not_dialable';
    if (embed === 'loading') return 'embed_loading';
    if (busy) return 'busy';
    const sent = postMakeCall(iframeRef.current?.contentWindow ?? null, rawNumber);
    if (!sent) return 'embed_unavailable';
    setPending({ number: rawNumber ?? '', at: nowRef.current() });
    setStalled(false);
    return 'sent';
  }, [embed, busy]);

  // 値が変わらない間は同じオブジェクトを返す (受け取る詳細欄・電話欄を描き直さない)
  const zoom = useMemo<ZoomPhone>(() => ({ embed, onLoad, call, pending, stalled, dial }), [embed, onLoad, call, pending, stalled, dial]);
  return { iframeRef, zoom };
}
