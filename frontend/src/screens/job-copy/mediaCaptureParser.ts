import type { CopyVersion, JobCopyRecord } from './data';
import type { CopyImage } from './images';

export const MAX_CAPTURE_FILE_BYTES = 32 * 1024 * 1024;
const MAX_IMAGE_URI_LENGTH = 2 * 1024 * 1024;

function trustedImageRoute(url: string, contentHash: string): boolean {
  if (/[\r\n]/.test(url)) return false;
  // Literal canonical same-origin path only: no URL resolution, decoding or
  // permissive URLSearchParams parsing that could hide duplicate parameters.
  const deferred = /^\/api\/job-copy\/snapshot-image\?listing_id=\d{1,30}&version=(?:[0-9]|10)&slot=[1-3]&image_hash=([a-f0-9]{64})$/.exec(url);
  if (deferred) return deferred[1] === contentHash.toLowerCase();
  const match = /^\/api\/job-copy\/image\?company_id=\d{1,30}&listing_id=\d{1,30}&manifest_id=[A-Za-z0-9_-]{10,200}&slot=([1-9]\d*)$/.exec(url);
  return match !== null && Number.isSafeInteger(Number(match[1]));
}

function invalid(): never {
  throw new Error('媒体取得データの形式を確認してください。対応形式はschemaVersion 1です。');
}

function record(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}

function text(value: unknown, maximum: number, allowBlank = false): string {
  if (typeof value !== 'string' || value.length > maximum || (!allowBlank && value.trim() === '')) return invalid();
  return value;
}

function timestamp(value: unknown): string {
  const result = text(value, 40);
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,6})?(?:Z|[+-]\d{2}:\d{2})$/.exec(result);
  if (!match || !Number.isFinite(Date.parse(result))) return invalid();
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const lastDay = new Date(Date.UTC(year, month, 0)).getUTCDate();
  if (month < 1 || month > 12 || day < 1 || day > lastDay || Number(match[4]) > 23 || Number(match[5]) > 59 || Number(match[6]) > 59) return invalid();
  return result;
}

function image(value: unknown): CopyImage {
  const item = record(value);
  const url = text(item.url, MAX_IMAGE_URI_LENGTH);
  // Only embedded raster bytes or the backend-authorized same-origin route.
  const encoded = /^data:image\/(?:jpeg|png|webp);base64,([A-Za-z0-9+/]+={0,2})$/.exec(url)?.[1];
  const contentHash = text(item.contentHash, 64);
  if (!/^[a-fA-F0-9]{64}$/.test(contentHash)) return invalid();
  if (!trustedImageRoute(url, contentHash) && (!encoded || encoded.length % 4 !== 0)) return invalid();
  const reference = item.sourceReferenceHash === undefined ? undefined : text(item.sourceReferenceHash, 64);
  if (reference !== undefined && !/^[a-fA-F0-9]{64}$/.test(reference)) return invalid();
  if (item.sourceSlot !== undefined && (typeof item.sourceSlot !== 'number' || !Number.isInteger(item.sourceSlot) || item.sourceSlot < 1 || item.sourceSlot > 3)) return invalid();
  return { id: text(item.id, 200), caption: text(item.caption, 500, true), url, contentHash: contentHash.toLowerCase(), ...(reference === undefined ? {} : { sourceReferenceHash: reference.toLowerCase() }), ...(typeof item.sourceSlot === 'number' ? { sourceSlot: item.sourceSlot } : {}) };
}

function acquisition(value: unknown, downloaded: number): { note: string; unavailable: boolean } {
  if (value === undefined) return { note: '', unavailable: false };
  const status = record(value);
  const counts = [status.expected, status.downloaded, status.failed];
  if (!counts.every(count => typeof count === 'number' && Number.isSafeInteger(count) && count >= 0 && count <= 1000)) return invalid();
  const expected = status.expected as number;
  const acquired = status.downloaded as number;
  const failed = status.failed as number;
  if (acquired !== downloaded || acquired + failed !== expected) return invalid();
  return {
    note: `画像取得：対象${String(expected)}点・取得${String(acquired)}点・失敗${String(failed)}点。${failed ? '失敗した画像を「画像なし」や削除とは判定していません。' : ''}`,
    unavailable: expected > 0 && acquired === 0,
  };
}

function observedVersion(item: Record<string, unknown>, id: string, capturedAt: string, historical: boolean): CopyVersion {
  if (!Array.isArray(item.images) || item.images.length > 3) return invalid();
  const images = item.images.map((value: unknown) => image(value));
  if (new Set(images.map(value => value.id)).size !== images.length) return invalid();
  const status = acquisition(item.imageAcquisition, images.length);
  if (item.historicalImageBytesAvailable !== undefined && typeof item.historicalImageBytesAvailable !== 'boolean') return invalid();
  if (item.publicationBoundaryKnown !== undefined && typeof item.publicationBoundaryKnown !== 'boolean') return invalid();
  const imageTime = item.imageAcquiredAt === undefined ? '' : `画像取得日時：${timestamp(item.imageAcquiredAt)}。`;
  if (item.imageAcquisitionStartedAt !== undefined) timestamp(item.imageAcquisitionStartedAt);
  const provenance = item.provenance === undefined ? '' : text(item.provenance, 2000, true);
  const timeBasis = item.observationTimeBasis === undefined ? '' : `観測日時の根拠：${text(item.observationTimeBasis, 500, true)}。`;
  const historicBytes = item.historicalImageBytesAvailable ?? (historical ? false : undefined);
  let references: { referenceHash: string; slot: number }[] | undefined;
  if (item.imageReferences !== undefined) {
    if (!Array.isArray(item.imageReferences) || item.imageReferences.length > 3) return invalid();
    references = item.imageReferences.map((value: unknown) => {
      const reference = record(value);
      if (typeof reference.referenceHash !== 'string' || !/^[a-f0-9]{64}$/i.test(reference.referenceHash) || typeof reference.slot !== 'number' || !Number.isInteger(reference.slot) || reference.slot < 1 || reference.slot > 3) return invalid();
      return { referenceHash: reference.referenceHash.toLowerCase(), slot: reference.slot };
    });
    if (new Set(references.map(reference => reference.slot)).size !== references.length) return invalid();
  }
  const archived = historicBytes === false ? images.length ? '過去時点の画像データは未保存です。表示画像は過去の画像参照を後日取得したもので、過去の画像内容の一致・変更は判定できません。' : '過去時点の画像原本は未取得です。過去の画像内容の一致・変更は判定できません。' : '';
  return {
    id, label: historical ? '過去CSV観測版' : '媒体CSV取得版', source: historical ? 'HRハッカー過去CSV・画像参照' : 'HRハッカーCSV・掲載画像',
    observedAt: capturedAt, certainty: 'unknown', kind: 'published', body: text(item.body, 100_000, true),
    ...(status.unavailable || (historicBytes === false && images.length === 0) ? {} : { images }), applications: null,
    ...(references === undefined ? {} : { imageReferences: references }),
    ...(typeof historicBytes === 'boolean' ? { historicalImageBytesAvailable: historicBytes } : {}),
    ...(item.observedRawStatus === undefined ? {} : { observedPublicationStatus: text(item.observedRawStatus, 100, true) }),
    note: `媒体CSVの観測版です。媒体での掲載・更新日時と応募情報は未取得です。${historical ? '過去CSVの取得ラベルは掲載切り替わり日時を示しません。' : ''}${status.note}${imageTime}${archived}${provenance}${timeBasis}`,
  };
}

/** Parse only an explicitly selected local capture; never fetch or persist it. */
export function parseMediaCapture(input: string): JobCopyRecord[] {
  if (input.length > MAX_CAPTURE_FILE_BYTES || new TextEncoder().encode(input).byteLength > MAX_CAPTURE_FILE_BYTES) return invalid();
  let decoded: unknown;
  try {
    decoded = JSON.parse(input) as unknown;
  } catch {
    return invalid();
  }
  const bundle = record(decoded);
  if (bundle.schemaVersion !== 1) return invalid();
  const capturedAt = timestamp(bundle.capturedAt);
  if (!Array.isArray(bundle.jobs) || bundle.jobs.length < 1 || bundle.jobs.length > 59) return invalid();
  const ids = new Set<string>();
  const versionIds = new Set<string>();
  return bundle.jobs.map((value: unknown) => {
    const job = record(value);
    const id = text(job.id, 200);
    if (ids.has(id)) return invalid();
    ids.add(id);
    if (job.history !== undefined && (!Array.isArray(job.history) || job.history.length > 10)) return invalid();
    const history: CopyVersion[] = (job.history === undefined ? [] : job.history as unknown[]).map((value: unknown) => {
      const previous = record(value);
      const previousId = text(previous.id, 200);
      const previousTime = timestamp(previous.capturedAt);
      if (Date.parse(previousTime) >= Date.parse(capturedAt) || versionIds.has(previousId)) return invalid();
      versionIds.add(previousId);
      return observedVersion(previous, previousId, previousTime, true);
    }).sort((left, right) => Date.parse(left.observedAt) - Date.parse(right.observedAt));
    if (new Set(history.map(version => Date.parse(version.observedAt))).size !== history.length) return invalid();
    const currentId = `capture-${id}-${capturedAt}`;
    if (versionIds.has(currentId)) return invalid();
    versionIds.add(currentId);
    return {
      id, title: text(job.title, 500), company: text(job.company, 500, true), media: text(job.media, 100),
      mediaJobId: text(job.mediaJobId, 200), location: text(job.location, 500, true),
      versions: [...history, observedVersion(job, currentId, capturedAt, false)],
    };
  });
}
