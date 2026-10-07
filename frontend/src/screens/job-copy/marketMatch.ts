/**
 * Choose the market (prefecture × Indeed job category) to show next to one job.
 * Never picks a "close enough" category: with no exact or contained match the result is null and
 * the screen asks the user to choose.
 */
const PREFECTURE = /^(北海道|東京都|京都府|大阪府|[^\s都道府県]{2,3}県)/;

export function prefectureFromLocation(location: string, prefectures?: readonly string[]): string | null {
  const text = location.normalize('NFKC').trim();
  const found = PREFECTURE.exec(text)?.[1] ?? null;
  if (!found) return null;
  if (prefectures && !prefectures.includes(found)) return null;
  return found;
}

const normalize = (value: string) => value.normalize('NFKC').replace(/[\s・／/()（）【】]/g, '').toLowerCase();

export interface TitleMatch {
  title: string;
  how: 'exact' | 'partial';
  /** Every category that matched, longest first (the first one is chosen). */
  candidates: string[];
}

export function matchMarketTitle(jobTitle: string, titles: readonly string[]): TitleMatch | null {
  const target = normalize(jobTitle);
  if (!target) return null;
  const exact = titles.find(title => normalize(title) === target);
  if (exact) return { title: exact, how: 'exact', candidates: [exact] };
  const candidates = titles
    .filter(title => {
      const value = normalize(title);
      // One-character categories match far too much; require at least two characters.
      return value.length >= 2 && (target.includes(value) || value.includes(target));
    })
    .sort((a, b) => normalize(b).length - normalize(a).length || a.localeCompare(b, 'ja'));
  const first = candidates[0];
  return first ? { title: first, how: 'partial', candidates } : null;
}

export interface MarketChoice {
  title: string | null;
  prefecture: string | null;
  titleHow: TitleMatch['how'] | null;
  candidates: string[];
}

export function chooseMarket(job: { title: string; location: string }, titles: readonly string[], prefectures: readonly string[]): MarketChoice {
  const match = matchMarketTitle(job.title, titles);
  return { title: match?.title ?? null, prefecture: prefectureFromLocation(job.location, prefectures), titleHow: match?.how ?? null, candidates: match?.candidates ?? [] };
}
