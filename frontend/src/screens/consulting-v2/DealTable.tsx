import { DASH, num, orText, yen } from './format';
import { CaseLink, FlagChips } from './parts';
import type { DealRow } from './types';

export type DealColumn =
  | 'name'
  | 'flags'
  | 'consultant'
  | 'phase'
  | 'months'
  | 'days_left'
  | 'nps'
  | 'contact'
  | 'mtg'
  | 'amount'
  | 'link';

const HEAD: Record<DealColumn, string> = {
  name: '案件',
  flags: '注意',
  consultant: '担当',
  phase: '段階',
  months: '何ヶ月目',
  days_left: '満了まで',
  nps: '満足度',
  contact: '最後の接触',
  mtg: '最後のMTG',
  amount: '金額',
  link: '案件管理',
};

const NUMERIC = new Set<DealColumn>(['months', 'days_left', 'nps', 'contact', 'mtg', 'amount']);

export const BOARD_COLUMNS: DealColumn[] = [
  'name',
  'flags',
  'consultant',
  'phase',
  'months',
  'days_left',
  'nps',
  'contact',
  'mtg',
  'amount',
  'link',
];

export const TODAY_COLUMNS: DealColumn[] = [
  'name',
  'flags',
  'nps',
  'consultant',
  'phase',
  'days_left',
  'contact',
  'mtg',
  'amount',
  'link',
];

function Months({ r }: { r: DealRow }) {
  if (r.not_started === true) {
    return (
      <>
        <span className="cv2-dim">開始前</span>
        {r.start ? <small> {r.start} から</small> : null}
      </>
    );
  }
  if (r.past_expiry === true) {
    return (
      <>
        <b className="cv2-red">満了後</b>
        {r.period ? <small> 契約 {num(r.period)} か月</small> : null}
      </>
    );
  }
  if (r.months === null || r.months === undefined) return <span className="cv2-dim">{DASH}</span>;
  if (r.period && r.months > r.period) {
    return (
      <>
        {num(r.months)}
        <small> か月目（契約 {num(r.period)} か月。満了日が後ろにずれています）</small>
      </>
    );
  }
  return (
    <>
      {num(r.months)}
      {r.period ? ` / ${num(r.period)}` : ''}
      <small> か月目</small>
      {r.band ? <small title="契約長を 3 つに分けた、契約のどのあたりか"> 契約の{r.band}</small> : null}
    </>
  );
}

function DaysLeft({ r }: { r: DealRow }) {
  const d = r.days_left;
  if (d === null || d === undefined) return <span className="cv2-dim">{DASH}</span>;
  if (d < 0) return <b className="cv2-red">満了を{num(-d)}日過ぎている</b>;
  return d <= 60 ? <b className="cv2-amber">{num(d)}日</b> : <>{num(d)}日</>;
}

function Nps({ r }: { r: DealRow }) {
  if (r.nps === null || r.nps === undefined) {
    return <span className="cv2-dim" title="満足度の回答がまだありません">満足度 未回答</span>;
  }
  const cls = r.nps <= 4 ? 'cv2-red' : r.nps >= 9 ? 'cv2-green' : '';
  return (
    <>
      <b className={cls}>{num(r.nps)}</b>
      <small> {r.nps_month ?? ''}</small>
    </>
  );
}

function Contact({ r }: { r: DealRow }) {
  if (r.n_contact === 0) return <b className="cv2-red">記録が見つからない</b>;
  const d = r.days_since_contact;
  if (d === null || d === undefined) return <span className="cv2-dim">{DASH}</span>;
  const cls = d > 30 ? 'cv2-red' : d > 14 ? 'cv2-amber' : '';
  return (
    <>
      <b className={cls}>{num(d)}日</b>
      <small> 前</small>
    </>
  );
}

function Mtg({ r }: { r: DealRow }) {
  if (r.mtg_band === 'onboarding') return <small>立ち上がり期</small>;
  if (r.mtg_days === null || r.mtg_days === undefined) return <small>記録が見つからない</small>;
  const cls = r.mtg_band === 'critical' ? 'cv2-red' : r.mtg_band === 'yellow' ? 'cv2-amber' : 'cv2-green';
  const src = (r.mtg_source_label ?? '').startsWith('Zoom')
    ? 'Zoom 開催記録'
    : r.mtg_source === 'recording' || r.mtg_source === 'both'
      ? '録画'
      : r.mtg_source === 'mail'
        ? 'メール（推定）'
        : '';
  return (
    <>
      {r.mtg_band === 'yellow' ? (
        <span className={cls}>{num(r.mtg_days)}日前</span>
      ) : (
        <b className={cls}>{num(r.mtg_days)}日前</b>
      )}
      {src ? <small> {src}</small> : null}
      {r.mtg_forced_by_expiry === true ? <small> 満了前</small> : null}
    </>
  );
}

function Cell({ col, r, onOpen }: { col: DealColumn; r: DealRow; onOpen: (id: string) => void }) {
  switch (col) {
    case 'name':
      return (
        <>
          <button type="button" className="cv2-link" onClick={() => { onOpen(r.deal_id); }}>
            {orText(r.name, '取引名なし')}
          </button>
          {r.focus === true ? <span className="cv2-tag">重点顧客</span> : null}
        </>
      );
    case 'flags':
      return <FlagChips flags={r.flags} />;
    case 'consultant':
      return <>{orText(r.consultant, DASH)}</>;
    case 'phase':
      return r.phase_key === 'overdue' ? (
        <small>満了後</small>
      ) : r.phase === null || r.phase === undefined ? (
        <small>不明</small>
      ) : (
        <>{r.phase_label ?? DASH}</>
      );
    case 'months':
      return <Months r={r} />;
    case 'days_left':
      return <DaysLeft r={r} />;
    case 'nps':
      return <Nps r={r} />;
    case 'contact':
      return <Contact r={r} />;
    case 'mtg':
      return <Mtg r={r} />;
    case 'amount':
      return <>{yen(r.amount)}</>;
    case 'link':
      return <CaseLink dealId={r.deal_id} />;
  }
}

export function DealTable({
  rows,
  columns,
  onOpen,
  caption,
}: {
  rows: DealRow[];
  columns: DealColumn[];
  onOpen: (id: string) => void;
  caption: string;
}) {
  if (rows.length === 0) return <p className="cv2-empty">この条件に合う案件はありません。</p>;
  return (
    <div className="cv2-table-wrap">
      <table className="cv2-table">
        <caption className="cv2-sr">{caption}</caption>
        <thead>
          <tr>
            {columns.map((c) => (
              <th key={c} scope="col" className={NUMERIC.has(c) ? 'cv2-num' : undefined}>
                {HEAD[c]}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.deal_id}>
              {columns.map((c) => (
                <td key={c} className={NUMERIC.has(c) ? 'cv2-num' : undefined}>
                  <Cell col={c} r={r} onOpen={onOpen} />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
