// /app/call-practice: 架電の練習 (第 1 段)。管理者だけ。
// 練習する人の声はブラウザの音声認識で文字にして送る。お客さん役の返事は文字のまま。
// 状態は URL の ?session= に持つ (再読み込みで同じ練習に戻れる)。?mode=fixture は決まった返事で動く確認用。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { PracticeResult } from '../../generated/PracticeResult';
import type { TalkTurn } from '../../generated/TalkTurn';
import { PracticeError, realApi, type PracticeApi } from './api';
import { createFixtureApi } from './fixtureApi';
import { APPT_SCRIPT_LINES, FIELDS, HEAT_OPTIONS, RANGES, SCRIPT_LINES, blankLabels, emptyRecord, hasHearing, isRange, toRequestRecord, type FieldDef, type Range, type RecordState } from './record';
import { ResultView } from './ResultView';
import { useSpeechInput } from './useSpeechInput';

type Step = 'loading' | 'unavailable' | 'intro' | 'practice' | 'scoring' | 'result';

const RESTORE_FAILED = '練習が途切れました。もう一度始めてください。';

function isFixture(): boolean {
  return typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('mode') === 'fixture';
}

function setSessionParam(id: string | null): void {
  const q = new URLSearchParams(window.location.search);
  if (id === null) q.delete('session');
  else q.set('session', id);
  const s = q.toString();
  window.history.replaceState(null, '', s === '' ? window.location.pathname : `?${s}`);
}

function newId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `t${String(Date.now())}${String(Math.random()).slice(2, 8)}`;
}

function mmss(sec: number): string {
  return `${String(Math.floor(sec / 60))}:${String(sec % 60).padStart(2, '0')}`;
}

export function CallPracticeScreen({ api: apiProp }: { api?: PracticeApi }) {
  const fixture = isFixture();
  const api = useMemo<PracticeApi>(() => apiProp ?? (fixture ? createFixtureApi() : realApi), [apiProp, fixture]);

  const [step, setStep] = useState<Step>('loading');
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [scriptSeen, setScriptSeen] = useState(false);
  const [range, setRange] = useState<Range>('full');
  const [dateAgreed, setDateAgreed] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<TalkTurn[]>([]);
  const [ended, setEnded] = useState(false);
  const [maxTurns, setMaxTurns] = useState(30);
  const [draft, setDraft] = useState('');
  const [pendingRep, setPendingRep] = useState<string | null>(null);
  const [streaming, setStreaming] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [record, setRecord] = useState<RecordState>(emptyRecord);
  const [heat, setHeat] = useState('');
  const [confirmBlank, setConfirmBlank] = useState(false);
  const [result, setResult] = useState<PracticeResult | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const startedAt = useRef<number>(0);
  const lastTry = useRef<{ text: string; id: string } | null>(null);
  const chatEnd = useRef<HTMLDivElement | null>(null);
  const busyRef = useRef(false);
  const endedRef = useRef(false);
  const stepRef = useRef<Step>('loading');
  const sessionRef = useRef<string | null>(null);
  useEffect(() => {
    stepRef.current = step;
    endedRef.current = ended;
    sessionRef.current = sessionId;
  });

  // 最初の読み込み: 使えるか・前の練習に戻れるか
  useEffect(() => {
    const ctl = { cancelled: false as boolean };
    const gone = (): boolean => ctl.cancelled;
    void (async () => {
      try {
        const cfg = await api.config();
        if (gone()) return;
        if (!cfg.enabled) {
          setUnavailable(cfg.unavailable_reason ?? 'いまは使えません。');
          setStep('unavailable');
          return;
        }
        setMaxTurns(cfg.max_turns);
        const sid = new URLSearchParams(window.location.search).get('session');
        if (sid !== null && sid !== '') {
          try {
            const v = await api.get(sid);
            if (gone()) return;
            setSessionId(v.session_id);
            setTranscript(v.transcript);
            setEnded(v.ended);
            setScriptSeen(v.script_seen);
            if (isRange(v.range)) setRange(v.range);
            setDateAgreed(v.date_agreed);
            if (v.result !== null) {
              setResult(v.result);
              setStep('result');
            } else {
              startedAt.current = Date.now();
              setStep('practice');
            }
            return;
          } catch {
            setSessionParam(null);
            setError(RESTORE_FAILED);
          }
        }
        setStep('intro');
      } catch (e) {
        if (gone()) return;
        if (e instanceof PracticeError && e.kind === 'forbidden') setUnavailable(e.message);
        else setUnavailable(e instanceof PracticeError ? e.message : 'いまは使えません。');
        setStep('unavailable');
      }
    })();
    return () => {
      ctl.cancelled = true;
    };
  }, [api]);

  // 経過時間
  useEffect(() => {
    if (step !== 'practice' || ended) return;
    const t = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startedAt.current) / 1000));
    }, 1000);
    return () => {
      clearInterval(t);
    };
  }, [step, ended]);

  useEffect(() => {
    chatEnd.current?.scrollIntoView({ block: 'end' });
  }, [transcript, pendingRep, streaming]);

  const send = useCallback(
    async (raw: string, source: 'text' | 'voice'): Promise<void> => {
      const text = raw.trim();
      const sid = sessionRef.current;
      if (text === '' || sid === null || busyRef.current || endedRef.current || stepRef.current !== 'practice') return;
      busyRef.current = true;
      setBusy(true);
      setError(null);
      setPendingRep(text);
      setStreaming('');
      setDraft('');
      // 同じ文を送り直すときは同じ番号を使う (サーバが二重に進めない)
      const id = lastTry.current?.text === text ? lastTry.current.id : newId();
      lastTry.current = { text, id };
      try {
        const resp = await api.turn(sid, { client_turn_id: id, text, source }, (d) => {
          setStreaming((s) => s + d);
        });
        setTranscript((t) => [...t, resp.rep_turn, resp.customer_turn]);
        lastTry.current = null;
        if (resp.ended) setEnded(true);
        if (resp.date_agreed) setDateAgreed(true);
      } catch (e) {
        // 発言は入力欄に残す
        setDraft(text);
        const closed = e instanceof PracticeError && (e.kind === 'session_closed' || e.kind === 'session_finished');
        if (closed) setEnded(true);
        setError(e instanceof PracticeError ? e.message : 'うまく動きませんでした。もう一度送ってください。');
      } finally {
        setPendingRep(null);
        setStreaming('');
        busyRef.current = false;
        setBusy(false);
      }
    },
    [api],
  );

  // 音声: 話し終わりで自動で送る。お客さん役が返事をしている間は受け付けない
  const speech = useSpeechInput({
    onCommit: (text) => {
      void send(text, 'voice');
    },
    paused: busy || ended || step !== 'practice',
  });

  const begin = async (personaId: string | null, rangeArg?: Range): Promise<void> => {
    setError(null);
    try {
      const r = await api.start({ persona_id: personaId, script_seen: scriptSeen, range: rangeArg ?? range });
      if (isRange(r.range)) setRange(r.range);
      setDateAgreed(r.range === 'hearing');
      setSessionId(r.session_id);
      sessionRef.current = r.session_id;
      setSessionParam(r.session_id);
      setTranscript([r.opening]);
      setMaxTurns(r.max_turns);
      setEnded(false);
      setRecord(emptyRecord());
      setHeat('');
      setResult(null);
      setConfirmBlank(false);
      setElapsed(0);
      startedAt.current = Date.now();
      lastTry.current = null;
      setStep('practice');
      speech.start();
    } catch (e) {
      setError(e instanceof PracticeError ? e.message : 'うまく動きませんでした。もう一度試してください。');
    }
  };

  const finish = async (): Promise<void> => {
    if (sessionId === null || busy) return;
    speech.stop();
    setError(null);
    setStep('scoring');
    try {
      const r = await api.finish(sessionId, { record: toRequestRecord(record), heat_guess: hasHearing(range) ? heat : 'mixed' });
      setResult(r);
      setStep('result');
    } catch (e) {
      setError(e instanceof PracticeError ? e.message : 'うまく動きませんでした。もう一度試してください。');
      setStep('practice');
    }
  };

  const onFinishClick = (): void => {
    if (!hasHearing(range)) {
      void finish();
      return;
    }
    if (heat === '') {
      setError('見立てを選んでから終えてください。');
      return;
    }
    if (blankLabels(record).length > 0 && !confirmBlank) {
      setConfirmBlank(true);
      return;
    }
    void finish();
  };

  const again = (personaId: string | null, rangeArg?: Range): void => {
    setSessionParam(null);
    setSessionId(null);
    setTranscript([]);
    setResult(null);
    void begin(personaId, rangeArg);
  };

  const backToIntro = (): void => {
    setSessionParam(null);
    setSessionId(null);
    setTranscript([]);
    setResult(null);
    setError(null);
    setStep('intro');
  };

  const setField = (def: FieldDef, patch: Partial<{ notHeard: boolean; value: string }>): void => {
    setRecord((r) => ({ ...r, [def.slot]: { ...r[def.slot], ...patch } }));
    setConfirmBlank(false);
  };

  return (
    <div className="w8-page">
      <nav className="w8-nav" aria-label="架電の練習">
        <a href="/" className="w8-nav-muted">
          ← ダッシュボード
        </a>
      </nav>
      <h1 className="w8-h1">架電の練習</h1>

      {step === 'loading' && <p className="w8-muted">読み込んでいます。</p>}

      {step === 'unavailable' && (
        <p className="cp-error" data-testid="practice-unavailable">
          {unavailable}
        </p>
      )}

      {step === 'intro' && (
        <section className="w8-card cp-intro" data-testid="practice-intro">
          <h2 className="w8-h2">練習する範囲を選んでください</h2>
          <div className="cp-ranges" role="radiogroup" aria-label="練習する範囲">
            {RANGES.map((r) => (
              <label key={r.value} className={`cp-range ${range === r.value ? 'cp-range-on' : ''}`} data-testid={`range-${r.value}`}>
                <input
                  type="radio"
                  name="range"
                  value={r.value}
                  checked={range === r.value}
                  onChange={() => {
                    setRange(r.value);
                  }}
                />{' '}
                <strong>{r.label}</strong>
                <span className="w8-muted">{r.detail}</span>
              </label>
            ))}
          </div>
          <ol data-testid="intro-steps">
            {range !== 'hearing' && <li>担当者につながった直後から始まります。断り文句をかわして、日時を決めてください。</li>}
            {range === 'hearing' && <li>アポの日時が決まった直後から始まります。</li>}
            {range === 'full' && <li>日時が決まったら、そのまま 4 項目のヒアリングに進みます。</li>}
            {hasHearing(range) && <li>台本の 4 項目を聞いて、右の記録欄に書いてください。</li>}
            <li>終えたら、結果と、お客さんの型を見せます。</li>
          </ol>
          <p className="w8-muted">マイクに向かって話すと、話し終わりに自動で送られます。文字で入力することもできます。</p>
          <label>
            <input
              type="checkbox"
              checked={scriptSeen}
              onChange={(e) => {
                setScriptSeen(e.target.checked);
              }}
            />{' '}
            台本を横に出して練習する
          </label>
          <div className="cp-actions">
            <button
              type="button"
              className="w8-button"
              onClick={() => {
                void begin(null);
              }}
            >
              練習を始める
            </button>
          </div>
          {error !== null && <p className="cp-error">{error}</p>}
        </section>
      )}

      {(step === 'practice' || step === 'scoring') && (
        <div className="cp-layout" data-testid="practice-room">
          <div>
            {scriptSeen && (
              <div className="cp-script" data-testid="script-panel">
                {[...(range === 'hearing' ? [] : APPT_SCRIPT_LINES), ...(hasHearing(range) ? SCRIPT_LINES : [])].map((s) => (
                  <div key={s.group}>
                    <strong>{s.group}</strong>
                    <p>{s.line}</p>
                  </div>
                ))}
              </div>
            )}
            {range !== 'hearing' && (
              <p className={`cp-stage ${dateAgreed ? 'cp-stage-ok' : ''}`} data-testid="stage-banner">
                {dateAgreed ? '日時が決まりました' : '日時はまだ決まっていません'}
              </p>
            )}
            <div className="cp-chat" data-testid="chat" aria-live="polite">
              {transcript.map((t) => (
                <div key={t.n} className={`cp-msg ${t.speaker === 'rep' ? 'cp-msg-rep' : 'cp-msg-customer'}`} data-testid={`turn-${String(t.n)}`}>
                  <small>{t.speaker === 'rep' ? 'あなた' : 'お客さん'}</small>
                  {t.text}
                </div>
              ))}
              {pendingRep !== null && (
                <div className="cp-msg cp-msg-rep" data-testid="pending-rep">
                  <small>あなた</small>
                  {pendingRep}
                </div>
              )}
              {busy && (
                <div className="cp-msg cp-msg-customer" data-testid="streaming">
                  <small>お客さん</small>
                  {streaming === '' ? '…' : streaming}
                </div>
              )}
              <div ref={chatEnd} />
            </div>

            {ended ? (
              <p className="cp-notice" data-testid="ended-notice">
                {range === 'appointment'
                  ? dateAgreed
                    ? '日時が決まりました。「終える」を押して結果を見てください。'
                    : 'お客さんとの会話は終わりました。「終える」を押して結果を見てください。'
                  : 'お客さんとの会話は終わりました。右の記録を見直して終えてください。'}
              </p>
            ) : (
              <>
                <div className="cp-mic">
                  {speech.supported ? (
                    <button
                      type="button"
                      className={`w8-button ${speech.listening ? 'cp-mic-on' : ''}`}
                      aria-pressed={speech.listening}
                      data-testid="mic-toggle"
                      onClick={speech.listening ? speech.stop : speech.start}
                    >
                      {speech.listening ? '止める' : '話す'}
                    </button>
                  ) : null}
                  <span className="w8-muted" data-testid="mic-state">
                    {speech.listening ? '聞き取っています' : '文字で入力できます'}
                  </span>
                  <span className="cp-elapsed" data-testid="elapsed">
                    経過 {mmss(elapsed)}
                  </span>
                  <span className="w8-muted">往復は {maxTurns} 回まで</span>
                </div>
                <div className="cp-interim" data-testid="interim" aria-live="polite">
                  {speech.interim !== '' ? `聞き取り中: ${speech.interim}` : ''}
                </div>
                {speech.notice !== null && (
                  <p className="cp-notice" data-testid="speech-notice">
                    {speech.notice}
                  </p>
                )}
                <form
                  className="cp-input"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void send(draft, 'text');
                  }}
                >
                  <input
                    type="text"
                    aria-label="あなたの発言"
                    data-testid="draft"
                    value={draft}
                    placeholder="ここに文字で入力して送ることもできます"
                    disabled={busy}
                    onChange={(e) => {
                      setDraft(e.target.value);
                    }}
                  />
                  <button type="submit" className="w8-button" disabled={busy || draft.trim() === ''}>
                    送る
                  </button>
                </form>
              </>
            )}
            {error !== null && (
              <p className="cp-error" data-testid="practice-error">
                {error}
              </p>
            )}
          </div>

          <div className="w8-card" data-testid="record-panel">
            {hasHearing(range) && (
              <>
            <h2 className="w8-h2">記録欄</h2>
            <div className="cp-fields">
              {FIELDS.map((f, i) => (
                <div key={f.slot} className="cp-field">
                  {FIELDS[i - 1]?.group !== f.group && <p className="cp-group">{f.group}</p>}
                  <label className="cp-field-label" htmlFor={`rec-${f.slot}`}>
                    {f.label}
                  </label>
                  <div className="cp-field-row">
                    {f.options === undefined ? (
                      <input
                        id={`rec-${f.slot}`}
                        type="text"
                        value={record[f.slot].value}
                        placeholder={f.placeholder ?? ''}
                        disabled={record[f.slot].notHeard}
                        onChange={(e) => {
                          setField(f, { value: e.target.value });
                        }}
                      />
                    ) : (
                      <select
                        id={`rec-${f.slot}`}
                        value={record[f.slot].value}
                        disabled={record[f.slot].notHeard}
                        onChange={(e) => {
                          setField(f, { value: e.target.value });
                        }}
                      >
                        <option value="">選んでください</option>
                        {f.options.map((o) => (
                          <option key={o.value} value={o.value}>
                            {o.label}
                          </option>
                        ))}
                      </select>
                    )}
                    <label>
                      <input
                        type="checkbox"
                        checked={record[f.slot].notHeard}
                        onChange={(e) => {
                          setField(f, { notHeard: e.target.checked });
                        }}
                      />{' '}
                      聞けていない
                    </label>
                  </div>
                </div>
              ))}
            </div>
            <fieldset className="cp-heat">
              <legend>見立て</legend>
              {HEAT_OPTIONS.map((o) => (
                <label key={o.value}>
                  <input
                    type="radio"
                    name="heat"
                    value={o.value}
                    checked={heat === o.value}
                    onChange={() => {
                      setHeat(o.value);
                    }}
                  />{' '}
                  {o.label}
                </label>
              ))}
            </fieldset>
              </>
            )}
            {!hasHearing(range) && <p className="w8-muted" data-testid="appt-panel-note">日時が決まったら、または会話が終わったら「終える」を押してください。</p>}
            {confirmBlank && (
              <div className="cp-notice" data-testid="blank-confirm">
                <p>空欄が残っています。空欄は「聞けていない」として採点します。</p>
                <button
                  type="button"
                  className="w8-button"
                  onClick={() => {
                    void finish();
                  }}
                >
                  このまま進む
                </button>{' '}
                <button
                  type="button"
                  className="w8-button"
                  onClick={() => {
                    setConfirmBlank(false);
                  }}
                >
                  戻る
                </button>
              </div>
            )}
            <div className="cp-actions">
              <button type="button" className="w8-button" data-testid="finish" disabled={busy || step === 'scoring'} onClick={onFinishClick}>
                終える
              </button>
            </div>
            {step === 'scoring' && (
              <p className="w8-muted" data-testid="scoring">
                採点しています。少しお待ちください。
              </p>
            )}
          </div>
        </div>
      )}

      {step === 'result' && result !== null && (
        <ResultView
          result={result}
          onAgainSame={() => {
            again(result.persona.id, isRange(result.range) ? result.range : undefined);
          }}
          onAgainOther={() => {
            again(null, isRange(result.range) ? result.range : undefined);
          }}
          onChangeRange={backToIntro}
        />
      )}
    </div>
  );
}
