import { DASH, num, orText, pct1, yen } from './format';
import { CaseLink, FlagChips, HubspotLink } from './parts';
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
  | 'oubo'
  | 'mensetu'
  | 'syoudaku'
  | 'goal'
  | 'contact_rate'
  | 'cpa'
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
  oubo: '応募',
  mensetu: '面接',
  syoudaku: '採用',
  goal: '採用目標に対して',
  contact_rate: '接触率',
  cpa: '採用単価',
  link: '案件管理',
};

const NUMERIC = new Set<DealColumn>(['months', 'days_left', 'nps', 'contact', 'mtg', 'amount', 'oubo', 'mensetu', 'syoudaku', 'goal', 'contact_rate', 'cpa']);

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
  'oubo',
  'mensetu',
  'syoudaku',
  'goal',
  'contact_rate',
  'cpa',
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

function Carry({ v, carry }: { v: number | null | undefined; carry: boolean | null | undefined }) {
  if (v === null || v === undefined) return <span className="cv2-dim">{DASH}</span>;
  return (
    <>
      {num(v)}
      {carry === true ? <small title="その月に書き換えが無く、前の値を引き継いだ"> 持越</small> : null}
    </>
  );
}

function Goal({ r }: { r: DealRow }) {
  if (r.saiyomokuhyou === null || r.saiyomokuhyou === undefined || r.saiyomokuhyou <= 0) return <small>目標なし</small>;
  if (r.rate_tassei === null || r.rate_tassei === undefined) return <span className="cv2-dim">{DASH}</span>;
  const done = r.rate_tassei >= 1;
  return (
    <>
      <b className={done ? 'cv2-green' : undefined}>{pct1(r.rate_tassei * 100, 0)}</b>
      <small> {num(r.syoudaku ?? null) || DASH}/{num(r.saiyomokuhyou)}{done ? '' : '（途中）'}</small>
    </>
  );
}

function ContactRate({ r }: { r: DealRow }) {
  if (r.not_started === true) return <small>開始前</small>;
  if (r.contact_rate === null || r.contact_rate === undefined) return <span className="cv2-dim">{DASH}</span>;
  const cls = r.contact_rate < 40 ? 'cv2-red' : r.contact_rate < 70 ? 'cv2-amber' : 'cv2-green';
  return (
    <>
      <b className={cls}>{pct1(r.contact_rate, 0)}</b>
      <small> {num(r.contact_touched ?? null)}/{num(r.contact_months ?? null)} か月</small>
    </>
  );
}

function Cpa({ r }: { r: DealRow }) {
  if (r.amount === null || r.amount === undefined) return <small>金額なし</small>;
  if (r.cpa === null || r.cpa === undefined) return <small>採用0</small>;
  const hot = r.cpa_vs_band !== null && r.cpa_vs_band !== undefined && r.cpa_vs_band >= 1.5;
  return (
    <>
      <b className={hot ? 'cv2-red' : undefined}>{yen(r.cpa)}</b>
      {r.cpa_band_median === null || r.cpa_band_median === undefined ? (
        r.cpa_band ? <small> {r.cpa_band} は件数が足りず比べていない</small> : null
      ) : (
        <small> {r.cpa_band ?? '同じ進み具合の区分'} の中央 {yen(r.cpa_band_median)}</small>
      )}
    </>
  );
}

function Cell({ col, r, onOpen, portal }: { col: DealColumn; r: DealRow; onOpen: (id: string) => void; portal: string | undefined }) {
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
      // 初回契約の関門が期限超過なら、その理由を名札の下に小さく出す（名札は増やさない）
      return (
        <>
          <FlagChips flags={r.flags} />
          {r.first_contract?.reason ? <small className="cv2-first-reason">{r.first_contract.reason}</small> : null}
        </>
      );
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
    case 'oubo':
      return <Carry v={r.oubo} carry={r.oubo_carry} />;
    case 'mensetu':
      return <Carry v={r.mensetu} carry={r.mensetu_carry} />;
    case 'syoudaku':
      return <Carry v={r.syoudaku} carry={r.syoudaku_carry} />;
    case 'goal':
      return <Goal r={r} />;
    case 'contact_rate':
      return <ContactRate r={r} />;
    case 'cpa':
      return <Cpa r={r} />;
    case 'link':
      return (
        <>
          <CaseLink dealId={r.deal_id} />
          <HubspotLink portal={portal} dealId={r.deal_id} />
        </>
      );
  }
}

export function DealTable({
  rows,
  columns,
  onOpen,
  caption,
  portal,
}: {
  rows: DealRow[];
  columns: DealColumn[];
  onOpen: (id: string) => void;
  caption: string;
  portal?: string | undefined;
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
                  <Cell col={c} r={r} onOpen={onOpen} portal={portal} />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
