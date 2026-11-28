/**
 * 顧客報告をスライド型で作る画面。並びは定期ミーティングの資料に合わせる（2026-11-20。見た目・出力形式は後で決める）。
 * 左に範囲・期間・表紙・スライドの一覧、中央に選んだスライドと、そのスライドに書く欄。PDF はブラウザの印刷で横長1枚ずつ。
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { closeReport } from './navigation';
import { GoLink } from './Pressable';
import { loadReportInput } from './reportSlidesLoad';
import type { ReportTarget } from './reportSlidesLoad';
import { alignedPeriod, buildReport, DECK_NAME, DEFAULT_TITLE, emptyListingText, emptyTexts, initialPeriods, periodsFromMtg, REPORT_SENDER, shortDay, SUPPLEMENT_SLIDES, usableMtgDates } from './reportSlidesModel';
import type { ListingText, NumberRow, ReportDraft, ReportInput, ReportTexts, Slide, TextBox } from './reportSlidesModel';
import { addDays, todayJst } from './timelineModel';
import './report-slides.css';

type Source = { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; input: ReportInput };

export function ReportSlides({ target }: { target: ReportTarget }) {
  const key = `${target.scope}:${target.id}`;
  const [loaded, setLoaded] = useState<{ key: string; revision: number; source: Source } | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    // 開き直しはこのタブで読んだ結果を使う。「最新にする」（revision > 0）だけ読み直す。
    const onUpdate = (input: ReportInput) => {
      if (!controller.signal.aborted) setLoaded({ key: `${target.scope}:${target.id}`, revision, source: { status: 'ready', input } });
    };
    void loadReportInput({ scope: target.scope, id: target.id }, controller.signal, { fresh: revision > 0, onUpdate }).then(result => {
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

function initialDraft(input: ReportInput): ReportDraft {
  const today = todayJst();
  // 表紙の題の既定は空（空なら「運用戦略」）。補足のスライドは最初は入れない。
  return { ...initialPeriods(input.mtgDates, input.asOf, today), addressee: input.company ?? '', subject: '', owner: '', meetingOn: today, off: [...SUPPLEMENT_SLIDES], texts: emptyTexts() };
}
/**
 * 下書き（期間・表紙・入れるスライド・書いた文章）はこの画面の中だけに持つ。保存には対応していない。
 * 既存の記録欄の保存先（job_copy_listing_notes）は欄の種類が固定（表の CHECK 制約）で求人ごとの表のため、
 * 報告の文章を載せるには表の作り直しが要る。新しい保存先は作らない（2026-11-20 利用者決定）。
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
    const name = slide.dataset.name;
    if ((tall || cut) && name) names.push(`${name}${tall ? '（枠からはみ出しています）' : '（名前が長く、途中までしか出ていません）'}`);
  }
  return names;
}

const isDate = (value: string) => /^\d{4}-\d{2}-\d{2}$/.test(value);

function ReportEditor({ input, back, onRefresh }: { input: ReportInput; back: ReactNode; onRefresh: () => void }) {
  const draftKey = `${input.scope}:${input.id}`;
  const [draft, setDraft] = useState<ReportDraft>(() => drafts.get(draftKey) ?? initialDraft(input));
  const [current, setCurrent] = useState<string>('cover');
  const [overflow, setOverflow] = useState<string[]>([]);
  const deck = useRef<HTMLDivElement>(null);
  const change = (update: Partial<ReportDraft>) => { setDraft(value => { const next = { ...value, ...update }; drafts.set(draftKey, next); return next; }); };
  const setTexts = (update: Partial<ReportTexts>) => { setDraft(value => { const next = { ...value, texts: { ...value.texts, ...update } }; drafts.set(draftKey, next); return next; }); };
  const today = todayJst();
  const mtgDates = useMemo(() => usableMtgDates(input.mtgDates, input.asOf, today).reverse(), [input.mtgDates, input.asOf, today]);
  const validPeriod = isDate(draft.from) && isDate(draft.to) && draft.from <= draft.to;
  const validPrevious = isDate(draft.previousFrom) && isDate(draft.previousTo) && draft.previousFrom <= draft.previousTo && draft.previousTo < draft.from;
  const fallback = useMemo(() => initialPeriods(input.mtgDates, input.asOf, today), [input.mtgDates, input.asOf, today]);
  const period = validPeriod ? draft : fallback;
  const meetingOn = isDate(draft.meetingOn) ? draft.meetingOn : today;
  const result = useMemo(() => buildReport(input, {
    from: period.from, to: period.to, previousFrom: validPeriod && validPrevious ? draft.previousFrom : undefined, previousTo: validPeriod && validPrevious ? draft.previousTo : undefined,
    addressee: draft.addressee, subject: draft.subject, owner: draft.owner, sender: REPORT_SENDER, createdOn: meetingOn, texts: draft.texts, off: draft.off, include: { outcomes: draft.outcomes !== false },
  }), [input, period, validPeriod, validPrevious, meetingOn, draft]);
  const included = result.slides.filter(slide => !draft.off.includes(slide.id) && !slide.blank);
  const shown = result.slides.find(slide => slide.id === current) ?? result.slides[0];
  const index = shown ? result.slides.indexOf(shown) : 0;
  const pageOf = (slide: Slide): number | null => { const n = included.indexOf(slide); return n < 0 ? null : n + 1; };
  const move = (step: number) => { const next = result.slides[(index + step + result.slides.length) % result.slides.length]; if (next) setCurrent(next.id); };
  const toggle = (id: string, on: boolean) => { change({ off: on ? draft.off.filter(item => item !== id) : [...draft.off, id] }); };
  const chooseMtg = (value: string) => {
    if (!value) { change({ mtg: null }); return; }
    const earlier = mtgDates.find(date => date < value) ?? null;
    change({ ...periodsFromMtg(value, earlier, input.asOf), mtg: value });
  };
  const weeks = (n: number) => { const now = alignedPeriod(input.asOf, n); change({ ...now, previousFrom: addDays(now.from, -7 * n), previousTo: addDays(now.from, -1), mtg: null }); };
  useLayoutEffect(() => {
    const frame = requestAnimationFrame(() => { setOverflow(overflowing(deck.current)); });
    return () => { cancelAnimationFrame(frame); };
  }, [result, draft.off]);
  // 下の表示は左の番号・スライド右下の番号と同じ、PDF のページ番号にする。
  const shownPage = shown ? pageOf(shown) : null;
  const status = shown ? draft.off.includes(shown.id) ? '（出力しない）' : shown.blank ? '（未記入のため出力しない）' : '' : '';

  return <main className="rs-root" aria-label="報告のスライドを作る">
    <div className="rs-top">{back}<h1>報告のスライドを作る</h1>
      <div className="rs-actions">
        <button type="button" className="jc-button" onClick={onRefresh}>最新にする</button>
        <button type="button" className="jc-button jc-primary rs-print-button" disabled={!included.length || !validPeriod} onClick={() => { window.print(); }}>PDF に出す</button>
      </div>
    </div>
    <p className="rs-save-status">期間・表紙・入れるスライド・書いた文章は、この画面の中だけに残ります（再読み込みで消えます）。保存には対応していません。HubSpot には書きません。</p>
    <p className="rs-hint">数字は集計から入ります。文章は、スライドを選んで下の欄に書きます。「PDF に出す」で印刷の画面が開き、送り先を「PDF に保存」にすると横長のスライドが1枚ずつのPDFになります。</p>
    <div className="rs-deck">
      <aside className="rs-side" aria-label="報告の設定">
        <h2>範囲</h2>
        <p className="rs-scope">{input.scope === 'deal' ? '案件' : '求人'}：{input.name ?? (input.scope === 'deal' ? '案件名を読み込めませんでした' : '求人名を読み込めませんでした')}{input.scope === 'deal' && <small>この案件の求人 {String(input.listings.length)}件をまとめます</small>}</p>
        <h2>期間</h2>
        {mtgDates.length > 0 && <label className="rs-field">前回のMTG（記録から）<select value={draft.mtg ?? ''} onChange={event => { chooseMtg(event.target.value); }}>
          {mtgDates.slice(0, 8).map(date => <option key={date} value={date}>{date.replaceAll('-', '/')}</option>)}
          <option value="">自分で決める</option>
        </select></label>}
        <p className="rs-basis">{draft.mtg
          ? `前回のMTG（${shortDay(draft.mtg)} の記録）から、応募データのある ${shortDay(input.asOf)} までを今回の期間にしています。`
          : mtgDates.length ? '期間を自分で決めています。'
            : input.scope === 'deal' ? (input.mtgDates ? 'MTGの記録が無いため、直近の4週と、その前の4週にしています。期間は変えられます。' : 'MTGの記録を読み込めなかったため、直近の4週と、その前の4週にしています。期間は変えられます。')
              : '求人の報告では MTG の記録を読みません。直近の4週と、その前の4週にしています。期間は変えられます。'}</p>
        <div className="rs-period-h">今回</div>
        <div className="rs-period">
          <label>開始<input type="date" value={draft.from} max={draft.to} onChange={event => { change({ from: event.target.value, mtg: null }); }} /></label>
          <label>終了<input type="date" value={draft.to} min={draft.from} onChange={event => { change({ to: event.target.value, mtg: null }); }} /></label>
        </div>
        <div className="rs-period-h">前回（比べる期間）</div>
        <div className="rs-period">
          <label>開始<input type="date" aria-label="前回の開始" value={draft.previousFrom} max={draft.previousTo} onChange={event => { change({ previousFrom: event.target.value, mtg: null }); }} /></label>
          <label>終了<input type="date" aria-label="前回の終了" value={draft.previousTo} min={draft.previousFrom} onChange={event => { change({ previousTo: event.target.value, mtg: null }); }} /></label>
        </div>
        <div className="rs-presets" role="group" aria-label="期間をそろえる">{[2, 4, 8].map(n => <button type="button" key={n} className="jc-button" onClick={() => { weeks(n); }}>直近{String(n)}週</button>)}</div>
        {!validPeriod && <p className="jc-error" role="alert">開始日と終了日を正しく入力してください（開始日は終了日以前）。</p>}
        {validPeriod && !validPrevious && <p className="jc-error" role="alert">前回の期間は、今回の開始日より前にしてください。今は、今回と同じ長さの直前の期間と比べています。</p>}
        <h2>表紙</h2>
        <label className="rs-field">宛名（御中の前）<input value={draft.addressee} maxLength={100} onChange={event => { change({ addressee: event.target.value }); }} /></label>
        <label className="rs-field">題（空欄なら「{DEFAULT_TITLE}」）<input value={draft.subject} maxLength={100} onChange={event => { change({ subject: event.target.value }); }} /></label>
        <label className="rs-field">定期MTGの日<input type="date" value={draft.meetingOn} onChange={event => { change({ meetingOn: event.target.value }); }} /></label>
        <label className="rs-field">担当者名<input value={draft.owner} maxLength={50} onChange={event => { change({ owner: event.target.value }); }} /></label>
        <h2>スライド</h2>
        <ol className="rs-thumbs">{result.slides.map(slide => {
          const on = !draft.off.includes(slide.id);
          return <li key={slide.id} className={`${slide.id === shown?.id ? 'rs-on' : ''} ${on && !slide.blank ? '' : 'rs-off'}`}>
            <input type="checkbox" checked={on} aria-label={`${slide.name}を入れる`} onChange={event => { toggle(slide.id, event.target.checked); }} />
            <button type="button" aria-current={slide.id === shown?.id ? 'true' : undefined} onClick={() => { setCurrent(slide.id); }}><span className="rs-num">{String(pageOf(slide) ?? '－')}</span><span>{slide.name}{slide.blank && <small className="rs-blank">未記入</small>}</span></button>
          </li>;
        })}</ol>
        {input.scope === 'deal' && <label className="rs-check"><input type="checkbox" checked={draft.outcomes !== false} onChange={event => { change({ outcomes: event.target.checked }); }} />面接・採用を出す<small>案件に記録された、ご契約からの合計です（数字の表に出ます）。値が古いときは外してください。</small></label>}
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
        {shown && <div className={draft.off.includes(shown.id) || shown.blank ? 'rs-big rs-dim' : 'rs-big'}><SlideView slide={shown} page={pageOf(shown)} /></div>}
        <div className="rs-nav"><button type="button" className="jc-button" onClick={() => { move(-1); }}>‹ 前</button><span>{shownPage === null ? '－' : String(shownPage)} / {String(included.length)} ページ{status}</span><button type="button" className="jc-button" onClick={() => { move(1); }}>次 ›</button></div>
        {shown && <SlideInputs slide={shown} texts={draft.texts} setTexts={setTexts} />}
      </section>
    </div>
    {createPortal(<div className="rs-print-deck" aria-hidden="true" ref={deck}>{included.map(slide => <SlideView key={slide.id} slide={slide} page={pageOf(slide)} />)}</div>, document.body)}
  </main>;
}

function Line({ label, value, onChange, max = 60, placeholder }: { label: string; value: string; onChange: (value: string) => void; max?: number; placeholder?: string }) {
  return <label className="rs-field">{label}<input value={value} maxLength={max} placeholder={placeholder} onChange={event => { onChange(event.target.value); }} /></label>;
}
function Area({ label, value, onChange, rows = 4, placeholder }: { label: string; value: string; onChange: (value: string) => void; rows?: number; placeholder?: string }) {
  return <label className="rs-field">{label}<textarea value={value} rows={rows} maxLength={600} placeholder={placeholder ?? '1行が1つの箇条書きになります'} onChange={event => { onChange(event.target.value); }} /></label>;
}

/** 選んだスライドに書く欄。数字だけのスライドには出さない。 */
function SlideInputs({ slide, texts, setTexts }: { slide: Slide; texts: ReportTexts; setTexts: (update: Partial<ReportTexts>) => void }) {
  const listingId = slide.kind === 'listing' || slide.kind === 'measure' ? slide.id.slice(slide.id.indexOf(':') + 1) : '';
  const listingText = texts.listings[listingId] ?? emptyListingText();
  const setListing = (update: Partial<ListingText>) => { setTexts({ listings: { ...texts.listings, [listingId]: { ...listingText, ...update } } }); };
  const fields = (() => {
    switch (slide.kind) {
      case 'goals': return <>
        {texts.goals.map((goal, i) => <div className="rs-pairin" key={i}>
          <Line label={`職種 ${String(i + 1)}`} value={goal.name} max={30} onChange={name => { setTexts({ goals: texts.goals.map((item, n) => n === i ? { ...item, name } : item) }); }} />
          <Line label="目標" value={goal.goal} max={60} placeholder="例：○年○月までに ＋○名" onChange={value => { setTexts({ goals: texts.goals.map((item, n) => n === i ? { ...item, goal: value } : item) }); }} />
        </div>)}
        <div className="rs-cols3">
          <Area label="無料の媒体" value={texts.mediaFree} rows={3} onChange={mediaFree => { setTexts({ mediaFree }); }} />
          <Area label="有料の媒体" value={texts.mediaPaid} rows={3} onChange={mediaPaid => { setTexts({ mediaPaid }); }} />
          <Area label="その他" value={texts.mediaOther} rows={3} onChange={mediaOther => { setTexts({ mediaOther }); }} />
        </div>
      </>;
      case 'review': return <div className="rs-cols2">
        <Area label="総評" value={texts.overall} rows={6} onChange={overall => { setTexts({ overall }); }} />
        <Area label="今後の対策・所感" value={texts.outlook} rows={6} onChange={outlook => { setTexts({ outlook }); }} />
      </div>;
      case 'listing': return <>
        <Line label="見出しのひとこと（求人名の後ろ）" value={listingText.catch} max={40} onChange={value => { setListing({ catch: value }); }} />
        <Area label="課題仮説" value={listingText.hypothesis} rows={6} onChange={hypothesis => { setListing({ hypothesis }); }} />
      </>;
      case 'policy': return <>
        <Line label="対象の職種・求人（見出しの後ろ）" value={texts.policyTitle} max={30} onChange={policyTitle => { setTexts({ policyTitle }); }} />
        <div className="rs-cols3">{texts.policies.map((item, i) => <div key={i}>
          <Area label={`${String(i + 1)}\u3000対象の状況`} value={item.when} rows={3} onChange={when => { setTexts({ policies: texts.policies.map((policy, n) => n === i ? { ...policy, when } : policy) }); }} />
          <Area label="対応方針" value={item.how} rows={4} onChange={how => { setTexts({ policies: texts.policies.map((policy, n) => n === i ? { ...policy, how } : policy) }); }} />
        </div>)}</div>
      </>;
      case 'measure': return <div className="rs-cols2">
        <Area label="訴求のポイント" value={listingText.appeal} rows={6} onChange={appeal => { setListing({ appeal }); }} />
        <Area label="施策の内容" value={listingText.measure} rows={6} onChange={measure => { setListing({ measure }); }} />
      </div>;
      case 'plan': return <>
        <Line label="次回の定期MTGまでの共通目標" value={texts.commonGoal} max={60} onChange={commonGoal => { setTexts({ commonGoal }); }} />
        <div className="rs-cols3">
          <Area label="当社の行動計画" value={texts.planOurs} rows={5} onChange={planOurs => { setTexts({ planOurs }); }} />
          <Area label="ご担当者さまの行動計画" value={texts.planClient} rows={5} onChange={planClient => { setTexts({ planClient }); }} />
          <Area label="運用上のゴール" value={texts.planGoal} rows={5} onChange={planGoal => { setTexts({ planGoal }); }} />
        </div>
      </>;
      case 'closing': return <div className="rs-cols3">
        <Area label="アンケートのご案内" value={texts.survey} rows={3} placeholder="空欄なら「アンケートへのご協力をお願いいたします」" onChange={survey => { setTexts({ survey }); }} />
        <Area label="次回のお打ち合わせ日" value={texts.nextMeeting} rows={3} placeholder="例：○月○日（○）○時" onChange={nextMeeting => { setTexts({ nextMeeting }); }} />
        <Area label="その他のメモ" value={texts.memo} rows={3} placeholder="次の掲載期間・ご予算など" onChange={memo => { setTexts({ memo }); }} />
      </div>;
      default: return null;
    }
  })();
  if (!fields) return null;
  return <section className="rs-write" aria-label="このスライドに書くこと"><h2>このスライドに書くこと</h2>{fields}</section>;
}

const pct = (value: number, max: number) => `${String(max > 0 ? Math.max(2, Math.round(value / max * 100)) : 2)}%`;

function HBars({ rows, unit = '件' }: { rows: readonly { label: string; count: number }[]; unit?: string }) {
  const max = Math.max(0, ...rows.map(row => row.count));
  return <div className="rs-hb">{rows.map((row, i) => <div className="rs-hb-row" key={i}><span className="rs-hb-label">{row.label}</span><span className="rs-hb-bar"><i style={{ width: pct(row.count, max) }} /><b>{String(row.count)}{unit}</b></span></div>)}</div>;
}
function Boxes({ boxes, numbered = false }: { boxes: readonly TextBox[]; numbered?: boolean }) {
  return <div className="rs-boxes" style={{ gridTemplateColumns: `repeat(${String(Math.max(1, boxes.length))}, 1fr)` }}>{boxes.map((box, i) => <div className="rs-box" key={i}>
    <h3>{numbered ? `${String(i + 1)}\u3000` : ''}{box.title}</h3>
    <ul>{box.lines.map((line, n) => <li key={n}>{line}</li>)}</ul>
  </div>)}</div>;
}
const numberCell = (value: number | null) => value === null ? '—' : value.toLocaleString('ja-JP');
function NumberRowView({ row, hasPerf, total = false }: { row: NumberRow; hasPerf: boolean; total?: boolean }) {
  return <tr className={total ? 'rs-total' : undefined}><th scope="row">{row.label}</th>
    {hasPerf && <><td>{numberCell(row.impressions)}</td><td>{row.ctr ?? '—'}</td><td>{numberCell(row.clicks)}</td></>}
    <td>{numberCell(row.applications)}</td></tr>;
}

export function SlideView({ slide, page }: { slide: Slide; page: number | null }) {
  if (slide.kind === 'cover') return <div className="rs-slide rs-cover" data-slide={slide.id} data-name={slide.name}><div className="rs-in">
    {slide.addressee && <div className="rs-to">{slide.addressee}</div>}
    <div className="rs-title">{slide.title}</div>
    <div className="rs-sub">{slide.subtitle}</div>
  </div><div className="rs-cover-foot">{slide.footer}</div></div>;
  return <div className="rs-slide" data-slide={slide.id} data-name={slide.name}><div className="rs-in">
    <div className="rs-kicker">{slide.kicker}</div>
    <div className="rs-msg">{slide.headline}</div>
    <div className="rs-body">{body(slide)}</div>
    {slide.note && <div className="rs-note">{slide.note}</div>}
  </div><div className="rs-foot"><span>{DECK_NAME}</span><span>{page === null ? '' : String(page)}</span></div></div>;
}

function body(slide: Slide) {
  switch (slide.kind) {
    case 'toc': return <ul className="rs-toc">{slide.items.map(item => <li key={item}>{item}</li>)}</ul>;
    case 'listings': return <>
      <ul className="rs-names">{slide.names.map((name, i) => <li key={i}>{name}</li>)}</ul>
      <div className="rs-sum">合計：{String(slide.total)}求人</div>
    </>;
    case 'route': return <>
      <div className="rs-route">{slide.nodes.map((node, i) => <div className="rs-node" key={i}>{node}</div>)}<div className="rs-route-to">応募・お問い合わせ</div></div>
      <div className="rs-caption">{slide.message}</div>
    </>;
    case 'goals': return <>
      {slide.goals.length > 0 && <dl className="rs-goals">{slide.goals.map((goal, i) => <div key={i}><dt>{goal.name}</dt><dd>{goal.goal && <>目標：{goal.goal}</>}</dd></div>)}</dl>}
      <Boxes boxes={slide.media} />
    </>;
    case 'numbers': return <>
      <table className="rs-table">
        <thead><tr><th scope="col">求人</th>{slide.hasPerf && <><th scope="col">表示回数</th><th scope="col">クリック率</th><th scope="col">クリック数</th></>}<th scope="col">応募数</th></tr></thead>
        {slide.groups.map((group, i) => <tbody key={i}>
          {group.name && <tr className="rs-group"><th scope="rowgroup" colSpan={slide.hasPerf ? 5 : 2}>{group.name}</th></tr>}
          {group.rows.map((row, n) => <NumberRowView key={n} row={row} hasPerf={slide.hasPerf} />)}
          {group.total && <NumberRowView row={group.total} hasPerf={slide.hasPerf} total />}
        </tbody>)}
      </table>
      {slide.totals && <div className="rs-totals"><div className="rs-totals-h">{slide.totals.heading}</div><div className="rs-kp">{slide.totals.kpis.map(kpi => <div key={kpi.label}><span>{kpi.label}</span><b>{kpi.value}</b></div>)}</div></div>}
    </>;
    case 'review': return <div className={slide.points.length || slide.outlook.length ? 'rs-split' : undefined}>
      {(slide.points.length > 0 || slide.outlook.length > 0) && <div>
        {slide.points.length > 0 && <><h3 className="rs-h">総評</h3><ul className="rs-points">{slide.points.map((point, i) => <li key={i}>{point}</li>)}</ul></>}
        {slide.outlook.length > 0 && <><h3 className="rs-h">今後の対策・所感</h3><ul className="rs-points">{slide.outlook.map((point, i) => <li key={i}>{point}</li>)}</ul></>}
      </div>}
      {slide.compare.length > 0 && <div><h3 className="rs-h">前回との比較（前回 → 今回）</h3><table className="rs-table rs-compare">
        <thead><tr><th scope="col">求人</th>{slide.columns.map(column => <th scope="col" key={column}>{column}</th>)}</tr></thead>
        <tbody>{slide.compare.map((row, i) => <tr key={i} className={row.label === '全体' ? 'rs-total' : undefined}><th scope="row">{row.label}</th>{row.cells.map((cell, n) => <td key={n}>{cell}</td>)}</tr>)}</tbody>
      </table></div>}
    </div>;
    case 'listing': return <div className="rs-split">
      <div><h3 className="rs-h">現状</h3><div className="rs-facts">{slide.facts.map(fact => <div key={fact.label}><span>{fact.label}</span><b>{fact.value}</b>{fact.sub && <small>{fact.sub}</small>}</div>)}</div></div>
      <div><h3 className="rs-h">課題仮説</h3><ul className="rs-points">{slide.hypothesis.map((line, i) => <li key={i}>{line}</li>)}</ul></div>
    </div>;
    case 'trend': {
      const max = Math.max(0, ...slide.bars.map(bar => bar.count));
      return <div className="rs-vbars">{slide.bars.map((bar, i) => <div className={bar.marks.length ? 'rs-c rs-ev' : 'rs-c'} key={i}>
        {bar.marks.length > 0 && <span className="rs-mark">{bar.marks.length > 1 ? `見直し ${String(bar.marks.length)}件` : bar.marks[0]}</span>}
        <span className="rs-v">{String(bar.count)}</span><i style={{ height: pct(bar.count, max) }} /><em>{bar.label}</em></div>)}</div>;
    }
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
    case 'around': return <>
      <div className="rs-kp">{slide.kpis.map(kpi => <div key={kpi.label}><span>{kpi.label}</span><b>{kpi.value}</b>{kpi.sub && <small>{kpi.sub}</small>}</div>)}</div>
      {slide.bars.length > 0 && <HBars rows={slide.bars} />}
    </>;
    case 'policy': return <div className="rs-boxes" style={{ gridTemplateColumns: `repeat(${String(Math.max(1, slide.cases.length))}, 1fr)` }}>{slide.cases.map((item, i) => <div className="rs-box" key={i}>
      <h3>{`${String(i + 1)}\u3000対象の状況`}</h3><ul>{item.when.map((line, n) => <li key={n}>{line}</li>)}</ul>
      <h3 className="rs-box-sub">対応方針</h3><ul>{item.how.map((line, n) => <li key={n}>{line}</li>)}</ul>
    </div>)}</div>;
    case 'measure': return <Boxes boxes={slide.boxes} />;
    case 'plan': return <>
      {slide.goal && <div className="rs-goal">{slide.goal}</div>}
      <Boxes boxes={slide.columns} numbered />
    </>;
    case 'closing': return <Boxes boxes={slide.boxes} />;
    default: return null;
  }
}
