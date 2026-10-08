import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { EMPTY_CALL, isFromEmbed, parseZoomMessage, postMakeCall, reduceCall, toE164Jp } from './smartEmbed';
import type { CallState } from './smartEmbed';

/** iframe が読み込まれるまでの待ち。超えたら「読み込めない」案内に切り替える */
export const EMBED_LOAD_TIMEOUT_MS = 12_000;
/** 発信を依頼してから「呼び出し中」のイベントが来るまでの待ち。超えたら「Zoom が応答しない」と案内し、Zoom の枠を開く */
export const DIAL_STALL_MS = 6_000;

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
  /**
   * 読み込んでから、枠 (Zoom) から何かしらのメッセージを 1 度でも受けたか。
   * 受けていないまま発信するときは、サインインしていない可能性が高い
   */
  heard: boolean;
  /** 時刻 (通話の経過時間の表示に使う。通話の開始時刻と同じ時計) */
  now: () => number;
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
  // 時計は最初に渡されたものを使い続ける (描画中にも読むので ref ではなく state に置く)
  const [now] = useState(() => opts.now ?? Date.now);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [timedOut, setTimedOut] = useState(false);
  const [call, setCall] = useState<CallState>(EMPTY_CALL);
  const [pending, setPending] = useState<DialRequest | null>(null);
  const [stalled, setStalled] = useState(false);
  const [heard, setHeard] = useState(false);
  const [prevEnabled, setPrevEnabled] = useState(enabled);
  if (prevEnabled !== enabled) {
    setPrevEnabled(enabled);
    if (!enabled) { setLoaded(false); setTimedOut(false); setCall(EMPTY_CALL); setPending(null); setStalled(false); setHeard(false); }
  }

  useEffect(() => {
    if (!enabled) return;
    const onMessage = (e: MessageEvent) => {
      const frameWindow = iframeRef.current?.contentWindow ?? null;
      if (!isFromEmbed({ origin: e.origin, source: e.source }, frameWindow)) return;
      setHeard(true);
      const ev = parseZoomMessage({ origin: e.origin, source: e.source, data: e.data }, frameWindow);
      if (ev === null) return;
      setCall(prev => reduceCall(prev, ev, now()));
      // 通話が動いたら、発信の依頼は届いている (呼び出し中を飛ばして通話中・終了が来ても待ちを解く)
      if (ev.type !== 'log_completed') { setPending(null); setStalled(false); }
    };
    window.addEventListener('message', onMessage);
    return () => { window.removeEventListener('message', onMessage); };
  }, [enabled, now]);

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
    setPending({ number: rawNumber ?? '', at: now() });
    setStalled(false);
    return 'sent';
  }, [embed, busy, now]);

  // 値が変わらない間は同じオブジェクトを返す (受け取る詳細欄・電話欄を描き直さない)
  const zoom = useMemo<ZoomPhone>(() => ({ embed, onLoad, call, pending, stalled, heard, now, dial }), [embed, onLoad, call, pending, stalled, heard, now, dial]);
  return { iframeRef, zoom };
}
