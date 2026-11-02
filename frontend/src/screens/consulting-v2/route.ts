// URL の状態: ?view=team|today|board|detail&c=担当&flag=名札&band=区分&id=取引ID
export type View = 'team' | 'today' | 'board' | 'detail';

export interface Route {
  view: View;
  /** 担当 (コンサル担当者) で絞る */
  c: string;
  /** 注意の名札で絞る */
  flag: string;
  /** MTG が空いている期間の区分 (critical など) で絞る */
  band: string;
  /** 案件の詳細で開く取引 ID */
  id: string;
}

export const VIEWS: { key: View; label: string; note: string }[] = [
  { key: 'team', label: 'チームと担当', note: '担当者ごとの持ち件数と注意の件数' },
  { key: 'today', label: '今日', note: '今日動く案件' },
  { key: 'board', label: '案件一覧', note: '稼働中の案件すべて' },
  { key: 'detail', label: '案件の詳細', note: '接触履歴と契約の連なり' },
];

export function parseRoute(search: string): Route {
  const q = new URLSearchParams(search);
  const v = q.get('view');
  const view: View = v === 'today' || v === 'board' || v === 'detail' ? v : 'team';
  return { view, c: q.get('c') ?? '', flag: q.get('flag') ?? '', band: q.get('band') ?? '', id: q.get('id') ?? '' };
}

export function routeHref(route: Route): string {
  const q = new URLSearchParams();
  if (route.view !== 'team') q.set('view', route.view);
  if (route.c !== '') q.set('c', route.c);
  if (route.flag !== '') q.set('flag', route.flag);
  if (route.band !== '') q.set('band', route.band);
  if (route.id !== '') q.set('id', route.id);
  const s = q.toString();
  return s === '' ? window.location.pathname : `?${s}`;
}
