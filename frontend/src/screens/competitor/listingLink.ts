// 案件管理の求人から競合調査を開くときの URL と、その読み取り。
// 案件管理（入口を作る側）と競合調査（受け取る側）が同じ決まりを使うよう、1 か所に置く。
import { JOB_NAME_MAX_CHARS, TEXT_MAX_CHARS, type FormValues } from './form';

/** 競合調査を開いた元の求人。id は HubSpot の求人の番号（数字だけ）。 */
export interface ListingTarget {
  id: string;
  /** 求人の名前。確かめる文に出す。URL に無ければ null。 */
  name: string | null;
  prefecture: string;
  municipality: string;
  /** 職種名の欄に入れる言葉。 */
  jobName: string;
}

const ID = /^\d{1,30}$/;
const clip = (value: string, max: number) => Array.from(value.trim()).slice(0, max).join('');

/** 求人から競合調査を開く URL。求人から分かる条件（職種名・都道府県・市区町村）を付ける。 */
export function competitorUrl(listing: {
  id: string;
  name?: string | null | undefined;
  prefecture?: string | null | undefined;
  municipality?: string | null | undefined;
  jobName?: string | null | undefined;
}): string {
  const params = new URLSearchParams({ listing: listing.id });
  const set = (key: string, value: string | null | undefined, max: number) => {
    const text = clip(value ?? '', max);
    if (text) params.set(key, text);
  };
  set('listing_name', listing.name, TEXT_MAX_CHARS);
  set('job_name', listing.jobName, JOB_NAME_MAX_CHARS);
  set('prefecture', listing.prefecture, TEXT_MAX_CHARS);
  // 市区町村は都道府県があるときだけ（競合調査の入力と同じ決まり）。
  if (params.has('prefecture')) set('municipality', listing.municipality, TEXT_MAX_CHARS);
  return `/app/competitor?${params.toString()}`;
}

/** URL から元の求人を読む。求人の番号が無い・数字でないときは null（ふつうの競合調査として開く）。 */
export function readListingTarget(search: string): ListingTarget | null {
  const params = new URLSearchParams(search);
  const id = params.get('listing') ?? '';
  if (!ID.test(id)) return null;
  const prefecture = clip(params.get('prefecture') ?? '', TEXT_MAX_CHARS);
  return {
    id,
    name: clip(params.get('listing_name') ?? '', TEXT_MAX_CHARS) || null,
    prefecture,
    municipality: prefecture ? clip(params.get('municipality') ?? '', TEXT_MAX_CHARS) : '',
    jobName: clip(params.get('job_name') ?? '', JOB_NAME_MAX_CHARS),
  };
}

/** 入力欄に求人の条件を入れる。CSV・検索語などほかの欄は変えない。 */
export function withListing(values: FormValues, target: ListingTarget): FormValues {
  return { ...values, jobName: target.jobName, prefecture: target.prefecture, municipality: target.municipality };
}

/**
 * 選択肢が届いたあと、選択肢に無い地域を外す（HubSpot の書き方が選択肢と違うことがある）。
 * 都道府県が無ければ全国に、市区町村が無ければ「指定しない」にする。
 */
export function fitRegion(
  values: FormValues,
  prefectures: readonly string[],
  municipalities: Readonly<Record<string, readonly string[] | undefined>>,
): FormValues {
  if (values.prefecture !== '' && !prefectures.includes(values.prefecture)) return { ...values, prefecture: '', municipality: '' };
  if (values.municipality !== '' && !municipalities[values.prefecture]?.includes(values.municipality)) return { ...values, municipality: '' };
  return values;
}

/** 2026-11 → 「11月」。確かめる文に出す、今の月（日本時間）。 */
export function currentMonthLabel(now: Date = new Date()): string {
  return `${String(new Date(now.getTime() + 9 * 3600_000).getUTCMonth() + 1)}月`;
}
