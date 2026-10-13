/**
 * Choose the market (prefecture × Indeed job category) to show next to one job.
 *
 * The occupation is never guessed from the job's text: a person designates it per job (the HubSpot
 * property 「比べる Indeed 職種」). Without a designation there is no market comparison.
 */
const PREFECTURE = /^(北海道|東京都|京都府|大阪府|[^\s都道府県]{2,3}県)/;

export function prefectureFromLocation(location: string, prefectures?: readonly string[]): string | null {
  const text = location.normalize('NFKC').trim();
  const found = PREFECTURE.exec(text)?.[1] ?? null;
  if (!found) return null;
  if (prefectures && !prefectures.includes(found)) return null;
  return found;
}

/** set: a title was designated. none: 「市場と比べない」 was chosen on purpose. unset: nobody decided yet. */
export type IndeedTitleState = 'set' | 'none' | 'unset';

export interface IndeedTitleFields { indeedTitle?: string | null | undefined; indeedTitleState?: IndeedTitleState | undefined }

/** The state of a job; a record without the fields (an older response) counts as 未指定. */
export function indeedTitleState(job: IndeedTitleFields): IndeedTitleState {
  if (job.indeedTitleState === 'set' && job.indeedTitle) return 'set';
  return job.indeedTitleState === 'none' ? 'none' : 'unset';
}

/** The plain note shown instead of market data when there is no designated title. */
export function marketTitleNote(state: IndeedTitleState): string | null {
  if (state === 'unset') return '市場と比べる職種が未指定です';
  if (state === 'none') return '市場と比べないと指定されています';
  return null;
}

export interface MarketChoice {
  /** The designated title, or null (nothing is compared). */
  title: string | null;
  titleState: IndeedTitleState;
  prefecture: string | null;
}

export function chooseMarket(job: IndeedTitleFields & { location: string }, prefectures: readonly string[]): MarketChoice {
  const titleState = indeedTitleState(job);
  return { title: titleState === 'set' ? job.indeedTitle ?? null : null, titleState, prefecture: prefectureFromLocation(job.location, prefectures) };
}
