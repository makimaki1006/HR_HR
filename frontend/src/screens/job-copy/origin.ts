/**
 * 案件・求人のページを、どのメニューから開いたか（URL の ?from=）。
 * 戻るリンクの言葉と、上のメニューでどれを選んだ表示にするかに使う。
 * today は「今日」（?view= なし）。分からないときは null。
 */
export const ORIGIN_LABELS: Readonly<Record<string, string>> = {
  today: '今日', risk: '危ない案件', good: 'いい案件', deals: '案件一覧', houjin: '顧客の一覧',
  occupations: '職種の指定', reasons: '応募理由', unlinked: '案件が決まっていないMTG録画',
};
export function originOf(search: string): string | null {
  const from = new URLSearchParams(search).get('from');
  return from && Object.hasOwn(ORIGIN_LABELS, from) ? from : null;
}
/** 来た元の言葉（「案件一覧」など）。分からないときは null。 */
export function originLabel(from: string | null): string | null {
  return from ? ORIGIN_LABELS[from] ?? null : null;
}
/** 上のメニューで選んだ表示にする ?view= の値。今日は null。 */
export function originMenuView(from: string | null): string | null {
  return from === null || from === 'today' ? null : from;
}

/** 案件のページの「← ○○に戻る」の行き先。求人のページから開いた → 求人、メニューから → そのメニュー、顧客のページから → 顧客、分からない → 案件管理。 */
export interface DealBack { kind: 'listing' | 'origin' | 'company' | 'home'; label: string }
export function dealBackTarget({ listingReady, origin, hasCompany }: { listingReady: boolean; origin: string | null; hasCompany: boolean }): DealBack {
  if (listingReady) return { kind: 'listing', label: '求人に戻る' };
  const label = originLabel(origin);
  if (label) return { kind: 'origin', label: `${label}に戻る` };
  if (hasCompany) return { kind: 'company', label: '顧客のページに戻る' };
  return { kind: 'home', label: '案件管理に戻る' };
}
/** 来た元へ戻るときの ?view=。今日（view なし）なら null、来た元が分からなければ undefined。 */
export function originView(origin: string | null): string | null | undefined {
  if (!originLabel(origin)) return undefined;
  return originMenuView(origin);
}
/**
 * 案件のページのパンくずを押したときの動き。
 * - 顧客：URL に顧客が無くても、案件の顧客が分かっていればその顧客のページへ（company）
 * - 先頭：来た元が分かっていれば来た元へ（origin）
 * - それ以外：その階層へ（goto）
 */
export function crumbAction(level: 'home' | 'company' | 'deal' | 'listing', { onDealPage, trailHasCompany, knownCompany, origin, listingReady }: { onDealPage: boolean; trailHasCompany: boolean; knownCompany: boolean; origin: string | null; listingReady: boolean }): 'company' | 'origin' | 'goto' {
  if (level === 'company' && !trailHasCompany && knownCompany) return 'company';
  if (level === 'home' && onDealPage && originLabel(origin) && !listingReady) return 'origin';
  return 'goto';
}
