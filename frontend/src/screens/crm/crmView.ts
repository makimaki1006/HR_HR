export type CrmView = 'queue' | 'moc' | 'reference' | 'single';

/**
 * `/app/crm` の `?view=`。既定 (指定なし・不明な値) は架電画面 (`queue`)。
 * - `queue`: 架電画面 (既定。以前の URL `?view=queue` もそのまま使える)
 * - `moc` (旧名 `calling`): 連続架電の MOC (BatchCallWorkspace)
 * - `reference`: HubSpot のレコード画面を参照した見本
 * - `single`: 単発の架電 MOC
 */
export function crmView(search: string): CrmView {
  const v = new URLSearchParams(search).get('view');
  if (v === 'moc' || v === 'calling') return 'moc';
  if (v === 'reference' || v === 'single') return v;
  return 'queue';
}
