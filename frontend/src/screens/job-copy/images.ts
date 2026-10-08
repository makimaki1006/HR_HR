import deliveryUrl from './assets/delivery.svg';
import workplaceUrl from './assets/workplace.svg';
import trainingUrl from './assets/training.svg';

export interface CopyImage { id: string; url: string; caption: string; contentHash?: string; sourceReferenceHash?: string; sourceSlot?: number }
export function referenceImages(version: { id: string; images?: CopyImage[]; imageReferences?: { referenceHash: string; slot: number }[] } | undefined): CopyImage[] | undefined {
  if (!version) return undefined;
  if (version.imageReferences !== undefined) return [...version.imageReferences].sort((a, b) => a.slot - b.slot).map(item => ({ id: `reference-${String(item.slot)}`, url: item.referenceHash, sourceReferenceHash: item.referenceHash, sourceSlot: item.slot, caption: `画像参照${String(item.slot)}` }));
  return version.images ?? imagesByVersion[version.id];
}
const delivery: CopyImage = { id: 'demo-delivery', url: deliveryUrl, caption: '配送業務の紹介（デモ素材）' };
const workplace: CopyImage = { id: 'demo-workplace', url: workplaceUrl, caption: '職場・作業環境（デモ素材）' };
const training: CopyImage = { id: 'demo-training', url: trainingUrl, caption: '研修・サポート（デモ素材）' };

// Missing entry means not acquired, rather than "there were no images".
export const imagesByVersion: Record<string, CopyImage[] | undefined> = {
  'demo-001-v1': [delivery, workplace],
  'demo-001-v2': [training, workplace],
  'demo-001-v3': [workplace, delivery],
  'demo-001-draft': [training],
  'demo-002-v1': [workplace], 'demo-002-v2': [workplace, training],
  'demo-003-v1': [workplace], 'demo-003-v2': [training, workplace],
  'demo-004-v1': [], 'demo-004-v2': [workplace],
  'demo-005-v1': [workplace, training], 'demo-005-v2': [workplace],
  'demo-006-v1': [workplace], 'demo-006-v2': [training],
  'demo-007-v1': [workplace], 'demo-007-v2': [workplace, training],
};

export function compareImages(before: CopyImage[] | undefined, after: CopyImage[] | undefined) {
  if (!before || !after) return { status: 'unknown' as const, added: [], removed: [], reordered: false };
  const identity = (image: CopyImage) => {
    if (image.sourceReferenceHash !== undefined) return image.sourceReferenceHash;
    // Transport URLs change when a saved image is served through a different
    // backend route. Without a publisher reference, retain embedded-file identity.
    const transport = image.url.startsWith('data:image/') || image.url.startsWith('/api/job-copy/image?') || image.url.startsWith('/api/job-copy/snapshot-image?');
    return transport && /^[a-f0-9]{64}$/i.test(image.contentHash ?? '') ? `file:${image.contentHash?.toLowerCase() ?? ''}` : image.url;
  };
  const countUrls = (images: CopyImage[]) => {
    const counts = new Map<string, number>();
    for (const image of images) counts.set(identity(image), (counts.get(identity(image)) ?? 0) + 1);
    return counts;
  };
  const oldCounts = countUrls(before);
  const newCounts = countUrls(after);
  const unmatched = (images: CopyImage[], available: Map<string, number>) => {
    const remaining = new Map(available);
    return images.filter(image => {
      const count = remaining.get(identity(image)) ?? 0;
      if (count === 0) return true;
      remaining.set(identity(image), count - 1);
      return false;
    });
  };
  const sharedOrder = (images: CopyImage[], available: Map<string, number>) => {
    const remaining = new Map(available);
    return images.flatMap(image => {
      const count = remaining.get(identity(image)) ?? 0;
      if (count === 0) return [];
      remaining.set(identity(image), count - 1);
      return [identity(image)];
    });
  };
  const added = unmatched(after, oldCounts);
  const removed = unmatched(before, newCounts);
  const oldOrder = sharedOrder(before, newCounts);
  const newOrder = sharedOrder(after, oldCounts);
  const slotsMoved = before.some(image => {
    const oldMatches = before.filter(item => identity(item) === identity(image));
    const newMatches = after.filter(item => identity(item) === identity(image));
    return oldMatches.some((old, index) => old.sourceSlot !== undefined && newMatches[index]?.sourceSlot !== undefined && old.sourceSlot !== newMatches[index].sourceSlot);
  });
  const reordered = slotsMoved || oldOrder.some((url, index) => url !== newOrder[index]);
  return { status: added.length || removed.length || reordered ? 'changed' as const : 'same_reference' as const, added, removed, reordered };
}

/** Compare archived raw file hashes independently from URL references and order. */
export function compareImageBytes(before: CopyImage[] | undefined, after: CopyImage[] | undefined, beforeArchived?: boolean, afterArchived?: boolean): 'unknown' | 'same_files' | 'changed_files' | 'not_applicable' {
  if (!before || !after || beforeArchived === false || afterArchived === false) return 'unknown';
  if (before.length === 0 && after.length === 0) return 'not_applicable';
  const hashes = (images: CopyImage[]) => images.every(image => /^[a-f0-9]{64}$/i.test(image.contentHash ?? '')) ? images.map(image => image.contentHash?.toLowerCase() ?? '').sort() : null;
  const left = hashes(before); const right = hashes(after);
  if (!left || !right) return 'unknown';
  return JSON.stringify(left) === JSON.stringify(right) ? 'same_files' : 'changed_files';
}

/**
 * How the images of a version differ from the previous version, using both comparisons:
 * the references (which image sits in which place, compareImages) and the saved files
 * (compareImageBytes). 'same' only when both agree; 'reference_only' when the references and the
 * order are the same but the files could not be compared. 'missing': this version has no image
 * data (not acquired; not "no images"). 'unknown': the previous version has no image data.
 */
export type ImageChangeKind = 'initial' | 'missing' | 'replaced' | 'reordered' | 'content' | 'same' | 'reference_only' | 'unknown';
interface ImageVersion { id: string; images?: CopyImage[]; imageReferences?: { referenceHash: string; slot: number }[]; historicalImageBytesAvailable?: boolean }
export function imageChangeKind(previous: ImageVersion | undefined, current: ImageVersion): ImageChangeKind {
  const after = referenceImages(current);
  if (after === undefined) return 'missing';
  if (!previous) return 'initial';
  const before = referenceImages(previous);
  if (before === undefined) return 'unknown';
  const references = compareImages(before, after);
  if (references.added.length || references.removed.length) return 'replaced';
  if (references.reordered) return 'reordered';
  const files = compareImageBytes(previous.images ?? imagesByVersion[previous.id], current.images ?? imagesByVersion[current.id], previous.historicalImageBytesAvailable, current.historicalImageBytesAvailable);
  if (files === 'changed_files') return 'content';
  if (files === 'same_files' || files === 'not_applicable') return 'same';
  return 'reference_only';
}
/** The short mark and the spoken text of the 画像 lane, from the same kind. */
export const IMAGE_CHANGE_MARK: Record<ImageChangeKind, { text: string; spoken: string }> = {
  initial: { text: '最初', spoken: '最初の版' },
  missing: { text: '未取得', spoken: 'この版の画像は未取得です' },
  replaced: { text: '差し替え', spoken: '画像の差し替えがあります' },
  reordered: { text: '並び順', spoken: '画像の並び順が変わりました' },
  content: { text: '中身', spoken: '同じ場所の画像の中身が変わりました' },
  same: { text: '同じ', spoken: '画像の参照・並び順・中身とも前の版と同じです' },
  reference_only: { text: '中身未確認', spoken: '画像の参照と並び順は前の版と同じです。中身は確認できません' },
  unknown: { text: '不明', spoken: '前の版の画像が未取得のため比べられません' },
};
