import { AREA_MASTER } from './areaMaster';
import type { ApplicantDistribution } from './applicantCompositionModel';
import type { JobCopyRecord } from './data';
import type { JointDemographics } from './reverseSearchModel';

/**
 * 応募者の住所は「都道府県 + 市区町村」までに丸めて表示する。
 * 番地・建物名・部屋番号などの元の文字列は、どの経路でも画面に出さない。
 * サーバー（src/geo/applicant_area.rs）も JSON を返す前に同じ規則で丸める。ここでも丸め直す（念のため）。
 * 市区町村はマスタ（src/geo/master_city.csv）にある名前だけを採用し、
 * 読み取れないときは推測せず「（市区町村不明）」にする。
 */
export const AREA_OTHER = 'その他';
export const AREA_UNKNOWN = '不明';
/** 応募がこの件数未満の地域は「その他」にまとめる。 */
export const MINIMUM_AREA_COUNT = 3;

const normalize = (value: string) => value.normalize('NFKC').replace(/\s+/gu, '').replace(/ヶ/gu, 'ケ').replace(/ヵ/gu, 'カ');

interface PrefectureEntry { name: string; short: string; cities: { key: string; name: string }[] }

// Built on the first call, not when the module loads: opening the timeline or the demo does not
// pay for normalizing every municipality name.
let prefectureCache: PrefectureEntry[] | null = null;
function prefectures(): PrefectureEntry[] {
  prefectureCache ??= buildPrefectures();
  return prefectureCache;
}
const buildPrefectures = (): PrefectureEntry[] => AREA_MASTER.map(([name, joined]) => {
  const aliases = new Map<string, string>();
  for (const city of joined.split('|')) {
    aliases.set(normalize(city), city);
    // 郡名を省いた書き方（例: 石狩郡当別町 → 当別町）
    const county = /^.+?郡(.+[町村])$/u.exec(city);
    if (county?.[1] && !aliases.has(normalize(county[1]))) aliases.set(normalize(county[1]), city);
    // 政令指定都市の区を書かない住所や、マスタより新しい区名（例: 浜松市中央区）は市までにする
    const ward = /^(.+?市).+区$/u.exec(city);
    if (ward?.[1] && !aliases.has(normalize(ward[1]))) aliases.set(normalize(ward[1]), ward[1]);
  }
  const cities = [...aliases].map(([key, value]) => ({ key, name: value })).sort((left, right) => right.key.length - left.key.length);
  return { name, short: name === '北海道' ? name : name.slice(0, -1), cities };
});

/** マスタにある市区町村名か（郡名を省いた書き方・区を省いた市の書き方を含む）。応募理由の文の町名を見つけるのに使う。 */
export function isMunicipalityName(text: string): boolean {
  const key = normalize(text);
  return prefectures().some(prefecture => prefecture.cities.some(city => city.key === key));
}

/** Prefecture names in code order (1–47). */
export const PREFECTURE_NAMES: readonly string[] = AREA_MASTER.map(([name]) => name);

function splitPrefecture(text: string): { prefecture: PrefectureEntry; rest: string } | null {
  for (const prefecture of prefectures()) {
    if (text.startsWith(prefecture.name)) return { prefecture, rest: text.slice(prefecture.name.length) };
  }
  const exact = prefectures().find(prefecture => prefecture.short === text);
  return exact ? { prefecture: exact, rest: '' } : null;
}

function matchCity(prefecture: PrefectureEntry, text: string): string | null {
  return prefecture.cities.find(city => text.startsWith(city.key))?.name ?? null;
}

export interface RoundedArea { prefecture: string | null; municipality: string | null }

/** 都道府県の欄と市区町村の欄（どちらも住所全体が入っていることがある）から、都道府県と市区町村だけを取り出す。 */
export function parseApplicantArea(prefectureText: string | null | undefined, municipalityText: string | null | undefined): RoundedArea {
  const prefectureValue = prefectureText ? normalize(prefectureText) : '';
  const cityValue = municipalityText ? normalize(municipalityText) : '';
  const fromPrefecture = prefectureValue ? splitPrefecture(prefectureValue) : null;
  const fromCity = cityValue ? splitPrefecture(cityValue) : null;
  const found = fromPrefecture?.prefecture ?? fromCity?.prefecture ?? null;
  // 市区町村の欄が都道府県から始まるときは、それを外してから市区町村を探す
  let rest = fromCity?.prefecture === found ? fromCity.rest : cityValue;
  if (!rest) rest = fromPrefecture?.rest ?? '';
  if (found) return { prefecture: found.name, municipality: matchCity(found, rest) };
  // 都道府県が分からないときは、市区町村名が一つの都道府県にしか無い場合だけ採用する
  let best: { prefecture: string; city: string; length: number }[] = [];
  for (const prefecture of prefectures()) {
    const city = prefecture.cities.find(item => rest.startsWith(item.key));
    if (!city) continue;
    if (!best[0] || city.key.length > best[0].length) best = [{ prefecture: prefecture.name, city: city.name, length: city.key.length }];
    else if (city.key.length === best[0].length) best.push({ prefecture: prefecture.name, city: city.name, length: city.key.length });
  }
  const only = best.length === 1 ? best[0] : undefined;
  return only ? { prefecture: only.prefecture, municipality: only.city } : { prefecture: null, municipality: null };
}

export function prefectureLabel(area: RoundedArea): string {
  return area.prefecture ?? AREA_UNKNOWN;
}

export function municipalityLabel(area: RoundedArea): string {
  if (!area.prefecture) return AREA_UNKNOWN;
  return area.municipality ? `${area.prefecture}${area.municipality}` : `${area.prefecture}（市区町村不明）`;
}

const reserved = (label: string) => label === AREA_OTHER || label === AREA_UNKNOWN;

/**
 * 集計済みのラベルを丸める。サーバーの市区町村ラベルは「都道府県 / 市区町村」の形
 * （都道府県が無いときは「都道府県不明 / …」）。丸め済みのラベルを渡しても同じ結果になる。
 */
export function roundAreaLabel(dimension: 'prefecture' | 'municipality', label: string, fallbackPrefecture?: string | null): string {
  const trimmed = label.trim();
  if (reserved(trimmed) || trimmed === '') return trimmed === '' ? AREA_UNKNOWN : trimmed;
  if (dimension === 'prefecture') return prefectureLabel(parseApplicantArea(trimmed, null));
  const separator = trimmed.indexOf(' / ');
  if (separator >= 0) {
    const prefecture = trimmed.slice(0, separator);
    const city = trimmed.slice(separator + 3);
    return municipalityLabel(parseApplicantArea(prefecture === '都道府県不明' ? fallbackPrefecture ?? null : prefecture, city));
  }
  const unknownCity = /^(.+)（市区町村不明）$/u.exec(trimmed);
  if (unknownCity?.[1]) return municipalityLabel({ prefecture: parseApplicantArea(unknownCity[1], null).prefecture, municipality: null });
  return municipalityLabel(parseApplicantArea(fallbackPrefecture ?? null, trimmed));
}

/** 応募の件数が MINIMUM_AREA_COUNT 未満の地域名を「その他」に置き換える（「不明」はそのまま）。 */
function smallAreas(counts: Map<string, number>): Set<string> {
  return new Set([...counts].filter(([label, count]) => !reserved(label) && count < MINIMUM_AREA_COUNT).map(([label]) => label));
}

function orderAreas<T extends { category: string }>(rows: T[]): T[] {
  const rank = (label: string) => label === AREA_OTHER ? 1 : label === AREA_UNKNOWN ? 2 : 0;
  return [...rows].sort((left, right) => rank(left.category) - rank(right.category));
}

/** 地域の分布を丸め、件数の少ない地域を「その他」にまとめる。合計件数は変えない。 */
export function roundAreaDistribution(distribution: ApplicantDistribution, dimension: 'prefecture' | 'municipality'): ApplicantDistribution {
  const rounded = new Map<string, number>();
  for (const row of distribution.categories) {
    const label = roundAreaLabel(dimension, row.category);
    rounded.set(label, (rounded.get(label) ?? 0) + row.count);
  }
  const small = smallAreas(rounded);
  const merged = new Map<string, number>();
  for (const [label, count] of rounded) {
    const target = small.has(label) ? AREA_OTHER : label;
    merged.set(target, (merged.get(target) ?? 0) + count);
  }
  const total = distribution.total;
  return { total, categories: orderAreas([...merged].map(([category, count]) => ({ category, count, percentage: total ? count / total * 100 : null }))) };
}

/** 件数の表（ラベル → 件数）を丸める。HubSpot 読み取り結果の表示用。 */
export function roundAreaCounts(dimension: 'prefecture' | 'municipality', counts: Record<string, number>): Record<string, number> {
  const total = Object.values(counts).reduce((sum, count) => sum + count, 0);
  const distribution = roundAreaDistribution({ total, categories: Object.entries(counts).map(([category, count]) => ({ category, count, percentage: null })) }, dimension);
  return Object.fromEntries(distribution.categories.map(row => [row.category, row.count]));
}

type JointCell = JointDemographics['cells'][number];
function mergeCells(cells: readonly JointCell[]): JointCell[] {
  const merged = new Map<string, JointCell>();
  for (const cell of cells) {
    const key = JSON.stringify([cell.gender, cell.age, cell.prefecture, cell.municipality]);
    const existing = merged.get(key);
    merged.set(key, existing ? { ...existing, count: existing.count + cell.count } : cell);
  }
  return [...merged.values()];
}

/**
 * 複合条件（性別 × 年代 × 地域）の集計を丸める。合計は変えない。サーバー（applicant_area.rs の
 * protect_joint_cells）と同じ規則。
 * 1. 地域を都道府県 + 市区町村に丸める。市区町村が分かれば、都道府県はその市区町村から決め直す。
 * 2. 求人内の応募が 3 件未満の都道府県・市区町村は「その他」にする。
 * 3. それでも 3 件未満のセルは市区町村を「その他」にし、まだ 3 件未満なら都道府県も「その他」にする
 *    （「女性・60代・由布市 = 1件」のように、組み合わせで 1 件の応募を特定できる地域を出さない）。
 * 地域ごとの合計は、このセルから数え直したものだけを出す（roundApplicantAreasInRecord）。
 */
export function roundJointDemographics(joint: JointDemographics): JointDemographics {
  const rounded = joint.cells.map(cell => {
    const prefecture = roundAreaLabel('prefecture', cell.prefecture);
    const municipality = roundAreaLabel('municipality', cell.municipality, prefecture === AREA_UNKNOWN ? null : prefecture);
    return { ...cell, prefecture: reserved(municipality) ? prefecture : roundAreaLabel('prefecture', municipality), municipality };
  });
  const prefectureCounts = new Map<string, number>();
  const municipalityCounts = new Map<string, number>();
  for (const cell of rounded) {
    prefectureCounts.set(cell.prefecture, (prefectureCounts.get(cell.prefecture) ?? 0) + cell.count);
    municipalityCounts.set(cell.municipality, (municipalityCounts.get(cell.municipality) ?? 0) + cell.count);
  }
  const smallPrefectures = smallAreas(prefectureCounts);
  const smallMunicipalities = smallAreas(municipalityCounts);
  const byArea = mergeCells(rounded.map(cell => ({ ...cell, prefecture: smallPrefectures.has(cell.prefecture) ? AREA_OTHER : cell.prefecture, municipality: smallMunicipalities.has(cell.municipality) ? AREA_OTHER : cell.municipality })));
  const withoutCity = mergeCells(byArea.map(cell => cell.count < MINIMUM_AREA_COUNT && !reserved(cell.municipality) ? { ...cell, municipality: AREA_OTHER } : cell));
  const cells = mergeCells(withoutCity.map(cell => cell.count < MINIMUM_AREA_COUNT && !reserved(cell.prefecture) ? { ...cell, prefecture: AREA_OTHER } : cell));
  return { total: joint.total, cells };
}

type Distributions = Partial<Record<'gender' | 'age' | 'prefecture' | 'municipality', ApplicantDistribution>>;
function roundDistributions<T extends Distributions>(distributions: T): T {
  const next = { ...distributions };
  if (next.prefecture) next.prefecture = roundAreaDistribution(next.prefecture, 'prefecture');
  if (next.municipality) next.municipality = roundAreaDistribution(next.municipality, 'municipality');
  return next;
}

/**
 * 求人全体の地域の分布を、丸めた複合条件のセルから数え直す（サーバーと同じ）。地域ごとの合計を元の地域で数えると、
 * 名前の出ているセルを引き算して「その他」にしたセルの地域が分かってしまうため。セルの合計が分布の合計と違うときは、
 * どの地域の名前も出さない。
 */
function areasFromCells(distributions: Distributions, joint: JointDemographics | undefined): Distributions {
  const next = { ...distributions };
  for (const dimension of ['prefecture', 'municipality'] as const) {
    const distribution = next[dimension];
    if (!distribution) continue;
    const counts = new Map<string, number>();
    const cellTotal = joint?.cells.reduce((sum, cell) => sum + cell.count, 0);
    if (joint && cellTotal === distribution.total) {
      for (const cell of joint.cells) counts.set(cell[dimension], (counts.get(cell[dimension]) ?? 0) + cell.count);
    } else {
      for (const row of distribution.categories) {
        const label = reserved(row.category) ? row.category : AREA_OTHER;
        counts.set(label, (counts.get(label) ?? 0) + row.count);
      }
    }
    const total = distribution.total;
    next[dimension] = roundAreaDistribution({ total, categories: [...counts].map(([category, count]) => ({ category, count, percentage: total ? count / total * 100 : null })) }, dimension);
  }
  return next;
}

/** 取り込んだ求人の応募者の地域（求人全体・版別・複合条件）をすべて丸める。取り込み直後に必ず通す。 */
export function roundApplicantAreasInRecord(job: JobCopyRecord): JobCopyRecord {
  const joint = job.jointDemographics ? roundJointDemographics(job.jointDemographics) : undefined;
  return {
    ...job,
    ...(job.overallApplications ? { overallApplications: { ...job.overallApplications, distributions: areasFromCells(roundDistributions(job.overallApplications.distributions), joint) } } : {}),
    ...(joint ? { jointDemographics: joint } : {}),
    versions: job.versions.map(version => version.distributions ? { ...version, distributions: roundDistributions(version.distributions) } : version),
  };
}
