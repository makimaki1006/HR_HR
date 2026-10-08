import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ApplicantReasons } from './ApplicantReasons';
import { reasonCohorts } from './applicantReasonsModel';
import { parseApplicantReasons } from './applicantReasonsParser';
import { jobs } from './data';

function first<T>(rows: T[]): T { const row = rows[0]; if (row === undefined) throw new Error('Missing synthetic item'); return row; }

function fixture() {
  return { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null,
    fetched_at: '2026-10-05T00:00:00Z', total_applicants: 2, total_source_values: 6,
    source_counts: { oubodouki: { missing: 0, blank: 0, nonblank: 2 }, ouboriyuu_baitaikisai: { missing: 1, blank: 0, nonblank: 1 }, ouboriyuu_hiaringu: { missing: 1, blank: 1, nonblank: 0 } },
    missing: 2, blank: 1, truncated: false,
    items: [
      { id: 'a'.repeat(64), text: '研修について読んだためです。', source: 'hubspot', source_property: 'oubodouki', application_date: '2026-09-01', collected_at: null, version_id: 'before' as string | null },
      { id: 'b'.repeat(64), text: '週末の勤務について確認したいです。', source: 'hubspot', source_property: 'oubodouki', application_date: '2026-09-20', collected_at: null, version_id: 'after' as string | null },
      { id: 'c'.repeat(64), text: '研修について読んだためです。', source: 'hubspot', source_property: 'ouboriyuu_baitaikisai', application_date: '2026-09-20', collected_at: null, version_id: null as string | null },
    ] };
}
const parse = (raw: unknown) => parseApplicantReasons(raw, 2, ['before', 'after']);

describe('recorded applicant reasons', () => {
  it('counts source texts including identical multi-source text without claiming applicant counts', () => {
    const result = reasonCohorts(parse(fixture()), 'before', 'after');
    expect(result?.before).toHaveLength(1);
    expect(result?.after).toHaveLength(1);
    expect(result?.unknown).toHaveLength(1);
    expect(result?.displayed).toBe(3);
    expect(reasonCohorts(parse(fixture()), 'before', 'after', 'oubodouki')?.unknown).toHaveLength(0);
  });
  it('keeps every item unknown when the backend supplies no verified cohort even when application dates are known', () => {
    const raw = fixture(); raw.items.forEach(item => { item.version_id = null; });
    const result = reasonCohorts(parse(raw), 'before', 'after');
    expect(result?.before).toHaveLength(0);
    expect(result?.after).toHaveLength(0);
    expect(result?.unknown).toHaveLength(3);
    expect(result?.versionAttributionAvailable).toBe(false);
  });
  it('retains missing and blank independently and distinguishes absent data from acquired empty texts', () => {
    const parsed = parse(fixture());
    expect(parsed).toMatchObject({ missing: 2, blank: 1, totalSourceValues: 6 });
    expect(parse(undefined)).toBeUndefined();
    expect(reasonCohorts(undefined, 'before', 'after')).toBeNull();
    const raw = fixture(); raw.items = []; raw.missing = 6; raw.blank = 0;
    Object.values(raw.source_counts).forEach(counts => { counts.missing = 2; counts.blank = 0; counts.nonblank = 0; });
    expect(parse(raw)?.available).toBe(true);
    expect(reasonCohorts(parse(raw), 'before', 'after')?.displayed).toBe(0);
  });
  it('validates counts, unique opaque IDs and published-only cohort references', () => {
    const invalidCount = fixture(); invalidCount.total_source_values = 7; expect(() => parse(invalidCount)).toThrow();
    const duplicate = fixture(); first(duplicate.items.slice(1)).id = first(duplicate.items).id; expect(() => parse(duplicate)).toThrow();
    const unknownVersion = fixture(); first(unknownVersion.items).version_id = 'unpublished-ai'; expect(() => parse(unknownVersion)).toThrow();
    const invalidDate = fixture(); first(invalidDate.items).application_date = '2026-02-30'; expect(() => parse(invalidDate)).toThrow();
  });
  it('accepts 2000 Unicode characters and explicit truncation, rejects unmarked omission or oversized text', () => {
    const raw = fixture(); first(raw.items).text = '😀'.repeat(2000); expect(parse(raw)?.items[0]?.text).toBe(first(raw.items).text);
    first(raw.items).text += 'a'; expect(() => parse(raw)).toThrow();
    const omitted = fixture(); omitted.items.pop(); expect(() => parse(omitted)).toThrow();
    omitted.truncated = true; expect(parse(omitted)?.items).toHaveLength(2);
  });
  it('rejects standalone personal fields while keeping original texts collapsed, escaped and explicitly internal', () => {
    expect(() => parse({ ...fixture(), email: 'synthetic@example.invalid' })).toThrow();
    const raw = fixture(); raw.items.forEach(item => { item.version_id = null; }); first(raw.items).text = '<img src=x onerror="alert(1)">';
    const job = jobs[0]; if (!job) throw new Error('Missing demo job');
    const html = renderToStaticMarkup(createElement(ApplicantReasons, { job: { ...job, applicantReasons: parse(raw) } }));
    expect(html).toContain('&lt;img');
    expect(html).not.toContain('<img');
    expect(html).toContain('記録された文を開く（社内確認用）');
    expect(html).toContain('それ以外の個人情報が残っていることがあります');
    expect(html).not.toContain('<details open');
    expect(html).not.toContain('synthetic@example.invalid');
  });
});

/** The shape the server sends since 2026-10-08: six sources, applicant keys and category selections. */
function current() {
  const counts = (nonblank: number, blank = 0) => ({ missing: 2 - nonblank - blank, blank, nonblank });
  return { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null,
    fetched_at: '2026-10-08T00:00:00Z', total_applicants: 2, total_source_values: 12,
    source_counts: { oubodouki: counts(1), ouboriyuu_baitaikisai: counts(0), ouboriyuu_hiaringu: counts(0, 1), genshokumaeshokukaranotenshokuriyuu: counts(1),
      ouboriyuukategori_hiaringu: counts(2), ouboriyuukategori_baitaikisai: counts(0) },
    missing: 7, blank: 1, truncated: false,
    items: [
      { id: 'a'.repeat(64), applicant: '1'.repeat(64), text: '家から近いため', source: 'hubspot', source_property: 'oubodouki', application_date: '2026-09-01', collected_at: null, version_id: null },
      { id: 'b'.repeat(64), applicant: '2'.repeat(64), text: '山田さんの店が遠かった', source: 'hubspot', source_property: 'genshokumaeshokukaranotenshokuriyuu', application_date: null, collected_at: null, version_id: null },
    ],
    selections: [
      { applicant: '1'.repeat(64), source_property: 'ouboriyuukategori_hiaringu', value: 'kyuuyo', label: '給与', application_date: '2026-09-01' },
      { applicant: '2'.repeat(64), source_property: 'ouboriyuukategori_hiaringu', value: 'kinmuchi', label: null as string | null, application_date: null },
    ] as { applicant: string; source_property: string; value: string; label: string | null; application_date: string | null }[] };
}
const parseCurrent = (raw: unknown) => parseApplicantReasons(raw, 2, []);

describe('reasons with every source and category selections', () => {
  it('keeps applicant keys, selections with their labels, and the transfer reason as its own source', () => {
    const parsed = parseCurrent(current());
    expect(parsed?.items.map(item => [item.applicant?.slice(0, 1), item.sourceProperty, item.text])).toEqual([
      ['1', 'oubodouki', '家から近いため'], ['2', 'genshokumaeshokukaranotenshokuriyuu', '＊＊さんの店が遠かった'],
    ]);
    expect(parsed?.selections?.map(row => [row.value, row.label, row.applicationDate])).toEqual([['kyuuyo', '給与', '2026-09-01'], ['kinmuchi', null, null]]);
    expect(Object.keys(parsed?.sourceCounts ?? {})).toHaveLength(6);
  });
  it('rejects selections that do not match the category counts, and mixed old and new shapes', () => {
    const fewer = current(); fewer.selections.pop(); expect(() => parseCurrent(fewer)).toThrow();
    const wrongSource = current(); first(wrongSource.selections).source_property = 'oubodouki'; expect(() => parseCurrent(wrongSource)).toThrow();
    const noKey = current(); delete (first(noKey.items) as { applicant?: string }).applicant; expect(() => parseCurrent(noKey)).toThrow();
    const noSelections = current(); delete (noSelections as { selections?: unknown }).selections; expect(() => parseCurrent(noSelections)).toThrow();
    const oldWithSelections = { ...fixture(), selections: [] }; expect(() => parse(oldWithSelections)).toThrow();
    const categoryText = current(); first(categoryText.items).source_property = 'ouboriyuukategori_hiaringu'; expect(() => parseCurrent(categoryText)).toThrow();
  });
});
