import { useCallback, useEffect, useRef, useState } from 'react';
import { EMPTY_CALL, parseZoomMessage, postMakeCall, reduceCall, toE164Jp } from './smartEmbed';
import type { CallState } from './smartEmbed';

/** iframe が読み込まれるまでの待ち。超えたら「読み込めない」案内に切り替える */
export const EMBED_LOAD_TIMEOUT_MS = 12_000;
/** 発信を依頼してから「呼び出し中」のイベントが来るまでの待ち。超えたら案内を出す */
export const DIAL_STALL_MS = 10_000;

export type EmbedPhase = 'disabled' | 'loading' | 'loaded' | 'timeout';
export type DialResult = 'sent' | 'not_dialable' | 'embed_unavailable' | 'busy';

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
 * - `enabled` が false (架空サンプル等) の間は iframe を出さず、発信も受け付けない
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
  const dial = useCallback((rawNumber: string | null | undefined): DialResult => {
    if (!enabled) return 'embed_unavailable';
    if (toE164Jp(rawNumber) === null) return 'not_dialable';
    if (busy) return 'busy';
    const sent = postMakeCall(iframeRef.current?.contentWindow ?? null, rawNumber);
    if (!sent) return 'embed_unavailable';
    setPending({ number: rawNumber ?? '', at: nowRef.current() });
    setStalled(false);
    return 'sent';
  }, [enabled, busy]);

  const embed: EmbedPhase = !enabled ? 'disabled' : loaded ? 'loaded' : timedOut ? 'timeout' : 'loading';
  return { iframeRef, zoom: { embed, onLoad, call, pending, stalled, dial } };
}
