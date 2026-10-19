/** 既存の取引 URL とビュー条件はそのまま持ち、応募だけ別モードにする。 */
export function applicantMode(search: string): boolean {
  const p = new URLSearchParams(search);
  return p.has('applicant') || p.get('mode') === 'applicants' || p.get('crmMode') === 'applicants';
}
export function modeUrl(search: string, applicants: boolean): string {
  const p = new URLSearchParams(search);
  if (p.get('mode') === 'applicants') p.delete('mode');
  if (applicants) p.set('crmMode', 'applicants'); else { p.delete('crmMode'); p.delete('applicant'); }
  return `?${p.toString()}`;
}
export function CrmModeSwitch({ applicants = false }: { applicants?: boolean }) {
  const search = window.location.search;
  return <nav aria-label="CRM のモード" className="crm-mode-switch">
    <a href={modeUrl(search, false)} aria-current={!applicants ? 'page' : undefined}>取引</a>
    <a href={modeUrl(search, true)} aria-current={applicants ? 'page' : undefined}>応募</a>
  </nav>;
}
