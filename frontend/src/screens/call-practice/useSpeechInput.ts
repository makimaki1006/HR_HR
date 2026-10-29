// ブラウザの音声認識 (Web Speech API) で、練習する人の声を文字にする。
// 話している途中の文字を interim に出し、話し終わり (認識の確定、または無音が続いた時点) で onCommit に渡す。
// お客さん役が返事をしている間 (paused) は、聞き取った文字を捨てる。

import { useCallback, useEffect, useRef, useState } from 'react';

interface RecResult {
  readonly isFinal: boolean;
  readonly 0: { readonly transcript: string };
}
interface RecEvent {
  readonly resultIndex: number;
  readonly results: { readonly length: number; readonly [i: number]: RecResult };
}
interface Recognition {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onresult: ((e: RecEvent) => void) | null;
  onerror: ((e: { error: string }) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
}
type RecognitionCtor = new () => Recognition;

function getCtor(): RecognitionCtor | null {
  if (typeof window === 'undefined') return null;
  const w = window as unknown as { SpeechRecognition?: RecognitionCtor; webkitSpeechRecognition?: RecognitionCtor };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

export const UNSUPPORTED_MESSAGE = 'このブラウザでは音声入力が使えません。文字で入力してください。';
export const DENIED_MESSAGE = 'マイクが使えませんでした。文字で入力してください。';
const NO_MIC_MESSAGE = 'マイクが見つかりません。文字で入力してください。';
const NETWORK_MESSAGE = '音声を文字にする機能につながりませんでした。文字で入力してください。';

/** 確定してからまとめて送るまでの待ち / 確定しないまま無音が続いたときの待ち (ミリ秒) */
export const FINAL_WAIT_MS = 900;
export const INTERIM_WAIT_MS = 1800;

export interface SpeechInput {
  supported: boolean;
  listening: boolean;
  /** 話している途中の文字 */
  interim: string;
  /** 使えない理由 (画面にそのまま出せる) */
  notice: string | null;
  start: () => void;
  stop: () => void;
}

export function useSpeechInput(opts: { onCommit: (text: string) => void; paused: boolean }): SpeechInput {
  const supported = getCtor() !== null;
  const [listening, setListening] = useState(false);
  const [interim, setInterim] = useState('');
  const [notice, setNotice] = useState<string | null>(supported ? null : UNSUPPORTED_MESSAGE);

  const onCommitRef = useRef(opts.onCommit);
  const pausedRef = useRef(opts.paused);
  const recRef = useRef<Recognition | null>(null);
  const wantRef = useRef(false);
  const finalBuf = useRef('');
  const interimBuf = useRef('');
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    onCommitRef.current = opts.onCommit;
  });

  const clearTimer = (): void => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  };

  const reset = useCallback((): void => {
    clearTimer();
    finalBuf.current = '';
    interimBuf.current = '';
    setInterim('');
  }, []);

  // お客さん役が返事をしている間: 聞き取った文字を捨てる (表示は描画の中で、バッファは副作用で片づける)
  const [prevPaused, setPrevPaused] = useState(opts.paused);
  if (prevPaused !== opts.paused) {
    setPrevPaused(opts.paused);
    if (opts.paused) setInterim('');
  }
  useEffect(() => {
    pausedRef.current = opts.paused;
    if (opts.paused) {
      clearTimer();
      finalBuf.current = '';
      interimBuf.current = '';
    }
  }, [opts.paused]);

  const commit = useCallback(
    (viaInterim: boolean): void => {
      const text = (finalBuf.current + interimBuf.current).trim();
      reset();
      // 確定しないまま送るときは、あとから同じ言葉の確定が届いて二重にならないよう、聞き取りを張り直す
      if (viaInterim) recRef.current?.abort();
      if (text !== '' && !pausedRef.current) onCommitRef.current(text);
    },
    [reset],
  );

  const spawn = useCallback((): void => {
    const Ctor = getCtor();
    if (Ctor === null) return;
    const rec = new Ctor();
    rec.lang = 'ja-JP';
    rec.continuous = true;
    rec.interimResults = true;
    rec.onresult = (e) => {
      if (pausedRef.current) return;
      let interimNow = '';
      for (let i = e.resultIndex; i < e.results.length; i += 1) {
        const r = e.results[i];
        if (r === undefined) continue;
        if (r.isFinal) finalBuf.current += r[0].transcript;
        else interimNow += r[0].transcript;
      }
      interimBuf.current = interimNow;
      setInterim(finalBuf.current + interimNow);
      clearTimer();
      const viaInterim = interimNow !== '';
      timer.current = setTimeout(() => {
        commit(viaInterim);
      }, viaInterim ? INTERIM_WAIT_MS : FINAL_WAIT_MS);
    };
    rec.onerror = (e) => {
      if (e.error === 'not-allowed' || e.error === 'service-not-allowed') {
        wantRef.current = false;
        setNotice(DENIED_MESSAGE);
      } else if (e.error === 'audio-capture') {
        wantRef.current = false;
        setNotice(NO_MIC_MESSAGE);
      } else if (e.error === 'network') {
        wantRef.current = false;
        setNotice(NETWORK_MESSAGE);
      }
      // no-speech / aborted は続けてよい
    };
    rec.onend = () => {
      if (wantRef.current) {
        // Chrome は無音が続くと聞き取りを終えるので、聞きたい間は張り直す
        setTimeout(() => {
          if (wantRef.current && recRef.current === rec) {
            try {
              rec.start();
            } catch {
              // すでに始まっている
            }
          }
        }, 150);
      } else {
        setListening(false);
      }
    };
    recRef.current = rec;
    try {
      rec.start();
    } catch {
      setNotice(DENIED_MESSAGE);
      wantRef.current = false;
      return;
    }
    setListening(true);
  }, [commit]);

  const start = useCallback((): void => {
    if (getCtor() === null) {
      setNotice(UNSUPPORTED_MESSAGE);
      return;
    }
    setNotice(null);
    wantRef.current = true;
    if (recRef.current === null) spawn();
    else {
      try {
        recRef.current.start();
        setListening(true);
      } catch {
        // すでに聞き取り中
        setListening(true);
      }
    }
  }, [spawn]);

  const stop = useCallback((): void => {
    wantRef.current = false;
    reset();
    recRef.current?.stop();
    setListening(false);
  }, [reset]);

  useEffect(
    () => () => {
      wantRef.current = false;
      clearTimer();
      recRef.current?.abort();
      recRef.current = null;
    },
    [],
  );

  return { supported, listening, interim, notice, start, stop };
}
