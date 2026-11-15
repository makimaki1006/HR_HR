/**
 * 案件・求人のページを、どのメニューから開いたか（URL の ?from=）。
 * 戻るリンクの言葉と、上のメニューでどれを選んだ表示にするかに使う。
 * today は「今日」（?view= なし）。分からないときは null。
 */
export const ORIGIN_LABELS: Readonly<Record<string, string>> = {
  today: '今日', risk: '危ない案件', good: 'いい案件', deals: '案件一覧', houjin: '顧客',
  occupations: '職種の指定', reasons: '応募理由', unlinked: '未連携のMTG',
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
