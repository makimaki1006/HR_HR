// 日本の祝日 (振替休日・国民の休日を含む)。データは npm の @holiday-jp/holiday_jp (MIT、内閣府の祝日 CSV 由来) を
// 同梱している。年ごとのファイルを必要になったときだけ読む (カレンダーを開くまで一切読まない)。
// 対応年は 2020〜2039。範囲外の年は祝日なし(土日の色だけ)として扱う。

type YearModule = { default?: Record<string, { name: string }> } & Record<string, { name: string }>;

const loaders = import.meta.glob<YearModule>(
  '../../node_modules/@holiday-jp/holiday_jp/lib/holidays_every_year/20[2-3][0-9].js',
);

const byYear = new Map<number, Promise<Record<string, string>>>();

/** その年の祝日 (YYYY-MM-DD → 名称)。読めない年・範囲外の年は空 */
export function loadHolidayYear(year: number): Promise<Record<string, string>> {
  const hit = byYear.get(year);
  if (hit !== undefined) return hit;
  const key = Object.keys(loaders).find(k => k.endsWith(`/${String(year)}.js`));
  const loader = key === undefined ? undefined : loaders[key];
  const p: Promise<Record<string, string>> = loader === undefined ? Promise.resolve({})
    : loader().then(mod => {
      const table = (mod.default ?? mod) as Record<string, { name: string }>;
      return Object.fromEntries(Object.entries(table).map(([d, h]) => [d, h.name === '休日' ? '国民の休日' : h.name]));
    }, () => { byYear.delete(year); return {}; });
  byYear.set(year, p);
  return p;
}

export async function loadHolidays(years: number[]): Promise<Record<string, string>> {
  const parts = await Promise.all([...new Set(years)].map(loadHolidayYear));
  return Object.assign({}, ...parts) as Record<string, string>;
}
