import { useState } from 'react';
import type { ApplicantDetail } from '../../generated/ApplicantDetail';
import { LinkView } from './CenterTabs';
import type { LinkTab } from './centerLinks';

type MapView = 'route' | 'job' | 'applicant';
const VIEW_LABELS: Record<MapView, string> = { route: '応募者の住所から求人まで', job: '求人の場所', applicant: '応募者の住所' };

/** 地図の検索ページ (「新しいタブで開く」用) */
export function mapSearchUrl(address: string): string {
  const u = new URL('https://www.google.com/maps/search/');
  u.searchParams.set('api', '1');
  u.searchParams.set('query', address);
  return u.toString();
}
function mapRouteUrl(from: string, to: string): string {
  const u = new URL('https://www.google.com/maps/dir/');
  u.searchParams.set('api', '1');
  u.searchParams.set('origin', from);
  u.searchParams.set('destination', to);
  return u.toString();
}
/** 枠に出す地図 (output=embed の形だけが枠の中に出せる) */
function mapEmbedUrl(params: Record<string, string>): string {
  const u = new URL('https://www.google.com/maps');
  for (const [k, v] of Object.entries(params)) u.searchParams.set(k, v);
  u.searchParams.set('output', 'embed');
  u.searchParams.set('hl', 'ja');
  return u.toString();
}
/** 地図のタブの「新しいタブで開く」先。求人の住所を優先し、無ければ応募者の住所。どちらも無ければ null */
export function applicantMapUrl(detail: ApplicantDetail | null): string | null {
  const address = detail?.job_address?.address ?? detail?.applicant_address ?? null;
  return address === null ? null : mapSearchUrl(address);
}

/** 求人の住所と応募者の住所を Google マップで出す。両方あれば経路、片方だけならその場所 */
export function ApplicantMap({ detail }: { detail: ApplicantDetail }) {
  const job = detail.job_address?.address ?? null;
  const person = detail.applicant_address ?? null;
  const views: MapView[] = [...(job && person ? ['route' as const] : []), ...(job ? ['job' as const] : []), ...(person ? ['applicant' as const] : [])];
  const [picked, setPicked] = useState<MapView | null>(null);
  const view = picked !== null && views.includes(picked) ? picked : views[0] ?? null;
  const tab: LinkTab | null = view === null ? null
    : view === 'route' && job && person ? { id: 'applicant-map', label: '地図', host: 'www.google.com', url: mapRouteUrl(person, job), embed: mapEmbedUrl({ saddr: person, daddr: job }) }
    : view === 'job' && job ? { id: 'applicant-map', label: '地図', host: 'www.google.com', url: mapSearchUrl(job), embed: mapEmbedUrl({ q: job }) }
    : person ? { id: 'applicant-map', label: '地図', host: 'www.google.com', url: mapSearchUrl(person), embed: mapEmbedUrl({ q: person }) } : null;
  return <div className="applicant-map">
    <dl className="applicant-map-addresses">
      <div><dt>求人</dt><dd>{job ?? '住所が登録されていません'}{detail.job_address && <small>（{detail.job_address.source}）</small>}</dd></div>
      <div><dt>応募者</dt><dd>{person ?? '住所が登録されていません'}</dd></div>
    </dl>
    {views.length >= 2 && <div className="applicant-pages" role="group" aria-label="地図の表示">{views.map(v => <button key={v} type="button" aria-pressed={v === view} onClick={() => { setPicked(v); }}>{VIEW_LABELS[v]}</button>)}</div>}
    {tab === null ? <div className="dock-scroll"><p>求人にも応募者にも住所が登録されていないため、地図を出せません。</p></div> : <LinkView key={tab.embed} tab={tab} showFrameHint={false} />}
  </div>;
}
