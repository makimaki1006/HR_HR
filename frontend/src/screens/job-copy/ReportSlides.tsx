/**
 * 顧客報告をスライド型で作る画面（暫定 2026-11-15。見た目は後で決める）。
 * 左に範囲・期間・スライドの一覧、中央に選んだスライドを大きく。PDF はブラウザの印刷で横長1枚ずつ。
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { closeReport } from './navigation';
import { GoLink } from './Pressable';
import { loadReportInput } from './reportSlidesLoad';
import type { ReportTarget } from './reportSlidesLoad';
import { alignedPeriod, buildReport, defaultPeriod, MAX_NEXT_ACTIONS, REPORT_SENDER, shortDay, SLIDE_NAMES } from './reportSlidesModel';
import type { NextAction, ReportDraft, ReportInput, Slide, SlideId } from './reportSlidesModel';
import { todayJst } from './timelineModel';
import './report-slides.css';

type Source = { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; input: ReportInput };

export function ReportSlides({ target }: { target: ReportTarget }) {
  const key = `${target.scope}:${target.id}`;
  const [loaded, setLoaded] = useState<{ key: string; revision: number; source: Source } | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    // 開き直しはこのタブで読んだ結果を使う。「最新にする」（revision > 0）だけ読み直す。
    void loadReportInput({ scope: target.scope, id: target.id }, controller.signal, { fresh: revision > 0 }).then(result => {
      if (controller.signal.aborted) return;
      setLoaded({ key: `${target.scope}:${target.id}`, revision, source: result.ok ? { status: 'ready', input: result.input } : { status: 'error', message: result.message || '読み込めませんでした。' } });
    });
    return () => { controller.abort(); };
  }, [target.scope, target.id, revision]);
  const source: Source = loaded?.key === key && loaded.revision === revision ? loaded.source : { status: 'loading' };
  const back = <GoLink onClick={closeReport}>{target.scope === 'deal' ? '案件の画面に戻る' : '求人の画面に戻る'}</GoLink>;
  const refresh = () => { setRevision(value => value + 1); };
  if (source.status === 'loading') return <main className="rs-root" aria-label="報告のスライドを作る"><div className="rs-top">{back}<h1>報告のスライドを作る</h1></div><p role="status">{target.scope === 'deal' ? '案件の求人と応募を読み込んでいます…' : '求人と応募を読み込んでいます…'}</p></main>;
  if (source.status === 'error') return <main className="rs-root" aria-label="報告のスライドを作る"><div className="rs-top">{back}<h1>報告のスライドを作る</h1></div><p role="alert">{source.message}</p><button type="button" className="jc-button" onClick={refresh}>もう一度読み込む</button></main>;
  return <ReportEditor key={key} input={source.input} back={back} onRefresh={refresh} />;
}

const emptyActions = (): NextAction[] => Array.from({ length: MAX_NEXT_ACTIONS }, () => ({ text: '', sub: '' }));
function initialDraft(input: ReportInput): ReportDraft {
  // 件名の既定は空（案件名には社内の書き方が入るため）。空なら表紙は期間だけ。
  return { ...defaultPeriod(input.asOf), addressee: input.company ?? '', subject: '', owner: '', off: [], next: emptyActions(), nextHeadline: '' };
}
/**
 * 下書き（期間・表紙・入れるスライド・次に行うこと）はこの画面の中だけに持つ。
 * 既存の記録欄の保存先（job_copy_listing_notes）は欄の種類が固定（表の CHECK 制約）で、新しい欄を足すには表の作り直しが要るため使わない。
 * 同じタブで報告の画面を閉じて開き直したときだけ残る（再読み込みで消える）。
 */
const drafts = new Map<string, ReportDraft>();

/** 印刷用に並べたスライドのうち、枠からはみ出しているものの名前。 */
function overflowing(deck: HTMLElement | null): string[] {
  if (!deck) return [];
  const names: string[] = [];
  for (const slide of deck.querySelectorAll<HTMLElement>('.rs-slide')) {
    const parts = [...slide.querySelectorAll<HTMLElement>('.rs-in, .rs-body')];
    const tall = parts.some(part => part.clientHeight > 0 && part.scrollHeight > part.clientHeight + 2);
    const cut = [...slide.querySelectorAll<HTMLElement>('.rs-hb-label, .rs-d')].some(label => label.clientWidth > 0 && label.scrollWidth > label.clientWidth + 1);
    const id = slide.dataset.slide as SlideId | undefined;
    if ((tall || cut) && id) names.push(`${SLIDE_NAMES[id]}${tall ? '（枠からはみ出しています）' : '（名前が長く、途中までしか出ていません）'}`);
  }
  return names;
}

function ReportEditor({ input, back, onRefresh }: { input: ReportInput; back: ReactNode; onRefresh: () => void }) {
  const draftKey = `${input.scope}:${input.id}`;
  const [draft, setDraft] = useState<ReportDraft>(() => drafts.get(draftKey) ?? initialDraft(input));
  const [current, setCurrent] = useState<SlideId>('cover');
  const [overflow, setOverflow] = useState<string[]>([]);
  const deck = useRef<HTMLDivElement>(null);
  const change = (update: Partial<ReportDraft>) => { setDraft(value => { const next = { ...value, ...update }; drafts.set(draftKey, next); return next; }); };
  const validPeriod = /^\d{4}-\d{2}-\d{2}$/.test(draft.from) && /^\d{4}-\d{2}-\d{2}$/.test(draft.to) && draft.from <= draft.to;
  const fallback = useMemo(() => defaultPeriod(input.asOf), [input.asOf]);
  const from = validPeriod ? draft.from : fallback.from;
  const to = validPeriod ? draft.to : fallback.to;
  const result = useMemo(() => buildReport(input, { from, to, addressee: draft.addressee, subject: draft.subject, owner: draft.owner, sender: REPORT_SENDER, createdOn: todayJst(), nextActions: draft.next, nextHeadline: draft.nextHeadline, off: draft.off, include: { outcomes: draft.outcomes !== false } }), [input, from, to, draft]);
  const included = result.slides.filter(slide => !draft.off.includes(slide.id));
  const shown = result.slides.find(slide => slide.id === current) ?? result.slides[0];
  const index = shown ? result.slides.indexOf(shown) : 0;
  const pageOf = (slide: Slide): number | null => { const n = included.indexOf(slide); return n < 0 ? null : n + 1; };
  const move = (step: number) => { const next = result.slides[(index + step + result.slides.length) % result.slides.length]; if (next) setCurrent(next.id); };
  const toggle = (id: SlideId, on: boolean) => { change({ off: on ? draft.off.filter(item => item !== id) : [...draft.off, id] }); };
  const setAction = (i: number, update: Partial<NextAction>) => { change({ next: draft.next.map((item, n) => n === i ? { ...item, ...update } : item) }); };
  const preset = (weeks: number) => { change(alignedPeriod(input.asOf, weeks)); };
  useLayoutEffect(() => {
    const frame = requestAnimationFrame(() => { setOverflow(overflowing(deck.current)); });
    return () => { cancelAnimationFrame(frame); };
  }, [result, draft.off]);

  return <main className="rs-root" aria-label="報告のスライドを作る">
    <div className="rs-top">{back}<h1>報告のスライドを作る</h1>
      <div className="rs-actions">
        <button type="button" className="jc-button" onClick={onRefresh}>最新にする</button>
        <button type="button" className="jc-button jc-primary rs-print-button" disabled={!included.length || !validPeriod} onClick={() => { window.print(); }}>PDF に出す</button>
      </div>
    </div>
    <p className="rs-save-status">期間・表紙・入れるスライド・次に行うことは、この画面の中だけに残ります（再読み込みで消えます）。HubSpot には書きません。</p>
    <p className="rs-hint">「PDF に出す」で印刷の画面が開きます。送り先を「PDF に保存」にすると、横長のスライドが1枚ずつのPDFになります。</p>
    <div className="rs-deck">
      <aside className="rs-side" aria-label="報告の設定">
        <h2>範囲</h2>
        <p className="rs-scope">{input.scope === 'deal' ? '案件' : '求人'}：{input.name ?? (input.scope === 'deal' ? '案件名を読み込めませんでした' : '求人名を読み込めませんでした')}{input.scope === 'deal' && <small>この案件の求人 {String(input.listings.length)}件をまとめます</small>}</p>
        <h2>期間</h2>
        <div className="rs-period">
          <label>開始<input type="date" value={draft.from} max={draft.to} onChange={event => { change({ from: event.target.value }); }} /></label>
          <label>終了<input type="date" value={draft.to} min={draft.from} onChange={event => { change({ to: event.target.value }); }} /></label>
        </div>
        <div className="rs-presets" role="group" aria-label="期間をそろえる">{[4, 8, 12].map(weeks => <button type="button" key={weeks} className="jc-button" onClick={() => { preset(weeks); }}>直近{String(weeks)}週</button>)}</div>
        {!validPeriod && <p className="jc-error" role="alert">開始日と終了日を正しく入力してください（開始日は終了日以前）。</p>}
        <h2>表紙</h2>
        <label className="rs-field">宛名（御中の前）<input value={draft.addressee} maxLength={100} onChange={event => { change({ addressee: event.target.value }); }} /></label>
        <label className="rs-field">件名（空欄なら期間だけ）<input value={draft.subject} maxLength={100} onChange={event => { change({ subject: event.target.value }); }} /></label>
        <label className="rs-field">担当者名<input value={draft.owner} maxLength={50} onChange={event => { change({ owner: event.target.value }); }} /></label>
        <h2>スライド</h2>
        <ol className="rs-thumbs">{result.slides.map(slide => {
          const on = !draft.off.includes(slide.id);
          return <li key={slide.id} className={`${slide.id === shown?.id ? 'rs-on' : ''} ${on ? '' : 'rs-off'}`}>
            <input type="checkbox" checked={on} aria-label={`${slide.name}を入れる`} onChange={event => { toggle(slide.id, event.target.checked); }} />
            <button type="button" aria-current={slide.id === shown?.id ? 'true' : undefined} onClick={() => { setCurrent(slide.id); }}><span className="rs-num">{String(pageOf(slide) ?? '－')}</span>{slide.name}</button>
          </li>;
        })}</ol>
        {input.scope === 'deal' && <label className="rs-check"><input type="checkbox" checked={draft.outcomes !== false} onChange={event => { change({ outcomes: event.target.checked }); }} />面接・採用を出す<small>案件に記録された、ご契約からの合計です。値が古いときは外してください。</small></label>}
        <h2>次に行うこと</h2>
        <div className="rs-next-inputs">
          {draft.next.map((item, i) => <fieldset key={i}><legend>{String(i + 1)}</legend>
            <input aria-label={`次に行うこと${String(i + 1)}`} value={item.text} maxLength={120} placeholder="行うこと" onChange={event => { setAction(i, { text: event.target.value }); }} />
            <input aria-label={`次に行うこと${String(i + 1)}の補足`} value={item.sub} maxLength={120} placeholder="時期・補足" onChange={event => { setAction(i, { sub: event.target.value }); }} />
          </fieldset>)}
          <label className="rs-field">このスライドの見出し<input value={draft.nextHeadline} maxLength={120} placeholder="空欄なら1つ目の文" onChange={event => { change({ nextHeadline: event.target.value }); }} /></label>
        </div>
        {overflow.length > 0 && <section className="rs-excluded rs-overflow" aria-label="はみ出し">
          <h2>はみ出し（社内向け）</h2>
          <ul>{overflow.map(name => <li key={name}>{name}</li>)}</ul>
        </section>}
        {result.excluded.length > 0 && <section className="rs-excluded" aria-label="外したもの">
          <h2>外したもの（社内向け）</h2>
          <ul>{result.excluded.map((item, i) => <li key={i}><b>{item.name}</b>：{item.reason}</li>)}</ul>
        </section>}
      </aside>
      <section className="rs-stage" aria-label="スライド">
        {shown && <div className={draft.off.includes(shown.id) ? 'rs-big rs-dim' : 'rs-big'}><SlideView slide={shown} page={pageOf(shown)} /></div>}
        <div className="rs-nav"><button type="button" className="jc-button" onClick={() => { move(-1); }}>‹ 前</button><span>{String(index + 1)} / {String(result.slides.length)}{shown && draft.off.includes(shown.id) ? '（出力しない）' : ''}</span><button type="button" className="jc-button" onClick={() => { move(1); }}>次 ›</button></div>
      </section>
    </div>
    {createPortal(<div className="rs-print-deck" aria-hidden="true" ref={deck}>{included.map(slide => <SlideView key={slide.id} slide={slide} page={pageOf(slide)} />)}</div>, document.body)}
  </main>;
}

const pct = (value: number, max: number) => `${String(max > 0 ? Math.max(2, Math.round(value / max * 100)) : 2)}%`;

function HBars({ rows, unit = '件' }: { rows: readonly { label: string; count: number }[]; unit?: string }) {
  const max = Math.max(0, ...rows.map(row => row.count));
  return <div className="rs-hb">{rows.map((row, i) => <div className="rs-hb-row" key={i}><span className="rs-hb-label">{row.label}</span><span className="rs-hb-bar"><i style={{ width: pct(row.count, max) }} /><b>{String(row.count)}{unit}</b></span></div>)}</div>;
}

export function SlideView({ slide, page }: { slide: Slide; page: number | null }) {
  if (slide.id === 'cover') return <div className="rs-slide rs-cover" data-slide={slide.id}><div className="rs-in">
    {slide.addressee && <div className="rs-to">{slide.addressee}</div>}
    <div className="rs-title">{slide.title}</div>
    <div className="rs-sub">{slide.subtitle}</div>
  </div><div className="rs-cover-foot">{slide.footer}</div></div>;
  return <div className="rs-slide" data-slide={slide.id}><div className="rs-in">
    <div className="rs-kicker">{slide.kicker}</div>
    <div className="rs-msg">{slide.headline}</div>
    <div className="rs-body">{body(slide)}</div>
    {slide.note && <div className="rs-note">{slide.note}</div>}
  </div><div className="rs-foot"><span>採用活動のご報告</span><span>{page === null ? '' : String(page)}</span></div></div>;
}

function body(slide: Slide) {
  switch (slide.id) {
    case 'summary': return <>
      <div className="rs-kp">{slide.kpis.map(kpi => <div key={kpi.label}><span>{kpi.label}</span><b>{kpi.value}</b>{kpi.sub && <small>{kpi.sub}</small>}</div>)}</div>
      {slide.totals && <div className="rs-totals"><div className="rs-totals-h">{slide.totals.heading}</div><div className="rs-kp">{slide.totals.kpis.map(kpi => <div key={kpi.label}><span>{kpi.label}</span><b>{kpi.value}</b></div>)}</div></div>}
      {slide.points.length > 0 && <ul className="rs-points">{slide.points.map((point, i) => <li key={i}>{point}</li>)}</ul>}
    </>;
    case 'trend': {
      const max = Math.max(0, ...slide.bars.map(bar => bar.count));
      return <div className="rs-vbars">{slide.bars.map((bar, i) => <div className={bar.marks.length ? 'rs-c rs-ev' : 'rs-c'} key={i}>
        {bar.marks.length > 0 && <span className="rs-mark">{bar.marks.length > 1 ? `見直し ${String(bar.marks.length)}件` : bar.marks[0]}</span>}
        <span className="rs-v">{String(bar.count)}</span><i style={{ height: pct(bar.count, max) }} /><em>{bar.label}</em></div>)}</div>;
    }
    case 'listings': return <HBars rows={slide.bars} />;
    case 'changes': {
      const max = Math.max(1, ...slide.cards.flatMap(card => [card.before, card.after]));
      return <div className="rs-pairs">{slide.cards.map((card, i) => <div className="rs-pair" key={i}>
        <div className="rs-d">{shortDay(card.date)} ・ {card.title}</div>
        <div className="rs-w">{card.what}</div>
        <div className="rs-pp"><span>前の4週</span><i className="rs-b1" style={{ width: pct(card.before, max) }} /><b>{String(card.before)}件</b><span>後の4週</span><i className="rs-b2" style={{ width: pct(card.after, max) }} /><b>{String(card.after)}件</b></div>
      </div>)}</div>;
    }
    case 'applicants': return <div className="rs-three">{slide.groups.map(group => <div key={group.title}><h3>{group.title}</h3><HBars rows={group.rows} unit="" /></div>)}</div>;
    case 'reasons': return <HBars rows={slide.bars} />;
    case 'next': return <ol className="rs-next">{slide.items.map((item, i) => <li key={i}><span>{item.text}{item.sub && <small>{item.sub}</small>}</span></li>)}</ol>;
    default: return null;
  }
}

