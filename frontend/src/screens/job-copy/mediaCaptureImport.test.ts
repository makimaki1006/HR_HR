import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { createElement } from 'react';
import { parseMediaCapture } from './mediaCaptureParser';
import { MediaCaptureImport } from './MediaCaptureImport';

const capturedAt = '2026-10-03T06:00:00.000Z';
const sample = () => ({
  schemaVersion: 1, capturedAt,
  jobs: [{ id: 'fixture-job', title: '架空の求人', company: '架空会社', media: 'HRハッカー', mediaJobId: 'FAKE-001', location: '架空市', body: '職場紹介\n本文を保持します。\n',
    images: [{ id: 'fixture-image', url: 'data:image/jpeg;base64,/9j/2Q==', caption: '架空画像', contentHash: 'a'.repeat(64) }] }],
});

describe('private media capture import', () => {
  const snapshotRoute = '/api/job-copy/snapshot-image?listing_id=30&version=10&slot=3&image_hash=' + 'a'.repeat(64);
  it('binds the delayed request to normalized content evidence without changing reference evidence', () => {
    const source = sample();
    const original = source.jobs[0]?.images[0];
    if (!original) throw new Error('Missing test fixture.');
    const acquired = { ...original, url: snapshotRoute, contentHash: 'A'.repeat(64), sourceReferenceHash: 'B'.repeat(64), sourceSlot: 3 };
    const jobs = [{ ...source.jobs[0], images: [acquired] }];
    expect(parseMediaCapture(JSON.stringify({ ...source, jobs }))[0]?.versions[0]?.images?.[0]).toMatchObject({ url: snapshotRoute, contentHash: 'a'.repeat(64), sourceReferenceHash: 'b'.repeat(64), sourceSlot: 3 });
  });
  it.each([snapshotRoute, snapshotRoute.replace('version=10', 'version=0'), snapshotRoute.replace('listing_id=30', 'listing_id=' + '9'.repeat(30))])('retains a canonical delayed image route and its evidence: %s', url => {
    const source = sample();
    const original = source.jobs[0]?.images[0];
    if (!original) throw new Error('Missing test fixture.');
    original.url = url;
    expect(parseMediaCapture(JSON.stringify(source))[0]?.versions[0]?.images?.[0]).toMatchObject({ url, contentHash: 'a'.repeat(64) });
  });
  it.each([
    `https://example.test${snapshotRoute}`, `//example.test${snapshotRoute}`, `${snapshotRoute}#image`, `${snapshotRoute}&token=secret`, `${snapshotRoute}&slot=1`, `${snapshotRoute}\n`,
    snapshotRoute.replace('listing_id=30', 'listing_id='), snapshotRoute.replace('listing_id=30', 'listing_id=' + '9'.repeat(31)), snapshotRoute.replace('listing_id=30', 'listing_id=%33%30'),
    snapshotRoute.replace('version=10', 'version=11'), snapshotRoute.replace('version=10', 'version=01'), snapshotRoute.replace('version=10', 'version=-1'), snapshotRoute.replace('version=10', 'version=1.0'),
    snapshotRoute.replace('slot=3', 'slot=0'), snapshotRoute.replace('slot=3', 'slot=4'), snapshotRoute.replace('slot=3', 'slot=03'), snapshotRoute.replace('listing_id=30&version=10', 'version=10&listing_id=30'),
    snapshotRoute.replace('/snapshot-image', '/other'),
    snapshotRoute.replace('&image_hash=' + 'a'.repeat(64), ''), snapshotRoute.replace('a'.repeat(64), 'A'.repeat(64)), snapshotRoute.replace('a'.repeat(64), 'b'.repeat(64)), snapshotRoute.replace('a'.repeat(64), 'a'.repeat(63)),
  ])('rejects ambiguous or noncanonical delayed image URLs: %s', url => {
    const source = sample();
    const original = source.jobs[0]?.images[0];
    if (!original) throw new Error('Missing test fixture.');
    original.url = url;
    expect(() => parseMediaCapture(JSON.stringify(source))).toThrow();
  });
  const imageRoute = '/api/job-copy/image?company_id=10&listing_id=30&manifest_id=synthetic_Manifest-123&slot=2';
  it('retains only the exact authorized same-origin image route with unchanged hash evidence', () => {
    const source = sample();
    const original = source.jobs[0]?.images[0];
    if (!original) throw new Error('Missing test fixture.');
    original.url = imageRoute;
    const parsed = parseMediaCapture(JSON.stringify(source));
    expect(parsed[0]?.versions[0]?.images?.[0]).toMatchObject({ url: imageRoute, contentHash: 'a'.repeat(64), caption: '架空画像' });
  });
  it.each([
    `https://example.test${imageRoute}`, `//example.test${imageRoute}`, `${imageRoute}#image`,
    `${imageRoute}&token=secret`, `${imageRoute}&slot=1`, `${imageRoute}&company_id=11`,
    imageRoute.replace('company_id=10', 'company_id='), imageRoute.replace('listing_id=30', 'listing_id=abc'),
    imageRoute.replace('manifest_id=synthetic_Manifest-123', 'manifest_id=short'),
    imageRoute.replace('manifest_id=synthetic_Manifest-123', 'manifest_id=' + 'a'.repeat(201)),
    imageRoute.replace('manifest_id=synthetic_Manifest-123', 'manifest_id=unsafe%2Fmanifest'),
    imageRoute.replace('slot=2', 'slot=0'), imageRoute.replace('slot=2', 'slot=-1'),
    imageRoute.replace('slot=2', 'slot=02'), imageRoute.replace('slot=2', 'slot=1.5'),
    imageRoute.replace('slot=2', 'slot=9007199254740992'),
    imageRoute.replace('/api/job-copy/image', '/api/job-copy/other'),
    imageRoute.replace('/api/job-copy/image', '/api/job-copy/../image'),
    imageRoute.replace('company_id=10&listing_id=30', 'listing_id=30&company_id=10'),
    imageRoute.replace('company_id=10', 'company_id=%31%30'), `${imageRoute}\n`,
  ])('rejects noncanonical proxy references including query ambiguity: %s', url => {
    const source = sample();
    const original = source.jobs[0]?.images[0];
    if (!original) throw new Error('Missing test fixture.');
    original.url = url;
    expect(() => parseMediaCapture(JSON.stringify(source))).toThrow();
  });
  it('preserves hashed references and original slots when image pixels are unavailable', () => {
    const source = sample();
    const jobs = [{ ...source.jobs[0], images: [], imageReferences: [{ referenceHash: 'B'.repeat(64), slot: 2 }], imageAcquisition: { expected: 1, downloaded: 0, failed: 1 } }];
    const version = parseMediaCapture(JSON.stringify({ ...source, jobs }))[0]?.versions[0];
    expect(version?.imageReferences).toEqual([{ referenceHash: 'b'.repeat(64), slot: 2 }]);
    expect(version).not.toHaveProperty('images');
  });

  it('rejects malformed reference hashes and ambiguous or invalid image slots', () => {
    const source = sample();
    for (const imageReferences of [
      [{ referenceHash: 'invalid', slot: 1 }],
      [{ referenceHash: 'b'.repeat(64), slot: 0 }],
      [{ referenceHash: 'b'.repeat(64), slot: 4 }],
      [{ referenceHash: 'b'.repeat(64), slot: 1 }, { referenceHash: 'c'.repeat(64), slot: 1 }],
    ]) {
      expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], imageReferences }] }))).toThrow();
    }
  });

  it('preserves full text and images as a single observation without invented history or applicants', () => {
    const source = sample();
    const records = parseMediaCapture(JSON.stringify(source));
    expect(records).toHaveLength(1);
    expect(records[0]?.versions).toHaveLength(1);
    expect(records[0]?.versions[0]).toMatchObject({ observedAt: capturedAt, body: source.jobs[0]?.body, images: source.jobs[0]?.images, applications: null, certainty: 'unknown' });
    expect(records[0]?.versions[0]).not.toHaveProperty('publishedFrom');
    expect(records[0]?.versions[0]).not.toHaveProperty('publishedUntil');
  });

  it('allows an acquired empty gallery and empty text without making up content', () => {
    const source = sample();
    const job = source.jobs[0];
    if (!job) throw new Error('Missing test fixture.');
    job.images = []; job.body = '';
    expect(parseMediaCapture(JSON.stringify(source))[0]?.versions[0]).toMatchObject({ body: '', images: [], applications: null });
  });

  it.each(['https://example.invalid/private.jpg', 'javascript:alert(1)', 'data:image/svg+xml;base64,PHN2Zz4=', 'data:text/html;base64,PGgxPg==', 'data:image/jpeg;base64,'])('rejects an unsafe or external image reference %s', (url) => {
    const source = sample();
    const image = source.jobs[0]?.images[0];
    if (!image) throw new Error('Missing test fixture.');
    image.url = url;
    expect(() => parseMediaCapture(JSON.stringify(source))).toThrow('媒体取得データの形式');
  });

  it.each(['not-a-date', '2026-02-30T06:00:00Z', '2026-10-03', '2026-10-03T24:00:00Z'])('rejects unsupported or impossible timestamps %s', (value) => {
    expect(() => parseMediaCapture(JSON.stringify({ ...sample(), capturedAt: value }))).toThrow();
  });

  it('rejects empty and oversized batches and duplicate job identifiers', () => {
    const source = sample();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [] }))).toThrow();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: Array.from({ length: 60 }, (_, i) => ({ ...source.jobs[0], id: `fixture-${String(i)}` })) }))).toThrow();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [...source.jobs, ...source.jobs] }))).toThrow();
  });

  it('accepts the documented 59-job boundary', () => {
    const source = sample();
    const jobs = Array.from({ length: 59 }, (_, i) => ({ ...source.jobs[0], id: `fixture-${String(i)}` }));
    expect(parseMediaCapture(JSON.stringify({ ...source, jobs }))).toHaveLength(59);
  });

  it('retains a partial acquisition warning instead of claiming all images were acquired', () => {
    const source = sample();
    const jobs = [{ ...source.jobs[0], imageAcquisition: { expected: 2, downloaded: 1, failed: 1 } }];
    const version = parseMediaCapture(JSON.stringify({ ...source, jobs }))[0]?.versions[0];
    expect(version?.images).toHaveLength(1);
    expect(version?.note).toContain('対象2点・取得1点・失敗1点');
    expect(version?.note).toContain('失敗した画像を「画像なし」や削除とは判定していません');
  });

  it('represents all failed images as unavailable rather than an acquired zero-image gallery', () => {
    const source = sample();
    const jobs = [{ ...source.jobs[0], images: [], imageAcquisition: { expected: 2, downloaded: 0, failed: 2 } }];
    const version = parseMediaCapture(JSON.stringify({ ...source, jobs }))[0]?.versions[0];
    expect(version).not.toHaveProperty('images');
    expect(version?.note).toContain('対象2点・取得0点・失敗2点');
  });

  it('rejects acquisition counts inconsistent with the embedded images', () => {
    const source = sample();
    const jobs = [{ ...source.jobs[0], imageAcquisition: { expected: 2, downloaded: 2, failed: 0 } }];
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs }))).toThrow();
  });

  it('imports older CSV text with explicit image acquisition provenance and no publication or applicant attribution', () => {
    const source = sample();
    const previous = { ...source.jobs[0], id: 'previous-csv', capturedAt: '2026-07-26T06:00:56+09:00', body: '過去CSVに保存されていた文面',
      historicalImageBytesAvailable: false, imageAcquiredAt: '2026-10-03T06:00:00.488157+00:00',
      imageAcquisitionStartedAt: '2026-10-03T06:00:00.000001+00:00', publicationBoundaryKnown: false,
      provenance: '過去参照から後日取得した画像', observationTimeBasis: 'source filename; not publication timestamp' };
    const jobs = [{ ...source.jobs[0], history: [previous] }];
    const versions = parseMediaCapture(JSON.stringify({ ...source, jobs }))[0]?.versions;
    expect(versions).toHaveLength(2);
    expect(versions?.[0]).toMatchObject({ id: 'previous-csv', observedAt: previous.capturedAt, body: previous.body, historicalImageBytesAvailable: false, applications: null });
    expect(versions?.[0]?.note).toContain('過去の画像内容の一致・変更は判定できません');
    expect(versions?.[0]?.note).toContain(previous.imageAcquiredAt);
    expect(versions?.[0]).not.toHaveProperty('publishedFrom');
    expect(versions?.[1]?.observedAt).toBe(capturedAt);
  });

  it('sorts history by observation time and defaults historical pixels to unavailable', () => {
    const source = sample();
    const history = [{ ...source.jobs[0], id: 'second', capturedAt: '2026-09-01T00:00:00Z' }, { ...source.jobs[0], id: 'first', capturedAt: '2026-08-01T00:00:00Z' }];
    const versions = parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], history }] }))[0]?.versions;
    expect(versions?.map(version => version.id).slice(0, 2)).toEqual(['first', 'second']);
    expect(versions?.[0]?.historicalImageBytesAvailable).toBe(false);
  });

  it('does not report zero images when historical pixels were never acquired', () => {
    const source = sample();
    const history = [{ ...source.jobs[0], id: 'old-no-pixels', capturedAt: '2026-09-01T00:00:00Z', images: [], historicalImageBytesAvailable: false }];
    const version = parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], history }] }))[0]?.versions[0];
    expect(version).not.toHaveProperty('images');
    expect(version?.note).toContain('画像原本は未取得');
  });

  it('rejects future, simultaneous, duplicate, or excessive historical observations', () => {
    const source = sample();
    const previous = { ...source.jobs[0], id: 'previous', capturedAt: '2026-08-01T00:00:00Z' };
    const withHistory = (history: unknown) => JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], history }] });
    expect(() => parseMediaCapture(withHistory([{ ...previous, capturedAt }]))).toThrow();
    expect(() => parseMediaCapture(withHistory([{ ...previous, capturedAt: '2027-01-01T00:00:00Z' }]))).toThrow();
    expect(() => parseMediaCapture(withHistory([previous, previous]))).toThrow();
    expect(() => parseMediaCapture(withHistory([previous, { ...previous, id: 'same-instant' }]))).toThrow();
    expect(() => parseMediaCapture(withHistory(Array.from({ length: 11 }, (_, i) => ({ ...previous, id: `old-${String(i)}` }))))).toThrow();
    expect(() => parseMediaCapture(withHistory({}))).toThrow();
  });

  it('validates images and timestamps inside history with the same bounds as current observations', () => {
    const source = sample();
    const previous = { ...source.jobs[0], id: 'previous', capturedAt: '2026-08-01T00:00:00Z' };
    const withPrevious = (value: unknown) => JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], history: [value] }] });
    expect(() => parseMediaCapture(withPrevious({ ...previous, body: 'x'.repeat(100_001) }))).toThrow();
    expect(() => parseMediaCapture(withPrevious({ ...previous, images: [{ ...previous.images?.[0], url: 'https://example.invalid/old.jpg' }] }))).toThrow();
    expect(() => parseMediaCapture(withPrevious({ ...previous, imageAcquiredAt: '2026-02-30T00:00:00Z' }))).toThrow();
    expect(() => parseMediaCapture(withPrevious({ ...previous, historicalImageBytesAvailable: 'true' }))).toThrow();
  });

  it('preserves optional hashed original image references without exposing external URLs', () => {
    const source = sample();
    const original = source.jobs[0]?.images[0];
    if (!original) throw new Error('Missing test fixture.');
    const jobs = [{ ...source.jobs[0], images: [{ ...original, sourceReferenceHash: 'B'.repeat(64) }] }];
    expect(parseMediaCapture(JSON.stringify({ ...source, jobs }))[0]?.versions[0]?.images?.[0]?.sourceReferenceHash).toBe('b'.repeat(64));
    const invalidJobs = [{ ...source.jobs[0], images: [{ ...original, sourceReferenceHash: 'https://example.invalid/image.jpg' }] }];
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: invalidJobs }))).toThrow();
  });

  it('enforces text and image bounds', () => {
    const source = sample();
    const job = source.jobs[0];
    if (!job) throw new Error('Missing test fixture.');
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...job, title: 'x'.repeat(501) }] }))).toThrow();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...job, body: 'x'.repeat(100_001) }] }))).toThrow();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...job, images: Array.from({ length: 4 }, (_, i) => ({ ...job.images[0], id: `image-${String(i)}` })) }] }))).toThrow();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...job, images: [{ ...job.images[0], url: `data:image/jpeg;base64,${'A'.repeat(2 * 1024 * 1024)}` }] }] }))).toThrow();
  });

  it('rejects malformed JSON, schema, missing images, and invalid content fingerprints', () => {
    expect(() => parseMediaCapture('{private-value')).toThrow('媒体取得データの形式');
    expect(() => parseMediaCapture(JSON.stringify({ ...sample(), schemaVersion: 2 }))).toThrow();
    const source = sample();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], images: undefined }] }))).toThrow();
    expect(() => parseMediaCapture(JSON.stringify({ ...source, jobs: [{ ...source.jobs[0], images: [{ ...source.jobs[0]?.images[0], contentHash: 'not-a-fingerprint' }] }] }))).toThrow();
  });

  it('never includes imported content in an error message', () => {
    const privateTitle = 'PRIVATE-FIXTURE-TITLE';
    expect(() => parseMediaCapture(JSON.stringify({ ...sample(), jobs: [{ id: 'fixture', title: privateTitle }] }))).toThrow(/^媒体取得データの形式を確認してください/);
  });

  it('offers a local file input and explicit display action without auto-importing', () => {
    const html = renderToStaticMarkup(createElement(MediaCaptureImport, { onImport: () => undefined }));
    expect(html).toContain('媒体取得データを読み込む');
    expect(html).toContain('type="file"');
    expect(html).toContain('APIへ送信せず');
    expect(html).not.toContain('取得データを表示');
  });
});
