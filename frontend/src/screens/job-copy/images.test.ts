import { describe, expect, it } from 'vitest';
import { compareImages, compareImageBytes, referenceImages, type CopyImage } from './images';

const first: CopyImage = { id: 'first', url: '/demo/first.svg', caption: '業務風景' };
const second: CopyImage = { id: 'second', url: '/demo/second.svg', caption: '職場紹介' };
const third: CopyImage = { id: 'third', url: '/demo/third.svg', caption: '研修紹介' };

describe('archived raw bytes independently from references', () => {
  const original = { ...first, contentHash: 'a'.repeat(64) };
  it('detects same-URL content replacement and keeps reference status separate', () => {
    const replacement = { ...original, contentHash: 'b'.repeat(64) };
    expect(compareImages([original], [replacement]).status).toBe('same_reference');
    expect(compareImageBytes([original], [replacement], true, true)).toBe('changed_files');
  });
  it('does not treat a historical URL fetched now as archived historical bytes', () => {
    expect(compareImageBytes([original], [original], false, true)).toBe('unknown');
    expect(compareImageBytes([first], [first])).toBe('unknown');
  });
  it('distinguishes URL/order changes from file content changes', () => {
    const other = { ...second, contentHash: 'b'.repeat(64) };
    expect(compareImageBytes([original, other], [other, { ...original, url: '/new-url.jpg' }], true, true)).toBe('same_files');
    expect(compareImageBytes([], [])).toBe('not_applicable');
  });
});

describe('CSV references without historical image downloads', () => {
  it('compares hashed references while keeping original bytes unknown', () => {
    const previous = { id: 'old', imageReferences: [{ referenceHash: 'a'.repeat(64), slot: 2 }] };
    const current = { id: 'new', imageReferences: [{ referenceHash: 'b'.repeat(64), slot: 2 }] };
    const comparison = compareImages(referenceImages(previous), referenceImages(current));
    expect(comparison.added).toHaveLength(1); expect(comparison.removed).toHaveLength(1);
    expect(compareImageBytes(undefined, undefined)).toBe('unknown');
  });
  it('detects a changed image slot even when the only reference is unchanged', () => {
    const previous = { id: 'old', imageReferences: [{ referenceHash: 'a'.repeat(64), slot: 2 }] };
    const current = { id: 'new', imageReferences: [{ referenceHash: 'a'.repeat(64), slot: 1 }] };
    expect(compareImages(referenceImages(previous), referenceImages(current))).toEqual({ status: 'changed', added: [], removed: [], reordered: true });
  });
});

describe('captured image reference identity', () => {
  it('does not mistake deferred delivery or version indices for changes to an embedded file', () => {
    const contentHash = 'a'.repeat(64);
    const embedded = { ...first, url: 'data:image/png;base64,AAAA', contentHash, sourceSlot: 2 };
    const deferred = { ...embedded, url: `/api/job-copy/snapshot-image?listing_id=30&version=0&slot=1&image_hash=${contentHash}` };
    const historical = { ...deferred, url: deferred.url.replace('version=0', 'version=1') };
    expect(compareImages([embedded], [deferred]).status).toBe('same_reference');
    expect(compareImages([historical], [deferred]).status).toBe('same_reference');
    expect(compareImages([embedded], [{ ...deferred, contentHash: 'b'.repeat(64) }]).status).toBe('changed');
  });
  it('keeps the source reference distinct from the embedded preview bytes', () => {
    expect(compareImages([{ ...first, sourceReferenceHash: 'a'.repeat(64) }], [{ ...first, sourceReferenceHash: 'b'.repeat(64) }]).status).toBe('changed');
  });
  it('does not call a regenerated preview a new source reference', () => {
    expect(compareImages([{ ...first, sourceReferenceHash: 'a'.repeat(64) }], [{ ...first, url: '/new-preview.jpg', sourceReferenceHash: 'a'.repeat(64) }]).status).toBe('same_reference');
  });
});

describe('job copy image references', () => {
  it.each([
    [undefined, undefined],
    [undefined, []],
    [[], undefined],
    [undefined, [first]],
    [[first], undefined],
  ])('keeps unavailable acquisition distinct from an observed empty gallery', (before, after) => {
    expect(compareImages(before, after)).toEqual({ status: 'unknown', added: [], removed: [], reordered: false });
  });

  it('recognizes two observed empty galleries as the same references', () => {
    expect(compareImages([], [])).toEqual({ status: 'same_reference', added: [], removed: [], reordered: false });
  });

  it('reports an image added to an explicitly empty gallery', () => {
    expect(compareImages([], [first])).toEqual({ status: 'changed', added: [first], removed: [], reordered: false });
  });

  it('reports an image removed when an empty gallery was actually acquired', () => {
    expect(compareImages([first], [])).toEqual({ status: 'changed', added: [], removed: [first], reordered: false });
  });

  it('identifies replacement while retaining the shared image', () => {
    expect(compareImages([first, second], [third, second])).toEqual({ status: 'changed', added: [third], removed: [first], reordered: false });
  });

  it('reports changed order independently of additions or removals', () => {
    expect(compareImages([first, second], [second, first])).toEqual({ status: 'changed', added: [], removed: [], reordered: true });
  });

  it('does not confuse insertion before shared images with reordering', () => {
    expect(compareImages([first, second], [third, first, second])).toEqual({ status: 'changed', added: [third], removed: [], reordered: false });
  });

  it('counts a second occurrence of the same URL as an added image reference', () => {
    const duplicate = { ...first, id: 'duplicate-first' };
    expect(compareImages([first], [first, duplicate])).toEqual({ status: 'changed', added: [duplicate], removed: [], reordered: false });
  });

  it('counts removal of a repeated URL instead of treating it as unchanged', () => {
    const duplicate = { ...first, id: 'duplicate-first' };
    expect(compareImages([first, duplicate], [first])).toEqual({ status: 'changed', added: [], removed: [duplicate], reordered: false });
  });

  it('checks ordering of retained references even with repeated URLs', () => {
    const duplicate = { ...first, id: 'duplicate-first' };
    expect(compareImages([first, duplicate, second], [first, second, duplicate])).toEqual({ status: 'changed', added: [], removed: [], reordered: true });
  });

  it('claims only same_reference for an unchanged URL, never pixel equivalence', () => {
    // A publisher can overwrite a file at the same URL; changed local record ID
    // or caption cannot establish whether the downloaded pixels are identical.
    const reacquired = { ...first, id: 'new-acquisition', caption: '取得時の別説明' };
    expect(compareImages([first], [reacquired])).toEqual({ status: 'same_reference', added: [], removed: [], reordered: false });
  });
});
